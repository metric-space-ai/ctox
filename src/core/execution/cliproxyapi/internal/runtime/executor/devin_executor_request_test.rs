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
