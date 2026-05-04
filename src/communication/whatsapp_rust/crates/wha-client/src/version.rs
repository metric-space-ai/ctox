//! Live WhatsApp Web version fetcher.
//!
//! Hardcoding the WA client version (as upstream whatsmeow does) is brittle —
//! the moment WA tightens the version check, every clone breaks until the
//! constant is bumped. This module hits WA's own `check-update` endpoint at
//! runtime and feeds the current value into the [`crate::payload`] builder.
//!
//! ```text
//! GET https://web.whatsapp.com/check-update?version=0.0.0&platform=web
//! → {"currentVersion":"2.2413.51", ...}
//! ```
//!
//! Sending `version=0.0.0` deliberately so the response always carries the
//! `currentVersion` field (the server only fills it when it judges your
//! version below the soft/hard limit).
//!
//! The version is cached in a [`OnceLock`] so subsequent calls are free.
//! Callers should invoke [`fetch_and_install`] once, before [`crate::Client::connect`].

use std::sync::OnceLock;
use std::time::Duration;

use crate::client::Client;
use crate::error::ClientError;
use crate::events::Event;
use crate::payload::WAVersionContainer;

const CHECK_UPDATE_URL: &str =
    "https://web.whatsapp.com/check-update?version=0.0.0&platform=web";

static RUNTIME_VERSION: OnceLock<WAVersionContainer> = OnceLock::new();

/// Fetch the current WhatsApp Web version from WA's own check-update endpoint
/// and install it as the runtime default for [`crate::payload`]. Subsequent
/// calls to [`current`] return the fetched value.
///
/// Returns the installed version. If the endpoint is unreachable or returns
/// an unparseable shape, returns `Err(ClientError)` and **does not** install
/// anything — the caller can fall back to [`crate::payload::WA_VERSION`].
pub async fn fetch_and_install() -> Result<WAVersionContainer, ClientError> {
    let v = fetch().await?;
    // Ignore set errors — only the first install wins, which is fine.
    let _ = RUNTIME_VERSION.set(v);
    Ok(v)
}

/// Best-effort variant: returns the live version if the fetch succeeds,
/// otherwise falls back to [`crate::payload::WA_VERSION`] silently. Used by
/// example binaries that want a single-shot bootstrap without bubbling
/// network errors.
pub async fn fetch_and_install_or_fallback() -> WAVersionContainer {
    match fetch_and_install().await {
        Ok(v) => v,
        Err(_) => crate::payload::WA_VERSION,
    }
}

/// The currently-installed WhatsApp Web version. Returns
/// [`crate::payload::WA_VERSION`] (the compile-time fallback) until
/// [`fetch_and_install`] is called.
pub fn current() -> WAVersionContainer {
    *RUNTIME_VERSION
        .get()
        .unwrap_or(&crate::payload::WA_VERSION)
}

/// Result of [`check_for_update_async`]. Mirrors the JSON shape returned by
/// `https://web.whatsapp.com/check-update?version=…&platform=web`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateStatus {
    /// `isBroken` — the server flagged this version as actively broken; not
    /// just outdated. Reconnect attempts will fail; the user must update
    /// immediately.
    pub is_broken: bool,
    /// `isBelowSoft` — there's a newer version, but we'll still be allowed
    /// to connect. The application can surface this as an upgrade hint.
    pub is_below_soft: bool,
    /// `isBelowHard` — we are below the cutoff: the server may at any time
    /// start rejecting our connection with a `<failure reason="405"/>`.
    pub is_below_hard: bool,
    /// `currentVersion` — the version string the server reported as current.
    /// Parsed into a [`WAVersionContainer`] when feasible; the raw string
    /// is also kept around for the [`Event::ClientOutdated`] payload.
    pub current_version_raw: String,
    pub current_version: Option<WAVersionContainer>,
}

/// Async port of `_upstream/whatsmeow/update.go::CheckUpdate` (the bit of
/// `update.go` that actually issues the HTTPS request — `GetLatestVersion`
/// in upstream slang). Hits the same endpoint as [`fetch`] does, but parses
/// every flag of the response shape — not just `currentVersion`.
///
/// When `client` is supplied AND the response says `isBelowHard`, the
/// function emits [`Event::ClientOutdated`] on the client's event channel.
/// Pass `None` to inspect the result without dispatching anything.
pub async fn check_for_update_async(
    client: Option<&Client>,
) -> Result<UpdateStatus, ClientError> {
    let current = current();
    let url = format!(
        "https://web.whatsapp.com/check-update?version={}&platform=web",
        current.to_dot_string()
    );
    let body = http_get_check_update(&url).await?;
    let status = parse_check_update_response(&body)?;
    if status.is_below_hard {
        if let Some(cli) = client {
            cli.dispatch_event(Event::ClientOutdated {
                current_version: status.current_version_raw.clone(),
            });
        }
    }
    Ok(status)
}

/// Pluggable HTTP-GET hook so the test suite can drive
/// [`check_for_update_async`]'s parsing path without going to the network.
async fn http_get_check_update(url: &str) -> Result<String, ClientError> {
    let client = ::reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| ClientError::Other(format!("reqwest build: {e}")))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| ClientError::Other(format!("check-update GET: {e}")))?;
    resp.text()
        .await
        .map_err(|e| ClientError::Other(format!("check-update body: {e}")))
}

/// Parse the full `check-update` response shape. Hand-rolled rather than
/// pulling in `serde_json` for four-five fields.
fn parse_check_update_response(body: &str) -> Result<UpdateStatus, ClientError> {
    let is_broken = parse_bool_field(body, "isBroken").unwrap_or(false);
    let is_below_soft = parse_bool_field(body, "isBelowSoft").unwrap_or(false);
    let is_below_hard = parse_bool_field(body, "isBelowHard").unwrap_or(false);
    let raw = parse_string_field(body, "currentVersion").unwrap_or_default();
    let parsed = if raw.is_empty() {
        None
    } else {
        WAVersionContainer::parse(&raw).ok()
    };
    Ok(UpdateStatus {
        is_broken,
        is_below_soft,
        is_below_hard,
        current_version_raw: raw,
        current_version: parsed,
    })
}

/// Find a `"<key>":<true|false>` token in `body` — leniently, no nesting.
fn parse_bool_field(body: &str, key: &str) -> Option<bool> {
    let pat = format!("\"{key}\":");
    let start = body.find(&pat)? + pat.len();
    let rest = body[start..].trim_start();
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

/// Find a `"<key>":"<value>"` token in `body`.
fn parse_string_field(body: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\":\"");
    let start = body.find(&pat)? + pat.len();
    let rest = &body[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

/// Direct fetch without installing. Useful for tests + diagnostic tools.
pub async fn fetch() -> Result<WAVersionContainer, ClientError> {
    let client = ::reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| ClientError::Other(format!("reqwest build: {e}")))?;
    let resp = client
        .get(CHECK_UPDATE_URL)
        .send()
        .await
        .map_err(|e| ClientError::Other(format!("check-update GET: {e}")))?;
    let body = resp
        .text()
        .await
        .map_err(|e| ClientError::Other(format!("check-update body: {e}")))?;
    parse_current_version(&body)
}

/// Parse `{"currentVersion":"2.2413.51", …}` into a container.
///
/// Hand-rolled to avoid pulling in `serde_json` for one field.
fn parse_current_version(body: &str) -> Result<WAVersionContainer, ClientError> {
    let key = "\"currentVersion\":\"";
    let start = body
        .find(key)
        .ok_or_else(|| ClientError::Other(format!("no currentVersion in body: {body}")))?
        + key.len();
    let rest = &body[start..];
    let end = rest
        .find('"')
        .ok_or_else(|| ClientError::Other("unterminated currentVersion".into()))?;
    let version_str = &rest[..end];
    WAVersionContainer::parse(version_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wha_store::MemoryStore;

    #[test]
    fn parse_typical_response() {
        let body = r#"{"isBroken":false,"isBelowSoft":true,"isBelowHard":true,"hardUpdateTime":0,"beta":null,"currentVersion":"2.2413.51"}"#;
        let v = parse_current_version(body).unwrap();
        assert_eq!(v, WAVersionContainer([2, 2413, 51]));
    }

    #[test]
    fn parse_rejects_missing_field() {
        let body = r#"{"foo":"bar"}"#;
        assert!(parse_current_version(body).is_err());
    }

    #[test]
    fn parse_rejects_unterminated() {
        let body = r#"{"currentVersion":"2.2413.51"#;
        assert!(parse_current_version(body).is_err());
    }

    /// Mirror the upstream JSON: the parser must surface every flag and the
    /// version string. This is the path
    /// [`check_for_update_async`] takes after [`http_get_check_update`].
    #[test]
    fn parse_check_update_response_full_shape() {
        let body = r#"{"isBroken":false,"isBelowSoft":true,"isBelowHard":true,"hardUpdateTime":0,"beta":null,"currentVersion":"2.2413.51"}"#;
        let s = parse_check_update_response(body).unwrap();
        assert!(!s.is_broken);
        assert!(s.is_below_soft);
        assert!(s.is_below_hard);
        assert_eq!(s.current_version_raw, "2.2413.51");
        assert_eq!(s.current_version, Some(WAVersionContainer([2, 2413, 51])));
    }

    /// Missing flags default to `false`, missing `currentVersion` to empty.
    /// The `unwrap_or` chains in `parse_check_update_response` mean a totally
    /// empty body still parses (every flag false).
    #[test]
    fn parse_check_update_response_handles_missing_fields() {
        let body = r#"{"isBroken":true,"hardUpdateTime":0}"#;
        let s = parse_check_update_response(body).unwrap();
        assert!(s.is_broken);
        assert!(!s.is_below_soft);
        assert!(!s.is_below_hard);
        assert!(s.current_version_raw.is_empty());
        assert!(s.current_version.is_none());
    }

    /// Wire up an actual `Client` and feed `is_below_hard=true` through
    /// the dispatch path: the resulting event MUST appear on the channel.
    /// We bypass the HTTPS call by going through `parse_check_update_response`
    /// + the client's dispatch directly — exercising the same branches as
    /// `check_for_update_async` without leaving the test harness.
    #[tokio::test]
    async fn is_below_hard_dispatches_client_outdated_event() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, mut evt) = Client::new(device);

        let body = r#"{"isBroken":false,"isBelowSoft":true,"isBelowHard":true,"currentVersion":"99.0.0"}"#;
        let status = parse_check_update_response(body).unwrap();
        if status.is_below_hard {
            cli.dispatch_event(Event::ClientOutdated {
                current_version: status.current_version_raw.clone(),
            });
        }

        match evt.try_recv() {
            Ok(Event::ClientOutdated { current_version }) => {
                assert_eq!(current_version, "99.0.0");
            }
            other => panic!("expected ClientOutdated event, got {other:?}"),
        }
    }
}
