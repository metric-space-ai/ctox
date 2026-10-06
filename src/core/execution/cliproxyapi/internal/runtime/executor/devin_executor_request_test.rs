// ref: internal/runtime/executor/devin_executor_test.go:42-124,356-393,677-773
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;
use crate::internal::registry::{RegistryModelInfo, RegistryThinkingSupport};
use crate::sdk::translator::ResponseTransform;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct Fixture {
    registry: Registry,
    models: DevinModelsStore,
    catalog: StaticModelsCatalog,
    turns: DevinSessionTurns,
}
impl Fixture {
    fn new() -> Self {
        let mut catalog = StaticModelsCatalog::default();
        catalog.devin = vec![RegistryModelInfo {
            id: "swe-2".into(),
            max_completion_tokens: 4096,
            thinking: Some(RegistryThinkingSupport {
                levels: vec!["low".into(), "high".into()],
                ..Default::default()
            }),
            ..Default::default()
        }];
        Self {
            registry: Registry::new(),
            models: DevinModelsStore::default(),
            catalog,
            turns: DevinSessionTurns::default(),
        }
    }
    fn owner<'a>(&'a self, session: &'a str) -> DevinRequestOwner<'a> {
        DevinRequestOwner {
            registry: &self.registry,
            models: &self.models,
            catalog: &self.catalog,
            turns: &self.turns,
            canonical_session_id: session,
            matcher: None,
        }
    }
}
fn request() -> ExecutorRequest {
    ExecutorRequest {
        model:"swe-2(high)".into(),
        auth_attributes:BTreeMap::from([("api_key".into(),"test-only-token".into()),
            ("device_seed".into(),"test-device".into())]),
        payload:br#"{"input":[{"type":"user_input","content":"hello"}],"session_id":"same-thread","generation_config":{"max_output_tokens":5000,"thinking_level":"low"}}"#.to_vec(),
        ..ExecutorRequest::default()
    }
}
fn header<'a>(request: &'a HttpRequest, name: &str) -> &'a str {
    request
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, values)| values.first())
        .map(String::as_str)
        .unwrap_or_default()
}

#[test]
fn candidate_devin_prepared_http_credentials_preserve_exact_selected_account_precedence() {
    let mut attributes = BTreeMap::from([
        ("api_key".into(), " first ".into()),
        ("session_token".into(), "second".into()),
        ("token".into(), "third".into()),
        ("base_url".into(), DEVIN_DEFAULT_BASE_URL.into()),
    ]);
    let values = BTreeMap::from([
        ("api_key".into(), Value::String("metadata-key".into())),
        (
            "session_token".into(),
            Value::String("metadata-session".into()),
        ),
        (
            "token".into(),
            Value::String("ignored-metadata-token".into()),
        ),
        (
            "base_url".into(),
            Value::String(" https://private.invalid ".into()),
        ),
        ("device_seed".into(), Value::String(" seed ".into())),
    ]);
    let first = devin_auth_credentials(&attributes, &values);
    assert_eq!(
        (first.session_token, first.base_url, first.device_seed),
        ("first", "https://private.invalid", "seed")
    );
    assert!(!format!("{first:?}").contains("private.invalid"));
    attributes.remove("api_key");
    assert_eq!(
        devin_auth_credentials(&attributes, &values).session_token,
        "second"
    );
    attributes.remove("session_token");
    assert_eq!(
        devin_auth_credentials(&attributes, &values).session_token,
        "third"
    );
    attributes.remove("token");
    assert_eq!(
        devin_auth_credentials(&attributes, &values).session_token,
        "metadata-key"
    );
    let wrong_type = BTreeMap::from([
        ("api_key".into(), Value::Bool(true)),
        (
            "session_token".into(),
            Value::String(" metadata-session ".into()),
        ),
        ("token".into(), Value::String("ignored".into())),
    ]);
    assert_eq!(
        devin_auth_credentials(&attributes, &wrong_type).session_token,
        "metadata-session"
    );
    let ignored = BTreeMap::from([("token".into(), Value::String("ignored".into()))]);
    assert_eq!(
        devin_auth_credentials(&BTreeMap::new(), &ignored).session_token,
        ""
    );
    attributes.insert("base_url".into(), "https://chosen.invalid".into());
    attributes.insert("device_seed".into(), "account-seed".into());
    let selected = devin_auth_credentials(&attributes, &values);
    assert_eq!(
        (selected.base_url, selected.device_seed),
        ("https://chosen.invalid", "account-seed")
    );
}

#[test]
fn candidate_devin_prepared_http_headers_distinguish_unary_paths_and_custom_overrides() {
    let attributes = request().auth_attributes;
    let mut chat = HttpRequest {
        url: "https://upstream.invalid/chat?next=GetUserStatus".into(),
        ..Default::default()
    };
    prepare_devin_http_headers(&mut chat, &attributes, &BTreeMap::new());
    assert_eq!(
        header(&chat, "Authorization"),
        "Basic test-only-token-test-only-token"
    );
    assert_eq!(header(&chat, "Content-Type"), "application/connect+proto");
    assert_eq!(header(&chat, "Connect-Protocol-Version"), "1");
    assert_eq!(header(&chat, "Accept"), "*/*");
    assert_eq!(chat.headers["User-Agent"], vec![String::new()]);
    assert!(!header(&chat, "Sentry-Trace").is_empty());
    for path in [
        "GetUserStatus",
        "GetCliModelConfigs",
        "SeatManagementService/Read",
    ] {
        let mut unary = HttpRequest {
            url: format!("https://upstream.invalid/{path}"),
            ..Default::default()
        };
        prepare_devin_http_headers(&mut unary, &attributes, &BTreeMap::new());
        assert!(!unary.headers.contains_key("Sentry-Trace"));
    }
    let mut retained = HttpRequest {
        url: "https://upstream.invalid/chat".into(),
        headers: BTreeMap::from([("sentry-trace".into(), vec!["existing-trace".into()])]),
        ..Default::default()
    };
    prepare_devin_http_headers(&mut retained, &attributes, &BTreeMap::new());
    assert_eq!(header(&retained, "Sentry-Trace"), "existing-trace");
    let mut custom = attributes.clone();
    custom.insert(
        "header:Authorization".into(),
        "Custom selected credential".into(),
    );
    custom.insert("header:User-Agent".into(), "selected-client".into());
    custom.insert("header:Sentry-Trace".into(), "configured-trace".into());
    prepare_devin_http_headers(&mut chat, &custom, &BTreeMap::new());
    assert_eq!(header(&chat, "Authorization"), "Custom selected credential");
    assert_eq!(header(&chat, "User-Agent"), "selected-client");
    assert_eq!(header(&chat, "Sentry-Trace"), "configured-trace");
}

#[test]
fn candidate_devin_prepared_http_sessions_keep_uuid_spelling_and_oid_cache_identity() {
    let upper = "DB3FCCD3-CDA6-503C-B050-0C756828BE1E";
    assert_eq!(normalize_devin_uuid(&format!(" {upper} ")), upper);
    let urn = format!("URN:UUID:{upper}");
    assert_eq!(normalize_devin_uuid(&urn), urn);
    assert_eq!(
        normalize_devin_uuid("same-thread"),
        "db3fccd3-cda6-503c-b050-0c756828be1e"
    );
    let (session, cascade) = resolve_devin_session_and_cascade_ids("", "", "owner-thread");
    assert_eq!(session, "f6422876-5c80-5dd6-bbbc-f9709141e4f0");
    assert_eq!(session, cascade);
    let (explicit, cascade) =
        resolve_devin_session_and_cascade_ids("same-thread", upper, "owner-thread");
    assert_eq!(explicit, "db3fccd3-cda6-503c-b050-0c756828be1e");
    assert_eq!(cascade, upper);
    let first = normalize_devin_uuid("");
    let second = normalize_devin_uuid("");
    assert_ne!(first, second);
    assert_eq!(Uuid::parse_str(&first).unwrap().get_version_num(), 4);
}

#[test]
fn candidate_devin_prepared_http_connect_request_uses_owner_models_and_session_without_mutation() {
    let fixture = Fixture::new();
    let mut input = request();
    input
        .auth_attributes
        .insert("base_url".into(), "https://chosen.invalid///".into());
    let original = input.payload.clone();
    let prepared = prepare_devin_http_request(&fixture.owner("owner-thread"), &input).unwrap();
    assert_eq!(
        prepared.http_request.url,
        "https://chosen.invalid/exa.api_server_pb.ApiServerService/GetChatMessage"
    );
    assert_eq!(prepared.http_request.method, "POST");
    assert_eq!(prepared.chat_model_uid, "swe-2-high");
    assert_eq!(prepared.max_tokens, 4096);
    assert_eq!(prepared.session_id, "db3fccd3-cda6-503c-b050-0c756828be1e");
    assert_eq!(prepared.session_id, prepared.cascade_id);
    assert_eq!(prepared.translated_payload, original);
    assert_eq!(input.payload, original);
    let mut cursor = std::io::Cursor::new(&prepared.http_request.body);
    let connect = super::super::helps::devin_wire::read_connect_frame(&mut cursor).unwrap();
    assert_eq!(connect.flag, 0);
    assert_eq!(cursor.position() as usize, prepared.http_request.body.len());
    for expected in [
        "swe-2-high",
        "db3fccd3-cda6-503c-b050-0c756828be1e",
        "test-only-token",
    ] {
        assert!(connect
            .payload
            .windows(expected.len())
            .any(|part| part == expected.as_bytes()));
    }
    let private = format!("{prepared:?}");
    assert!(
        !private.contains("test-only-token")
            && !private.contains("hello")
            && !private.contains("chosen.invalid")
            && !private.contains("db3fccd3")
    );
    fixture.models.load(br#"{"models":[{"id":"swe-2","max_completion_tokens":2048,"thinking":{"levels":["low","high"]}}]}"#,"test").unwrap();
    let updated = prepare_devin_http_request(&fixture.owner("owner-thread"), &input).unwrap();
    assert_eq!(updated.max_tokens, 2048);
    assert_eq!(updated.session_id, prepared.session_id);
}

#[test]
fn candidate_devin_prepared_http_translation_occurs_once_and_retains_caller_original() {
    let fixture = Fixture::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    fixture.registry.register(
        Format::from("openai"),Format::from("interactions"),
        Some(Arc::new(move |model,body,stream| {
            observed.fetch_add(1,Ordering::SeqCst);
            assert_eq!(model,"swe-2(high)");
            assert!(stream);
            assert!(std::str::from_utf8(body).unwrap().contains("source-text"));
            br#"{"input":[{"type":"user_input","content":"translated-text"}],"generation_config":{"max_output_tokens":100}}"#.to_vec()
        })),ResponseTransform {stream:None,non_stream:None,token_count:None});
    let mut input = request();
    input.source_format = "openai".into();
    input.stream = true;
    input.payload = br#"{"messages":[{"role":"user","content":"source-text"}]}"#.to_vec();
    input.original_request = input.payload.clone();
    let prepared = prepare_devin_http_request(&fixture.owner("owner-thread"), &input).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(prepared.max_tokens, 100);
    assert_eq!(prepared.session_id, "f6422876-5c80-5dd6-bbbc-f9709141e4f0");
    assert!(std::str::from_utf8(&prepared.translated_payload)
        .unwrap()
        .contains("translated-text"));
    assert!(std::str::from_utf8(&input.original_request)
        .unwrap()
        .contains("source-text"));
}

#[test]
fn candidate_devin_prepared_http_missing_credentials_and_bad_url_do_not_disclose_input() {
    let fixture = Fixture::new();
    let mut input = request();
    input.auth_attributes.clear();
    let error = prepare_devin_http_request(&fixture.owner("owner-thread"), &input).unwrap_err();
    assert!(matches!(error, DevinRequestError::MissingCredentials));
    input
        .auth_attributes
        .insert("api_key".into(), "private-credential".into());
    input
        .auth_attributes
        .insert("base_url".into(), "://private-config".into());
    let error = prepare_devin_http_request(&fixture.owner("owner-thread"), &input).unwrap_err();
    assert!(matches!(error, DevinRequestError::InvalidUrl(_)));
    assert!(!format!("{error:?}").contains("private-credential"));
    assert!(!error.to_string().contains("private-config"));
}
