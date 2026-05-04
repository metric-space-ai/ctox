//! Trusted-contact (`tc`) token IQs — port of
//! `_upstream/whatsmeow/tctoken.go`.
//!
//! ## Caveat: wire-shape divergence from upstream
//!
//! The upstream `tctoken.go` has no public `RefreshTCToken` / `PutTCToken`
//! method. The only IQ the file actually constructs is the internal
//! `issuePrivacyToken(...)`, which sends:
//!
//! ```text
//! <iq xmlns="privacy" type="set" to="s.whatsapp.net">
//!     <tokens>
//!         <token jid="<peer>" t="<unix_ts>" type="trusted_contact"/>
//!     </tokens>
//! </iq>
//! ```
//!
//! …i.e. it issues a tc-style privacy token *for a specific peer JID*, and
//! the `tctoken.go` machinery on top is bucketing + cache invalidation
//! around the `PrivacyTokens` store rather than a single per-device token.
//! There is no `<iq xmlns="tc"><tc/></iq>` exchange in the upstream codebase.
//!
//! This Rust module follows the task brief — which asked for the
//! simplified `xmlns="tc"` wire shape and a `tc_token: Option<Vec<u8>>` slot
//! on [`wha_store::Device`] — but documents the divergence so a future
//! interop pass against the live server can decide whether to keep the
//! simplified shape, port the full `privacy/tokens` flow, or both. Tests
//! pin the IQ shape exactly as the task spec asked for.
//!
//! Bucket math (`tcTokenBucketDuration`, `tcTokenNumBuckets`) is ported
//! verbatim — those constants are the same regardless of which wire shape
//! the IQ ends up taking, so callers can already use
//! [`is_tc_token_expired`] and [`should_send_new_tc_token`] against any
//! token they obtain.

use wha_binary::{Attrs, Node, Value};
use wha_types::Jid;

use crate::client::Client;
use crate::error::ClientError;
use crate::request::{InfoQuery, IqType};

// ---------------------------------------------------------------------------
// Bucket constants — direct port of upstream.
// ---------------------------------------------------------------------------

/// Duration of a single bucket in seconds (7 days). Matches the AB prop
/// `tctoken_duration` upstream.
pub const TC_TOKEN_BUCKET_DURATION: i64 = 604_800;

/// Number of rolling buckets (4 = ~28-day window). Matches the AB prop
/// `tctoken_num_buckets` upstream.
pub const TC_TOKEN_NUM_BUCKETS: i64 = 4;

/// Compute the cutoff timestamp for the rolling bucket window. Mirrors
/// `currentTCTokenCutoffTimestamp` upstream.
pub fn current_tc_token_cutoff_timestamp(now_unix: i64) -> i64 {
    let current_bucket = now_unix / TC_TOKEN_BUCKET_DURATION;
    let cutoff_bucket = current_bucket - (TC_TOKEN_NUM_BUCKETS - 1);
    cutoff_bucket * TC_TOKEN_BUCKET_DURATION
}

/// Whether a token issued at `ts` is now expired. Mirrors `isTCTokenExpired`
/// upstream — `ts == 0` is treated as expired (zero / never-issued).
pub fn is_tc_token_expired(ts: i64, now_unix: i64) -> bool {
    if ts == 0 {
        return true;
    }
    ts < current_tc_token_cutoff_timestamp(now_unix)
}

/// Whether a new token should be issued — i.e. whether the current bucket
/// is newer than the bucket of `sender_ts`. Mirrors `shouldSendNewTCToken`.
pub fn should_send_new_tc_token(sender_ts: i64, now_unix: i64) -> bool {
    if sender_ts == 0 {
        return true;
    }
    now_unix / TC_TOKEN_BUCKET_DURATION > sender_ts / TC_TOKEN_BUCKET_DURATION
}

// ---------------------------------------------------------------------------
// IQ builders.
// ---------------------------------------------------------------------------

fn server_jid() -> Jid {
    Jid::new("", wha_types::jid::server::DEFAULT_USER)
}

/// Build the fetch IQ — `<iq xmlns="tc" type="get" to="s.whatsapp.net"><tc/></iq>`.
///
/// Wire shape per task spec; see module-level caveat about upstream
/// divergence.
pub fn build_fetch_tc_token_iq(ts: i64) -> InfoQuery {
    let mut tc_attrs = Attrs::new();
    if ts > 0 {
        tc_attrs.insert("t".into(), Value::String(ts.to_string()));
    }
    let tc = Node::new("tc", tc_attrs, None);
    InfoQuery::new("tc", IqType::Get)
        .to(server_jid())
        .content(Value::Nodes(vec![tc]))
}

/// Build the put IQ — `<iq xmlns="tc" type="set" to="s.whatsapp.net"><tc>token-bytes</tc></iq>`.
pub fn build_put_tc_token_iq(token: &[u8]) -> InfoQuery {
    let tc = Node::new("tc", Attrs::new(), Some(Value::Bytes(token.to_vec())));
    InfoQuery::new("tc", IqType::Set)
        .to(server_jid())
        .content(Value::Nodes(vec![tc]))
}

// ---------------------------------------------------------------------------
// Public client API.
// ---------------------------------------------------------------------------

/// Fetch the device's tc-token from the server. The token is the byte
/// content of the `<tc>` child of the response — empty `Vec` if the server
/// reply contained no body.
pub async fn fetch_tc_token(client: &Client, ts: i64) -> Result<Vec<u8>, ClientError> {
    let resp = client.send_iq(build_fetch_tc_token_iq(ts)).await?;
    let tc = resp
        .children()
        .iter()
        .find(|c| c.tag == "tc")
        .ok_or_else(|| ClientError::Malformed("tc IQ response missing <tc> child".into()))?;
    Ok(tc
        .content
        .as_bytes()
        .map(|b| b.to_vec())
        .unwrap_or_default())
}

/// Upload a freshly-minted tc-token to the server. The IQ shape is
/// `<iq xmlns="tc" type="set"><tc>...token...</tc></iq>` — see module-level
/// caveat.
pub async fn put_tc_token(client: &Client, token: &[u8]) -> Result<(), ClientError> {
    let _ = client.send_iq(build_put_tc_token_iq(token)).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_iq_has_expected_shape() {
        let iq = build_fetch_tc_token_iq(0);
        let node = iq.into_node("REQ-A".into());
        assert_eq!(node.tag, "iq");
        assert_eq!(node.get_attr_str("xmlns"), Some("tc"));
        assert_eq!(node.get_attr_str("type"), Some("get"));
        let to = node.get_attr_jid("to").expect("to");
        assert_eq!(to.server, wha_types::jid::server::DEFAULT_USER);
        let kids = node.children();
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].tag, "tc");
        // ts == 0 → no `t` attribute.
        assert!(kids[0].get_attr_str("t").is_none());
    }

    #[test]
    fn fetch_iq_with_ts_attaches_t_attr() {
        let iq = build_fetch_tc_token_iq(1_700_000_000);
        let node = iq.into_node("REQ-B".into());
        let tc = &node.children()[0];
        assert_eq!(tc.get_attr_str("t"), Some("1700000000"));
    }

    #[test]
    fn put_iq_carries_token_bytes() {
        let token = b"\xCA\xFE\xBA\xBE";
        let iq = build_put_tc_token_iq(token);
        let node = iq.into_node("REQ-C".into());
        assert_eq!(node.get_attr_str("type"), Some("set"));
        let tc = &node.children()[0];
        assert_eq!(tc.tag, "tc");
        assert_eq!(tc.content.as_bytes(), Some(token.as_ref()));
    }

    #[test]
    fn bucket_constants_match_upstream() {
        assert_eq!(TC_TOKEN_BUCKET_DURATION, 604_800);
        assert_eq!(TC_TOKEN_NUM_BUCKETS, 4);
    }

    #[test]
    fn cutoff_timestamp_is_three_buckets_back() {
        // Pick a "now" exactly on a bucket boundary to make the math obvious.
        let now = 100 * TC_TOKEN_BUCKET_DURATION;
        let cutoff = current_tc_token_cutoff_timestamp(now);
        // 100 - 3 buckets back × bucket duration.
        assert_eq!(cutoff, (100 - (TC_TOKEN_NUM_BUCKETS - 1)) * TC_TOKEN_BUCKET_DURATION);
    }

    #[test]
    fn is_expired_handles_zero_and_old_ts() {
        let now = 100 * TC_TOKEN_BUCKET_DURATION;
        assert!(is_tc_token_expired(0, now), "zero timestamp must be expired");
        // Older than cutoff → expired.
        let cutoff = current_tc_token_cutoff_timestamp(now);
        assert!(is_tc_token_expired(cutoff - 1, now));
        // Inside the rolling window → not expired.
        assert!(!is_tc_token_expired(cutoff + 1, now));
        // Exactly at cutoff is not "before" cutoff → not expired (matches upstream's `Before`).
        assert!(!is_tc_token_expired(cutoff, now));
    }

    #[test]
    fn should_send_new_when_bucket_advances() {
        let now = 100 * TC_TOKEN_BUCKET_DURATION;
        // Same bucket → no new token needed.
        assert!(!should_send_new_tc_token(now, now));
        // Previous bucket → new token needed.
        assert!(should_send_new_tc_token(now - TC_TOKEN_BUCKET_DURATION, now));
        // Never issued → new token needed.
        assert!(should_send_new_tc_token(0, now));
    }
}
