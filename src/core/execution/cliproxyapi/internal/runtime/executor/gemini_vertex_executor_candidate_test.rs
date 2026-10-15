// ref: internal/runtime/executor/gemini_vertex_executor.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only

use super::super::helps::{PayloadApplyConfig, PayloadModelRule, PayloadRule};

use super::*;
use crate::internal::modelconfig::{HomeModelOptions, ModelInfo};
use crate::sdk::translator::ResponseTransform;
use std::collections::BTreeMap;
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
        _: &Format,
        to: &Format,
        _: &str,
        _: &[u8],
        _: bool,
    ) -> Vec<u8> {
        assert_eq!(to.as_str(), "gemini");
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
