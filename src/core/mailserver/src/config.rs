// ref: stalwart/src/config/mod.rs:1-40
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StalwartConfig {
    pub server: ServerConfig,
    pub smtp: SmtpConfig,
    pub imap: ImapConfig,
    pub caldav: CalDavConfig,
    pub carddav: CardDavConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub db_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SmtpConfig {
    pub bind_address: SocketAddr,
    pub outbound_throttle_per_min: usize,
    pub max_connections: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImapConfig {
    pub bind_address: SocketAddr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CalDavConfig {
    pub bind_address: SocketAddr,
    pub enable_scheduling: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CardDavConfig {
    pub bind_address: SocketAddr,
}

impl Default for StalwartConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "localhost".to_string(),
                db_path: "runtime/ctox.sqlite3".to_string(),
            },
            smtp: SmtpConfig {
                bind_address: "127.0.0.1:25".parse().unwrap(),
                outbound_throttle_per_min: 120,
                max_connections: 10,
            },
            imap: ImapConfig {
                bind_address: "127.0.0.1:1143".parse().unwrap(),
            },
            caldav: CalDavConfig {
                bind_address: "127.0.0.1:8080".parse().unwrap(),
                enable_scheduling: true,
            },
            carddav: CardDavConfig {
                bind_address: "127.0.0.1:8081".parse().unwrap(),
            },
        }
    }
}
