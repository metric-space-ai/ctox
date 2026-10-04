// ref: internal/runtime/executor/helps/gemini_content_turns_test.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::gemini_content_turns::*;
use std::borrow::Cow;

fn roles(payload: &[u8], path: &str) -> Vec<String> {
    let text = std::str::from_utf8(payload).unwrap();
    gjson::get(text, path)
        .array()
        .iter()
        .map(|content| gjson::get(content.json(), "role").str().to_owned())
        .collect()
}

#[test]
fn candidate_google_content_leading_turn_contract() {
    for (input, path, expected) in [
        (
            r#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#,
            "contents",
            vec!["user"],
        ),
        (
            r#"{"contents":[{"role":"model","parts":[{"functionCall":{"name":"run"}}]},{"role":"user","parts":[{"functionResponse":{"name":"run"}}]}]}"#,
            "contents",
            vec!["user", "model", "user"],
        ),
        (
            r#"{"request":{"contents":[{"role":"model","parts":[{"text":"answer"}]}]}}"#,
            "request.contents",
            vec!["user", "model"],
        ),
        (
            r#"{"contents":[{"role":"assistant","parts":[]}]}"#,
            "contents",
            vec!["assistant"],
        ),
        (r#"{"contents":[]}"#, "contents", vec![]),
        (r#"{"model":"test"}"#, "contents", vec![]),
        (r#"{"contents":{}}"#, "contents", vec![]),
    ] {
        let out = ensure_gemini_leading_user_content(input.as_bytes(), path);
        assert_eq!(roles(&out, path), expected, "{input}");
        if !expected.is_empty()
            && expected.first() == Some(&"user")
            && input.contains("\"role\":\"model\"")
        {
            let text = std::str::from_utf8(&out).unwrap();
            let empty = gjson::get(text, &format!("{path}.0.parts.0.text"));
            assert!(empty.exists());
            assert_eq!(empty.str(), "");
        }
    }
}

#[test]
fn candidate_google_content_trailing_turn_and_function_response_contract() {
    for (input, expected) in [
        (
            r#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#,
            vec!["user"],
        ),
        (
            r#"{"contents":[{"role":"model","parts":[{"functionResponse":{"name":"run"}}]}]}"#,
            vec!["model"],
        ),
        (
            r#"{"contents":[{"role":"assistant","parts":[{"functionResponse":null}]}]}"#,
            vec!["assistant"],
        ),
        (
            r#"{"contents":[{"role":"model","parts":[{"functionCall":{"name":"run"}}]}]}"#,
            vec!["model", "user"],
        ),
        (
            r#"{"contents":[{"role":"assistant","parts":[{"text":"answer"}]}]}"#,
            vec!["assistant", "user"],
        ),
        (r#"{"contents":[]}"#, vec![]),
    ] {
        let out = ensure_gemini_trailing_user_content(input.as_bytes(), "contents");
        assert_eq!(roles(&out, "contents"), expected, "{input}");
        if expected.last() == Some(&"user") && expected.len() > 1 {
            let text = std::str::from_utf8(&out).unwrap();
            let empty = gjson::get(
                text,
                &format!("contents.{}.parts.0.text", expected.len() - 1),
            );
            assert!(empty.exists());
            assert_eq!(empty.str(), "");
        }
    }
}

#[test]
fn candidate_google_content_boundaries_preserve_raw_items_and_outer_fields() {
    let model = r#"{ "role":"model", "parts":[{"text":"answer","number":900719925474099312345}], "duplicate":1,"duplicate":2}"#;
    let input = format!(
        r#"{{"keep":1.000e+30,"duplicate":1,"duplicate":2,"request":{{"contents":[{model}],"negative_zero":-0.0}}}}"#
    );
    let output = ensure_gemini_boundary_user_content(input.as_bytes(), "request.contents");
    assert_eq!(
        roles(&output, "request.contents"),
        ["user", "model", "user"]
    );
    let text = std::str::from_utf8(&output).unwrap();
    assert!(text.contains(model));
    assert!(text.contains(r#""keep":1.000e+30,"duplicate":1,"duplicate":2"#));
    assert!(text.contains(r#""negative_zero":-0.0"#));
}

#[test]
fn candidate_google_content_large_valid_media_keeps_borrowed_storage() {
    let input = format!(
        r#"{{"contents":[{{"role":"user","parts":[{{"inlineData":{{"mimeType":"video/mp4","data":"{}"}}}}]}}]}}"#,
        "A".repeat(4 << 20)
    );
    for output in [
        ensure_gemini_leading_user_content(input.as_bytes(), "contents"),
        ensure_gemini_trailing_user_content(input.as_bytes(), "contents"),
        ensure_gemini_boundary_user_content(input.as_bytes(), "contents"),
    ] {
        assert!(matches!(output, Cow::Borrowed(_)));
        assert_eq!(output.as_ptr(), input.as_ptr());
        assert_eq!(output.len(), input.len());
    }
}

#[test]
fn candidate_google_content_boundaries_are_idempotent_and_keep_tool_results() {
    let input =
        br#"{"request":{"contents":[{"role":"model","parts":[{"functionResponse":null}]}]}}"#;
    let first = ensure_gemini_boundary_user_content(input, "request.contents");
    assert_eq!(roles(&first, "request.contents"), ["user", "model"]);
    let second = ensure_gemini_boundary_user_content(&first, "request.contents");
    assert!(matches!(second, Cow::Borrowed(_)));
    assert_eq!(second.as_ref(), first.as_ref());
    let invalid = [0xff, 0xfe];
    assert!(matches!(
        ensure_gemini_boundary_user_content(&invalid, "contents"),
        Cow::Borrowed(_)
    ));
}
