//! Authenticated signaling ICE configuration. Secrets never enter diagnostics.
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use webrtc::peer_connection::RTCIceServer;

#[derive(Clone, Serialize)]
#[serde(transparent)]
pub struct SignalingIceServers(pub(crate) Vec<RTCIceServer>);

impl std::fmt::Debug for SignalingIceServers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalingIceServers")
            .field("count", &self.0.len())
            .finish()
    }
}

impl<'de> Deserialize<'de> for SignalingIceServers {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(d)?;
        parse_servers(&value)
            .map(Self)
            .ok_or_else(|| serde::de::Error::custom("invalid signaling ICE server configuration"))
    }
}

fn parse_servers(value: &Value) -> Option<Vec<RTCIceServer>> {
    let entries = value.as_array()?;
    if entries.is_empty() || entries.len() > 16 {
        return None;
    }
    entries
        .iter()
        .map(|entry| {
            let urls: Vec<String> = match entry.get("urls")? {
                Value::String(url) => vec![url.trim().to_owned()],
                Value::Array(urls) => urls
                    .iter()
                    .map(|url| Some(url.as_str()?.trim().to_owned()))
                    .collect::<Option<_>>()?,
                _ => return None,
            };
            if urls.is_empty()
                || urls.len() > 16
                || urls.iter().any(|url| {
                    let lower = url.to_ascii_lowercase();
                    url.len() > 2048
                        || url.chars().any(char::is_whitespace)
                        || !["stun:", "turn:", "turns:"].iter().any(|prefix| {
                            lower
                                .strip_prefix(prefix)
                                .is_some_and(|rest| !rest.is_empty())
                        })
                })
            {
                return None;
            }
            let bounded = |key, max| match entry.get(key) {
                None => Some(String::new()),
                Some(Value::String(value)) if value.len() <= max => Some(value.clone()),
                _ => None,
            };
            Some(RTCIceServer {
                urls,
                username: bounded("username", 1024)?,
                credential: bounded("credential", 4096)?,
                ..Default::default()
            })
        })
        .collect()
}

pub(crate) fn is_turn(url: &str) -> bool {
    let url = url.to_ascii_lowercase();
    url.starts_with("turn:") || url.starts_with("turns:")
}

#[derive(Clone)]
pub(crate) struct SignalingIceSnapshot {
    pub servers: Vec<RTCIceServer>,
    pub received_at_ms: u64,
    pub expires_at_ms: u64,
    pub revision: u64,
    pub refresh_attempt_at_ms: u64,
}

impl SignalingIceSnapshot {
    pub fn new(servers: Vec<RTCIceServer>, now: u64, revision: u64) -> Self {
        let expires_at_ms = servers
            .iter()
            .filter(|server| server.urls.iter().any(|url| is_turn(url)))
            .filter_map(|server| {
                server
                    .username
                    .split(':')
                    .next()?
                    .parse::<u64>()
                    .ok()?
                    .checked_mul(1000)
            })
            .filter(|expiry| *expiry > 0)
            .min()
            .unwrap_or(0);
        Self {
            servers,
            received_at_ms: now,
            expires_at_ms,
            revision,
            refresh_attempt_at_ms: 0,
        }
    }

    pub fn usable_servers(&self, now: u64) -> Vec<RTCIceServer> {
        self.servers
            .iter()
            .filter_map(|server| {
                let mut server = server.clone();
                if self.expires_at_ms > 0 && now >= self.expires_at_ms {
                    server.urls.retain(|url| !is_turn(url));
                }
                (!server.urls.is_empty()).then_some(server)
            })
            .collect()
    }

    pub fn status(&self, now: u64) -> Value {
        let usable = self.usable_servers(now);
        serde_json::json!({
            "source": "signaling", "receivedAtMs": self.received_at_ms,
            "revision": self.revision, "serverCount": self.servers.len(),
            "turnCredentialExpiresAtMs": self.expires_at_ms,
            "turnCredentialsExpired": self.expires_at_ms > 0 && now >= self.expires_at_ms,
            "refreshAttemptAtMs": self.refresh_attempt_at_ms,
            "turnConfigured": self.servers.iter().any(|s| s.urls.iter().any(|u| is_turn(u))),
            "credentialedTurnReady": usable.iter().any(|s| !s.username.is_empty() && !s.credential.is_empty() && s.urls.iter().any(|u| is_turn(u))),
            "turnUdpConfigured": usable.iter().any(|s| s.urls.iter().any(|u| u.to_ascii_lowercase().starts_with("turn:") && !u.contains("transport=tcp"))),
            "turnTcpConfigured": usable.iter().any(|s| s.urls.iter().any(|u| u.to_ascii_lowercase().starts_with("turn:") && u.contains("transport=tcp"))),
            "turnTlsConfigured": usable.iter().any(|s| s.urls.iter().any(|u| u.to_ascii_lowercase().starts_with("turns:"))),
        })
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signaling_turn_contract_redacts_and_expires_credentials() {
        let servers: SignalingIceServers = serde_json::from_value(serde_json::json!([
            {"urls":["stun:localhost:3478", "turn:localhost:3478?transport=tcp", "turns:localhost:443"], "username":"2000000000:fixture", "credential":"fixture-private-password"}
        ])).unwrap();
        assert!(!format!("{servers:?}").contains("fixture"));
        let snapshot = SignalingIceSnapshot::new(servers.0, 100, 1);
        assert_eq!(snapshot.expires_at_ms, 2_000_000_000_000);
        assert_eq!(snapshot.usable_servers(100)[0].urls.len(), 3);
        assert_eq!(
            snapshot.usable_servers(snapshot.expires_at_ms)[0].urls,
            ["stun:localhost:3478"]
        );
        let status = snapshot.status(100);
        assert_eq!(status["credentialedTurnReady"], true);
        assert_eq!(status["turnTcpConfigured"], true);
        assert_eq!(status["turnTlsConfigured"], true);
        assert!(!status.to_string().contains("fixture"));
        assert!(serde_json::from_value::<SignalingIceServers>(
            serde_json::json!([{"urls":"https://example.invalid", "credential":"fixture"}])
        )
        .is_err());
    }
}
