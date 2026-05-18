# whatsapp-rust

A Rust library for the WhatsApp Web multi-device API — clean-room port of
[whatsmeow](https://github.com/tulir/whatsmeow). Pair against a phone with a
QR code, send and receive messages, sync history, persist across restarts,
and keep going.

Designed to embed into other Rust programs. The canonical use case is
**giving an AI agent a WhatsApp account**: open a database, drive the event
loop, reply to messages.

## Quick start

```toml
# Cargo.toml
[dependencies]
whatsapp = { git = "https://github.com/example/whatsapp-rust" }
tokio = { version = "1", features = ["full"] }
rustls = "0.23"
```

```rust
use whatsapp::{Account, Event};

#[tokio::main]
async fn main() -> whatsapp::Result<()> {
    rustls::crypto::ring::default_provider().install_default().unwrap();

    let account = Account::open("whatsapp.sqlite")
        .await?
        .with_push_name("My Bot");

    let mut events = account.connect().await?;

    while let Some(evt) = events.recv().await {
        match evt {
            Event::Qr { code, .. } => {
                // First run only. Display this string as a QR code, then
                // scan with WhatsApp on your phone (Settings → Linked
                // Devices → Link a Device).
                println!("scan: {code}");
            }
            Event::Paired { jid } => println!("paired as {jid}"),
            Event::Connected { jid } => println!("ready as {jid}"),
            Event::Message(msg) => {
                if let Some(text) = msg.text() {
                    let reply = format!("you said: {text}");
                    account.send_text(&msg.chat, &reply).await?;
                }
            }
            Event::Disconnected { reason } => {
                eprintln!("dropped: {reason}");
                break;
            }
            _ => {}
        }
    }
    Ok(())
}
```

That's it. No HTTP layer, no out-of-process daemon — the library runs
in-process and the phone talks straight to your code.

The SQLite database holds the device identity. Delete it to re-pair from
scratch; restart the program with the file present and it skips the QR.

## Examples

The `whatsapp` crate ships two examples:

- **`echo_bot`** — pure Rust, terminal only, ~80 lines. The minimum viable
  embedding: print the QR string, echo every text message back. Run with:
  ```bash
  cargo run --example echo_bot -p whatsapp -- /tmp/bot.sqlite
  ```
  Render the printed QR string with any QR tool (e.g. `qrencode -t ANSI`).

- **`pair_live`** — same flow, but wraps an HTTP page on `localhost:9090`
  that renders the rotating QR as an SVG so you can scan from a browser.
  Useful as a reference UI / smoke test, not part of the library API.
  ```bash
  cargo run --example pair_live -p whatsapp
  open http://localhost:9090
  ```

## Account API

Everything an embedder needs is on `Account`:

| Method | Returns | Notes |
|---|---|---|
| `Account::open(path)` | `Result<Account>` | Opens or creates SQLite store |
| `account.with_push_name(s)` | `Account` | Builder; sets the display name |
| `account.is_paired()` | `Result<bool>` | `true` if a saved device exists |
| `account.connect()` | `Result<UnboundedReceiver<Event>>` | Drives pairing + login |
| `account.jid()` | `Option<Jid>` | Logged-in JID |
| `account.send_text(to, body)` | `Result<String>` | Returns message id |
| `account.send_image(to, jpeg, caption)` | `Result<String>` | |
| `account.send_document(to, bytes, mime, file_name)` | `Result<String>` | |
| `account.send_reaction(chat, target_id, target_sender, from_me, emoji)` | `Result<String>` | Empty string removes the reaction |
| `account.send_reply(chat, body, quoted_id, quoted_sender, quoted_msg)` | `Result<String>` | |
| `account.send_revoke(chat, target_id)` | `Result<String>` | Delete-for-everyone |
| `account.mark_read(ids, t, chat, sender)` | `Result<()>` | |
| `account.client()` | `Option<Arc<Client>>` | Lower-level escape hatch |

## Events

```rust
pub enum Event {
    Qr { code: String, refresh_in: Duration },
    Paired { jid: Jid },
    Connected { jid: Jid },
    Disconnected { reason: String },
    Message(IncomingMessage),
    Receipt { from: Jid, message_id: String, receipt_type: Option<String> },
    HistorySync(Box<HistorySync>),
    Error(String),
}

pub struct IncomingMessage {
    pub from: Jid,           // author (== chat for DMs, participant for groups)
    pub chat: Jid,           // conversation
    pub message_id: String,
    pub timestamp: i64,
    pub proto: Box<wha_proto::e2e::Message>,  // full decoded body
}

impl IncomingMessage {
    pub fn text(&self) -> Option<&str>;     // unwraps conversation / extended_text
    pub fn is_media(&self) -> bool;         // image | video | audio | document | sticker
    pub fn is_reaction(&self) -> bool;
}
```

`History sync` arrives automatically on first connect — the parsed
`HistorySync` proto contains your chat list, pushnames, and per-conversation
recent message snapshots.

## Status

Live-verified against `web.whatsapp.com`:

- ✓ Pair (HMAC + ADV signature + signed-device-identity)
- ✓ SQLite persistence — restart without QR
- ✓ Phase-2 login (active IQ + prekey upload + presence + `<dirty>` ack)
- ✓ Decrypt incoming `<message>` (pkmsg + msg)
- ✓ History sync download + decode (encrypted via media CDN, plus inline
  bootstrap chunks)
- ✓ Send text DM with multi-device fanout (server-side `<ack class="message">`
  observed)
- ✓ Auto `<receipt>` and `<ack>` for received messages
- ✓ Plaintext-cache mirroring whatsmeow's `bufferedDecrypt` (server retries
  don't fail after one-time pre-key consumption)
- ✓ Auto-keepalive (25 s ping with 3-strike timeout)

Implemented with unit tests pinning wire shape, **not yet driven against
live servers** in the current session:

- Reactions, replies, revoke (delete-for-everyone)
- Group skmsg send with sender-key distribution fanout
- Newsletter / channel send
- Media upload (image / video / audio / document / sticker)
- App-state mutation send (mark_read, mute, pin, archive, star)
- Calls handling (offer/accept/terminate/reject events surface; reject
  outbound implemented)
- Group operations (create, leave, info, participants, name, topic, invite
  link, join by link)
- Phone-code pairing (QR alternative)
- Privacy settings, push registration, status updates

400+ unit tests pass workspace-wide (`cargo test --workspace --lib`).

## Architecture

Hexagonal — every side effect (websocket, store, clock, RNG) is a trait.
The `whatsapp` crate is the composition root that wires production
implementations.

```
                        whatsapp (Account façade)
                                  │
                            wha-client
            ┌───────────┬─────────┼─────────┬────────────┐
        wha-signal  wha-binary    │   wha-appstate   wha-media
                                  │
              wha-proto      wha-socket      wha-store
                                                  │
                                            wha-store-sqlite
                                  │
                            wha-crypto, wha-types
```

Storage is plug-replaceable: drop in `MemoryStore` for tests, `SqliteStore`
for production, or implement the traits yourself for any other backend.

| Crate | Purpose |
|---|---|
| `whatsapp` | High-level `Account` façade and re-exports |
| `wha-client` | Low-level `Client` — IQs, dispatch, send/recv pipelines, all 30+ feature modules |
| `wha-types` | `Jid`, `MessageId`, `BotMap`, error newtypes |
| `wha-binary` | WhatsApp binary XML codec |
| `wha-crypto` | Curve25519, AES-CBC + GCM + CTR, HKDF, HMAC, media-key derivation |
| `wha-proto` | Generated `prost` bindings for the curated WA proto tree |
| `wha-socket` | TLS + WebSocket + Noise XX handshake |
| `wha-signal` | Signal protocol kernel — sessions, X3DH, Double Ratchet, sender keys |
| `wha-store` | Device + store traits, in-memory backend |
| `wha-store-sqlite` | rusqlite backend with schema migrations + multi-account `Container` |
| `wha-appstate` | LTHash, key expansion, patch encode/decode, mutation builders |
| `wha-media` | Encrypted media upload/download against the WA CDN |

## Building

```bash
cargo build --workspace
cargo test --workspace --lib
```

`wha-proto` regenerates bindings at build time via `protox` (pure-Rust
protobuf compiler — no external `protoc` needed). The upstream `whatsmeow`
clone in `_upstream/whatsmeow/` is the source of `.proto` files and the
reference for every protocol decision.

## Lower-level escape hatch

If you need something the `Account` façade doesn't expose, all the layered
crates are re-exported under `whatsapp::*` modules and the live `Client`
is reachable via `account.client()`:

```rust
let client = account.client().expect("connected");
let info = client.get_user_info(&[some_jid]).await?;
let groups = client.get_joined_groups().await?;
```

## License

MPL-2.0, matching upstream.
