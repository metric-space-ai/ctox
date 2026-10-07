// ref: internal/api/middleware/request_logging.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::io::Read;

pub const MAX_ERROR_ONLY_CAPTURED_REQUEST_BODY_BYTES: i64 = 1 << 20;
pub const MAX_DEFERRED_ERROR_REQUEST_BODY_BYTES: i64 = 32 << 20;
const DECOMPRESSED_TRUNCATION_MARKER: &[u8] = b"[DECOMPRESSED REQUEST BODY TRUNCATED]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestMetadata {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, Vec<String>>,
    pub content_length: i64,
    pub has_body: bool,
}

impl RequestMetadata {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .and_then(|(_, values)| values.first())
            .map(String::as_str)
    }
}

pub fn should_skip_method_for_request_logging(request: Option<&RequestMetadata>) -> bool {
    let Some(request) = request else {
        return true;
    };
    if !request.method.eq_ignore_ascii_case("GET") {
        return false;
    }
    !is_responses_websocket_upgrade(request)
}

pub fn is_responses_websocket_upgrade(request: &RequestMetadata) -> bool {
    matches!(
        request.path.as_str(),
        "/v1/responses" | "/backend-api/codex/responses"
    ) && request
        .header("upgrade")
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("websocket"))
}

pub fn should_capture_request_body(
    logger_enabled: bool,
    request: Option<&RequestMetadata>,
) -> bool {
    if logger_enabled {
        return true;
    }
    let Some(request) = request else {
        return false;
    };
    if !request.has_body {
        return false;
    }
    if request
        .header("content-type")
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase()
        .starts_with("multipart/form-data")
    {
        return false;
    }
    request.content_length > 0
        && request.content_length <= MAX_ERROR_ONLY_CAPTURED_REQUEST_BODY_BYTES
}

pub fn should_log_request(path: &str) -> bool {
    !path.starts_with("/v0/management") && !path.starts_with("/management")
}

pub fn hide_api_key(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    match chars.len() {
        9.. => format!(
            "{}...{}",
            chars[..4].iter().collect::<String>(),
            chars[chars.len() - 4..].iter().collect::<String>()
        ),
        5..=8 => format!(
            "{}...{}",
            chars[..2].iter().collect::<String>(),
            chars[chars.len() - 2..].iter().collect::<String>()
        ),
        3..=4 => format!("{}...{}", chars[0], chars[chars.len() - 1]),
        _ => value.to_owned(),
    }
}

pub fn mask_authorization_header(value: &str) -> String {
    let trimmed = value.trim();
    match trimmed.split_once(' ') {
        Some((prefix, credential)) if !credential.is_empty() => {
            format!("{prefix} {}", hide_api_key(credential))
        }
        _ => hide_api_key(value),
    }
}

pub fn mask_sensitive_header_value(key: &str, value: &str) -> String {
    let key = key.trim().to_ascii_lowercase();
    if key == "x-ctox-account" {
        "[REDACTED]".to_owned()
    } else if key.contains("authorization") {
        mask_authorization_header(value)
    } else if key.contains("api-key")
        || key.contains("apikey")
        || key.contains("token")
        || key.contains("secret")
    {
        hide_api_key(value)
    } else {
        value.to_owned()
    }
}

pub fn mask_sensitive_query(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let mut parts = raw.split('&').map(str::to_owned).collect::<Vec<_>>();
    let mut changed = false;
    for part in &mut parts {
        if part.is_empty() {
            continue;
        }
        let (encoded_key, encoded_value) = part.split_once('=').unwrap_or((part.as_str(), ""));
        let decoded_key = percent_decode(encoded_key);
        if !should_mask_query_param(&decoded_key) {
            continue;
        }
        let decoded_value = percent_decode(encoded_value);
        let masked = hide_api_key(decoded_value.trim());
        *part = format!("{encoded_key}={}", percent_encode(&masked));
        changed = true;
    }
    if changed {
        parts.join("&")
    } else {
        raw.to_owned()
    }
}

fn should_mask_query_param(key: &str) -> bool {
    let key = key
        .trim()
        .to_ascii_lowercase()
        .trim_end_matches("[]")
        .to_owned();
    key == "key"
        || key.contains("api-key")
        || key.contains("apikey")
        || key.contains("api_key")
        || key.contains("token")
        || key.contains("secret")
}

fn percent_decode(value: &str) -> String {
    url::form_urlencoded::parse(value.as_bytes())
        .next()
        .map(|(key, value)| {
            if value.is_empty() {
                key.into_owned()
            } else {
                format!("{key}={value}")
            }
        })
        .unwrap_or_else(|| value.to_owned())
}

fn percent_encode(value: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("", value)
        .finish()
        .trim_start_matches('=')
        .to_owned()
}

pub fn decode_captured_request_body_for_log(raw: &[u8], encoding: &str) -> Vec<u8> {
    decode_captured_request_body(raw, encoding).unwrap_or_else(|_| raw.to_vec())
}

pub fn decode_captured_request_body_for_log_with_limit(
    raw: &[u8],
    encoding: &str,
    limit: u64,
) -> Vec<u8> {
    if raw.is_empty() || limit == 0 {
        return raw.to_vec();
    }
    let encoding = encoding.trim();
    if encoding.is_empty() || encoding.eq_ignore_ascii_case("identity") {
        return raw.to_vec();
    }
    let mut body = raw.to_vec();
    for part in encoding.split(',').rev() {
        match part.trim().to_ascii_lowercase().as_str() {
            "" | "identity" => {}
            "zstd" => match decode_zstd_with_limit(&body, limit) {
                Ok((decoded, false)) => body = decoded,
                Ok((mut decoded, true)) => {
                    if !decoded.is_empty() && !decoded.ends_with(b"\n") {
                        decoded.push(b'\n');
                    }
                    decoded.extend_from_slice(DECOMPRESSED_TRUNCATION_MARKER);
                    return decoded;
                }
                Err(_) => return raw.to_vec(),
            },
            _ => return raw.to_vec(),
        }
    }
    body
}

pub fn decode_captured_request_body(raw: &[u8], encoding: &str) -> std::io::Result<Vec<u8>> {
    let mut body = raw.to_vec();
    for part in encoding.trim().split(',').rev() {
        match part.trim().to_ascii_lowercase().as_str() {
            "" | "identity" => {}
            "zstd" => body = zstd::stream::decode_all(body.as_slice())?,
            other => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("unsupported request content encoding: {other}"),
                ))
            }
        }
    }
    Ok(body)
}

fn decode_zstd_with_limit(raw: &[u8], limit: u64) -> std::io::Result<(Vec<u8>, bool)> {
    let decoder = zstd::stream::read::Decoder::new(raw)?;
    let mut decoded = Vec::new();
    decoder.take(limit + 1).read_to_end(&mut decoded)?;
    let truncated = decoded.len() as u64 > limit;
    if truncated {
        decoded.truncate(limit as usize);
    }
    Ok((decoded, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, path: &str) -> RequestMetadata {
        RequestMetadata {
            method: method.to_owned(),
            path: path.to_owned(),
            headers: BTreeMap::new(),
            content_length: 0,
            has_body: false,
        }
    }

    #[test]
    fn skip_and_websocket_rules_match_upstream() {
        assert!(should_skip_method_for_request_logging(None));
        assert!(!should_skip_method_for_request_logging(Some(&request(
            "POST",
            "/v1/responses"
        ))));
        assert!(should_skip_method_for_request_logging(Some(&request(
            "GET",
            "/v1/models"
        ))));
        let mut websocket = request("GET", "/backend-api/codex/responses");
        websocket
            .headers
            .insert("Upgrade".to_owned(), vec!["websocket".to_owned()]);
        assert!(!should_skip_method_for_request_logging(Some(&websocket)));
    }

    #[test]
    fn error_only_capture_is_bounded_and_skips_multipart() {
        let mut small = request("POST", "/v1/responses");
        small.has_body = true;
        small.content_length = 2;
        small.headers.insert(
            "Content-Type".to_owned(),
            vec!["application/json".to_owned()],
        );
        assert!(should_capture_request_body(false, Some(&small)));
        small.content_length = MAX_ERROR_ONLY_CAPTURED_REQUEST_BODY_BYTES + 1;
        assert!(!should_capture_request_body(false, Some(&small)));
        assert!(should_capture_request_body(true, Some(&small)));
        small.content_length = 2;
        small.headers.insert(
            "Content-Type".to_owned(),
            vec!["multipart/form-data; boundary=abc".to_owned()],
        );
        assert!(!should_capture_request_body(false, Some(&small)));
    }

    #[test]
    fn management_routes_are_never_request_logged() {
        assert!(!should_log_request("/management/config"));
        assert!(!should_log_request("/v0/management/auth-files"));
        assert!(should_log_request("/v1/responses"));
    }

    #[test]
    fn masks_sensitive_headers_and_query_without_reencoding_noop() {
        assert_eq!(
            mask_sensitive_header_value("X-CTOX-Account", "unvalidated-selector-secret"),
            "[REDACTED]"
        );
        assert_eq!(
            mask_sensitive_header_value("Authorization", "Bearer abcdefghijkl"),
            "Bearer abcd...ijkl"
        );
        assert_eq!(
            mask_sensitive_header_value("X-Api-Key", "abcdefgh"),
            "ab...gh"
        );
        assert_eq!(mask_sensitive_query("model=a%2Fb&x=1"), "model=a%2Fb&x=1");
        let masked = mask_sensitive_query("model=gpt&auth_token=abcdefghijkl&x=1");
        assert_eq!(masked, "model=gpt&auth_token=abcd...ijkl&x=1");
        assert!(!masked.contains("abcdefghijkl"));
    }

    #[test]
    fn zstd_capture_decodes_and_limits_expansion() {
        let payload = vec![b'x'; 1024];
        let compressed = zstd::stream::encode_all(payload.as_slice(), 0).unwrap();
        assert_eq!(
            decode_captured_request_body_for_log(&compressed, "zstd"),
            payload
        );
        let limited = decode_captured_request_body_for_log_with_limit(&compressed, "zstd", 64);
        assert!(limited.len() < 128);
        assert!(limited
            .windows(DECOMPRESSED_TRUNCATION_MARKER.len())
            .any(|window| window == DECOMPRESSED_TRUNCATION_MARKER));
        assert_eq!(
            decode_captured_request_body_for_log(&compressed, "gzip"),
            compressed
        );
    }
}
