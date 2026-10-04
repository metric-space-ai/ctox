// ref: internal/util/gjson_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{get_gjson_bytes_no_copy, parse_gjson_bytes_no_copy};

#[test]
fn get_gjson_bytes_no_copy_matches_nested_upstream_lookup() {
    let input = br#"{"request":{"contents":[{"role":"user"}]}}"#;
    let contents = get_gjson_bytes_no_copy(input, "request.contents");

    assert!(contents.kind() == gjson::Kind::Array);
    assert_eq!(contents.get("0.role").str(), "user");
    let raw = contents.json().as_bytes();
    let input_range = input.as_ptr_range();
    assert!(raw.as_ptr() >= input_range.start && raw.as_ptr() < input_range.end);
}

#[test]
fn get_gjson_bytes_no_copy_empty_or_invalid_utf8_is_missing() {
    assert!(!get_gjson_bytes_no_copy(&[], "contents").exists());
    assert!(!get_gjson_bytes_no_copy(&[0xff], "contents").exists());
}

#[test]
fn get_gjson_bytes_no_copy_keeps_full_gjson_path_semantics() {
    let input = br#"{
        "fav.movie":"Deer Hunter",
        "friends":[
            {"name":"Dale","age":44},
            {"name":"Roger","age":68},
            {"name":"Jane","age":47}
        ]
    }"#;

    assert_eq!(
        get_gjson_bytes_no_copy(input, r"fav\.movie").str(),
        "Deer Hunter"
    );
    assert_eq!(
        get_gjson_bytes_no_copy(input, r#"friends.#(age>45)#.name"#).json(),
        r#"["Roger","Jane"]"#
    );
}

#[test]
fn parse_gjson_bytes_no_copy_borrows_the_document() {
    let input = br#"{"request":{"contents":[{"role":"user"}]}}"#;
    let root = parse_gjson_bytes_no_copy(input);
    assert!(root.kind() == gjson::Kind::Object);
    assert_eq!(root.get("request.contents.0.role").str(), "user");
    let raw = root.json().as_bytes();
    assert_eq!(raw.len(), input.len());
    assert_eq!(raw.as_ptr(), input.as_ptr());
}

#[test]
fn parse_gjson_bytes_no_copy_empty_or_invalid_utf8_is_missing() {
    assert!(!parse_gjson_bytes_no_copy(&[]).exists());
    assert!(!parse_gjson_bytes_no_copy(&[0xff]).exists());
}
