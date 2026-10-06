// Origin: CTOX
// License: AGPL-3.0-only
use super::valid_json_bytes;

#[test]
fn candidate_raw_json_preserves_byte_grammar_and_unparsed_numbers() {
    let valid: &[&[u8]] = &[
        b"null",
        b" true \n",
        b"false",
        b"0",
        b"-0",
        b"123456789012345678901234567890",
        b"1e400",
        b"-1.25E-99999",
        br#"{"n":9007199254740993,"n":1e400}"#,
        br#"{"escaped":"\"\\\/\b\f\n\r\t\uD800","a":[{},[],true,null]}"#,
        b"\"\xff\"",
    ];
    for input in valid {
        assert!(valid_json_bytes(input), "{input:?}");
        if let Ok(text) = std::str::from_utf8(input) {
            assert!(gjson::valid(text), "shallow oracle: {input:?}");
        }
    }
    let invalid: &[&[u8]] = &[
        b"",
        b" ",
        b"nul",
        b"true false",
        b"+1",
        b"01",
        b"-",
        b"1.",
        b"1e",
        b"1e+",
        b".1",
        b"NaN",
        b"[1,]",
        b"{\"a\":1,}",
        b"{a:1}",
        b"{\"a\" 1}",
        b"[}",
        b"{]",
        b"\"unterminated",
        b"\"a\nb\"",
        br#""\x""#,
        br#""\u12x4""#,
        br#""\u123""#,
        b"\"\x00\"",
        b"\x0bnull",
    ];
    for input in invalid {
        assert!(!valid_json_bytes(input), "{input:?}");
        if let Ok(text) = std::str::from_utf8(input) {
            assert!(!gjson::valid(text), "shallow oracle: {input:?}");
        }
    }
}

#[test]
fn candidate_raw_json_validates_deep_mixed_containers_without_recursion() {
    let depth = 20_000;
    let mut raw = Vec::with_capacity(depth * 6 + 4);
    for index in 0..depth {
        raw.extend_from_slice(if index % 2 == 0 { b"{\"a\":" } else { b"[" });
    }
    raw.extend_from_slice(b"null");
    let scalar_end = raw.len();
    for index in (0..depth).rev() {
        raw.push(if index % 2 == 0 { b'}' } else { b']' });
    }
    assert!(valid_json_bytes(&raw));
    assert!(!valid_json_bytes(&raw[..raw.len() - 1]));
    let mut trailing_comma = raw.clone();
    trailing_comma.insert(scalar_end, b',');
    assert!(!valid_json_bytes(&trailing_comma));
    let mut extra_root = raw;
    extra_root.extend_from_slice(b" null");
    assert!(!valid_json_bytes(&extra_root));
}
