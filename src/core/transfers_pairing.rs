//! First enrollment from an explicit native invite and independently trusted pin.
use super::*;
use crate::{
    native_data_device::NativeDeviceKeyScope, native_transfer_routing::NativeTransferRouting,
};
use futures_util::StreamExt;
use serde::Deserialize;
use std::io::Read;

#[derive(Deserialize)]
struct InviteSession {
    source: String,
    authenticated: bool,
    capability_token: String,
}

// No Debug/Serialize: input contains the one-time pairing secret.
#[derive(Deserialize)]
struct PairingInvite {
    #[serde(rename = "type")]
    kind: String,
    version: u8,
    instance_id: String,
    sync_room: String,
    signaling_urls: Vec<String>,
    signaling_auth_version: String,
    signaling_browser_token: String,
    signaling_browser_token_hash: String,
    signaling_native_token_hash: String,
    transport: String,
    data_plane: String,
    expires_at: String,
    session: InviteSession,
}

impl PairingInvite {
    fn read(path: &Path) -> Result<Self> {
        let metadata = std::fs::metadata(path).context("cannot read native pairing file")?;
        ensure!(
            metadata.is_file() && metadata.len() <= 65536,
            "invalid native pairing file"
        );
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(65537)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 65536, "native pairing file too large");
        Self::decode(&bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let mut value: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|_| anyhow::anyhow!("invalid native pairing file"))?;
        // Accept either the source CLI's envelope or its `invite` object.
        let value = value
            .get_mut("invite")
            .map(serde_json::Value::take)
            .unwrap_or(value);
        serde_json::from_value(value).map_err(|_| anyhow::anyhow!("invalid native pairing invite"))
    }

    fn routing(&self, now: i64) -> Result<NativeTransferRouting> {
        ensure!(
            self.kind == "ctox-business-os-invite"
                && self.version == 1
                && self.transport == "webrtc"
                && self.data_plane == "rxdb-webrtc"
                && self.session.source == "mobile_invite"
                && self.session.authenticated,
            "native one-time pairing invite required"
        );
        ensure!(
            self.session.capability_token.len() == 43
                && self
                    .session
                    .capability_token
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'),
            "native one-time pairing secret required"
        );
        let expires = chrono::DateTime::parse_from_rfc3339(&self.expires_at)
            .map_err(|_| anyhow::anyhow!("invalid native pairing expiry"))?
            .timestamp_millis()
            .min(now.saturating_add(30 * 60 * 1000));
        ensure!(
            expires > now.saturating_add(1000),
            "native pairing invite expired"
        );
        let route = NativeTransferRouting {
            room: self.sync_room.clone(),
            signaling_urls: self.signaling_urls.clone(),
            browser_token: self.signaling_browser_token.clone(),
            browser_token_hash: self.signaling_browser_token_hash.clone(),
            native_token_hash: self.signaling_native_token_hash.clone(),
            auth_version: self.signaling_auth_version.clone(),
            ice_servers: Vec::new(),
            refreshed_at_ms: now,
            refresh_after_ms: now + (expires - now) / 2,
            expires_at_ms: expires,
        };
        route.validate(&self.instance_id, now)?;
        Ok(route)
    }
}

async fn enroll(
    host: Arc<NativeTransferAccountHost>,
    database: Arc<QueryDatabase>,
    scope: NativeDeviceKeyScope,
    invite: PairingInvite,
    session_slot: &mut AdmissionSession,
) -> Result<NativeTransferAccount> {
    let routing = invite.routing(chrono::Utc::now().timestamp_millis())?;
    let provider = host
        .pairing_provider(scope.clone(), invite.session.capability_token)
        .await?;
    let mut options = database.options().await?;
    options.room = routing.room.clone();
    let peer_id = options.peer_session_id.clone();
    let instance_id = scope.source_instance_id.clone();
    options.signaling_urls = Arc::new(move || {
        routing
            .signaling_at(
                &instance_id,
                &peer_id,
                chrono::Utc::now().timestamp_millis(),
            )
            .unwrap_or_default()
    });
    // Initial invite schema supplies no ICE descriptor. Use the native core's
    // existing bootstrap defaults; after admission, source-confirmed routing
    // replaces them before any transfer payload is allowed.
    options.ice_servers = Vec::new();
    options.local_session_provider = Some(provider);
    // Subscribe before joining: preserve even an immediate handshake rejection.
    let (errors_tx, errors_rx) = tokio::sync::oneshot::channel();
    session_slot.starting = Some(tokio::spawn(async move {
        let session = NativeSyncSession::start_data_client_with_pool_setup(options, |pool| {
            let _ = errors_tx.send(pool.error_subject.subscribe());
            Ok(())
        })
        .await?;
        Ok(Arc::new(session))
    }));
    session_slot.finish_start().await?;
    let session = session_slot
        .session
        .as_ref()
        .context("native pairing session missing")?;
    let mut errors = errors_rx
        .await
        .context("native pairing diagnostics unavailable")?;
    let mut errors_open = true;
    let mut last_error = "none";
    let readiness = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            host.require_new_target(&scope.target_id).await?;
            let pool = session.pool();
            ensure!(
                !pool.canceled.load(std::sync::atomic::Ordering::SeqCst),
                "native pairing stopped"
            );
            let connections = pool.connection_handler.current_connections();
            ensure!(connections.len() <= 1, "ambiguous native pairing source");
            if let Some(connection) = connections.into_iter().next() {
                if pool.is_peer_ready_for_control(&connection) {
                    return Ok(connection);
                }
            }
            tokio::select! {
                error = errors.next(), if errors_open => match error {
                    Some(error) => last_error = pairing_error_class(&error),
                    None => errors_open = false,
                },
                _ = tokio::time::sleep(Duration::from_millis(20)) => {},
            }
        }
    })
    .await;
    let connection = readiness.with_context(|| {
        let transport = session.pool().connection_handler.frame_transport_status();
        format!(
            "native pairing readiness timed out (signaling_connected={}, signaling_joined={}, peers={}, open_data_channels={}, last_peer_error={})",
            transport.signaling_socket_connected,
            transport.signaling_join_accepted,
            transport.peer_count,
            transport.open_data_channels,
            last_error,
        )
    })??;
    host.provision_from_session(scope, session, &connection)
        .await
}

/// Never print peer-controlled messages, parameters, or signaling URLs.
/// Even forged codes resolve to fixed native literals.
fn pairing_error_class(error: &rxdb::rx_error::RxError) -> &'static str {
    match error
        .parameters()
        .get("code")
        .and_then(serde_json::Value::as_str)
    {
        Some("local_session_credentials_unavailable") => "local_session_credentials_unavailable",
        Some("peer_authentication_failed") => "peer_authentication_failed",
        _ => match error.code() {
            "RC_WEBRTC_PROTOCOL" => "protocol_error",
            "RC_WEBRTC_SIGNAL" => "signaling_error",
            "RC_WEBRTC_PEER" => "peer_error",
            _ => "other_peer_error",
        },
    }
}

/// An existing target (including its disconnect tombstone) cannot be replaced.
/// Recovery of old jobs must preserve their original authority instead.
pub(crate) fn pair(
    root: &Path,
    target_id: &str,
    source_pin: &str,
    path: &Path,
) -> Result<serde_json::Value> {
    let invite = PairingInvite::read(path)?;
    invite.routing(chrono::Utc::now().timestamp_millis())?;
    let scope = NativeDeviceKeyScope {
        target_id: target_id.into(),
        source_public_identity: source_pin.into(),
        source_instance_id: invite.instance_id.clone(),
        account_epoch: 1,
    };
    let directory = crate::paths::runtime_dir(root).join("transfers/admission");
    std::fs::create_dir_all(&directory)?;
    let temporary = tempfile::Builder::new()
        .prefix("pair-")
        .tempdir_in(directory)?;
    let database = QueryDatabase::new(temporary.path());
    let host = account_host(root, database.clone());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let mut session = AdmissionSession::default();
        let result = tokio::time::timeout(
            Duration::from_secs(60),
            enroll(host.clone(), database.clone(), scope, invite, &mut session),
        )
        .await
        .context("native pairing deadline")
        .and_then(|result| result);
        session.close().await?;
        database.close().await?;
        let account = result?;
        same_account(&host, &account).await?;
        Ok(
            serde_json::json!({"targetId":account.target_id,"sourceInstanceId":account.instance_id,
            "sourcePublicIdentity":account.public_identity,"accountEpoch":account.account_epoch,
            "deviceId":account.principal.device.map(|device| device.device_id)}),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn pairing_diagnostics_never_echo_private_error_payloads() {
        use rxdb::rx_error::new_rx_error;
        let secret = "private-invite-and-routing-credential";
        for code in ["RC_WEBRTC_PROTOCOL", "RC_WEBRTC_PEER", secret] {
            for detail in [
                "local_session_credentials_unavailable",
                "peer_authentication_failed",
                secret,
            ] {
                let error = new_rx_error(
                    code,
                    Some(serde_json::json!({
                        "code": detail, "message": secret, "url": secret, "capabilityToken": secret,
                    })),
                );
                let class = pairing_error_class(&error);
                assert!(!class.contains(secret));
                assert!(!class.is_empty());
            }
        }
    }

    fn invite(now: i64) -> serde_json::Value {
        serde_json::json!({"type":"ctox-business-os-invite","version":1,
            "instance_id":"source","sync_room":"ctox-business-os:source:room",
            "signaling_urls":["ws://127.0.0.1/signal"],"signaling_auth_version":"ctox-role-bound-v1",
            "signaling_browser_token":"browser","signaling_browser_token_hash":format!("{:x}",Sha256::digest(b"browser")),
            "signaling_native_token_hash":"a".repeat(64),"transport":"webrtc","data_plane":"rxdb-webrtc",
            "expires_at":chrono::DateTime::from_timestamp_millis(now+60000).unwrap().to_rfc3339(),
            "session":{"source":"mobile_invite","authenticated":true,"capability_token":"n".repeat(43)}})
    }

    #[test]
    fn accepts_native_envelope_but_rejects_expiry_bearer_and_foreign_room() {
        let now = chrono::Utc::now().timestamp_millis();
        let value = invite(now);
        let input = PairingInvite::decode(
            &serde_json::to_vec(&serde_json::json!({"invite":value})).unwrap(),
        )
        .unwrap();
        assert!(input.routing(now).is_ok());
        assert!(input.routing(now + 60000).is_err());
        let mut input = input;
        input.session.capability_token = "renderer.jwt.bearer".into();
        assert!(input.routing(now).is_err());
        input.session.capability_token = "n".repeat(43);
        input.sync_room = "ctox-business-os:foreign:room".into();
        assert!(input.routing(now).is_err());
    }
}
