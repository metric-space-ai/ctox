// ref: stalwart/src/smtp/mod.rs:1-12
pub mod client;
pub mod client_queue;
pub mod dkim;
pub mod dsn;
pub mod server;

pub use client::SmtpOutboundClient;
pub use client_queue::SmtpOutboundQueue;
pub use dkim::DkimSigner;
pub use server::SmtpInboundServer;
