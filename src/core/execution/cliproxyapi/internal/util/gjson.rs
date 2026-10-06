// ref: internal/util/gjson.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

/// Returns a GJSON value that may borrow directly from `data`.
///
/// Callers must not retain the result beyond `data`. Rust enforces the lifetime
/// rule that upstream documents manually. Invalid UTF-8 cannot be valid JSON
/// and returns the same missing value as empty input without an unchecked byte
/// to string conversion.
#[must_use]
pub fn get_gjson_bytes_no_copy<'a>(data: &'a [u8], path: &'a str) -> gjson::Value<'a> {
    if data.is_empty() {
        return gjson::Value::default();
    }
    let Ok(document) = std::str::from_utf8(data) else {
        return gjson::Value::default();
    };
    gjson::get(document, path)
}

/// Parses `data` into a GJSON value that borrows `data` directly.
///
/// `gjson::parse` on an owned copy is too expensive for multi-megabyte
/// payloads. Callers must not retain the result beyond `data`. Invalid UTF-8
/// returns the same missing value as empty input.
#[must_use]
pub fn parse_gjson_bytes_no_copy(data: &[u8]) -> gjson::Value<'_> {
    if data.is_empty() {
        return gjson::Value::default();
    }
    let Ok(document) = std::str::from_utf8(data) else {
        return gjson::Value::default();
    };
    gjson::parse(document)
}
