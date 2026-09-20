//! Stored Leadfeeder provider-response fixtures through the real adapter's
//! parser, extractor and native result serializer. No provider/network calls.
use super::*;
use crate::credentials::{
    CredentialReference, CredentialResolveError, CredentialResolver, SecretValue,
};
use crate::sources::{leadfeeder, Country, ResearchMode, SourceCtx, SourceError};

const COMPANY: &str = "Example Manufacturing AG";
const ACCOUNT: &str = "acct-fixture-1";
const MATCH: &str =
    include_str!("../fixtures/sources/leadfeeder/match_example_manufacturing_fixture.json");
const FIDELITY: &str =
    include_str!("../fixtures/sources/leadfeeder/match_field_fidelity_fixture.json");
const COUNTRY_MISMATCH: &str =
    include_str!("../fixtures/sources/leadfeeder/match_country_mismatch_at_fixture.json");
const AMBIGUOUS: &str =
    include_str!("../fixtures/sources/leadfeeder/match_ambiguous_ids_fixture.json");
const UNRELATED: &str = include_str!("../fixtures/sources/leadfeeder/match_unrelated_fixture.json");

fn serialize_provider_fixture(
    response: &Value,
    company: &str,
    country: Country,
) -> Result<Value, SourceError> {
    let ctx = SourceCtx {
        root: Path::new(""),
        country: Some(country),
        mode: ResearchMode::NewRecord,
    };
    let hits =
        leadfeeder::fixture_current_match_hits(response, ACCOUNT, company, Some(country.as_iso()))?;
    let hits = hits
        .into_iter()
        .enumerate()
        .map(|(index, hit)| direct_source_hit(leadfeeder::module(), &ctx, company, hit, index + 1))
        .collect();
    let result = SearchResponse {
        provider: "leadfeeder.com".into(),
        hits,
        evidence: vec![],
        executed_queries: vec![],
        source_failures: vec![],
    };
    Ok(ctox_web_search_payload(
        company,
        &SearchToolRequest::default(),
        ContextSize::Medium,
        &result,
        String::new(),
    ))
}

#[test]
fn leadfeeder_provider_records_reach_native_field_array_with_exact_provenance() {
    for (fixture, id, domain, employees) in [
        (
            MATCH,
            "co-fixture-1",
            "example-manufacturing.test",
            "101-500",
        ),
        (FIDELITY, "co-fixture-fields-1", "factory-24.test", "120"),
    ] {
        let response: Value = serde_json::from_str(fixture).unwrap();
        let payload = serialize_provider_fixture(&response, COMPANY, Country::De).unwrap();
        let rows = payload["results"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        let url = format!("https://api.leadfeeder.com/v1/companies/{id}?account_id={ACCOUNT}");
        assert_eq!(row["source"], "leadfeeder.com");
        assert_eq!(row["title"], COMPANY);
        assert_eq!(row["url"], url);
        let fields = row["extracted_fields"]
            .as_array()
            .expect("native array contract");
        for (field, value) in [
            ("firma_name", COMPANY),
            ("firma_domain", domain),
            ("mitarbeiter", employees),
        ] {
            let matching: Vec<_> = fields
                .iter()
                .filter(|entry| entry["field"] == field)
                .collect();
            assert_eq!(matching.len(), 1, "exactly one {field}");
            assert_eq!(matching[0]["value"], value);
        }
        for field in fields {
            assert_eq!(field["source_url"], url);
            assert_eq!(field["confidence"], "high");
            assert!(field["note"].is_null());
        }
        assert_eq!(row["verification_status"], "unverified");
        for flag in [
            "transport_verified",
            "content_extracted",
            "evidence_eligible",
        ] {
            assert_eq!(row[flag], false);
        }
        assert_eq!(payload["citations"], json!([]));
        // Real API fields are reconstructed again; generic-cache omission must
        // not make repeated fresh requests silently lose fields.
        assert_eq!(
            serialize_provider_fixture(&response, COMPANY, Country::De).unwrap()["results"],
            payload["results"]
        );
    }
}

#[test]
fn leadfeeder_rejects_foreign_country_ambiguous_ids_and_missing_ids_before_serialization() {
    for fixture in [COUNTRY_MISMATCH, UNRELATED, AMBIGUOUS] {
        let response: Value = serde_json::from_str(fixture).unwrap();
        assert!(serialize_provider_fixture(&response, COMPANY, Country::De).is_err());
    }
    let mut response: Value = serde_json::from_str(MATCH).unwrap();
    let summary = &mut response["data"][0][0]["relationships"]["company_summary"];
    summary["id"] = json!("");
    assert!(matches!(
        serialize_provider_fixture(&response, COMPANY, Country::De),
        Err(SourceError::NoMatch)
    ));

    // Valid in Austria, rejected for Germany: the rejection isn't an empty
    // fixture or parser that simply discards every record.
    let austrian: Value = serde_json::from_str(COUNTRY_MISMATCH).unwrap();
    assert_eq!(
        serialize_provider_fixture(&austrian, COMPANY, Country::At).unwrap()["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn leadfeeder_mixed_response_never_attaches_foreign_fields_to_the_owned_row() {
    let mut response: Value = serde_json::from_str(MATCH).unwrap();
    let mut foreign = response["data"][0][0].clone();
    foreign["attributes"]["match_score"] = json!(0.99);
    foreign["id"] = json!("foreign-company");
    let summary = &mut foreign["relationships"]["company_summary"];
    summary["id"] = json!("foreign-company");
    summary["attributes"]["name"] = json!("Other Holdings GmbH");
    summary["attributes"]["url"] = json!("https://foreign-fixture.test");
    response["data"][0].as_array_mut().unwrap().push(foreign);
    let payload = serialize_provider_fixture(&response, COMPANY, Country::De).unwrap();
    assert_eq!(payload["results"].as_array().unwrap().len(), 1);
    assert!(!payload.to_string().contains("foreign-fixture"));
    assert!(!payload.to_string().contains("foreign-company"));
}

struct UnsuccessfulResolver(Result<(), CredentialResolveError>);
impl CredentialResolver for UnsuccessfulResolver {
    fn resolve(
        &self,
        reference: &CredentialReference,
    ) -> Result<Option<SecretValue>, CredentialResolveError> {
        assert_eq!(reference.scope, "credentials");
        assert!(["LEADFEEDER_API_KEY", "LEADFEEDER_LEGACY_API_TOKEN"]
            .contains(&reference.name.as_str()));
        self.0.map(|()| None)
    }
}

#[test]
fn real_leadfeeder_auth_setup_preserves_absence_and_resolution_failure() {
    let root = std::env::temp_dir().join(format!(
        "ctox-leadfeeder-native-auth-fixture-{}",
        std::process::id()
    ));
    let ctx = SourceCtx {
        root: &root,
        country: Some(Country::De),
        mode: ResearchMode::NewRecord,
    };
    let missing = UnsuccessfulResolver(Ok(()));
    let error = fetch_direct_source(leadfeeder::module(), &ctx, COMPANY, Some(&missing))
        .unwrap()
        .unwrap_err();
    assert!(matches!(error, SourceError::CredentialMissing { .. }));
    let error = fetch_direct_source(leadfeeder::module(), &ctx, COMPANY, None)
        .unwrap()
        .unwrap_err();
    assert!(!matches!(error, SourceError::CredentialMissing { .. }));
    assert!(error.to_string().contains("unavailable"));
    for classification in [
        CredentialResolveError::Denied,
        CredentialResolveError::Unavailable,
        CredentialResolveError::DecryptionFailed,
        CredentialResolveError::InvalidEncoding,
    ] {
        let resolver = UnsuccessfulResolver(Err(classification));
        let error = fetch_direct_source(leadfeeder::module(), &ctx, COMPANY, Some(&resolver))
            .unwrap()
            .unwrap_err();
        assert!(!matches!(error, SourceError::CredentialMissing { .. }));
        assert!(error.to_string().contains(&classification.to_string()));
    }
    assert!(
        !root.exists(),
        "failed authorization must not create a runtime root"
    );
}
