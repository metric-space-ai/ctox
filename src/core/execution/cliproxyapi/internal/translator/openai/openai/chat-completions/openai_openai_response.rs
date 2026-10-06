// ref: internal/translator/openai/openai/chat-completions/openai_openai_response.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

pub fn convert_openai_response_to_openai(raw_json: &[u8]) -> Vec<Vec<u8>> {
    let mut state = None;
    convert_openai_response_to_openai_with_state(raw_json, &mut state)
}

/// Drops every chunk after the terminal `[DONE]` marker for this translation.
/// The done flag lives in the request-scoped translation state, matching the
/// upstream `param` bool.
pub fn convert_openai_response_to_openai_with_state(
    raw_json: &[u8],
    state: &mut crate::sdk::translator::TranslationState,
) -> Vec<Vec<u8>> {
    if state
        .as_ref()
        .and_then(|value| value.downcast_ref::<bool>())
        .copied()
        == Some(true)
    {
        return Vec::new();
    }
    let payload = raw_json
        .strip_prefix(b"data:")
        .map(trim_like_go_bytes)
        .unwrap_or(raw_json);
    if payload == b"[DONE]" {
        *state = Some(Box::new(true));
        Vec::new()
    } else {
        vec![payload.to_vec()]
    }
}

pub fn convert_openai_response_to_openai_non_stream(raw_json: &[u8]) -> Vec<u8> {
    raw_json.to_vec()
}

fn trim_like_go_bytes(bytes: &[u8]) -> &[u8] {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.trim().as_bytes(),
        Err(_) => bytes.trim_ascii(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_strips_exact_data_prefix_and_drops_done() {
        assert_eq!(
            convert_openai_response_to_openai(b"data:  {\"id\":1}\n"),
            vec![b"{\"id\":1}".to_vec()]
        );
        assert!(convert_openai_response_to_openai(b"data: [DONE]\n").is_empty());
        assert_eq!(
            convert_openai_response_to_openai(b" data: [DONE]"),
            vec![b" data: [DONE]".to_vec()]
        );
    }

    #[test]
    fn chunks_after_done_are_dropped_for_the_same_translation() {
        let mut state = None;
        let first = convert_openai_response_to_openai_with_state(
            br#"data: {"id":"x","choices":[]}"#,
            &mut state,
        );
        assert_eq!(first, vec![br#"{"id":"x","choices":[]}"#.to_vec()]);
        assert!(
            convert_openai_response_to_openai_with_state(b"data: [DONE]", &mut state).is_empty()
        );
        assert_eq!(
            state
                .as_ref()
                .and_then(|value| value.downcast_ref::<bool>()),
            Some(&true)
        );
        assert!(convert_openai_response_to_openai_with_state(
            br#"data: {"choices":[],"cost":"0"}"#,
            &mut state
        )
        .is_empty());
    }

    #[test]
    fn non_stream_is_byte_identical() {
        let raw = b" { \"choices\" : [] } ";
        assert_eq!(convert_openai_response_to_openai_non_stream(raw), raw);
    }
}
