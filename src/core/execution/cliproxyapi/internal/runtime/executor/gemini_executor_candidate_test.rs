// ref: internal/runtime/executor/gemini_executor.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::super::helps::{PayloadApplyConfig, PayloadModelRule, PayloadRule};
use crate::sdk::translator::RequestTransform;

use super::*;
use crate::internal::modelconfig::{HomeModelOptions, ModelInfo};
use crate::sdk::translator::ResponseTransform;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Owner(AtomicUsize);
impl super::super::helps::CodexMultiAgentV2Processor for Owner {
    fn rewrite_spawn_agent_description(&self, _: &Headers, payload: &[u8]) -> Vec<u8> {
        payload.to_vec()
    }
    fn rewrite_input(&self, _: &Headers, payload: &[u8]) -> Vec<u8> {
        payload.to_vec()
    }
    fn translate_request(
        &self,
        _: &Headers,
        from: &Format,
        to: &Format,
        model: &str,
        payload: &[u8],
        _: bool,
    ) -> Vec<u8> {
        assert_eq!(from.as_str(), "openai-response");
        assert_eq!(to.as_str(), "gemini");
        assert_eq!(model, "gemini-test");
        let body: Value = serde_json::from_slice(payload).unwrap();
        assert_eq!(
            body.pointer("/tools/0/parameters/properties/limit/type")
                .and_then(Value::as_str),
            Some("integer")
        );
        self.0.fetch_add(1, Ordering::SeqCst);
        br#"{"contents":[],"owner_translation":true}"#.to_vec()
    }
    fn optimize_request(&self, _: &Headers, payload: &[u8]) -> (Vec<u8>, bool) {
        (payload.to_vec(), false)
    }
    fn restore_response(&self, payload: &[u8], _: bool) -> Vec<u8> {
        payload.to_vec()
    }
}
