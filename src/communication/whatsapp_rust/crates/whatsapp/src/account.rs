//! High-level WhatsApp account facade.
//!
//! Wraps the layered `wha-*` crates into a single ergonomic API designed for
//! embedding into other programs (chatbots, AI agents, automation tools). The
//! facade owns:
//!
//! - the SQLite-backed device store ([`wha_store_sqlite::SqliteStore`]),
//! - the lower-level [`wha_client::Client`] with its noise socket,
//! - all post-pair bootstrap (active IQ, prekey upload, presence,
//!   `<dirty>` ack), and
//! - automatic translation of low-level [`wha_client::Event`]s into a
//!   higher-level [`Event`] that exposes decrypted message bodies directly.
//!
//! Typical usage:
//!
//! ```no_run
//! use whatsapp::{Account, Event};
//!
//! # async fn run() -> whatsapp::Result<()> {
//! let mut account = Account::open("/var/lib/myagent/whatsapp.sqlite").await?;
//! let mut events = account.connect().await?;
//! while let Some(evt) = events.recv().await {
//!     match evt {
//!         Event::Qr { code, .. } => println!("scan: {code}"),
//!         Event::Paired { jid } => println!("paired as {jid}"),
//!         Event::Connected { .. } => println!("ready"),
//!         Event::Message(msg) => {
//!             if let Some(text) = msg.text() {
//!                 let reply = format!("you said: {text}");
//!                 account.send_text(&msg.chat, &reply).await?;
//!             }
//!         }
//!         _ => {}
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use prost::Message as _;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::warn;

use wha_binary::Node;
use wha_client::{Client, Event as LowEvent};
use wha_proto::e2e::{HistorySyncNotification, Message as ProtoMessage};
use wha_proto::history_sync::HistorySync;
use wha_store::Device;
use wha_store_sqlite::SqliteStore;
use wha_types::Jid;

use crate::error::{Error, Result};

/// One-line summary of a connected account ready for embedding into bot
/// frameworks. Open once with [`Account::open`], call [`Account::connect`] to
/// drive pairing and login, then read [`Event`]s off the returned channel and
/// call [`Account::send_text`] / friends to reply.
pub struct Account {
    db_path: PathBuf,
    store: Arc<SqliteStore>,
    push_name: String,
    client: Arc<RwLock<Option<Arc<Client>>>>,
    pump_handle: RwLock<Option<JoinHandle<()>>>,
}

impl Account {
    /// Open or create a SQLite-backed account at `path`. If a paired device
    /// already exists at that path, it is loaded and pairing is skipped on
    /// the next [`connect`](Self::connect) call.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let store = Arc::new(SqliteStore::open(path.to_str().ok_or_else(|| {
            Error::Config("sqlite path must be valid UTF-8".into())
        })?)?);
        Ok(Self {
            db_path: path,
            store,
            push_name: "WhatsApp Rust".to_owned(),
            client: Arc::new(RwLock::new(None)),
            pump_handle: RwLock::new(None),
        })
    }

    /// Set the push name advertised to other users on `<presence>`. Defaults
    /// to "WhatsApp Rust". Persists into the device blob on next pair-success
    /// or on the next connect after a paired session.
    pub fn with_push_name(mut self, name: impl Into<String>) -> Self {
        self.push_name = name.into();
        self
    }

    /// Path the SQLite database is opened from.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// `true` once a device has been paired (either now or in a previous run
    /// loaded from disk).
    pub async fn is_paired(&self) -> Result<bool> {
        Ok(self
            .store
            .load_device()
            .await?
            .map(|d| d.id.is_some())
            .unwrap_or(false))
    }

    /// JID this account is logged in as, once paired and connected.
    pub fn jid(&self) -> Option<Jid> {
        self.client
            .read()
            .as_ref()
            .and_then(|c| c.device.id.clone())
    }

    /// Drive pairing (if no device is saved) and login, returning the channel
    /// where high-level events arrive. The connection runs in a background
    /// tokio task; calling this method twice is an error.
    pub async fn connect(&self) -> Result<mpsc::UnboundedReceiver<Event>> {
        if self.client.read().is_some() {
            return Err(Error::Config("connect() may only be called once".into()));
        }
        let (etx, erx) = mpsc::unbounded_channel();

        // Decide between Phase 1 (fresh pair) and Phase 2 (resume).
        let saved = self.store.load_device().await?;
        let is_paired = saved.as_ref().map(|d| d.id.is_some()).unwrap_or(false);

        // Run Phase 1 inline so we can hand the user a fully-paired client
        // before returning. Phase 1 *does* emit events on `etx` while it's
        // running so the UI can show the QR. We block until pair-success.
        let device: Device = if is_paired {
            saved.expect("checked above")
        } else {
            run_pair_phase(self.store.clone(), saved, &etx).await?
        };

        // Phase 2: paired, run the long-lived event loop.
        let (cli, low_events) = Client::new(device);
        if cli.device.push_name.is_empty() {
            // We can't mutate Client.device directly here because we don't
            // hold a `&mut`; the push name in the device.* on disk is what
            // matters. Update on disk and reload.
            //
            // Easier: just set the field on the local Client before connect.
            // (Client owns its Device; consume + re-create.)
            let mut device = cli.device.clone();
            device.push_name = self.push_name.clone();
            drop(cli);
            drop(low_events);
            let (cli, ev) = Client::new(device);
            let cli = Arc::new(cli);
            *self.client.write() = Some(cli.clone());
            cli.connect().await?;
            let pump = spawn_event_pump(cli.clone(), ev, etx, self.store.clone());
            *self.pump_handle.write() = Some(pump);
        } else {
            let cli = Arc::new(cli);
            *self.client.write() = Some(cli.clone());
            cli.connect().await?;
            let pump = spawn_event_pump(cli.clone(), low_events, etx, self.store.clone());
            *self.pump_handle.write() = Some(pump);
        }

        Ok(erx)
    }

    /// Borrow the underlying low-level [`wha_client::Client`] for advanced
    /// operations not yet wrapped on `Account`. Returns `None` before
    /// [`connect`](Self::connect) has been called.
    pub fn client(&self) -> Option<Arc<Client>> {
        self.client.read().clone()
    }

    fn require_client(&self) -> Result<Arc<Client>> {
        self.client.read().clone().ok_or(Error::NotConnected)
    }

    // ---- Send helpers — delegate to Client ----

    /// Send a plain-text message and return the assigned message id.
    pub async fn send_text(&self, to: &Jid, body: &str) -> Result<String> {
        let cli = self.require_client()?;
        Ok(cli.send_text(to, body).await?)
    }

    /// Encrypt + upload a JPEG and send as an image with optional caption.
    pub async fn send_image(
        &self,
        to: &Jid,
        jpeg_bytes: &[u8],
        caption: Option<&str>,
    ) -> Result<String> {
        let cli = self.require_client()?;
        Ok(cli.send_image(to, jpeg_bytes, caption).await?)
    }

    /// Send arbitrary file as a document.
    pub async fn send_document(
        &self,
        to: &Jid,
        bytes: &[u8],
        mime_type: &str,
        file_name: &str,
    ) -> Result<String> {
        let cli = self.require_client()?;
        Ok(cli.send_document(to, bytes, mime_type, file_name).await?)
    }

    /// React to a message. Pass an empty string to remove the reaction.
    pub async fn send_reaction(
        &self,
        chat: &Jid,
        target_msg_id: &str,
        target_sender: &Jid,
        target_from_me: bool,
        emoji: &str,
    ) -> Result<String> {
        let cli = self.require_client()?;
        Ok(cli
            .send_reaction(chat, target_msg_id, target_sender, target_from_me, emoji)
            .await?)
    }

    /// Reply to a message, quoting it inline.
    pub async fn send_reply(
        &self,
        chat: &Jid,
        body: &str,
        quoted_msg_id: &str,
        quoted_sender: &Jid,
        quoted_msg: &ProtoMessage,
    ) -> Result<String> {
        let cli = self.require_client()?;
        Ok(cli
            .send_reply(chat, body, quoted_msg_id, quoted_sender, quoted_msg)
            .await?)
    }

    /// Delete one of our own messages for everyone in the chat.
    pub async fn send_revoke(&self, chat: &Jid, target_msg_id: &str) -> Result<String> {
        let cli = self.require_client()?;
        Ok(cli.send_revoke(chat, target_msg_id).await?)
    }

    /// Mark messages as read. `chat` is the conversation JID; `sender` is the
    /// participant's non-AD JID for groups, otherwise `None`.
    pub async fn mark_read(
        &self,
        message_ids: Vec<String>,
        timestamp: i64,
        chat: &Jid,
        sender: Option<&Jid>,
    ) -> Result<()> {
        let cli = self.require_client()?;
        Ok(wha_client::presence_receipt::mark_read(
            &cli,
            message_ids,
            timestamp,
            chat,
            sender,
        )
        .await?)
    }
}

impl Drop for Account {
    fn drop(&mut self) {
        if let Some(h) = self.pump_handle.write().take() {
            h.abort();
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 1 — pair flow (only used on first connect)
// ---------------------------------------------------------------------------

async fn run_pair_phase(
    store: Arc<SqliteStore>,
    existing: Option<Device>,
    etx: &mpsc::UnboundedSender<Event>,
) -> Result<Device> {
    let device = existing.unwrap_or_else(|| store.new_device());
    let (mut client, mut events) = Client::new(device);
    client.connect().await?;
    while let Some(evt) = events.recv().await {
        match evt {
            LowEvent::Connected => {}
            LowEvent::QrCode { code } => {
                let _ = etx.send(Event::Qr {
                    code: code.clone(),
                    refresh_in: Duration::from_secs(20),
                });
            }
            LowEvent::UnhandledNode { node } if node.tag == "iq" => {
                if let Some(child) = node.children().first() {
                    match child.tag.as_str() {
                        "pair-device" => {
                            wha_client::pair::handle_pair_device(&client, &node).await?;
                        }
                        "pair-success" => {
                            wha_client::pair::handle_pair_success(&mut client, &node).await?;
                            let device = client.device.clone();
                            store.save_device(&device).await?;
                            let jid = device.id.clone().ok_or(Error::Internal(
                                "pair-success without jid".into(),
                            ))?;
                            let _ = etx.send(Event::Paired { jid });
                            // WA disconnects us right after pair-success.
                            // Wait a beat then return into Phase 2.
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            return Ok(device);
                        }
                        _ => {}
                    }
                }
            }
            LowEvent::Disconnected { reason } => {
                return Err(Error::Disconnected(reason));
            }
            _ => {}
        }
    }
    Err(Error::Disconnected("pair pump ended".into()))
}

// ---------------------------------------------------------------------------
// Phase 2 — long-lived event pump
// ---------------------------------------------------------------------------

fn spawn_event_pump(
    client: Arc<Client>,
    mut low_events: mpsc::UnboundedReceiver<LowEvent>,
    etx: mpsc::UnboundedSender<Event>,
    store: Arc<SqliteStore>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut bootstrapped = false;
        while let Some(evt) = low_events.recv().await {
            match evt {
                LowEvent::Connected => { /* phase-2 socket up */ }
                LowEvent::Disconnected { reason } => {
                    let _ = etx.send(Event::Disconnected { reason });
                    return;
                }
                LowEvent::UnhandledNode { node } => {
                    if let Err(e) = handle_phase2_node(
                        &client,
                        &node,
                        &etx,
                        &store,
                        &mut bootstrapped,
                    )
                    .await
                    {
                        warn!(?e, "handle_phase2_node failed");
                    }
                }
                LowEvent::StreamError { code, text } => {
                    let _ = etx.send(Event::Error(format!("stream error {code}: {text}")));
                }
                LowEvent::ConnectFailure { reason, message } => {
                    let _ = etx.send(Event::Error(format!(
                        "connect failure: {:?} ({})",
                        reason,
                        message.unwrap_or_default()
                    )));
                }
                LowEvent::ClientOutdated { current_version } => {
                    let _ = etx.send(Event::Error(format!(
                        "client outdated; server reports version {current_version}"
                    )));
                }
                LowEvent::QrScannedWithoutMultidevice => {
                    let _ = etx.send(Event::Error(
                        "phone is on a non-multidevice WhatsApp build".into(),
                    ));
                }
                _ => {}
            }
        }
    })
}

async fn handle_phase2_node(
    client: &Arc<Client>,
    node: &Node,
    etx: &mpsc::UnboundedSender<Event>,
    _store: &Arc<SqliteStore>,
    bootstrapped: &mut bool,
) -> Result<()> {
    match node.tag.as_str() {
        "success" => {
            // Send the post-login bootstrap once: <iq xmlns="passive"
            // type="set"><active/></iq>, then upload one-time prekeys, then
            // <presence type="available">.
            send_active_iq(client).await?;
            let _ = wha_client::prekeys::upload_pre_keys(client).await;
            let _ = client
                .send_presence(wha_client::presence_receipt::PresenceState::Available)
                .await;
            *bootstrapped = true;
            if let Some(jid) = client.device.id.clone() {
                let _ = etx.send(Event::Connected { jid });
            }
        }
        "ib" => {
            // Auto-handle <dirty> with <clean> ack, otherwise ignore.
            if let Some(dirty) = node.children().iter().find(|c| c.tag == "dirty") {
                let typ = dirty.get_attr_str("type").unwrap_or("").to_string();
                let ts = dirty.get_attr_str("timestamp").unwrap_or("").to_string();
                send_clean_iq(client, &typ, &ts).await?;
            }
        }
        "notification" => {
            ack_notification(client, node).await?;
        }
        "receipt" => {
            // Surface delivery/read receipts.
            let from = node
                .get_attr_jid("from")
                .cloned()
                .or_else(|| {
                    node.get_attr_str("from")
                        .and_then(|s| s.parse::<Jid>().ok())
                })
                .unwrap_or_else(|| Jid::new("", wha_types::jid::server::DEFAULT_USER));
            let _ = etx.send(Event::Receipt {
                from,
                message_id: node.get_attr_str("id").unwrap_or("").to_string(),
                receipt_type: node.get_attr_str("type").map(String::from),
            });
        }
        "message" => {
            // Decrypt and translate.
            match client.decrypt_message(node).await {
                Ok(dec) => {
                    let proto =
                        ProtoMessage::decode(dec.plaintext.as_slice()).map_err(|e| {
                            Error::Internal(format!("plaintext proto decode failed: {e}"))
                        })?;

                    // Special case: `protocol_message.history_sync_notification`
                    // → trigger download and emit HistorySync.
                    if let Some(pm) = proto.protocol_message.as_deref() {
                        if let Some(notif) = pm.history_sync_notification.as_ref() {
                            handle_history_sync(client, notif, etx).await;
                            return Ok(());
                        }
                        // Other protocol messages (revoke, etc.) — surface as
                        // a typed message so the caller can inspect.
                    }

                    let incoming = build_incoming(dec, proto);
                    let _ = etx.send(Event::Message(incoming));
                }
                Err(e) => {
                    let _ = etx.send(Event::Error(format!("decrypt failed: {e}")));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

async fn handle_history_sync(
    client: &Arc<Client>,
    notif: &HistorySyncNotification,
    etx: &mpsc::UnboundedSender<Event>,
) {
    match client.download_history_sync(notif).await {
        Ok(parsed) => {
            let _ = etx.send(Event::HistorySync(Box::new(parsed)));
        }
        Err(e) => {
            let _ = etx.send(Event::Error(format!("history-sync download failed: {e}")));
        }
    }
}

fn build_incoming(
    dec: wha_client::recv_message::DecryptedMessage,
    proto: ProtoMessage,
) -> IncomingMessage {
    IncomingMessage {
        from: dec.participant.unwrap_or_else(|| dec.from.clone()),
        chat: dec.from,
        message_id: dec.message_id,
        timestamp: dec.timestamp,
        proto: Box::new(proto),
    }
}

async fn send_active_iq(client: &Arc<Client>) -> Result<()> {
    let mut attrs = wha_binary::Attrs::new();
    attrs.insert(
        "id".into(),
        wha_binary::Value::String(client.generate_request_id()),
    );
    attrs.insert(
        "to".into(),
        wha_binary::Value::Jid(Jid::new("", wha_types::jid::server::DEFAULT_USER)),
    );
    attrs.insert("type".into(), wha_binary::Value::String("set".into()));
    attrs.insert("xmlns".into(), wha_binary::Value::String("passive".into()));
    let iq = wha_binary::Node::new(
        "iq",
        attrs,
        Some(wha_binary::Value::Nodes(vec![wha_binary::Node::tag_only(
            "active",
        )])),
    );
    client.send_node(&iq).await?;
    Ok(())
}

async fn send_clean_iq(client: &Arc<Client>, typ: &str, ts: &str) -> Result<()> {
    let mut iq_attrs = wha_binary::Attrs::new();
    iq_attrs.insert(
        "id".into(),
        wha_binary::Value::String(format!("clean{}", rand::random::<u32>())),
    );
    iq_attrs.insert(
        "to".into(),
        wha_binary::Value::Jid(Jid::new("", wha_types::jid::server::DEFAULT_USER)),
    );
    iq_attrs.insert("type".into(), wha_binary::Value::String("set".into()));
    iq_attrs.insert(
        "xmlns".into(),
        wha_binary::Value::String("urn:xmpp:whatsapp:dirty".into()),
    );
    let mut clean_attrs = wha_binary::Attrs::new();
    clean_attrs.insert("type".into(), wha_binary::Value::String(typ.to_string()));
    clean_attrs.insert(
        "timestamp".into(),
        wha_binary::Value::String(ts.to_string()),
    );
    let clean = wha_binary::Node::new("clean", clean_attrs, None);
    let iq = wha_binary::Node::new(
        "iq",
        iq_attrs,
        Some(wha_binary::Value::Nodes(vec![clean])),
    );
    client.send_node(&iq).await?;
    Ok(())
}

async fn ack_notification(client: &Arc<Client>, node: &Node) -> Result<()> {
    let mut ack_attrs = wha_binary::Attrs::new();
    if let Some(id) = node.get_attr_str("id") {
        ack_attrs.insert("id".into(), wha_binary::Value::String(id.to_owned()));
    }
    ack_attrs.insert(
        "class".into(),
        wha_binary::Value::String("notification".into()),
    );
    if let Some(from) = node.attrs.get("from") {
        ack_attrs.insert("to".into(), from.clone());
    }
    if let Some(t) = node.get_attr_str("type") {
        ack_attrs.insert("type".into(), wha_binary::Value::String(t.to_owned()));
    }
    let ack = wha_binary::Node::new("ack", ack_attrs, None);
    client.send_node(&ack).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// High-level Event + IncomingMessage
// ---------------------------------------------------------------------------

/// Top-level events surfaced to library consumers. Higher-level than the
/// raw wire-protocol [`wha_client::Event`] — every variant here has a
/// well-defined application meaning.
#[derive(Debug, Clone)]
pub enum Event {
    /// A QR code is ready to render. The phone's camera should be pointed at
    /// the SVG / image rendering of `code`. Each code is valid for ~20 s
    /// (60 s for the very first one) before the server rotates it.
    Qr {
        code: String,
        refresh_in: Duration,
    },
    /// The QR code was scanned, the phone bound this device, and we have a
    /// JID. The device blob has been written to SQLite at this point.
    Paired { jid: Jid },
    /// Phase-2 login completed; the account is now live and ready to send
    /// and receive. Subsequent messages arrive as [`Event::Message`].
    Connected { jid: Jid },
    /// Lost the websocket. The pump task has already terminated.
    Disconnected { reason: String },
    /// A decrypted incoming message. Use [`IncomingMessage::text`] +
    /// helpers to extract the body, or inspect the raw `proto` for media
    /// fields.
    Message(IncomingMessage),
    /// Server-side delivery / read receipt for one of our outgoing messages.
    Receipt {
        from: Jid,
        message_id: String,
        /// `None` = delivered. `Some("read")` / `Some("played")` /
        /// `Some("retry")` etc. otherwise.
        receipt_type: Option<String>,
    },
    /// A history-sync chunk arrived and was decoded. Inspect
    /// `HistorySync::conversations` / `pushnames` / etc. to drive UI.
    HistorySync(Box<HistorySync>),
    /// Non-fatal protocol error surfaced for observability. Connection is
    /// still alive.
    Error(String),
}

/// A decrypted inbound message ready to be processed by the application.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    /// JID of the user that authored the message. For DMs this is the same
    /// as `chat`; for groups it's the participant's non-AD JID.
    pub from: Jid,
    /// JID of the conversation the message arrived in (DM = sender JID,
    /// group = group JID, status = `status@broadcast`).
    pub chat: Jid,
    /// Server-assigned id. Use this to reply / react / revoke.
    pub message_id: String,
    /// Server timestamp (seconds since epoch).
    pub timestamp: i64,
    /// Full decoded message proto with every typed body filled in. Use the
    /// helper methods below for the common cases; reach into `proto` for
    /// media metadata, context info, mentioning, replies, etc.
    pub proto: Box<ProtoMessage>,
}

impl IncomingMessage {
    /// Extract the text body if this is a plain-text or extended-text
    /// message. Returns `None` for media-only messages, reactions, etc.
    pub fn text(&self) -> Option<&str> {
        if let Some(c) = self.proto.conversation.as_deref() {
            return Some(c);
        }
        if let Some(et) = self.proto.extended_text_message.as_deref() {
            return et.text.as_deref();
        }
        None
    }

    /// `true` if this message is a reaction (emoji + target message id).
    pub fn is_reaction(&self) -> bool {
        self.proto.reaction_message.is_some()
    }

    /// `true` if this message carries an image / video / audio / document /
    /// sticker payload.
    pub fn is_media(&self) -> bool {
        let m = &self.proto;
        m.image_message.is_some()
            || m.video_message.is_some()
            || m.audio_message.is_some()
            || m.document_message.is_some()
            || m.sticker_message.is_some()
    }
}
