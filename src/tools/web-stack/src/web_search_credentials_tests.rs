use super::*;
use crate::credentials::{
    resolve_credential, CredentialReference, CredentialResolveError, CredentialResolver,
    SecretValue,
};

#[test]
fn cached_pinned_request_is_rejected_before_source_or_store_access() {
    let root =
        std::env::temp_dir().join(format!("ctox-pinned-cache-denial-{}", std::process::id()));
    let config = SearchConfig::from_root(&root);
    let request = SearchToolRequest {
        external_web_access: Some(false),
        pinned_sources: vec!["leadfeeder.com".into()],
        ..SearchToolRequest::default()
    };
    let query = SearchQuery {
        text: "Example GmbH".into(),
        count: 1,
        offset: 0,
        language: None,
        region: None,
        safe_search: 1,
    };
    let resolver = FixtureResolver {
        calls: AtomicUsize::new(0),
        outcome: Ok(true),
    };
    let error = execute_search_with_resolver(
        &root,
        &config,
        &request,
        &query.text,
        &query,
        Some(&resolver),
    )
    .unwrap_err();
    assert!(error.to_string().contains("cached mode is unsupported"));
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    assert!(!root.exists());
}
use crate::sources::{
    Confidence, Country, FieldEvidence, FieldKey, ResearchMode, ShapedQuery, SourceCtx,
    SourceError, SourceHit, SourceModule, Tier,
};
use std::sync::atomic::{AtomicUsize, Ordering};

// Synthetic API source exercises the production direct-source conversion and
// serializer. This is not a Leadfeeder provider-response or live API fixture.
struct FixtureSource;
const CANARY: &str = "synthetic-authorization-canary";
impl SourceModule for FixtureSource {
    fn id(&self) -> &'static str {
        "fixture.invalid"
    }
    fn tier(&self) -> Tier {
        Tier::C
    }
    fn countries(&self) -> &'static [Country] {
        &[Country::De]
    }
    fn authoritative_for(&self) -> &'static [FieldKey] {
        &[FieldKey::FirmaName]
    }
    fn shape_query(&self, _: &str, _: &SourceCtx<'_>) -> Option<ShapedQuery> {
        panic!("API auth failure must not choose crawl")
    }
    fn fetch_direct_with_resolver(
        &self,
        _: &SourceCtx<'_>,
        _: &str,
        resolver: Option<&dyn CredentialResolver>,
    ) -> Option<Result<Vec<SourceHit>, SourceError>> {
        Some((|| {
            let value = resolve_credential(
                resolver,
                &CredentialReference {
                    scope: "credentials".into(),
                    name: "FIXTURE".into(),
                },
            )
            .map_err(|error| SourceError::Other(error.into()))?
            .ok_or(SourceError::CredentialMissing {
                secret_name: "FIXTURE",
            })?;
            assert_eq!(value.expose_secret(), CANARY);
            Ok(vec![
                SourceHit {
                    title: "Example GmbH".into(),
                    url: "https://fixture.invalid/owned".into(),
                    snippet: "fixture".into(),
                },
                SourceHit {
                    title: "Other GmbH".into(),
                    url: "https://fixture.invalid/foreign".into(),
                    snippet: "fixture".into(),
                },
            ])
        })())
    }
    fn extract_from_hits(
        &self,
        _: &SourceCtx<'_>,
        company: &str,
        hits: &[SourceHit],
    ) -> Vec<(FieldKey, FieldEvidence)> {
        assert_eq!(
            hits.len(),
            1,
            "production conversion must preserve row identity"
        );
        hits.iter()
            .filter(|hit| hit.title == company)
            .map(|hit| {
                (
                    FieldKey::FirmaName,
                    FieldEvidence {
                        value: hit.title.clone(),
                        confidence: Confidence::High,
                        source_url: hit.url.clone(),
                        note: None,
                    },
                )
            })
            .collect()
    }
}
struct FixtureResolver {
    calls: AtomicUsize,
    outcome: Result<bool, CredentialResolveError>,
}
impl CredentialResolver for FixtureResolver {
    fn resolve(
        &self,
        reference: &CredentialReference,
    ) -> Result<Option<SecretValue>, CredentialResolveError> {
        assert_eq!(reference.scope, "credentials");
        assert_eq!(reference.name, "FIXTURE");
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.outcome
            .map(|present| present.then(|| SecretValue::new(CANARY.into())))
    }
}

#[test]
fn direct_fields_reach_actual_serializer_without_promoting_verification_or_caching() {
    let resolver = FixtureResolver {
        calls: AtomicUsize::new(0),
        outcome: Ok(true),
    };
    let ctx = SourceCtx {
        root: Path::new(""),
        country: Some(Country::De),
        mode: ResearchMode::NewRecord,
    };
    let hits = fetch_direct_source(&FixtureSource, &ctx, "Example GmbH", Some(&resolver))
        .unwrap()
        .unwrap();
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
    let cache = serde_json::to_value(&hits).unwrap();
    let fresh_hits = fetch_direct_source(&FixtureSource, &ctx, "Example GmbH", Some(&resolver))
        .unwrap()
        .unwrap();
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 2);
    assert_eq!(fresh_hits[0].extracted_fields, hits[0].extracted_fields);
    let mut request = SearchToolRequest::default();
    assert!(source_cache_allowed(&request));
    request.pinned_sources.push(FixtureSource.id().into());
    assert!(!source_cache_allowed(&request));
    // Changed authority cannot recover the previous row through this path.
    let unavailable = FixtureResolver {
        calls: AtomicUsize::new(0),
        outcome: Err(CredentialResolveError::Denied),
    };
    assert!(
        fetch_direct_source(&FixtureSource, &ctx, "Example GmbH", Some(&unavailable))
            .unwrap()
            .is_err()
    );
    assert!(cache[0].get("extracted_fields").is_none());
    let cached: Vec<SearchHit> = serde_json::from_value(cache).unwrap();
    assert!(cached[0].extracted_fields.is_empty());
    let result = SearchResponse {
        provider: "fixture".into(),
        hits,
        evidence: vec![],
        executed_queries: vec![],
        source_failures: vec![],
    };
    let payload = ctox_web_search_payload(
        "Example GmbH",
        &SearchToolRequest::default(),
        ContextSize::Medium,
        &result,
        String::new(),
    );
    assert_eq!(
        payload["results"][0]["extracted_fields"],
        json!([{
            "field": "firma_name", "value": "Example GmbH", "confidence": "high", "source_url": "https://fixture.invalid/owned", "note": null
        }])
    );
    assert_eq!(payload["results"][1]["extracted_fields"], json!([]));
    for row in payload["results"].as_array().unwrap() {
        assert_eq!(row["transport_verified"], false);
        assert_eq!(row["content_extracted"], false);
        assert_eq!(row["evidence_eligible"], false);
    }
    assert_eq!(payload["citations"], json!([]));
    assert!(!payload.to_string().contains(CANARY));
    assert!(!format!("{result:?}").contains(CANARY));
}

#[test]
fn direct_source_auth_failures_remain_errors_and_never_request_crawl() {
    let ctx = SourceCtx {
        root: Path::new(""),
        country: Some(Country::De),
        mode: ResearchMode::NewRecord,
    };
    let error = fetch_direct_source(&FixtureSource, &ctx, "Example GmbH", None)
        .unwrap()
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        CredentialResolveError::Unavailable.to_string()
    );
    for outcome in [
        Ok(false),
        Err(CredentialResolveError::Denied),
        Err(CredentialResolveError::Unavailable),
        Err(CredentialResolveError::DecryptionFailed),
        Err(CredentialResolveError::InvalidEncoding),
    ] {
        let resolver = FixtureResolver {
            calls: AtomicUsize::new(0),
            outcome,
        };
        let error = fetch_direct_source(&FixtureSource, &ctx, "Example GmbH", Some(&resolver))
            .unwrap()
            .unwrap_err();
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
        if outcome == Ok(false) {
            assert!(matches!(error, SourceError::CredentialMissing { .. }));
        } else {
            assert_eq!(error.to_string(), outcome.unwrap_err().to_string());
        }
        assert!(!format!("{error:?}").contains(CANARY));
    }
}
