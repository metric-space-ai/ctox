//! Original initialization evidence from the actual managed MCP connection.
//! Neither this snapshot nor copied protocol metadata grants effect reconciliation.
use ctox_protocol::ThreadId;
use serde::Serialize;
use serde_json::Value;
use std::io::{self, Write};

#[derive(Clone)]
pub struct NativeMcpStartupSnapshot {
    session_id: ThreadId,
    servers: Vec<NativeMcpStartupObservation>,
}
impl NativeMcpStartupSnapshot {
    pub(crate) fn new(session_id: ThreadId, servers: Vec<NativeMcpStartupObservation>) -> Self {
        Self {
            session_id,
            servers,
        }
    }
    pub fn session_id(&self) -> ThreadId {
        self.session_id
    }
    pub fn servers(&self) -> &[NativeMcpStartupObservation] {
        &self.servers
    }
}

#[derive(Clone)]
pub struct NativeMcpStartupObservation {
    server: String,
    http_url: Option<String>,
    initialize: Option<Value>,
}
const MAX_INITIALIZE_BYTES: usize = 16 * 1024;
struct BoundedBuffer(Vec<u8>);
impl Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_INITIALIZE_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other(
                "original MCP initialize evidence exceeds its bound",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl NativeMcpStartupObservation {
    pub(crate) fn from_original_initialize<T: Serialize>(
        server: String,
        http_url: Option<String>,
        initialize: &T,
    ) -> Self {
        // Do not clone an unbounded peer payload before applying the metadata limit.
        let mut buffer = BoundedBuffer(Vec::new());
        let initialize = serde_json::to_writer(&mut buffer, initialize)
            .ok()
            .and_then(|_| serde_json::from_slice(&buffer.0).ok());
        Self {
            server,
            http_url,
            initialize,
        }
    }
    pub fn server(&self) -> &str {
        &self.server
    }
    pub fn http_url(&self) -> Option<&str> {
        self.http_url.as_deref()
    }
    pub fn initialize(&self) -> Option<&Value> {
        self.initialize.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_initialize_metadata_is_owned_and_oversize_stays_unknown() {
        let mut result = serde_json::json!({"serverInfo":{"name":"native"}});
        let original = NativeMcpStartupObservation::from_original_initialize(
            "server".into(),
            Some("http://127.0.0.1:8788/mcp".into()),
            &result,
        );
        result["serverInfo"]["name"] = serde_json::json!("replaced");
        assert_eq!(
            original.initialize().unwrap()["serverInfo"]["name"],
            "native"
        );
        assert_eq!(original.server(), "server");
        assert_eq!(original.http_url(), Some("http://127.0.0.1:8788/mcp"));
        let oversized = NativeMcpStartupObservation::from_original_initialize(
            "unknown".into(),
            None,
            &serde_json::json!({"data":"x".repeat(MAX_INITIALIZE_BYTES)}),
        );
        assert!(oversized.initialize().is_none());
        let snapshot = NativeMcpStartupSnapshot::new(ThreadId::default(), vec![original]);
        assert_eq!(snapshot.session_id(), ThreadId::default());
        assert_eq!(snapshot.servers().len(), 1);
    }
}
