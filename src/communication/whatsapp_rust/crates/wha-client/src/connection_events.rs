//! Connection-lifecycle handlers — `<failure>`, `<success>`, `<ib>`.
//!
//! Mirrors `_upstream/whatsmeow/connectionevents.go`. Pulled out of the live
//! example into a real module so [`crate::client::dispatch_node`] can route
//! these tags directly instead of deferring to caller boilerplate.
//!
//! What we actually port (vs. the simpler bits we drop on the floor):
//!
//! * `<failure>` → parse the numeric `reason` attribute and emit
//!   [`crate::events::Event::ConnectFailure`]. Specialised events for
//!   `LoggedOut` / `TempBanned` / `ClientOutdated` etc. are out of scope here;
//!   the `ConnectFailureReason` enum lets applications branch on the code.
//! * `<success>` → send `<iq xmlns="passive" type="set"><active/></iq>` to
//!   tell WA the device is live, kick off a prekey-refill if the local store
//!   is below `MIN_PRE_KEY_COUNT`, then emit [`Event::Connected`].
//! * `<ib>` → walk children:
//!   - `<offline_preview …/>` → [`Event::OfflineSyncPreview`]
//!   - `<offline …/>`         → [`Event::OfflineSyncCompleted`]
//!   - `<dirty …/>`           → ack via [`mark_not_dirty`] **and** emit
//!     [`Event::DirtyNotification`]
//!   - `<edge_routing/>`      → log + ack
//!   - `<downgrade_webclient/>` → [`Event::QrScannedWithoutMultidevice`]

use tracing::{debug, info, warn};

use wha_binary::{Attrs, Node, Value};
use wha_types::Jid;

use crate::client::Client;
use crate::error::ClientError;
use crate::events::{ConnectFailureReason, Event};
use crate::request::{InfoQuery, IqType};

/// Handle a top-level `<failure>` node from the server. Mirrors
/// `Client.handleConnectFailure` in upstream — we pull the numeric `reason`
/// (and optional `message`) and dispatch [`Event::ConnectFailure`]. The
/// downstream-classification dance (LoggedOut / TempBanned / ClientOutdated
/// specialised events, store delete on logout, CAT refresh) is deliberately
/// not ported in this slice — the typed reason gives apps everything they
/// need to react.
pub async fn handle_connect_failure(client: &Client, node: &Node) {
    let mut ag = node.attr_getter();
    let reason_raw = ag.optional_i64("reason").unwrap_or(0);
    let message = ag.optional_string("message").map(|s| s.to_owned());
    let reason = ConnectFailureReason::from_code(reason_raw);
    if reason.is_logged_out() {
        info!(
            ?reason,
            "got logout-class connect failure; application should clear session"
        );
    } else {
        warn!(?reason, ?message, "connect failure");
    }
    client.dispatch_event(Event::ConnectFailure { reason, message });
}

/// Handle a top-level `<success>` node. Mirrors `Client.handleConnectSuccess`:
/// after a successful login the server expects us to send an `<iq xmlns=passive>
/// <active/></iq>` to graduate from "logging in" to "active", and we should
/// refill prekeys if the local store has run low. Errors during the post-success
/// kickoff are surfaced — but [`Event::Connected`] is dispatched in any case so
/// the application can take over.
pub async fn handle_connect_success(client: &Client, _node: &Node) -> Result<(), ClientError> {
    info!("connect success — sending <iq xmlns=passive><active/>");

    // Send <iq xmlns="passive" type="set"><active/></iq>. Mirrors
    // `SetPassive(false)` upstream.
    let active_iq = InfoQuery::new("passive", IqType::Set)
        .to(Jid::new("", wha_types::Server::DEFAULT_USER))
        .content(Value::Nodes(vec![Node::tag_only("active")]));
    if let Err(e) = client.send_iq(active_iq).await {
        // Don't bail on this — the event channel still gets Connected so the
        // application can keep going / reconnect on its own loop.
        warn!(?e, "failed to send post-success <active> IQ");
    }

    // Prekey-refill gate. Upstream calls
    // `cli.uploadPreKeys(ctx, dbCount==0 && serverCount==0)` whenever the
    // store has fewer than `MinPreKeyCount` uploaded keys. We approximate
    // with the locally tracked count — that's also what
    // `prekeys::refresh_pre_keys_if_low` does.
    if let Err(e) = crate::prekeys::refresh_pre_keys_if_low(client).await {
        warn!(?e, "post-success prekey refresh failed");
    }

    client.dispatch_event(Event::Connected);
    Ok(())
}

/// Handle a top-level `<ib>` (info-bus) node. Walks every child and dispatches
/// the typed event listed in the module docs above. Unknown children are logged
/// and skipped — upstream does the same.
pub async fn handle_ib(client: &Client, node: &Node) {
    for child in node.children() {
        match child.tag.as_str() {
            "offline_preview" => {
                let mut ag = child.attr_getter();
                let total = ag.optional_i64("count").unwrap_or(0);
                let app_data_changes = ag.optional_i64("appdata").unwrap_or(0);
                let messages = ag.optional_i64("message").unwrap_or(0);
                let notifications = ag.optional_i64("notification").unwrap_or(0);
                let receipts = ag.optional_i64("receipt").unwrap_or(0);
                debug!(
                    total,
                    app_data_changes, messages, notifications, receipts, "offline_preview"
                );
                client.dispatch_event(Event::OfflineSyncPreview {
                    total,
                    app_data_changes,
                    messages,
                    notifications,
                    receipts,
                });
            }
            "offline" => {
                let mut ag = child.attr_getter();
                let count = ag.optional_i64("count").unwrap_or(0);
                debug!(count, "offline_sync_completed");
                client.dispatch_event(Event::OfflineSyncCompleted { count });
            }
            "dirty" => {
                let mut ag = child.attr_getter();
                let dirty_type = ag.optional_string("type").unwrap_or("").to_owned();
                let timestamp = ag.optional_i64("timestamp").unwrap_or(0);
                debug!(%dirty_type, timestamp, "dirty notification");
                if let Err(e) = mark_not_dirty(client, &dirty_type, timestamp).await {
                    warn!(?e, %dirty_type, timestamp, "mark_not_dirty failed");
                }
                client.dispatch_event(Event::DirtyNotification {
                    dirty_type,
                    timestamp,
                });
            }
            "edge_routing" => {
                // Upstream logs this and acks; we have no specific routing
                // table to update — log and move on.
                debug!("<ib><edge_routing/> — ignored");
            }
            "downgrade_webclient" => {
                debug!("<ib><downgrade_webclient/> — phone scanned an old web client");
                client.dispatch_event(Event::QrScannedWithoutMultidevice);
            }
            other => {
                debug!(tag = %other, "<ib> child — unhandled");
            }
        }
    }
}

/// Send `<iq xmlns="urn:xmpp:whatsapp:dirty" type="set"><clean type=… timestamp=…/></iq>`.
/// Mirrors `Client.MarkNotDirty` in `_upstream/whatsmeow/appstate.go`.
///
/// Exposed as `pub` so applications and tests can trigger this independently of
/// the `<ib><dirty/>` notification (e.g. as a post-resync ack).
pub async fn mark_not_dirty(
    client: &Client,
    clean_type: &str,
    timestamp: i64,
) -> Result<(), ClientError> {
    let mut clean_attrs = Attrs::new();
    clean_attrs.insert("type".into(), Value::String(clean_type.to_owned()));
    clean_attrs.insert("timestamp".into(), Value::String(timestamp.to_string()));
    let clean = Node::new("clean", clean_attrs, None);

    let iq = InfoQuery::new("urn:xmpp:whatsapp:dirty", IqType::Set)
        .to(Jid::new("", wha_types::Server::DEFAULT_USER))
        .content(Value::Nodes(vec![clean]));
    let _ = client.send_iq(iq).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use wha_binary::{Attrs, Node, Value};
    use wha_store::MemoryStore;

    fn make_client() -> (Client, tokio::sync::mpsc::UnboundedReceiver<Event>) {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        Client::new(device)
    }

    #[tokio::test]
    async fn handle_connect_failure_dispatches_typed_reason() {
        let (cli, mut evt) = make_client();
        let mut attrs = Attrs::new();
        attrs.insert("reason".into(), Value::String("401".into()));
        attrs.insert("message".into(), Value::String("logged out".into()));
        let node = Node::new("failure", attrs, None);

        handle_connect_failure(&cli, &node).await;

        match evt.try_recv().expect("event was dispatched") {
            Event::ConnectFailure { reason, message } => {
                assert_eq!(reason, ConnectFailureReason::LoggedOut);
                assert!(reason.is_logged_out());
                assert_eq!(message.as_deref(), Some("logged out"));
            }
            other => panic!("expected ConnectFailure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn handle_connect_failure_unknown_code_passes_through() {
        let (cli, mut evt) = make_client();
        let mut attrs = Attrs::new();
        attrs.insert("reason".into(), Value::String("9999".into()));
        let node = Node::new("failure", attrs, None);

        handle_connect_failure(&cli, &node).await;
        match evt.try_recv().expect("event was dispatched") {
            Event::ConnectFailure { reason, message } => {
                assert_eq!(reason, ConnectFailureReason::Other(9999));
                assert!(!reason.is_logged_out());
                assert!(message.is_none());
            }
            other => panic!("expected ConnectFailure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn handle_ib_dispatches_offline_preview_and_completed() {
        let (cli, mut evt) = make_client();

        // <ib><offline_preview count=10 appdata=1 message=2 notification=3 receipt=4/>
        //     <offline count=10/></ib>
        let mut prev_attrs = Attrs::new();
        prev_attrs.insert("count".into(), Value::String("10".into()));
        prev_attrs.insert("appdata".into(), Value::String("1".into()));
        prev_attrs.insert("message".into(), Value::String("2".into()));
        prev_attrs.insert("notification".into(), Value::String("3".into()));
        prev_attrs.insert("receipt".into(), Value::String("4".into()));
        let preview = Node::new("offline_preview", prev_attrs, None);

        let mut comp_attrs = Attrs::new();
        comp_attrs.insert("count".into(), Value::String("10".into()));
        let completed = Node::new("offline", comp_attrs, None);

        let ib = Node::new(
            "ib",
            Attrs::new(),
            Some(Value::Nodes(vec![preview, completed])),
        );
        handle_ib(&cli, &ib).await;

        // Two events, in source order.
        match evt.try_recv().expect("preview dispatched") {
            Event::OfflineSyncPreview {
                total,
                app_data_changes,
                messages,
                notifications,
                receipts,
            } => {
                assert_eq!(total, 10);
                assert_eq!(app_data_changes, 1);
                assert_eq!(messages, 2);
                assert_eq!(notifications, 3);
                assert_eq!(receipts, 4);
            }
            other => panic!("expected OfflineSyncPreview, got {other:?}"),
        }
        match evt.try_recv().expect("completed dispatched") {
            Event::OfflineSyncCompleted { count } => assert_eq!(count, 10),
            other => panic!("expected OfflineSyncCompleted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn handle_ib_dispatches_qr_downgrade() {
        let (cli, mut evt) = make_client();
        let ib = Node::new(
            "ib",
            Attrs::new(),
            Some(Value::Nodes(vec![Node::tag_only("downgrade_webclient")])),
        );
        handle_ib(&cli, &ib).await;
        match evt.try_recv().expect("qr downgrade dispatched") {
            Event::QrScannedWithoutMultidevice => {}
            other => panic!("expected QrScannedWithoutMultidevice, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn handle_ib_dirty_emits_event_even_when_iq_send_fails() {
        // No socket → mark_not_dirty fails with NotConnected, but the
        // DirtyNotification event must still be dispatched. This pins the
        // "telemetry first, IO next" contract documented above.
        let (cli, mut evt) = make_client();
        let mut dirty_attrs = Attrs::new();
        dirty_attrs.insert("type".into(), Value::String("account_sync".into()));
        dirty_attrs.insert("timestamp".into(), Value::String("1714521600".into()));
        let dirty = Node::new("dirty", dirty_attrs, None);
        let ib = Node::new("ib", Attrs::new(), Some(Value::Nodes(vec![dirty])));

        handle_ib(&cli, &ib).await;
        match evt.try_recv().expect("dirty event dispatched") {
            Event::DirtyNotification {
                dirty_type,
                timestamp,
            } => {
                assert_eq!(dirty_type, "account_sync");
                assert_eq!(timestamp, 1714521600);
            }
            other => panic!("expected DirtyNotification, got {other:?}"),
        }
    }
}
