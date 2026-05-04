//! Inbound `<notification>` routing.
//!
//! Mirrors `_upstream/whatsmeow/notification.go::handleNotification`. Every
//! server-pushed `<notification>` stanza (account_sync, encrypt, picture,
//! server_sync, w:gp2, devices, mediaretry, privacy_token, link_code_companion_reg,
//! newsletter, mex, status, …) is parsed here, dispatched to the appropriate
//! handler, and acked back to the server with `<ack class="notification" …>`.
//!
//! For each notification type we either:
//!
//! 1. Emit a typed [`Event::*`] (`PictureChanged`, `DevicesChanged`, …)
//!    so the application can react to it directly, OR
//! 2. Dispatch into a deeper handler module (`appstate::fetch_app_state_patches`,
//!    `prekeys::refresh_pre_keys_if_low`, …) that mutates the device, OR
//! 3. Both.
//!
//! Either way an `<ack>` always goes back at the end of the function — that's
//! the wire-level contract every notification carries regardless of how the
//! payload is consumed.

use tracing::{debug, warn};

use wha_binary::{Attrs, Node, Value};
use wha_types::Jid;

use crate::client::Client;
use crate::error::ClientError;
use crate::events::Event;

/// Coarse classification of the inbound notification, derived from the
/// `class` attribute (matching upstream's `notifType` switch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    /// `account_sync` — own-account sync (devices, picture, privacy, blocklist).
    AccountSync,
    /// `encrypt` — server reports remote prekeys/identity changes.
    Encrypt,
    /// `picture` — profile-picture changed.
    Picture,
    /// `server_sync` — app-state collection update trigger.
    ServerSync,
    /// `devices` — contact-device-list changed.
    Devices,
    /// `fbid:devices` — Messenger device-list changed.
    FbDevices,
    /// `w:gp2` — group metadata update.
    Group,
    /// `mediaretry` — server bounce of `SendMediaRetryReceipt`.
    MediaRetry,
    /// `privacy_token` — incoming trusted-contact token.
    PrivacyToken,
    /// `link_code_companion_reg` — phone-code pair stage reply.
    LinkCodeCompanionReg,
    /// `newsletter` — newsletter live update.
    Newsletter,
    /// `mex` — GraphQL push (`xwa2_notify_newsletter_*`).
    Mex,
    /// `status` — user status (about) update.
    Status,
    /// Anything else — a plain `Event::UnhandledNode` is emitted.
    Other,
}

/// Look at the `type` (or fallback `class`) attribute of a `<notification>`
/// and return its kind.
///
/// Whatsmeow upstream switches on `type`, but our binary codec sometimes
/// surfaces the same label in `class`. We accept either.
pub fn parse_notification_kind(node: &Node) -> NotificationKind {
    let label = node
        .get_attr_str("type")
        .or_else(|| node.get_attr_str("class"))
        .unwrap_or("");
    match label {
        "account_sync" => NotificationKind::AccountSync,
        "encrypt" => NotificationKind::Encrypt,
        "picture" => NotificationKind::Picture,
        "server_sync" => NotificationKind::ServerSync,
        "devices" => NotificationKind::Devices,
        "fbid:devices" => NotificationKind::FbDevices,
        "w:gp2" => NotificationKind::Group,
        "mediaretry" => NotificationKind::MediaRetry,
        "privacy_token" => NotificationKind::PrivacyToken,
        "link_code_companion_reg" => NotificationKind::LinkCodeCompanionReg,
        "newsletter" => NotificationKind::Newsletter,
        "mex" => NotificationKind::Mex,
        "status" => NotificationKind::Status,
        _ => NotificationKind::Other,
    }
}

/// Build the `<ack class="..." from="..." id="..." participant="..." type="..."/>`
/// stanza we send back for every `<notification>`.
///
/// Mirrors upstream's `Client.sendAck` minus the message-specific `recipient`
/// rewriting — `<notification>` acks never carry a `recipient`. Missing
/// optional attributes (e.g. no `participant`) are simply omitted.
pub fn build_notification_ack(node: &Node, client_jid: &Jid) -> Node {
    let mut attrs = Attrs::new();
    attrs.insert("class".into(), Value::String(node.tag.clone()));
    if let Some(id) = node.get_attr_str("id") {
        attrs.insert("id".into(), Value::String(id.to_owned()));
    }
    if let Some(from) = node.attrs.get("from") {
        attrs.insert("to".into(), from.clone());
    } else {
        attrs.insert("to".into(), Value::Jid(client_jid.clone()));
    }
    if let Some(participant) = node.attrs.get("participant") {
        attrs.insert("participant".into(), participant.clone());
    }
    if let Some(notif_type) = node.get_attr_str("type") {
        attrs.insert("type".into(), Value::String(notif_type.to_owned()));
    }
    Node::new("ack", attrs, None)
}

/// Top-level routing for inbound `<notification>` stanzas. Mirrors
/// `Client.handleNotification` upstream.
pub async fn handle_notification(client: &Client, node: &Node) -> Result<(), ClientError> {
    let kind = parse_notification_kind(node);
    debug!(?kind, tag = %node.tag, "routing inbound notification");

    match kind {
        NotificationKind::AccountSync => handle_account_sync(client, node).await,
        NotificationKind::Encrypt => handle_encrypt(client, node).await,
        NotificationKind::Picture => handle_picture(client, node),
        NotificationKind::ServerSync => handle_server_sync(client, node).await,
        NotificationKind::Devices => handle_devices(client, node, false).await,
        NotificationKind::FbDevices => handle_devices(client, node, true).await,
        NotificationKind::Group => handle_group(client, node),
        NotificationKind::MediaRetry => handle_media_retry(client, node),
        NotificationKind::PrivacyToken => handle_privacy_token(client, node).await,
        NotificationKind::LinkCodeCompanionReg => handle_link_code_companion_reg(client, node),
        NotificationKind::Newsletter => handle_newsletter(client, node),
        NotificationKind::Mex => handle_mex(client, node),
        NotificationKind::Status => handle_status(client, node),
        NotificationKind::Other => {
            client.dispatch_event(Event::UnhandledNode { node: node.clone() });
        }
    }

    let own_jid = client
        .device
        .jid()
        .cloned()
        .unwrap_or_else(Jid::default);
    let ack = build_notification_ack(node, &own_jid);
    client.send_node(&ack).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-type handlers — mirror the corresponding `handle*Notification` upstream.
// ---------------------------------------------------------------------------

/// `encrypt` — either an OTK count update (server reports how many one-time
/// pre-keys we have left) or an identity change for a contact. Mirrors
/// `handleEncryptNotification`.
async fn handle_encrypt(client: &Client, node: &Node) {
    if let Some(count_node) = node.child_by_tag(&["count"]) {
        let mut ag = count_node.attr_getter();
        let value = ag.optional_i64("value");
        if !ag.ok() {
            warn!(errors = ?ag.errors, "encrypt notification <count> attrs invalid");
            return;
        }
        if let Some(left) = value {
            debug!(otks_left = left, "got prekey count from server");
            if (left as u32) < crate::prekeys::MIN_PRE_KEY_COUNT {
                if let Err(e) = crate::prekeys::refresh_pre_keys_if_low(client).await {
                    warn!(?e, "prekey refresh failed");
                }
            }
        }
        return;
    }
    if node.child_by_tag(&["identity"]).is_some() {
        // Identity rotation for a contact. Upstream wipes the identity +
        // session for the user; we emit a typed event so the application
        // (which owns the upper-layer caches) can react.
        let mut ag = node.attr_getter();
        let from = ag.jid("from");
        let timestamp = ag.optional_i64("t").unwrap_or(0);
        if !ag.ok() {
            warn!(errors = ?ag.errors, "identity-change notification missing from/t");
            return;
        }
        // Best-effort: clear stale identity + session entries for the contact.
        // Upstream wipes by phone number; we use the JID's user component.
        let phone = from.user.as_str();
        if let Err(e) = client.device.identities.delete_all_identities(phone).await {
            warn!(?e, %phone, "failed to delete identities after identity change");
        }
        if let Err(e) = client.device.sessions.delete_all_sessions(phone).await {
            warn!(?e, %phone, "failed to delete sessions after identity change");
        }
        client.dispatch_event(Event::IdentityChange {
            jid: from,
            timestamp,
        });
        return;
    }
    debug!("encrypt notification without <count> or <identity> child");
    client.dispatch_event(Event::UnhandledNode { node: node.clone() });
}

/// `server_sync` — server tells us a named app-state collection has bumped
/// version. Walk the `<collection>` children and trigger an incremental fetch
/// for each. Mirrors `handleAppStateNotification`.
async fn handle_server_sync(client: &Client, node: &Node) {
    use crate::appstate::{fetch_app_state_patches, PatchName};
    for collection in node.children().iter().filter(|c| c.tag == "collection") {
        let name_str = collection.get_attr_str("name").unwrap_or("");
        let parsed = match PatchName::parse(name_str) {
            Ok(p) => p,
            Err(e) => {
                warn!(name = %name_str, ?e, "unknown patch name in server_sync");
                continue;
            }
        };
        debug!(name = %name_str, "server_sync notification → fetching patches");
        if let Err(e) = fetch_app_state_patches(client, parsed, false).await {
            warn!(?e, name = %name_str, "fetch_app_state_patches failed");
        }
    }
}

/// `account_sync` — own-account sub-notifications: privacy/devices/picture/blocklist.
/// Mirrors `handleAccountSyncNotification`.
async fn handle_account_sync(client: &Client, node: &Node) {
    let ts = node
        .attr_getter()
        .optional_i64("t")
        .unwrap_or(0);
    for child in node.children() {
        match child.tag.as_str() {
            "privacy" => {
                client.dispatch_event(Event::AccountSyncPrivacy { raw: child.clone() });
            }
            "devices" => {
                // Own-device-list change. Upstream rebuilds its in-process
                // cache; we surface the typed event with a list of device JIDs.
                let mut device_jids = Vec::new();
                for sub in child.children() {
                    if sub.tag == "device" {
                        if let Some(j) = sub.get_attr_jid("jid") {
                            device_jids.push(j.clone());
                        }
                    }
                }
                let from = client
                    .device
                    .jid()
                    .map(|j| j.to_non_ad())
                    .unwrap_or_default();
                client.dispatch_event(Event::DevicesChanged { from, device_jids });
            }
            "picture" => {
                let own = client
                    .device
                    .jid()
                    .map(|j| j.to_non_ad())
                    .unwrap_or_default();
                client.dispatch_event(Event::PictureChanged {
                    jid: own,
                    author: None,
                    picture_id: None,
                    timestamp: ts,
                    removed: false,
                });
            }
            "blocklist" => {
                client.dispatch_event(Event::AccountSyncBlocklist { raw: child.clone() });
            }
            other => debug!(tag = %other, "unhandled account_sync child"),
        }
    }
}

/// `picture` — group/contact profile-picture changed. Mirrors
/// `handlePictureNotification`. Walks `<add>/<set>/<delete>` children and
/// emits one [`Event::PictureChanged`] per child.
fn handle_picture(client: &Client, node: &Node) {
    let ts = node.attr_getter().optional_i64("t").unwrap_or(0);
    for child in node.children() {
        let mut ag = child.attr_getter();
        let jid = ag.jid("jid");
        let author = ag.optional_jid("author").cloned();
        let removed = child.tag == "delete";
        let picture_id = if matches!(child.tag.as_str(), "add" | "set") {
            ag.optional_string("id").map(|s| s.to_owned())
        } else if removed {
            None
        } else {
            // Unknown sub-tag — skip.
            continue;
        };
        if !ag.ok() {
            debug!(errors = ?ag.errors, "picture notification child missing attrs");
            continue;
        }
        client.dispatch_event(Event::PictureChanged {
            jid,
            author,
            picture_id,
            timestamp: ts,
            removed,
        });
    }
}

/// `devices` / `fbid:devices` — contact's device list changed.  Emits
/// [`Event::DevicesChanged`] with the JIDs the server claims are now linked,
/// and triggers a `usync` lookup as a sanity check (matching upstream's
/// invalidation-then-refetch pattern).
async fn handle_devices(client: &Client, node: &Node, _fbid: bool) {
    let mut ag = node.attr_getter();
    let from = ag.jid("from");
    if !ag.ok() {
        warn!(errors = ?ag.errors, "devices notification missing from");
        return;
    }
    // Walk children and harvest add/remove `<device jid="…"/>` entries.
    let mut device_jids = Vec::new();
    for child in node.children() {
        if let Some(dev) = child.child_by_tag(&["device"]) {
            if let Some(j) = dev.get_attr_jid("jid") {
                device_jids.push(j.clone());
            }
        }
    }
    debug!(%from, n = device_jids.len(), "devices notification");
    client.dispatch_event(Event::DevicesChanged {
        from: from.clone(),
        device_jids,
    });
    // Best-effort verification — re-query the server's authoritative list.
    if client.is_connected() {
        if let Err(e) = crate::usync::fetch_user_devices(client, &[from.to_non_ad()]).await {
            debug!(?e, "fetch_user_devices verification failed (non-fatal)");
        }
    }
}

/// `w:gp2` — group v2 update. We don't have the full `parseGroupChange` ported
/// yet; surface the raw node + the group JID so the application can run its
/// own diff. Mirrors the dispatch surface of `handleNotification`'s `w:gp2`
/// branch.
fn handle_group(client: &Client, node: &Node) {
    let from = node
        .attr_getter()
        .optional_jid("from")
        .cloned()
        .unwrap_or_default();
    client.dispatch_event(Event::GroupInfoUpdate {
        group_jid: from,
        raw: node.clone(),
    });
}

/// `mediaretry` — phone responded to a `SendMediaRetryReceipt`. We surface the
/// raw node; callers parse it via [`crate::upload::parse_media_retry`].
fn handle_media_retry(client: &Client, node: &Node) {
    client.dispatch_event(Event::MediaRetry { raw: node.clone() });
}

/// `privacy_token` — server pushes a trusted-contact token for a sender.
/// Mirrors `handlePrivacyTokenNotification`. The parsed token is persisted
/// via the [`PrivacyTokenStore`](wha_store::PrivacyTokenStore) handle on
/// `device`, then surfaced as a typed event for the application layer.
async fn handle_privacy_token(client: &Client, node: &Node) {
    let mut parent_ag = node.attr_getter();
    let sender = parent_ag.jid("from").to_non_ad();
    if !parent_ag.ok() {
        warn!(errors = ?parent_ag.errors, "privacy_token notification missing from");
        return;
    }
    let tokens = match node.child_by_tag(&["tokens"]) {
        Some(t) => t,
        None => {
            warn!("privacy_token notification missing <tokens>");
            return;
        }
    };
    for token_child in tokens.children() {
        if token_child.tag != "token" {
            continue;
        }
        let mut ag = token_child.attr_getter();
        let token_type = ag.string("type");
        if token_type != "trusted_contact" {
            warn!(%token_type, "unexpected privacy_token type");
            continue;
        }
        let timestamp = ag.optional_i64("t").unwrap_or(0);
        let token = match token_child.content.as_bytes() {
            Some(b) => b.to_vec(),
            None => {
                warn!("privacy_token <token> content not bytes");
                continue;
            }
        };
        // Persist long-term. Mirrors upstream's
        // `cli.Store.PrivacyTokens.PutPrivacyTokens` — same upsert semantics.
        // Failures are non-fatal for the notification ack flow.
        if let Err(e) = client
            .device
            .privacy_tokens
            .put_privacy_token(sender.clone(), token.clone(), timestamp)
            .await
        {
            warn!(?e, %sender, "failed to persist privacy token");
        } else {
            debug!(%sender, %timestamp, "stored privacy token");
        }
        client.dispatch_event(Event::PrivacyToken {
            sender: sender.clone(),
            token,
            timestamp,
        });
    }
}

/// `link_code_companion_reg` — phone-code pair stage reply. Surface the raw
/// node so the pair-code state machine (in `pair.rs`) can pick it up.
fn handle_link_code_companion_reg(client: &Client, node: &Node) {
    client.dispatch_event(Event::LinkCodeCompanionReg { raw: node.clone() });
}

/// `newsletter` — newsletter live update. Surface the raw node + the
/// newsletter JID. Mirrors `handleNewsletterNotification`.
fn handle_newsletter(client: &Client, node: &Node) {
    let jid = node
        .attr_getter()
        .optional_jid("from")
        .cloned()
        .unwrap_or_default();
    client.dispatch_event(Event::Newsletter { jid, raw: node.clone() });
}

/// `mex` — GraphQL push. Mirrors `handleMexNotification`.
fn handle_mex(client: &Client, node: &Node) {
    client.dispatch_event(Event::MexUpdate { raw: node.clone() });
}

/// `status` — user about/status change. Mirrors `handleStatusNotification`.
fn handle_status(client: &Client, node: &Node) {
    let mut ag = node.attr_getter();
    let jid = ag.jid("from");
    let timestamp = ag.optional_i64("t").unwrap_or(0);
    if !ag.ok() {
        warn!(errors = ?ag.errors, "status notification missing attrs");
        return;
    }
    let set = match node.child_by_tag(&["set"]) {
        Some(s) => s,
        None => {
            debug!("status notification has no <set> child");
            return;
        }
    };
    let status = match set.content.as_bytes() {
        Some(b) => String::from_utf8_lossy(b).into_owned(),
        None => {
            warn!("status <set> content not bytes");
            return;
        }
    };
    client.dispatch_event(Event::UserAbout { jid, status, timestamp });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use wha_binary::{Attrs, Node, Value};
    use wha_store::MemoryStore;
    use wha_types::Jid;

    use crate::client::Client;

    fn notification_with(notif_type: &str) -> Node {
        let mut attrs = Attrs::new();
        attrs.insert("type".into(), Value::String(notif_type.into()));
        attrs.insert("id".into(), Value::String("nid-1".into()));
        attrs.insert(
            "from".into(),
            Value::Jid(Jid::new("15551234567", wha_types::jid::server::DEFAULT_USER)),
        );
        Node::new("notification", attrs, None)
    }

    fn make_client() -> (Client, tokio::sync::mpsc::UnboundedReceiver<Event>) {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        Client::new(device)
    }

    #[test]
    fn parse_notification_kind_dispatches_correctly() {
        // Smoke-test every routed type. Unknown types should fall through to
        // `Other`. This pins the upstream `notifType` switch byte-for-byte.
        let cases = [
            ("encrypt", NotificationKind::Encrypt),
            ("account_sync", NotificationKind::AccountSync),
            ("server_sync", NotificationKind::ServerSync),
            ("picture", NotificationKind::Picture),
            ("devices", NotificationKind::Devices),
            ("fbid:devices", NotificationKind::FbDevices),
            ("w:gp2", NotificationKind::Group),
            ("mediaretry", NotificationKind::MediaRetry),
            ("privacy_token", NotificationKind::PrivacyToken),
            ("link_code_companion_reg", NotificationKind::LinkCodeCompanionReg),
            ("newsletter", NotificationKind::Newsletter),
            ("mex", NotificationKind::Mex),
            ("status", NotificationKind::Status),
            ("some_unknown_thing_we_have_never_seen", NotificationKind::Other),
        ];
        for (label, expected) in cases {
            let n = notification_with(label);
            assert_eq!(parse_notification_kind(&n), expected, "label={label}");
        }
    }

    #[test]
    fn build_notification_ack_has_required_attrs() {
        let notif = notification_with("encrypt");
        let own = Jid::new("12345", wha_types::jid::server::DEFAULT_USER);
        let ack = build_notification_ack(&notif, &own);
        assert_eq!(ack.tag, "ack");
        assert_eq!(ack.get_attr_str("id"), Some("nid-1"));
        assert_eq!(ack.get_attr_str("class"), Some("notification"));
        assert_eq!(ack.get_attr_str("type"), Some("encrypt"));
        let to_jid = ack.get_attr_jid("to").expect("to should be JID");
        assert_eq!(to_jid.user, "15551234567");
    }

    /// `picture` notification routing — emits one [`Event::PictureChanged`]
    /// per `<add>/<set>/<delete>` child.
    #[tokio::test]
    async fn picture_notification_emits_picture_changed_event() {
        let (cli, mut evt) = make_client();
        let mut notif = notification_with("picture");
        notif.attrs.insert("t".into(), Value::String("1700000000".into()));
        let mut child_attrs = Attrs::new();
        child_attrs.insert(
            "jid".into(),
            Value::Jid(Jid::new("15551234567", wha_types::jid::server::DEFAULT_USER)),
        );
        child_attrs.insert("id".into(), Value::String("PIC-1".into()));
        let child = Node::new("set", child_attrs, None);
        notif.content = Value::Nodes(vec![child]);

        // `handle_picture` is sync so we can call it directly without a
        // socket. The ack send path is exercised separately.
        handle_picture(&cli, &notif);
        match evt.recv().await {
            Some(Event::PictureChanged {
                jid, picture_id, removed, timestamp, ..
            }) => {
                assert_eq!(jid.user, "15551234567");
                assert_eq!(picture_id.as_deref(), Some("PIC-1"));
                assert!(!removed);
                assert_eq!(timestamp, 1_700_000_000);
            }
            other => panic!("expected PictureChanged, got {other:?}"),
        }
    }

    /// `devices` notification routing — emits [`Event::DevicesChanged`] with
    /// the JIDs harvested from `<add>/<remove>` children.
    #[tokio::test]
    async fn devices_notification_emits_devices_changed_event() {
        let (cli, mut evt) = make_client();
        let mut notif = notification_with("devices");
        // Build <add><device jid="…"/></add>.
        let mut dev_attrs = Attrs::new();
        dev_attrs.insert(
            "jid".into(),
            Value::Jid(Jid::new("15551234567", wha_types::jid::server::DEFAULT_USER)),
        );
        let device = Node::new("device", dev_attrs, None);
        let add = Node::new("add", Attrs::new(), Some(Value::Nodes(vec![device])));
        notif.content = Value::Nodes(vec![add]);

        // We bypass `handle_notification` (which sends an ack) and call the
        // sub-handler — it dispatches the typed event directly.
        handle_devices(&cli, &notif, false).await;
        match evt.recv().await {
            Some(Event::DevicesChanged { from, device_jids }) => {
                assert_eq!(from.user, "15551234567");
                assert_eq!(device_jids.len(), 1);
                assert_eq!(device_jids[0].user, "15551234567");
            }
            other => panic!("expected DevicesChanged, got {other:?}"),
        }
    }

    /// `status` notification routing — emits [`Event::UserAbout`] with the
    /// status string from `<set>` content.
    #[tokio::test]
    async fn status_notification_emits_user_about_event() {
        let (cli, mut evt) = make_client();
        let mut notif = notification_with("status");
        notif.attrs.insert("t".into(), Value::String("1234567890".into()));
        let set = Node::new(
            "set",
            Attrs::new(),
            Some(Value::Bytes(b"hello world".to_vec())),
        );
        notif.content = Value::Nodes(vec![set]);

        handle_status(&cli, &notif);
        match evt.recv().await {
            Some(Event::UserAbout { jid, status, timestamp }) => {
                assert_eq!(jid.user, "15551234567");
                assert_eq!(status, "hello world");
                assert_eq!(timestamp, 1_234_567_890);
            }
            other => panic!("expected UserAbout, got {other:?}"),
        }
    }

    /// `newsletter` notification routing — emits [`Event::Newsletter`].
    #[tokio::test]
    async fn newsletter_notification_emits_newsletter_event() {
        let (cli, mut evt) = make_client();
        let mut notif = notification_with("newsletter");
        notif.attrs.insert(
            "from".into(),
            Value::Jid(Jid::new("123", wha_types::jid::server::NEWSLETTER)),
        );
        handle_newsletter(&cli, &notif);
        match evt.recv().await {
            Some(Event::Newsletter { jid, .. }) => {
                assert_eq!(jid.server, wha_types::jid::server::NEWSLETTER);
                assert_eq!(jid.user, "123");
            }
            other => panic!("expected Newsletter, got {other:?}"),
        }
    }

    /// `mex` notification routing — emits [`Event::MexUpdate`].
    #[tokio::test]
    async fn mex_notification_emits_mex_update_event() {
        let (cli, mut evt) = make_client();
        let notif = notification_with("mex");
        handle_mex(&cli, &notif);
        match evt.recv().await {
            Some(Event::MexUpdate { .. }) => {}
            other => panic!("expected MexUpdate, got {other:?}"),
        }
    }

    /// `privacy_token` notification routing — parses `<tokens><token
    /// jid="…" t="…" type="trusted_contact">…bytes…</token></tokens>`,
    /// persists the token via `device.privacy_tokens` (mirroring upstream's
    /// `cli.Store.PrivacyTokens.PutPrivacyTokens`), and emits a typed
    /// [`Event::PrivacyToken`] for the application layer.
    #[tokio::test]
    async fn privacy_token_notification_persists_and_emits_event() {
        use wha_store::PrivacyTokenStore;

        let (cli, mut evt) = make_client();
        let mut notif = notification_with("privacy_token");
        // Build <tokens><token type="trusted_contact" t="1714521600">…</token></tokens>.
        let mut token_attrs = Attrs::new();
        token_attrs.insert("type".into(), Value::String("trusted_contact".into()));
        token_attrs.insert("t".into(), Value::String("1714521600".into()));
        let token_bytes = b"\xCA\xFE\xBA\xBE".to_vec();
        let token = Node::new(
            "token",
            token_attrs,
            Some(Value::Bytes(token_bytes.clone())),
        );
        let tokens = Node::new("tokens", Attrs::new(), Some(Value::Nodes(vec![token])));
        notif.content = Value::Nodes(vec![tokens]);

        handle_privacy_token(&cli, &notif).await;

        // Event was emitted with the parsed payload.
        match evt.recv().await {
            Some(Event::PrivacyToken { sender, token, timestamp }) => {
                assert_eq!(sender.user, "15551234567");
                assert_eq!(token, token_bytes);
                assert_eq!(timestamp, 1_714_521_600);
            }
            other => panic!("expected PrivacyToken, got {other:?}"),
        }

        // Token was persisted in the store — subscribe_presence will find it.
        let sender = Jid::new("15551234567", wha_types::jid::server::DEFAULT_USER);
        let stored = cli
            .device
            .privacy_tokens
            .get_privacy_token(&sender)
            .await
            .unwrap();
        assert_eq!(stored, Some((token_bytes, 1_714_521_600)));
    }
}
