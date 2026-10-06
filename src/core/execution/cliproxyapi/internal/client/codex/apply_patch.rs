// ref: internal/client/codex/apply-patch/tool.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: partial
// License: MIT (upstream); modifications AGPL-3.0-only

//! Codex `apply_patch` function-call contract.
//!
//! Custom freeform declarations are rewritten into a single string `input`
//! field. Encoding matches Go `encoding/json`, including HTML escapes.

use gjson::Value;

const PARAMETERS_JSON: &str = r#"{"type":"object","properties":{"input":{"type":"string","description":"The complete apply_patch patch text."}},"required":["input"],"additionalProperties":false}"#;

const PATCH_INSTRUCTIONS: &str = "\
Call this function with a JSON object whose input field contains the complete patch text.
Use the Codex apply_patch format, not a conventional git unified diff.
Start with *** Begin Patch and end with *** End Patch.
Use *** Add File: path, *** Delete File: path, or *** Update File: path.
Every added-file content line starts with +.
For updates, use @@; context lines start with one space, removed lines with -, and added lines with +.
Use *** Move to: path for a rename and *** End of File when required by the patch grammar.
Example input:
*** Begin Patch
*** Update File: src/main.go
@@
-old
+new
*** End Patch";

const FREEFORM_WRAPPER: &str = "This is a FREEFORM tool, so do not wrap the patch in JSON.";

/// Reports whether the declaration is the custom `apply_patch` tool.
#[must_use]
pub fn is_custom_tool(tool: &Value<'_>) -> bool {
    tool.get("type").str() == "custom" && tool.get("name").str().trim() == "apply_patch"
}

/// Independent copy of the patch input schema.
#[must_use]
pub fn parameters() -> Vec<u8> {
    PARAMETERS_JSON.as_bytes().to_vec()
}

/// Explains the JSON wrapper and preserves the original patch grammar.
#[must_use]
pub fn description(tool: &Value<'_>) -> String {
    let original = tool.get("description").str().replace(FREEFORM_WRAPPER, "");
    let mut description = String::new();
    if !original.trim().is_empty() {
        description.push_str(&original);
        description.push_str("\n\n");
    }
    description.push_str(PATCH_INSTRUCTIONS);
    let grammar_field = tool.get("format.definition");
    let grammar = grammar_field.str();
    if !grammar.is_empty() {
        if grammar.contains("*** Environment ID:") {
            description.push_str("\n\nUse *** Environment ID: as specified by the patch grammar.");
        }
        description.push_str("\n\nOriginal patch grammar:\n");
        description.push_str(grammar);
    }
    description
}

/// Encodes the complete patch text as function arguments.
#[must_use]
pub fn wrap_input(input: &str) -> String {
    format!(r#"{{"input":{}}}"#, go_json_string(input))
}

/// Accepts only a JSON object containing one string field named `input`.
pub fn unwrap_input(arguments: &str) -> Result<String, &'static str> {
    let bytes = arguments.as_bytes();
    let mut index = skip_ws(bytes, 0);
    if bytes.get(index) != Some(&b'{') {
        return Err("apply_patch arguments must be a JSON object");
    }
    index = skip_ws(bytes, index + 1);
    if bytes.get(index) == Some(&b'}') {
        return Err("apply_patch arguments must contain the input field");
    }
    let (key, next) = parse_json_string(bytes, index)?;
    if key != "input" {
        return Err("apply_patch arguments must contain the input field");
    }
    index = skip_ws(bytes, next);
    if bytes.get(index) != Some(&b':') {
        return Err("decode apply_patch input key");
    }
    index = skip_ws(bytes, index + 1);
    if bytes.get(index) != Some(&b'"') {
        return Err("apply_patch input must be a string");
    }
    let (input, next) = parse_json_string(bytes, index)?;
    index = skip_ws(bytes, next);
    if bytes.get(index) != Some(&b'}') {
        return Err("apply_patch arguments must contain only one input field");
    }
    index = skip_ws(bytes, index + 1);
    if index != bytes.len() {
        return Err("apply_patch arguments must not contain trailing JSON");
    }
    Ok(input)
}

/// Encodes patch text for use inside a JSON string, without the surrounding quotes.
#[must_use]
pub fn escape_input_fragment(fragment: &str) -> String {
    let encoded = go_json_string(fragment);
    encoded[1..encoded.len() - 1].to_owned()
}

pub(crate) fn go_json_string(value: &str) -> String {
    let mut encoded = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '\u{0008}' => encoded.push_str("\\b"),
            '\u{000c}' => encoded.push_str("\\f"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            '<' => encoded.push_str("\\u003c"),
            '>' => encoded.push_str("\\u003e"),
            '&' => encoded.push_str("\\u0026"),
            '\u{2028}' => encoded.push_str("\\u2028"),
            '\u{2029}' => encoded.push_str("\\u2029"),
            control if (control as u32) < 0x20 => {
                encoded.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => encoded.push(other),
        }
    }
    encoded.push('"');
    encoded
}

fn skip_ws(bytes: &[u8], mut index: usize) -> usize {
    while matches!(bytes.get(index), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        index += 1;
    }
    index
}

fn parse_json_string(bytes: &[u8], mut index: usize) -> Result<(String, usize), &'static str> {
    if bytes.get(index) != Some(&b'"') {
        return Err("decode apply_patch arguments object");
    }
    index += 1;
    let mut decoded = String::new();
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        match byte {
            b'"' => return Ok((decoded, index)),
            b'\\' => {
                let escape = *bytes
                    .get(index)
                    .ok_or("decode apply_patch arguments object")?;
                index += 1;
                match escape {
                    b'"' => decoded.push('"'),
                    b'\\' => decoded.push('\\'),
                    b'/' => decoded.push('/'),
                    b'b' => decoded.push('\u{0008}'),
                    b'f' => decoded.push('\u{000c}'),
                    b'n' => decoded.push('\n'),
                    b'r' => decoded.push('\r'),
                    b't' => decoded.push('\t'),
                    b'u' => {
                        // encoding/json combines a surrogate pair and replaces an unpaired
                        // surrogate with U+FFFD. The streaming decoder still rejects those.
                        let (character, next) = decode_json_unicode_escape(bytes, index)?;
                        index = next;
                        decoded.push(character);
                    }
                    _ => return Err("decode apply_patch arguments object"),
                }
            }
            byte if byte < 0x20 => return Err("decode apply_patch arguments object"),
            byte => {
                let width = utf8_width(byte).ok_or("decode apply_patch arguments object")?;
                let start = index - 1;
                let end = start + width;
                let text = std::str::from_utf8(bytes.get(start..end).unwrap_or_default())
                    .map_err(|_| "decode apply_patch arguments object")?;
                decoded.push_str(text);
                index = end;
            }
        }
    }
    Err("decode apply_patch arguments object")
}

pub(crate) fn unmarshal_json_string(quoted: &[u8]) -> Result<String, &'static str> {
    let (value, next) = parse_json_string(quoted, 0)?;
    if next != quoted.len() {
        return Err("decode apply_patch arguments object");
    }
    Ok(value)
}

fn decode_json_unicode_escape(bytes: &[u8], index: usize) -> Result<(char, usize), &'static str> {
    let code = read_json_hex4(bytes, index)?;
    let next = index + 4;
    if (0xD800..=0xDBFF).contains(&code) {
        if bytes.get(next..next + 2) == Some(br"\u") {
            if let Ok(low) = read_json_hex4(bytes, next + 2) {
                if (0xDC00..=0xDFFF).contains(&low) {
                    let combined =
                        0x1_0000 + (u32::from(code - 0xD800) << 10) + u32::from(low - 0xDC00);
                    let character =
                        char::from_u32(combined).ok_or("decode apply_patch arguments object")?;
                    return Ok((character, next + 6));
                }
            }
        }
        return Ok(('\u{FFFD}', next));
    }
    if (0xDC00..=0xDFFF).contains(&code) {
        return Ok(('\u{FFFD}', next));
    }
    let character = char::from_u32(u32::from(code)).ok_or("decode apply_patch arguments object")?;
    Ok((character, next))
}

fn read_json_hex4(bytes: &[u8], index: usize) -> Result<u16, &'static str> {
    let hex = bytes
        .get(index..index + 4)
        .ok_or("decode apply_patch arguments object")?;
    let digits = std::str::from_utf8(hex).map_err(|_| "decode apply_patch arguments object")?;
    u16::from_str_radix(digits, 16).map_err(|_| "decode apply_patch arguments object")
}

fn utf8_width(byte: u8) -> Option<usize> {
    if byte < 0x80 {
        Some(1)
    } else if byte & 0b1110_0000 == 0b1100_0000 {
        Some(2)
    } else if byte & 0b1111_0000 == 0b1110_0000 {
        Some(3)
    } else if byte & 0b1111_1000 == 0b1111_0000 {
        Some(4)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{
        description, escape_input_fragment, is_custom_tool, parameters, unwrap_input, wrap_input,
    };

    #[test]
    fn parameters_are_independent_json() {
        let mut first = parameters();
        first[0] = b'x';
        let second = parameters();
        assert_eq!(second[0], b'{');
        assert!(serde_json::from_slice::<serde_json::Value>(&second).is_ok());
    }

    #[test]
    fn recognizes_only_the_custom_apply_patch_name() {
        let custom = gjson::parse(r#"{"type":"custom","name":" apply_patch "}"#);
        let function = gjson::parse(r#"{"type":"function","name":"apply_patch"}"#);
        let other = gjson::parse(r#"{"type":"custom","name":"other"}"#);
        assert!(is_custom_tool(&custom));
        assert!(!is_custom_tool(&function));
        assert!(!is_custom_tool(&other));
    }

    #[test]
    fn description_keeps_grammar_and_drops_the_freeform_wrapper() {
        let tool = gjson::parse(
            r#"{"description":"Edit files only. This is a FREEFORM tool, so do not wrap the patch in JSON. Preserve line endings.","format":{"definition":"*** Environment ID: env\nstart"}}"#,
        );
        let text = description(&tool);
        assert!(text.contains("Edit files only."));
        assert!(text.contains("Preserve line endings."));
        assert!(!text.contains("FREEFORM"));
        assert!(text.contains("Use *** Environment ID: as specified by the patch grammar."));
        assert!(text.contains("Original patch grammar:\n*** Environment ID: env\nstart"));
        assert!(text.contains("*** Begin Patch"));
    }

    #[test]
    fn wrap_and_escape_match_go_json_encoding() {
        let cases = [
            ("", r#"{"input":""}"#, ""),
            ("patch", r#"{"input":"patch"}"#, "patch"),
            (
                "\"C:\\file\"",
                r#"{"input":"\"C:\\file\""}"#,
                r#"\"C:\\file\""#,
            ),
            (
                "<>&\u{2028}\u{2029}",
                r#"{"input":"\u003c\u003e\u0026\u2028\u2029"}"#,
                r#"\u003c\u003e\u0026\u2028\u2029"#,
            ),
            (
                "\u{0000}\u{0008}\u{000c}\r\t",
                r#"{"input":"\u0000\b\f\r\t"}"#,
                r#"\u0000\b\f\r\t"#,
            ),
            ("补丁🙂", r#"{"input":"补丁🙂"}"#, "补丁🙂"),
        ];
        for (input, want_json, want_escaped) in cases {
            assert_eq!(wrap_input(input), want_json, "{input:?}");
            assert_eq!(escape_input_fragment(input), want_escaped, "{input:?}");
            assert_eq!(unwrap_input(want_json).as_deref(), Ok(input));
        }
    }

    #[test]
    fn unwrap_rejects_duplicate_extra_and_trailing_json() {
        for arguments in [
            "",
            " \r\n\t ",
            "{}",
            r#"{"patch":"text"}"#,
            r#"{"Input":"text"}"#,
            r#"{"input":null}"#,
            r#"{"input":{"x":1}}"#,
            r#"{"input":"patch","other":"value"}"#,
            r#"{"other":"value","input":"patch"}"#,
            r#"{"input":"first","input":"second"}"#,
            r#"{"input":"patch","\u0069nput":"second"}"#,
            r#"{"input":"patch"}{"input":"second"}"#,
            r#"{"input":"patch"} true"#,
            r#"["patch"]"#,
            r#"{"input":"patch",}"#,
            r#"{"input":"\x"}"#,
            "*** Begin Patch\n*** End Patch\n",
        ] {
            assert!(unwrap_input(arguments).is_err(), "{arguments}");
        }
        assert_eq!(
            unwrap_input(r#"{"\u0069nput":"patch"}"#).as_deref(),
            Ok("patch")
        );
        assert_eq!(
            unwrap_input(" \n\t{ \"input\" : \"patch\" }\r\n ").as_deref(),
            Ok("patch")
        );
        assert_eq!(
            unwrap_input(r#"{"input":"before\uD83D\uDE00after"}"#).as_deref(),
            Ok("before😀after")
        );
        assert_eq!(
            unwrap_input(r#"{"input":"\uD800\uDC00\uDBFF\uDFFF"}"#).as_deref(),
            Ok("\u{10000}\u{10FFFF}")
        );
        assert_eq!(
            unwrap_input(r#"{"input":"\uD83D"}"#).as_deref(),
            Ok("\u{FFFD}")
        );
    }
}
