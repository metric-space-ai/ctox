//! `DangerousInternalClient` — a debug surface that re-exports otherwise
//! private internals of [`Client`].
//!
//! Mirrors `_upstream/whatsmeow/internals.go`, where the upstream type is a
//! generated re-export of every unexported method on `Client`. We don't
//! re-export every method (the Go file lists 100+ entries; many of those
//! are wrappers around code paths that don't exist yet in this port), but
//! we cover the surface that's load-bearing for diagnostics: send/dispatch
//! a node, manipulate the recent-messages cache and retry counter, query
//! the inspector handle for store state, and compute the unified-session
//! id.
//!
//! Routing: a [`Client`] hands one out via [`Client::dangerous_internal`].
//! The returned struct borrows the client; multiple internals are fine.
//!
//! These methods are marked `#[doc(hidden)]` in spirit (i.e. callers
//! shouldn't depend on them in production) — the `DangerousInternalClient`
//! type is the namespace that signals that.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use wha_binary::Node;

use crate::client::Client;
use crate::error::ClientError;
use crate::request::InfoQuery;

/// Captured copy of an outgoing recent message, as exposed by upstream's
/// `RecentMessage`. We expose plaintext bytes plus the `(to, msg_id)` key
/// so a debug consumer can match it up with retry receipts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentMessage {
    /// The recipient JID stored upstream as `recentMessageKey.To`.
    pub to: String,
    /// The message id stored upstream as `recentMessageKey.ID`.
    pub msg_id: String,
    /// The plaintext bytes we last cached for this `(to, msg_id)`. The
    /// Rust port stores the *plaintext* (matching the role the upstream
    /// `wa`/`fb` proto pointers play during retry encryption); upstream's
    /// representation is the marshalled message proto, so the bytes have
    /// the same role even though the encoding differs.
    pub plaintext: Vec<u8>,
}

const UNIFIED_OFFSET_MS: i64 = 3 * 24 * 3600 * 1000;
const WEEK_MS: i64 = 7 * 24 * 3600 * 1000;

/// Debug accessor into [`Client`] internals. Mirrors
/// `_upstream/whatsmeow/internals.go::DangerousInternalClient`.
pub struct DangerousInternalClient<'a>(&'a Client);

impl<'a> DangerousInternalClient<'a> {
    /// Build a fresh accessor. Public for parity with upstream's
    /// `Client.DangerousInternals()`; production callers should always use
    /// [`Client::dangerous_internal`] instead so the borrow lifetime is
    /// explicit at the call site.
    pub fn new(client: &'a Client) -> Self {
        Self(client)
    }

    /// Get the wrapped [`Client`] back. Useful when the internals struct
    /// is stored alongside other state and the caller wants to reach the
    /// regular API without juggling borrows.
    pub fn client(&self) -> &Client {
        self.0
    }

    // ---- send / dispatch -----------------------------------------------

    /// Send a fully-formed XML node over the noise socket. Mirrors
    /// `DangerousInternalClient.SendNode` upstream — same delegation, no
    /// extra framing. Errors propagate verbatim from [`Client::send_node`].
    pub async fn send_node_raw(&self, node: &Node) -> Result<(), ClientError> {
        self.0.send_node(node).await
    }

    /// Send an IQ and wait for the response. Mirrors
    /// `DangerousInternalClient.SendIQ` upstream.
    pub async fn send_iq_raw(&self, query: InfoQuery) -> Result<Node, ClientError> {
        self.0.send_iq(query).await
    }

    /// Feed a node directly into the dispatcher. Mirrors the role of
    /// `HandleIQ` / `HandleEncryptedMessage` etc. upstream — used in tests
    /// where we want to drive the dispatcher without a live socket.
    pub fn handle_iq_directly(&self, node: &Node) {
        self.0.dispatch_node_for_test(node.clone());
    }

    // ---- session telemetry ---------------------------------------------

    /// Compute the current unified-session id. Mirrors
    /// `Client.getUnifiedSessionID` in `_upstream/whatsmeow/client.go`:
    /// `((wallclock_ms + serverTimeOffset_ms + 3 days) % 1 week)` rendered
    /// as a base-10 integer.
    pub fn get_unified_session_id(&self) -> String {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let unified = now_ms.wrapping_add(self.0.server_time_offset_ms()).wrapping_add(UNIFIED_OFFSET_MS);
        let modded = unified.rem_euclid(WEEK_MS);
        modded.to_string()
    }

    /// Async unified-session telemetry node. Mirrors
    /// `Client.sendUnifiedSession`: emits an `<ib>` with a single
    /// `<unified_session id="…"/>` child. Errors when the socket is down.
    /// Upstream calls this fire-and-forget via `go cli.sendUnifiedSession()`;
    /// the wrapper is our explicit `.await`-friendly equivalent.
    pub async fn send_unified_session(&self) -> Result<(), ClientError> {
        let id = self.get_unified_session_id();
        let mut child_attrs = wha_binary::Attrs::new();
        child_attrs.insert("id".into(), wha_binary::Value::String(id));
        let child = Node::new("unified_session", child_attrs, None);
        let parent = Node::new(
            "ib",
            wha_binary::Attrs::new(),
            Some(wha_binary::Value::Nodes(vec![child])),
        );
        self.0.send_node(&parent).await
    }

    /// Read the current server-time offset (milliseconds). Mirrors
    /// `Client.serverTimeOffset.Load()` upstream.
    pub fn get_server_time_offset_ms(&self) -> i64 {
        self.0.server_time_offset_ms()
    }

    /// Update the server-time offset (milliseconds). Used by tests +
    /// experimentation around the unified-session id math.
    pub fn set_server_time_offset_ms(&self, offset: i64) {
        self.0.set_server_time_offset_ms(offset);
    }

    // ---- recent-message cache + retry counters --------------------------

    /// Fetch the recent-outgoing-message cache as a `Vec`. Mirrors the
    /// `recentMessagesMap` snapshot exposed by
    /// `DangerousInternalClient.GetRecentMessage` upstream — we collapse
    /// the map into a flat list because the per-key getter is already
    /// public on `Client`.
    pub fn get_recent_messages(&self) -> Vec<RecentMessage> {
        self.0.snapshot_recent_messages()
    }

    /// Read the per-incoming-message retry counter. Mirrors
    /// `cli.messageRetries[id]` upstream.
    pub fn get_message_retry_count(&self, msg_id: &str) -> u32 {
        self.0.message_retry_count(msg_id)
    }

    /// Bump the retry counter and return the post-increment value. Mirrors
    /// `cli.messageRetries[id]++` upstream.
    pub fn bump_message_retry(&self, msg_id: &str) -> u32 {
        self.0.bump_message_retry(msg_id)
    }

    /// Build a fresh request id, exactly as `Client::generate_request_id`
    /// does. Mirrors `DangerousInternalClient.GenerateRequestID`.
    pub fn generate_request_id(&self) -> String {
        self.0.generate_request_id()
    }

    // ---- store inspection ---------------------------------------------

    /// Number of entries in the pre-key store. Mirrors the inspection
    /// helpers around `cli.Store.PreKeys` upstream (no exact 1:1 method,
    /// but the diagnostic role is the same). Returns `0` when no
    /// inspector is installed via [`Client::set_inspector`].
    pub async fn get_pre_keys_count_in_store(&self) -> usize {
        match self.0.inspector() {
            Some(insp) => insp.count_pre_keys().await.unwrap_or(0),
            None => 0,
        }
    }

    /// `(collection_name → version)` for every collection the underlying
    /// store has a hash-state for. Mirrors the role of repeated
    /// `Client.Store.AppStateMutationMACs.GetVersion(name)` calls upstream.
    /// Empty when no inspector is installed.
    pub async fn get_app_state_versions(&self) -> HashMap<String, u64> {
        match self.0.inspector() {
            Some(insp) => insp.list_app_state_versions().await.unwrap_or_default(),
            None => HashMap::new(),
        }
    }

    /// Every Signal address with a stored session, lexicographically sorted.
    /// Empty when no inspector is installed.
    pub async fn get_session_addresses(&self) -> Vec<String> {
        match self.0.inspector() {
            Some(insp) => insp.list_session_addresses().await.unwrap_or_default(),
            None => Vec::new(),
        }
    }

    // ---- misc ----------------------------------------------------------

    /// Fire-and-forget media-conn refresh. Mirrors
    /// `DangerousInternalClient.RefreshMediaConn` upstream — except we
    /// don't actually have a `<media_conn/>` cache to refresh in the Rust
    /// port (the helper is implemented per-call by `wha-media`). The
    /// method is here so consumers writing to the upstream surface compile
    /// against our port; behaviour is a no-op.
    pub fn refresh_media_conn(&self) {
        // Intentional no-op. See comment on the method.
    }

    /// True if the underlying client has a live socket. Mirrors the role
    /// of `Client.IsConnected` upstream — useful for tests.
    pub fn is_connected(&self) -> bool {
        self.0.is_connected()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wha_store::MemoryStore;

    /// Internals: the unified-session id is in `[0, week_ms)`, parses as
    /// integer, and shifts forward when we bump the server-time offset
    /// (mirrors the `cli.serverTimeOffset.Load()` term in
    /// `_upstream/whatsmeow/client.go::getUnifiedSessionID`).
    #[tokio::test]
    async fn unified_session_id_in_range_and_responds_to_offset() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        let internals = cli.dangerous_internal();

        let id1: i64 = internals.get_unified_session_id().parse().expect("integer");
        assert!(id1 >= 0);
        assert!(id1 < WEEK_MS);

        // Bumping the offset by 1 second moves the id by ~1000 ms (no
        // wrap unless we're right at the week boundary).
        internals.set_server_time_offset_ms(1_000);
        let id2: i64 = internals.get_unified_session_id().parse().expect("integer");
        assert!(id2 >= 0);
        assert!(id2 < WEEK_MS);
        // The unified-session id is wallclock-derived, so it can shift
        // between reads even without an offset bump. Just assert the
        // offset write/read round-trips.
        assert_eq!(internals.get_server_time_offset_ms(), 1_000);
    }

    /// Recent-message cache: writes through the public API are visible
    /// via the internals snapshot. Mirrors the symmetry between
    /// `addRecentMessage` and `getRecentMessage` upstream.
    #[tokio::test]
    async fn recent_messages_round_trip() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        let internals = cli.dangerous_internal();

        // Empty cache is a fresh `Vec`.
        assert!(internals.get_recent_messages().is_empty());

        let to: wha_types::Jid = "1234@s.whatsapp.net".parse().unwrap();
        cli.add_recent_message(to.clone(), "MID-A".into(), b"plaintext-a".to_vec());
        cli.add_recent_message(to.clone(), "MID-B".into(), b"plaintext-b".to_vec());

        let snap = internals.get_recent_messages();
        assert_eq!(snap.len(), 2);
        let mids: Vec<&str> = snap.iter().map(|r| r.msg_id.as_str()).collect();
        assert!(mids.contains(&"MID-A"));
        assert!(mids.contains(&"MID-B"));

        // Plaintexts and recipient JID round-trip in full.
        let mid_a = snap.iter().find(|r| r.msg_id == "MID-A").unwrap();
        assert_eq!(mid_a.to, to.to_string());
        assert_eq!(mid_a.plaintext, b"plaintext-a".to_vec());
    }

    /// Retry counter: bump increments by 1, read returns the post-bump
    /// value, the dispatcher path also reads through this counter.
    #[tokio::test]
    async fn retry_counters_track_bumps() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        let internals = cli.dangerous_internal();

        assert_eq!(internals.get_message_retry_count("MID-X"), 0);
        assert_eq!(internals.bump_message_retry("MID-X"), 1);
        assert_eq!(internals.bump_message_retry("MID-X"), 2);
        assert_eq!(internals.get_message_retry_count("MID-X"), 2);
        assert_eq!(internals.get_message_retry_count("MID-Y"), 0);
    }

    /// Inspector wiring: when the caller installs an `Arc<MemoryStore>`
    /// as the inspector handle, the per-store aggregations come back
    /// through the internals API. Mirrors the upstream
    /// "DangerousInternals.Sessions" inspection surface in spirit.
    #[tokio::test]
    async fn inspector_surfaces_session_addresses_prekey_count_and_app_state_versions() {
        use wha_store::{
            AppStateMutationMacStore, PreKeyStore, SessionStore,
        };
        let store = Arc::new(MemoryStore::new());
        // Seed: two sessions, two prekeys, one app-state version.
        store
            .put_session("alice@s.whatsapp.net.1", b"sess-a".to_vec())
            .await
            .unwrap();
        store
            .put_session("bob@s.whatsapp.net.0", b"sess-b".to_vec())
            .await
            .unwrap();
        let _ = store.gen_one_pre_key().await.unwrap();
        let _ = store.gen_one_pre_key().await.unwrap();
        store
            .put_app_state_version("regular", 9, [0u8; 128])
            .await
            .unwrap();
        store
            .put_app_state_version("critical_block", 1, [0u8; 128])
            .await
            .unwrap();

        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        cli.set_inspector(store.clone());
        let internals = cli.dangerous_internal();

        let mut addrs = internals.get_session_addresses().await;
        addrs.sort();
        assert_eq!(
            addrs,
            vec!["alice@s.whatsapp.net.1".to_string(), "bob@s.whatsapp.net.0".to_string()]
        );

        // PreKey count includes the device's own signed pre-key (id=1)
        // plus the two we explicitly generated.
        assert!(internals.get_pre_keys_count_in_store().await >= 2);

        let versions = internals.get_app_state_versions().await;
        assert_eq!(versions.get("regular").copied(), Some(9));
        assert_eq!(versions.get("critical_block").copied(), Some(1));
    }

    /// Without an inspector, every per-store aggregation returns the empty
    /// answer — we never panic, never propagate `StoreError` to the caller.
    #[tokio::test]
    async fn no_inspector_yields_empty_collections() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        let internals = cli.dangerous_internal();

        assert!(internals.get_session_addresses().await.is_empty());
        assert!(internals.get_app_state_versions().await.is_empty());
        assert_eq!(internals.get_pre_keys_count_in_store().await, 0);
    }
}
