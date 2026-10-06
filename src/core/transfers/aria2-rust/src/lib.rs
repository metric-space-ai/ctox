//! aria2-rust — clean-room reimplementation of aria2 in safe Rust.
#![forbid(unsafe_code)]
#![deny(unused_must_use)]

pub mod bencode;
pub mod bt;
pub mod checksum;
pub mod cookies;
pub mod control_file;
pub mod dht;
pub mod dns;
pub mod error;
pub mod filelog;
pub mod ftp;
pub mod gid;
pub mod http;
pub mod lpd;
pub mod metalink;
pub mod mse;
pub mod netrc;
pub mod options;
pub mod rpc;
pub mod rlimit;
pub mod room;
pub mod server_stat;
pub mod session;
#[cfg(feature = "sftp")]
pub mod sftp;
pub mod sockopt;
pub mod storage;
pub mod tls;
pub mod xmlrpc;

pub use error::{Error, Result};
pub use gid::Gid;
pub use options::OptionSet;
pub use session::Session;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const USER_AGENT: &str = concat!("aria2/", env!("CARGO_PKG_VERSION"), "-rust");
