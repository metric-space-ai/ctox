//! Source-confirmed native routing. This is encrypted host state, never IPC
//! renderer input, and never includes the source's native-role credential.
use anyhow::{ensure, Result};
use ctox_sync::business_data_contract::NativeBusinessDataPrincipal;
use rxdb::plugins::replication_webrtc::RTCIceServer;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

pub(crate) const NATIVE_TRANSFER_PROVISION_METHOD: &str = "ctox.transfer.account.v1";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeTransferProvisionRequest {
    pub source_public_identity: String,
    pub source_instance_id: String,
}

// Deliberately no Debug: signaling/TURN credentials must not enter receipts.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeTransferRouting {
    pub room: String,
    pub signaling_urls: Vec<String>,
    pub browser_token: String,
    pub browser_token_hash: String,
    pub native_token_hash: String,
    pub auth_version: String,
    pub ice_servers: Vec<NativeTransferIceServer>,
    pub refreshed_at_ms: i64,
    pub refresh_after_ms: i64,
    pub expires_at_ms: i64,
}

/// Retained signaling material can locate the originally pinned source after
/// short-lived routing expires. It carries no ICE credentials or payload lease.
/// Admission still requires fresh source proof plus the original device key.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct NativeTransferRendezvous {
    pub room: String,
    instance_id: String,
    signaling_urls: Vec<String>,
    browser_token: String,
    browser_token_hash: String,
    native_token_hash: String,
    auth_version: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeTransferIceServer {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeTransferProvisionReply {
    pub version: u8,
    pub source_public_identity: String,
    pub source_instance_id: String,
    pub principal: NativeBusinessDataPrincipal,
    pub capability_token: String,
    pub capability_expires_at_ms: i64,
    pub routing: NativeTransferRouting,
}

fn clean(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl NativeTransferRouting {
    pub(crate) fn validate(&self, instance_id: &str, now_ms: i64) -> Result<()> {
        ensure!(
            clean(&self.room, 1024)
                && self
                    .room
                    .starts_with(&format!("ctox-business-os:{instance_id}:")),
            "native routing source mismatch"
        );
        ensure!(
            self.refreshed_at_ms <= now_ms.saturating_add(60_000)
                && self.refresh_after_ms > self.refreshed_at_ms
                && self.refresh_after_ms < self.expires_at_ms
                && self.expires_at_ms > now_ms
                && self.expires_at_ms <= self.refreshed_at_ms.saturating_add(30 * 60 * 1000),
            "native routing expired or invalid"
        );
        ensure!(
            self.auth_version == "ctox-role-bound-v1"
                && clean(&self.browser_token, 4096)
                && hash(&self.browser_token_hash)
                && hash(&self.native_token_hash)
                && self.browser_token_hash != self.native_token_hash
                && format!("{:x}", Sha256::digest(self.browser_token.as_bytes()))
                    == self.browser_token_hash,
            "native routing role credential invalid"
        );
        ensure!(
            !self.signaling_urls.is_empty() && self.signaling_urls.len() <= 8,
            "native signaling unavailable"
        );
        for raw in &self.signaling_urls {
            let url = Url::parse(raw)?;
            let loopback = matches!(
                url.host_str(),
                Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
            );
            ensure!(
                clean(raw, 4096)
                    && url.host_str().is_some()
                    && (url.scheme() == "wss" || (url.scheme() == "ws" && loopback))
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.fragment().is_none(),
                "native signaling route invalid"
            );
        }
        ensure!(
            self.ice_servers.len() <= 16,
            "native ICE configuration invalid"
        );
        for server in &self.ice_servers {
            ensure!(
                !server.urls.is_empty()
                    && server.urls.len() <= 8
                    && server.username.len() <= 4096
                    && server.credential.len() <= 4096,
                "native ICE configuration invalid"
            );
            for raw in &server.urls {
                let url = Url::parse(raw)?;
                ensure!(
                    clean(raw, 4096) && matches!(url.scheme(), "stun" | "stuns" | "turn" | "turns"),
                    "native ICE route invalid"
                );
            }
            // Existing coturn credentials use an expiry-prefixed username.
            // A response cannot extend that actual credential's lifetime.
            if let Some((expiry, _)) = server.username.split_once(':') {
                if let Ok(seconds) = expiry.parse::<i64>() {
                    ensure!(
                        self.expires_at_ms <= seconds.saturating_mul(1000),
                        "native TURN credential expired"
                    );
                }
            }
        }
        Ok(())
    }

    pub(crate) fn ice(&self) -> Vec<RTCIceServer> {
        // An empty Vec means "use defaults" to the native transport. Preserve
        // the source's explicit no-STUN/TURN configuration instead.
        if self.ice_servers.is_empty() {
            return vec![RTCIceServer::default()];
        }
        self.ice_servers
            .iter()
            .map(|server| RTCIceServer {
                urls: server.urls.clone(),
                username: server.username.clone(),
                credential: server.credential.clone(),
                ..RTCIceServer::default()
            })
            .collect()
    }

    /// Recover only the stable locator from a previously valid source snapshot.
    /// Validate its original lifetime without moving either expiry timestamp.
    pub(crate) fn retained_rendezvous(
        &self,
        instance_id: &str,
        now_ms: i64,
    ) -> Result<NativeTransferRendezvous> {
        self.validate(instance_id, self.refreshed_at_ms)?;
        ensure!(
            self.refreshed_at_ms <= now_ms.saturating_add(60_000),
            "native routing timestamp is in the future"
        );
        Ok(NativeTransferRendezvous {
            room: self.room.clone(),
            instance_id: instance_id.into(),
            signaling_urls: self.signaling_urls.clone(),
            browser_token: self.browser_token.clone(),
            browser_token_hash: self.browser_token_hash.clone(),
            native_token_hash: self.native_token_hash.clone(),
            auth_version: self.auth_version.clone(),
        })
    }

    /// Recompute the freshness window for every transport reconnect. Only the
    /// browser/replica role is allowed, even for a native background consumer.
    pub(crate) fn signaling_at(
        &self,
        instance_id: &str,
        peer_id: &str,
        now_ms: i64,
    ) -> Result<Vec<String>> {
        self.validate(instance_id, now_ms)?;
        self.retained_rendezvous(instance_id, now_ms)?
            .signaling_at(peer_id, now_ms)
    }
}

impl NativeTransferRendezvous {
    pub(crate) fn signaling_at(&self, peer_id: &str, now_ms: i64) -> Result<Vec<String>> {
        ensure!(clean(peer_id, 256), "native peer session invalid");
        let issued = now_ms / 1000;
        self.signaling_urls
            .iter()
            .map(|raw| {
                let mut url = Url::parse(raw)?;
                if url.host_str() == Some("signaling.ctox.dev")
                    && matches!(url.path(), "/" | "/signal")
                {
                    url.set_path("/v2");
                }
                let preserved = url
                    .query_pairs()
                    .filter(|(key, _)| {
                        !matches!(
                            key.as_ref(),
                            "client"
                                | "role"
                                | "instance_id"
                                | "protocol"
                                | "cap"
                                | "token"
                                | "token_iat"
                                | "token_exp"
                                | "auth_version"
                                | "browser_token_hash"
                                | "native_token_hash"
                                | "native_peer_id"
                                | "signaling_browser_token"
                                | "signalingBrowserToken"
                                | "signaling_room_password"
                                | "signalingRoomPassword"
                                | "room_password"
                                | "roomPassword"
                        )
                    })
                    .map(|(key, value)| (key.into_owned(), value.into_owned()))
                    .collect::<Vec<_>>();
                url.set_query(None);
                {
                    let mut query = url.query_pairs_mut();
                    for (key, value) in preserved {
                        query.append_pair(&key, &value);
                    }
                    query.append_pair("client", peer_id);
                    query.append_pair("role", "browser");
                    query.append_pair("instance_id", &self.instance_id);
                    query.append_pair("protocol", "ctox-rxdb-protocol-v1");
                    query.append_pair("token", &self.browser_token);
                    query.append_pair("token_iat", &issued.to_string());
                    query.append_pair("token_exp", &(issued + 24 * 60 * 60).to_string());
                    query.append_pair("auth_version", &self.auth_version);
                    query.append_pair("browser_token_hash", &self.browser_token_hash);
                    query.append_pair("native_token_hash", &self.native_token_hash);
                    for capability in [
                        "ctox-control-plane-v1",
                        "ctox-role-bound-signaling-v1",
                        "ctox-rxdb-browser-v1",
                        "ctox-file-chunks-v1",
                        "ctox-schema-hash-v1",
                        "ctox-peer-session-v1",
                        "ctox-device-proof-v1",
                    ] {
                        query.append_pair("cap", capability);
                    }
                }
                Ok(url.to_string())
            })
            .collect()
    }
}
