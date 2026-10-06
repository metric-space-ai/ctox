// Origin: CTOX
// License: AGPL-3.0-only
// ref: internal/translator/common/apply_patch_events.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: partial

//! Responses payloads for one converted `apply_patch` call.
//!
//! The failure payload uses a fixed message. The original conversion error
//! stays on [`ApplyPatchErrorState`] for the executor.

use super::apply_patch_input::{ApplyPatchInputDecoder, ApplyPatchInputError};
use crate::internal::client::codex::apply_patch::go_json_string;

/// Owns the input decoder and identity of one tool call.
#[derive(Debug, Default)]
pub(crate) struct ApplyPatchCallState {
    pub(crate) item_id: String,
    pub(crate) call_id: String,
    pub(crate) name: String,
    pub(crate) namespace: String,
    pub(crate) output_index: i64,
    pub(crate) decoder: ApplyPatchInputDecoder,
}

impl ApplyPatchCallState {
    pub(crate) fn push_arguments(
        &mut self,
        fragment: &[u8],
    ) -> Result<String, ApplyPatchInputError> {
        self.decoder.push(fragment)
    }

    pub(crate) fn finish_arguments(
        &mut self,
        arguments: &[u8],
    ) -> Result<(String, String), ApplyPatchInputError> {
        let tail = self.decoder.finish(arguments)?;
        Ok((tail, self.decoder.input().to_owned()))
    }
}

/// Builds a Responses custom-tool input delta without SSE framing.
pub(crate) fn apply_patch_input_delta(
    state: &ApplyPatchCallState,
    delta: &str,
    sequence: i64,
) -> Vec<u8> {
    format!(
        r#"{{"type":"response.custom_tool_call_input.delta","item_id":{},"call_id":{},"output_index":{},"sequence_number":{},"delta":{}}}"#,
        go_json_string(&state.item_id),
        go_json_string(&state.call_id),
        state.output_index,
        sequence,
        go_json_string(delta),
    )
    .into_bytes()
}

/// Builds a Responses custom-tool input completion without SSE framing.
pub(crate) fn apply_patch_input_done(
    state: &ApplyPatchCallState,
    input: &str,
    sequence: i64,
) -> Vec<u8> {
    format!(
        r#"{{"type":"response.custom_tool_call_input.done","item_id":{},"call_id":{},"output_index":{},"sequence_number":{},"input":{}}}"#,
        go_json_string(&state.item_id),
        go_json_string(&state.call_id),
        state.output_index,
        sequence,
        go_json_string(input),
    )
    .into_bytes()
}

/// Builds a terminal Responses failure without exposing upstream arguments.
pub(crate) fn apply_patch_failure(response_id: &str, sequence: i64) -> Vec<u8> {
    format!(
        r#"{{"type":"response.failed","sequence_number":{},"response":{{"id":{},"object":"response","status":"failed","error":{{"type":"server_error","code":"invalid_tool_arguments","message":"Invalid apply_patch tool arguments received from upstream.","param":null}}}}}}"#,
        sequence,
        go_json_string(response_id),
    )
    .into_bytes()
}

/// Retains a conversion error for the caller.
#[derive(Debug, Default)]
pub(crate) struct ApplyPatchErrorState {
    error: Option<String>,
}

impl ApplyPatchErrorState {
    pub(crate) fn set_tool_input_error(&mut self, error: Option<String>) {
        self.error = error;
    }

    pub(crate) fn tool_input_error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_patch_failure, apply_patch_input_delta, apply_patch_input_done, ApplyPatchCallState,
        ApplyPatchErrorState,
    };
    use crate::internal::client::codex::apply_patch::wrap_input;

    #[test]
    fn interleaved_calls_keep_separate_decoders() {
        let mut first = ApplyPatchCallState {
            item_id: "item_1".to_owned(),
            call_id: "call_1".to_owned(),
            name: "apply_patch".to_owned(),
            namespace: "tools".to_owned(),
            output_index: 2,
            ..ApplyPatchCallState::default()
        };
        let mut second = ApplyPatchCallState {
            item_id: "item_2".to_owned(),
            call_id: "call_2".to_owned(),
            name: "apply_patch".to_owned(),
            output_index: 4,
            ..ApplyPatchCallState::default()
        };
        assert_eq!(
            first.push_arguments(br#"{"input":"first\uD83D"#).unwrap(),
            "first"
        );
        assert_eq!(
            second.push_arguments(b"{\"input\":\"second\xe4").unwrap(),
            "second"
        );
        assert_eq!(first.push_arguments(br"\uDE00\n").unwrap(), "😀\n");
        assert_eq!(second.push_arguments(&[0xb8, 0xad]).unwrap(), "中");
        let (tail, input) = first
            .finish_arguments(wrap_input("first😀\nlast").as_bytes())
            .unwrap();
        assert_eq!(tail, "last");
        assert_eq!(input, "first😀\nlast");
        let (tail, input) = second
            .finish_arguments(wrap_input("second中").as_bytes())
            .unwrap();
        assert_eq!(tail, "");
        assert_eq!(input, "second中");
        assert_eq!(first.item_id, "item_1");
        assert_eq!(first.call_id, "call_1");
        assert_eq!(first.namespace, "tools");
        assert_eq!(first.output_index, 2);
    }

    #[test]
    fn failure_is_isolated_to_the_bad_call() {
        let mut bad = ApplyPatchCallState::default();
        let mut good = ApplyPatchCallState::default();
        assert!(bad.push_arguments(br#"{"input":null"#).is_err());
        assert_eq!(good.push_arguments(br#"{"input":"good"#).unwrap(), "good");
        assert!(bad.finish_arguments(br#"{"input":"bad"}"#).is_err());
        assert_eq!(bad.decoder.input(), "");
        let (tail, input) = good.finish_arguments(br#"{"input":"good tail"}"#).unwrap();
        assert_eq!(tail, " tail");
        assert_eq!(input, "good tail");
        let (tail, input) = good.finish_arguments(br#"{"input":"good tail"}"#).unwrap();
        assert_eq!(tail, "");
        assert_eq!(input, "good tail");
    }

    #[test]
    fn independent_calls_do_not_share_input() {
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for index in 0..16_i64 {
                handles.push(scope.spawn(move || {
                    let mut state = ApplyPatchCallState {
                        item_id: format!("item_{index}"),
                        call_id: format!("call_{index}"),
                        output_index: index,
                        ..ApplyPatchCallState::default()
                    };
                    let want =
                        format!("*** Begin Patch\n+  call {index} 中文😀  \n*** End Patch\n");
                    let arguments = wrap_input(&want);
                    let mut output = String::new();
                    for offset in 0..arguments.len() {
                        output.push_str(
                            &state
                                .push_arguments(arguments.as_bytes()[offset..offset + 1].as_ref())
                                .unwrap(),
                        );
                    }
                    let (tail, input) = state.finish_arguments(arguments.as_bytes()).unwrap();
                    output.push_str(&tail);
                    assert_eq!(output, want);
                    assert_eq!(input, want);
                    let payload = apply_patch_input_done(&state, &input, index);
                    let document = std::str::from_utf8(&payload).unwrap();
                    assert!(serde_json::from_str::<serde_json::Value>(document).is_ok());
                    assert_eq!(gjson::get(document, "call_id").str(), state.call_id);
                }));
            }
            for handle in handles {
                handle.join().expect("call thread");
            }
        });
    }

    #[test]
    fn event_payloads_keep_identity_and_text() {
        let state = ApplyPatchCallState {
            item_id: "item_\"中".to_owned(),
            call_id: "call_\\1".to_owned(),
            name: "apply_patch".to_owned(),
            namespace: "tools".to_owned(),
            output_index: 3,
            ..ApplyPatchCallState::default()
        };
        let text = "*** Begin Patch\n+  中文 \\\"  \n*** End Patch\n";
        for (name, payload, kind, text_key, sequence) in [
            (
                "delta",
                apply_patch_input_delta(&state, text, 11),
                "response.custom_tool_call_input.delta",
                "delta",
                11,
            ),
            (
                "done",
                apply_patch_input_done(&state, text, 12),
                "response.custom_tool_call_input.done",
                "input",
                12,
            ),
        ] {
            let document = std::str::from_utf8(&payload).unwrap();
            assert!(
                serde_json::from_str::<serde_json::Value>(document).is_ok(),
                "{name}: {document}"
            );
            let root = gjson::parse(document);
            for (key, want) in [
                ("type", kind),
                ("item_id", state.item_id.as_str()),
                ("call_id", state.call_id.as_str()),
                (text_key, text),
            ] {
                assert_eq!(root.get(key).str(), want, "{name} {key}");
            }
            assert_eq!(root.get("output_index").i64(), 3, "{name}");
            assert_eq!(root.get("sequence_number").i64(), sequence, "{name}");
        }
        let zero = apply_patch_input_delta(&ApplyPatchCallState::default(), "", 0);
        let zero_document = std::str::from_utf8(&zero).unwrap();
        for key in [
            "item_id",
            "call_id",
            "output_index",
            "sequence_number",
            "delta",
        ] {
            assert!(gjson::get(zero_document, key).exists(), "{key}");
        }
    }

    #[test]
    fn failure_sanitizes_the_client_error() {
        let secret = "*** Begin Patch\n+secret-token-value\n*** End Patch\n";
        let original = format!("invalid input: {secret}");
        let mut state = ApplyPatchErrorState::default();
        assert!(state.tool_input_error().is_none());
        state.set_tool_input_error(None);
        state.set_tool_input_error(Some(original.clone()));
        assert_eq!(state.tool_input_error(), Some(original.as_str()));
        let payload = apply_patch_failure("resp_\"1", 17);
        let document = std::str::from_utf8(&payload).unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(document).is_ok());
        assert!(!document.contains("secret-token-value"));
        assert!(!document.contains("Begin Patch"));
        let root = gjson::parse(document);
        for (key, want) in [
            ("type", "response.failed"),
            ("response.id", "resp_\"1"),
            ("response.object", "response"),
            ("response.status", "failed"),
            ("response.error.code", "invalid_tool_arguments"),
            (
                "response.error.message",
                "Invalid apply_patch tool arguments received from upstream.",
            ),
        ] {
            assert_eq!(root.get(key).str(), want, "{key}");
        }
        assert_eq!(root.get("sequence_number").i64(), 17);
    }
}
