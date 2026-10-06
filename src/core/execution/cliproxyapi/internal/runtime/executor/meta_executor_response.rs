// ref: internal/runtime/executor/meta_executor_execute.go:153-242,301-341; meta_executor.go:335-386 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// ref: internal/runtime/executor/codex_executor_terminal.go:49-140; xai_executor_response.go:1030-1041 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: ported — request-owned output snapshots and typed status evidence
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::translator::common::set_raw_path;
use crate::sdk::cliproxy::auth::AuthError;
use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    time::{Duration, SystemTime},
};
use zeroize::Zeroizing;

pub const META_INCOMPLETE_STREAM_MESSAGE: &str =
    "meta stream error: stream disconnected before response.completed or response.incomplete";
pub const META_NOT_FOUND_COOLDOWN: Duration = Duration::from_secs(5 * 60);

/// Never reserialize provider items or usage just to reconstruct terminal output.
#[derive(Default)]
pub struct MetaOutputItems {
    indexed: BTreeMap<i64, Vec<u8>>,
    fallback: Vec<Vec<u8>>,
}
impl fmt::Debug for MetaOutputItems {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaOutputItems")
            .field("indexed_count", &self.indexed.len())
            .field("fallback_count", &self.fallback.len())
            .finish()
    }
}
impl MetaOutputItems {
    pub fn collect(&mut self, event: &[u8]) {
        let Ok(document) = std::str::from_utf8(event) else {
            return;
        };
        let item = gjson::get(document, "item");
        if !matches!(item.kind(), gjson::Kind::Object | gjson::Kind::Array) {
            return;
        }
        let index = gjson::get(document, "output_index");
        if index.exists() {
            self.indexed
                .insert(index.i64(), item.json().as_bytes().to_vec());
        } else {
            self.fallback.push(item.json().as_bytes().to_vec());
        }
    }
    pub fn patch_completed(&self, event: &[u8]) -> Vec<u8> {
        let Ok(document) = std::str::from_utf8(event) else {
            return event.to_vec();
        };
        let output = gjson::get(document, "response.output");
        if output.kind() == gjson::Kind::Array && !output.array().is_empty() {
            let mut patched = event.to_vec();
            for (index, item) in output.array().iter().enumerate() {
                let id = item.get("id");
                let has_id = id.exists()
                    && id.kind() != gjson::Kind::Null
                    && !(id.kind() == gjson::Kind::String && id.str().trim().is_empty());
                if has_id {
                    continue;
                }
                let Some(completed) = i64::try_from(index).ok().and_then(|i| self.indexed.get(&i))
                else {
                    continue;
                };
                let Ok(completed) = std::str::from_utf8(completed) else {
                    continue;
                };
                let id = gjson::get(completed, "id");
                if id.kind() == gjson::Kind::String && !id.str().trim().is_empty() {
                    patched = set_raw_path(
                        &patched,
                        &format!("response.output.{index}.id"),
                        id.json().as_bytes(),
                    );
                }
            }
            return patched;
        }
        if self.indexed.is_empty() && self.fallback.is_empty() {
            return event.to_vec();
        }
        let mut array = vec![b'['];
        for item in self.indexed.values().chain(self.fallback.iter()) {
            if array.len() > 1 {
                array.push(b',');
            }
            array.extend_from_slice(item);
        }
        array.push(b']');
        set_raw_path(event, "response.output", &array)
    }
}

/// Unary Meta replies may be SSE terminal events or a plain Responses object.
/// Arbitrary JSON and error objects cannot become a fabricated success.
pub fn meta_as_completed_event(data: &[u8]) -> Option<Vec<u8>> {
    let document = std::str::from_utf8(data).ok()?.trim();
    if !gjson::valid(document) {
        return None;
    }
    let root = gjson::parse(document);
    if matches!(
        root.get("type").str(),
        "response.completed" | "response.incomplete"
    ) {
        return Some(document.as_bytes().to_vec());
    }
    if root.get("object").str() == "response" || root.get("output").exists() {
        return Some(set_raw_path(
            br#"{"type":"response.completed"}"#,
            "response",
            document.as_bytes(),
        ));
    }
    None
}

/// Preserve status for the generic conductor through the existing AuthError
/// source chain. Retry/reset and account scope remain explicit typed evidence.
pub struct MetaHttpStatusError {
    pub retry_after: Option<Duration>,
    pub credential_scoped: bool,
    auth: AuthError,
}
impl MetaHttpStatusError {
    pub fn status_code(&self) -> u16 {
        self.auth.http_status
    }
}
impl fmt::Debug for MetaHttpStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaHttpStatusError")
            .field("status", &self.status_code())
            .field("retry_after", &self.retry_after)
            .field("credential_scoped", &self.credential_scoped)
            .field("body_bytes", &self.auth.message.len())
            .finish()
    }
}
impl fmt::Display for MetaHttpStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.auth, f)
    }
}
impl Error for MetaHttpStatusError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.auth)
    }
}
pub fn parse_meta_retry_after(status: u16, body: &[u8], now: SystemTime) -> Option<Duration> {
    if !matches!(status, 429 | 404) {
        return None;
    }
    let document = std::str::from_utf8(body).ok()?;
    let reset = gjson::get(document, "error.resets_at").i64();
    if reset <= 0 {
        return None;
    }
    let deadline = SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(reset as u64))?;
    deadline.duration_since(now).ok().filter(|d| !d.is_zero())
}
pub fn is_meta_subscription_quota(status: u16, body: &[u8]) -> bool {
    if status != 429 {
        return false;
    }
    let Ok(document) = std::str::from_utf8(body) else {
        return false;
    };
    let message = gjson::get(document, "error.message")
        .str()
        .to_ascii_lowercase();
    let code = gjson::get(document, "error.code")
        .str()
        .to_ascii_lowercase();
    message.contains("subscription quota")
        || message.contains("quota exhausted")
        || ((code == "rate_limit_exceeded" || code.contains("quota"))
            && gjson::get(document, "error.resets_at").exists())
}
pub fn meta_upstream_error(status: u16, body: &[u8], now: SystemTime) -> MetaHttpStatusError {
    MetaHttpStatusError {
        retry_after: parse_meta_retry_after(status, body, now)
            .or_else(|| (status == 404).then_some(META_NOT_FOUND_COOLDOWN)),
        credential_scoped: is_meta_subscription_quota(status, body),
        auth: AuthError {
            code: String::new(),
            message: String::from_utf8_lossy(body).into_owned(),
            http_status: status,
            retryable: status == 429 || status >= 500,
        },
    }
}
pub fn meta_stream_event_error(data: &[u8], now: SystemTime) -> Option<MetaHttpStatusError> {
    let document = std::str::from_utf8(data).ok()?;
    if !matches!(
        gjson::get(document, "type").str(),
        "error" | "response.failed"
    ) {
        return None;
    }
    let code = gjson::get(document, "error.code").i64();
    let status = if (400..=599).contains(&code) {
        code as u16
    } else {
        502
    };
    Some(meta_upstream_error(status, data, now))
}

/// Framing follows upstream's bounded line scanner. Ownership stays with the
/// active attempt; dropping it releases unconsumed upstream bytes.
#[derive(Default)]
pub struct MetaResponseLines {
    buffer: Zeroizing<Vec<u8>>,
}
pub const META_MAX_STREAM_LINE_BYTES: usize = 52_428_800;
impl MetaResponseLines {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, AuthError> {
        let mut lines = Vec::new();
        for part in bytes.split_inclusive(|byte| *byte == b'\n') {
            if self.buffer.len().saturating_add(part.len()) > META_MAX_STREAM_LINE_BYTES {
                return Err(AuthError {
                    code: "meta_stream_line_limit".into(),
                    message: "Meta response line exceeds limit".into(),
                    http_status: 502,
                    retryable: false,
                });
            }
            self.buffer.extend_from_slice(part);
            if part.last() == Some(&b'\n') {
                let mut line = std::mem::take(&mut *self.buffer);
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                lines.push(line);
            }
        }
        Ok(lines)
    }
    pub fn finish(&mut self) -> Option<Vec<u8>> {
        if self.buffer.is_empty() {
            None
        } else {
            let mut line = std::mem::take(&mut *self.buffer);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            Some(line)
        }
    }
}

#[cfg(test)]
#[path = "meta_executor_response_test.rs"]
mod tests;
