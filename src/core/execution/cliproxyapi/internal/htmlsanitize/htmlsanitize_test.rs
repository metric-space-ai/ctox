// ref: internal/htmlsanitize/htmlsanitize_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::json;

use super::{
    is_json_content_type, json_body, json_body_if_likely, json_value, looks_like_json, string,
    strings,
};

#[test]
fn json_body_escapes_only_recursive_string_values() {
    let (body, changed) = json_body(br#"{"title":"<script>alert(1)</script>","items":["safe & sound",{"description":"<b>mode</b>"}],"count":1}"#);
    assert!(changed);
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["title"], "&lt;script&gt;alert(1)&lt;/script&gt;");
    assert_eq!(body["items"][0], "safe &amp; sound");
    assert_eq!(body["items"][1]["description"], "&lt;b&gt;mode&lt;/b&gt;");
    assert_eq!(body["count"], 1);
}

#[test]
fn non_json_html_is_a_byte_identical_noop() {
    let body = b"<!doctype html><title>plugin</title>";
    let (output, changed) = json_body_if_likely(body, "text/html; charset=utf-8");
    assert!(!changed);
    assert_eq!(output, body);
}

#[test]
fn scalar_helpers_preserve_keys_numbers_bools_and_null() {
    assert_eq!(string(r#"<&'">"#), "&lt;&amp;&#39;&#34;&gt;");
    assert_eq!(
        strings(&["a&b".into(), "<c>".into()]),
        ["a&amp;b", "&lt;c&gt;"]
    );
    assert_eq!(
        json_value(&json!({"<key>": [true, null, 9223372036854775808u64, "<value>"]})),
        json!({"<key>": [true, null, 9223372036854775808u64, "&lt;value&gt;"]})
    );
}

#[test]
fn invalid_empty_and_trailing_documents_preserve_exact_bytes() {
    for body in [&b" \t\n"[..], &b"{invalid"[..], &b"{} {}"[..]] {
        let (output, changed) = json_body(body);
        assert!(!changed);
        assert_eq!(output, body);
    }
}
