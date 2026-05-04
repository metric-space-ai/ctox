//! Minimal echo bot — pure Rust, no HTTP, no UI.
//!
//! This is the canonical example of using `whatsapp` as a library: open an
//! `Account`, drive its event loop, reply to text messages. Pairing only
//! happens on first run; subsequent runs resume from the SQLite database.
//!
//! ```bash
//! cargo run --example echo_bot -p whatsapp -- /tmp/whatsapp-bot.sqlite
//! ```
//!
//! On first run, scan the printed QR data with WhatsApp on your phone
//! (Settings → Linked Devices → Link a Device) — you'll need to render the
//! QR string yourself (e.g. via `qrencode -t ANSI <code>` or paste it into
//! any online QR renderer). For a turnkey "QR in browser" demo see
//! `examples/pair_live.rs` instead.

use std::env;

use whatsapp::{Account, Event};

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> whatsapp::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install rustls crypto provider");
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let db_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/whatsapp-echo-bot.sqlite".to_owned());

    let account = Account::open(&db_path).await?.with_push_name("Echo Bot");

    println!("Account opened at {db_path}.");
    if account.is_paired().await? {
        println!("Resuming saved pairing — no QR needed.");
    } else {
        println!("Not paired yet — waiting for QR.");
    }

    let mut events = account.connect().await?;

    while let Some(evt) = events.recv().await {
        match evt {
            Event::Qr { code, .. } => {
                println!("\n=== Scan with WhatsApp on your phone ===");
                println!("{code}");
                println!("=== ===\n");
            }
            Event::Paired { jid } => println!("Paired as {jid}."),
            Event::Connected { jid } => println!("Logged in as {jid}. Ready to echo messages."),
            Event::Message(msg) => {
                let from = &msg.from;
                if let Some(text) = msg.text() {
                    println!("[{from}] {text}");
                    let reply = format!("you said: {text}");
                    if let Err(e) = account.send_text(&msg.chat, &reply).await {
                        eprintln!("send_text failed: {e}");
                    }
                } else if msg.is_media() {
                    println!("[{from}] (media message)");
                } else if msg.is_reaction() {
                    println!("[{from}] (reaction)");
                }
            }
            Event::Receipt { from, message_id, receipt_type } => {
                let kind = receipt_type.as_deref().unwrap_or("delivered");
                println!("receipt from {from}: {message_id} ({kind})");
            }
            Event::HistorySync(sync) => {
                println!(
                    "history sync: type={:?} {} chats, {} pushnames",
                    sync.sync_type(),
                    sync.conversations.len(),
                    sync.pushnames.len()
                );
            }
            Event::Disconnected { reason } => {
                eprintln!("disconnected: {reason}");
                break;
            }
            Event::Error(msg) => eprintln!("warn: {msg}"),
        }
    }
    Ok(())
}
