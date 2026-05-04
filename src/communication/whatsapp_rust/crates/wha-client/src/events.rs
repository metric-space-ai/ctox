//! Event types emitted by [`Client`]. Each event roughly mirrors an upstream
//! `events.*` struct; the foundation port exposes the most-load-bearing ones.

use wha_binary::Node;
use wha_types::Jid;

/// Mirrors `events.ConnectFailureReason` in
/// `_upstream/whatsmeow/types/events/events.go`. Numeric codes match the
/// upstream constants 1:1 so they round-trip with what the server sends as
/// the `reason` attribute on `<failure/>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectFailureReason {
    Generic,
    LoggedOut,
    TempBanned,
    MainDeviceGone,
    UnknownLogout,
    ClientOutdated,
    BadUserAgent,
    CatExpired,
    CatInvalid,
    NotFound,
    ClientUnknown,
    InternalServerError,
    Experimental,
    ServiceUnavailable,
    /// Catch-all for codes the port doesn't yet model.
    Other(i64),
}

impl ConnectFailureReason {
    /// Construct from the raw integer the server puts in `reason="..."`. Mirrors
    /// `ConnectFailureReason(int)` in upstream Go.
    pub fn from_code(n: i64) -> Self {
        match n {
            400 => Self::Generic,
            401 => Self::LoggedOut,
            402 => Self::TempBanned,
            403 => Self::MainDeviceGone,
            405 => Self::ClientOutdated,
            406 => Self::UnknownLogout,
            409 => Self::BadUserAgent,
            413 => Self::CatExpired,
            414 => Self::CatInvalid,
            415 => Self::NotFound,
            418 => Self::ClientUnknown,
            500 => Self::InternalServerError,
            501 => Self::Experimental,
            503 => Self::ServiceUnavailable,
            other => Self::Other(other),
        }
    }

    /// Mirrors `(ConnectFailureReason).IsLoggedOut` upstream — returns true
    /// for the codes where the device should locally clear its session.
    pub fn is_logged_out(self) -> bool {
        matches!(
            self,
            Self::LoggedOut | Self::MainDeviceGone | Self::UnknownLogout
        )
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    /// Websocket and noise handshake both complete.
    Connected,
    /// Server told us to reconnect or we lost the websocket.
    Disconnected { reason: String },
    /// QR code string the user should display when pairing as a primary device.
    QrCode { code: String },
    /// Pairing completed; the device now has a JID and identity.
    PairSuccess { id: Jid },
    /// Generic stream:error.
    StreamError { code: String, text: String },
    /// `<failure>` from the server while or after authenticating. Mirrors
    /// `events.ConnectFailure` upstream.
    ConnectFailure {
        reason: ConnectFailureReason,
        message: Option<String>,
    },
    /// `<ib><offline_preview .../></ib>` — server announces how many offline
    /// items it's about to fan out. Mirrors `events.OfflineSyncPreview`.
    OfflineSyncPreview {
        total: i64,
        app_data_changes: i64,
        messages: i64,
        notifications: i64,
        receipts: i64,
    },
    /// `<ib><offline count="..."/></ib>` — server signals it's done flushing
    /// the offline queue. Mirrors `events.OfflineSyncCompleted`.
    OfflineSyncCompleted { count: i64 },
    /// `<ib><dirty type="..." timestamp="..."/></ib>`. Mirrors the spirit of
    /// upstream's logged event; we surface it as a typed event so applications
    /// can re-sync the named app-state collection if they want to.
    DirtyNotification {
        dirty_type: String,
        timestamp: i64,
    },
    /// `<ib><downgrade_webclient/></ib>` — we paired against an old web client
    /// without multidevice. Mirrors `events.QRScannedWithoutMultidevice`.
    QrScannedWithoutMultidevice,
    /// Three keepalive pings in a row went unanswered. Mirrors
    /// `events.KeepAliveTimeout` upstream — surfaces as a hint that the link
    /// is dead so the application can reconnect. The `error_count` is the
    /// running tally of consecutive failures (1, 2, 3, …).
    KeepaliveTimeout {
        error_count: u32,
        /// Unix timestamp (seconds) of the last successful pong.
        last_success_unix: i64,
    },
    /// Keepalive started succeeding again after at least one failure. Mirrors
    /// `events.KeepAliveRestored` upstream.
    KeepaliveRestored,
    /// `check-update` told us we're below the hard-update threshold. Mirrors
    /// the `isBelowHard` branch in upstream's
    /// `_upstream/whatsmeow/update.go::CheckUpdate` reaction. The application
    /// must surface this to the user — once the server starts rejecting our
    /// version we can't reconnect at all. Carries the server-reported current
    /// version for diagnostics.
    ClientOutdated { current_version: String },
    /// An inbound `<iq>`/`<message>`/`<receipt>`/etc. that wasn't routed to a
    /// pending request waiter or to a built-in handler.
    UnhandledNode { node: Node },

    // ---- Notification-derived events. Mirror `events.*` upstream. ----
    /// Profile-picture changed for a user/group. Mirrors `events.Picture`.
    PictureChanged {
        jid: Jid,
        author: Option<Jid>,
        picture_id: Option<String>,
        timestamp: i64,
        removed: bool,
    },
    /// Linked-device list of a contact changed. Upstream's
    /// `handleDeviceNotification` mutates the in-process device cache; we
    /// surface a typed event so the application layer can refresh its own
    /// cache. The `device_jids` are taken straight off the notification's
    /// `<add>` / `<remove>` children.
    DevicesChanged { from: Jid, device_jids: Vec<Jid> },
    /// Group v2 metadata updated (member added/removed/promoted/demoted/etc.).
    /// Mirrors `events.GroupInfo`.
    GroupInfoUpdate { group_jid: Jid, raw: Node },
    /// Newsletter live update or join/leave/mute change. Mirrors
    /// `events.NewsletterLiveUpdate` / `events.NewsletterJoin` etc.
    Newsletter { jid: Jid, raw: Node },
    /// Status (about) update. Mirrors `events.UserAbout`.
    UserAbout {
        jid: Jid,
        status: String,
        timestamp: i64,
    },
    /// Privacy token refreshed for a contact. Mirrors `events.PrivacyToken`.
    PrivacyToken {
        sender: Jid,
        token: Vec<u8>,
        timestamp: i64,
    },
    /// Identity key rotated for a contact (encrypt notification, identity
    /// child). Mirrors `events.IdentityChange`.
    IdentityChange { jid: Jid, timestamp: i64 },
    /// `mex` GraphQL push from the server (newsletter join/leave/mute lifted
    /// into a strongly-typed event by upstream). The GraphQL schema isn't
    /// ported, so we surface the raw node.
    MexUpdate { raw: Node },
    /// Phone-code companion-registration response (`link_code_companion_reg`).
    /// Upstream's `tryHandleCodePairNotification` consumes this into the
    /// pair-code state machine; we surface the raw node so the higher-level
    /// pair flow can pick it up via the event channel.
    LinkCodeCompanionReg { raw: Node },
    /// Account-sync sub-event: privacy settings changed.
    AccountSyncPrivacy { raw: Node },
    /// Account-sync sub-event: blocklist changed. Mirrors `events.Blocklist`.
    AccountSyncBlocklist { raw: Node },
    /// Media re-upload reply received from the phone. Mirrors
    /// `events.MediaRetry` upstream. The notification module routes via this
    /// event so the application can re-download the media using the
    /// refreshed URL.
    MediaRetry { raw: Node },

    // ---- Call-signaling events. Mirror `events.Call*` upstream. ----
    /// Inbound `<call><offer .../></call>` — somebody is ringing the device.
    /// Mirrors `events.CallOffer`.
    CallOffer {
        call_id: String,
        from: Jid,
        timestamp: i64,
        basic_call_offer: BasicCallOffer,
        raw: Node,
    },
    /// `<call><offer_notice .../></call>` — silent offer notice (typically
    /// WhatsApp Business). Mirrors `events.CallOfferNotice`.
    CallOfferNotice {
        call_id: String,
        from: Jid,
        timestamp: i64,
        media: String,
        notice_type: String,
        raw: Node,
    },
    /// `<call><accept .../></call>` — peer accepted our outgoing call.
    /// Mirrors `events.CallAccept`.
    CallAccept {
        call_id: String,
        from: Jid,
        timestamp: i64,
        raw: Node,
    },
    /// `<call><preaccept .../></call>` — partial acceptance ahead of full
    /// negotiation. Mirrors `events.CallPreAccept`.
    CallPreAccept {
        call_id: String,
        from: Jid,
        timestamp: i64,
        raw: Node,
    },
    /// `<call><transport .../></call>` — ICE/transport negotiation update.
    /// Mirrors `events.CallTransport`. Most callers can ignore this.
    CallTransport {
        call_id: String,
        from: Jid,
        timestamp: i64,
        raw: Node,
    },
    /// `<call><terminate reason="..."/></call>` — call ended.
    /// Mirrors `events.CallTerminate`.
    CallTerminate {
        call_id: String,
        from: Jid,
        timestamp: i64,
        reason: String,
        raw: Node,
    },
    /// `<call><reject .../></call>` — peer declined the call.
    /// Mirrors `events.CallReject`. Upstream's `CallReject` does not carry a
    /// `reason` attribute; we surface an empty string when absent so callers
    /// have a uniform shape across the call-event family.
    CallReject {
        call_id: String,
        from: Jid,
        timestamp: i64,
        reason: String,
        raw: Node,
    },
    /// `<call><relaylatency .../></call>` — periodic relay-latency report.
    /// Mirrors `events.CallRelayLatency`.
    CallRelayLatency {
        call_id: String,
        from: Jid,
        timestamp: i64,
        raw: Node,
    },
    /// `<call>` with an unrecognized child or the wrong number of children.
    /// Mirrors `events.UnknownCallEvent`.
    CallUnknown { raw: Node },

    /// Inbound status broadcast — a `<message from="…@s.whatsapp.net"
    /// recipient="status@broadcast" …>` carrying a status update.
    /// Decrypted on the group-message path (status uses sender-key like a
    /// group chat); the embedded plaintext is a prost-encoded
    /// `wha_proto::e2e::Message`. Upstream does not expose a dedicated
    /// `events.StatusUpdate` — applications check
    /// `Info.Chat == StatusBroadcastJID` on a regular `events.Message`. The
    /// Rust port surfaces a distinct event for ergonomics.
    StatusUpdate {
        /// Author of the status (`from` attr — a non-AD user JID).
        from: Jid,
        /// Server-assigned message id.
        message_id: String,
        /// Decrypted plaintext (prost-encoded `wha_proto::e2e::Message`).
        /// Callers run `Message::decode` for the typed body.
        content: Vec<u8>,
    },

    // ---- Decrypted-message-derived events. ---------------------------------
    // These events surface the *content* of a successfully-decrypted DM. The
    // application is expected to call
    // [`crate::recv_message::handle_encrypted_message`] for each inbound
    // `<message>` and then [`crate::recv_message::classify_decrypted_message`]
    // (which dispatches one of the three events below). Mirrors upstream's
    // `events.Message` / `events.RevokeMessage` typed events.
    /// A successfully decrypted plain message arrived. `body` is the
    /// `conversation` text or the `extended_text_message.text` field; if the
    /// inbound message was a reply, `quoted` carries the embedded
    /// `context_info.quoted_message`. Mirrors upstream `events.Message`.
    Message {
        /// The `<message from=>` JID. For groups this is the group; the
        /// real author is `participant`.
        from: Jid,
        /// Group sender / LID-routed peer. None for plain 1:1 DMs.
        participant: Option<Jid>,
        /// Server-stamped message id (the `<message id="...">` attribute).
        message_id: String,
        /// Server-stamped delivery timestamp from `<message t="...">`.
        timestamp: i64,
        /// Plain text body, if the payload had one. None for non-text
        /// payloads (image, sticker, …).
        body: Option<String>,
        /// The full decrypted `Message` proto, for callers that want to
        /// branch on non-text fields.
        message: Box<wha_proto::e2e::Message>,
        /// Some(...) when the payload was a reply that carried
        /// `extended_text_message.context_info.quoted_message`. Mirrors
        /// upstream's `Info.QuotedMessage` plumbing.
        quoted: Option<Box<wha_proto::e2e::Message>>,
    },
    /// A reaction arrived on a previously delivered message. Lifted from
    /// `Message.ReactionMessage` into a typed event. `target_id` is the
    /// id of the message being reacted to; `target_sender` is its
    /// author; `emoji` is the reaction text (empty string = "remove
    /// reaction" — same convention as upstream).
    Reaction {
        from: Jid,
        target_id: String,
        target_sender: Jid,
        emoji: String,
    },
    /// A delete-for-everyone arrived. Lifted from
    /// `Message.ProtocolMessage{type:REVOKE}`. `target_id` is the
    /// message being revoked, `sender` is its original author, `by` is
    /// whoever sent the revocation envelope (own JID for self-revoke,
    /// admin for an admin-revoke).
    MessageRevoke {
        target_id: String,
        sender: Jid,
        by: Jid,
    },
}

/// Compact view of an `<offer>` payload — the bits a typical caller wants
/// when deciding whether to ring, decline, or accept. Mirrors the subset of
/// `types.BasicCallMeta` plus the `video`/`audio` distinction from the
/// outer `<call type="...">` attribute that is most useful at the API
/// boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicCallOffer {
    /// `call-id` from the `<offer>` child.
    pub call_id: String,
    /// `call-creator` from the `<offer>` child — who initiated the call.
    pub call_creator: Jid,
    /// `true` when the outer `<call type="video">`, `false` otherwise.
    pub video: bool,
}
