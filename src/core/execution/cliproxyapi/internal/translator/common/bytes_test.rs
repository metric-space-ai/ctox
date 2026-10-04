// ref: internal/translator/common/bytes_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    join_raw_array, new_raw_array_items, set_raw_array_items, set_string_without_html_escape,
    sse_event_data,
};

#[test]
fn join_raw_array_matches_empty_single_and_multiple_contracts() {
    assert_eq!(join_raw_array(&[]), b"[]");
    assert_eq!(join_raw_array(&[br#"{"id":1}"#.to_vec()]), br#"[{"id":1}]"#);
    assert_eq!(
        join_raw_array(&[br#"{"id":1}"#.to_vec(), br#"{"id":2}"#.to_vec()]),
        br#"[{"id":1},{"id":2}]"#
    );
}

#[test]
fn new_raw_array_items_preserves_nil_and_capacity() {
    assert!(new_raw_array_items(0).is_none());
    assert!(new_raw_array_items(-1).is_none());
    let items = new_raw_array_items(3).expect("positive capacity");
    assert_eq!(items.len(), 0);
    assert_eq!(items.capacity(), 3);
}

#[test]
fn set_raw_array_items_preserves_surrounding_bytes_and_dotted_paths() {
    type RawArrayCase<'a> = (&'a [u8], &'a str, Vec<Vec<u8>>, &'a [u8]);
    let cases: &[RawArrayCase<'_>] = &[
        (br#"{"items":[]}"#, "items", vec![], br#"{"items":[]}"#),
        (
            br#"{"before":1,"request":{"contents":[]},"after":2}"#,
            "request.contents",
            vec![br#"{"id":1}"#.to_vec()],
            br#"{"before":1,"request":{"contents":[{"id":1}]},"after":2}"#,
        ),
        (
            br#"{"items":[{"old":1},{"old":2}]}"#,
            "items",
            vec![br#"{"id":1}"#.to_vec()],
            br#"{"items":[{"id":1}]}"#,
        ),
        (
            br#"{"items":[]}"#,
            "items",
            vec![br#"{"id":1}"#.to_vec(), br#"{"id":2}"#.to_vec()],
            br#"{"items":[{"id":1},{"id":2}]}"#,
        ),
    ];
    for (data, path, items, expected) in cases {
        assert_eq!(set_raw_array_items(data, path, items), *expected);
    }
}

#[test]
fn sse_event_data_is_a_self_terminating_frame() {
    let frame = sse_event_data("response.completed", br#"{"id":"resp_1"}"#);
    assert_eq!(
        frame,
        b"event: response.completed\ndata: {\"id\":\"resp_1\"}\n\n"
    );
    let concatenated = [
        sse_event_data("event1", br#"{"a":1}"#),
        sse_event_data("event2", br#"{"b":2}"#),
    ]
    .concat();
    let lines: Vec<_> = concatenated.split(|byte| *byte == b'\n').collect();
    let frames: Vec<_> = lines
        .split(|line| line.is_empty())
        .filter(|frame| !frame.is_empty())
        .collect();
    assert_eq!(frames.len(), 2);
}

#[test]
fn set_string_without_html_escape_keeps_markup_and_round_trips() {
    let cases = [
        (
            br#"{"arguments":""}"#.as_slice(),
            "arguments",
            r#"gh issue view 5802 --json number,title,body,url,state,labels,assignees 2>&1 | head -100"#,
            br#"{"arguments":"gh issue view 5802 --json number,title,body,url,state,labels,assignees 2>&1 | head -100"}"#.as_slice(),
        ),
        (
            br#"{"arguments":""}"#,
            "arguments",
            r#"{"command": "gh issue view 5802 2>&1 | head -100", "timeout": 60}"#,
            br#"{"arguments":"{\"command\": \"gh issue view 5802 2>&1 | head -100\", \"timeout\": 60}"}"#,
        ),
        (
            br#"{"type":"function_call","name":"bash","arguments":""}"#,
            "arguments",
            r#"{"html": "<tag>&value</tag>"}"#,
            br#"{"type":"function_call","name":"bash","arguments":"{\"html\": \"<tag>&value</tag>\"}"}"#,
        ),
        (
            br#"{"item":{"arguments":""}}"#,
            "item.arguments",
            "2>&1",
            br#"{"item":{"arguments":"2>&1"}}"#,
        ),
        (
            br#"{"arguments":"old"}"#,
            "arguments",
            "",
            br#"{"arguments":""}"#,
        ),
        (
            br#"{"arguments":""}"#,
            "arguments",
            "line1\nline2\t\\path\\to\\file",
            br#"{"arguments":"line1\nline2\t\\path\\to\\file"}"#,
        ),
        (
            br#"{"arguments":""}"#,
            "arguments",
            "你好，世界！🚀 <&>",
            r#"{"arguments":"你好，世界！🚀 <&>"}"#.as_bytes(),
        ),
    ];
    for (data, path, value, expected) in cases {
        let got = set_string_without_html_escape(data, path, value);
        assert_eq!(got, expected);
        let document = std::str::from_utf8(&got).unwrap();
        assert_eq!(gjson::get(document, path).str(), value);
    }
}
