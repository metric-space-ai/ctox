//! Authenticated receipt for the readonly protocol startup of the native MCP listener.
//! No tool execution, imported state or later probe is reconciled by this receipt.
use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;

pub(crate) const HEADER: &str = "X-CTOX-Native-Startup-Nonce";
const CAPABILITY: &str = "ctox/native-startup";
const DOMAIN: &str = "ctox.native.mcp.startup.v1.";
const TTL_MS: i64 = 60_000;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    nonce: String,
    command_token_hash: String,
    listener_port: u16,
    issued_at_ms: i64,
    expires_at_ms: i64,
    initialize: Value,
}

fn validate_nonce(nonce: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        nonce.len() == 36 && uuid::Uuid::parse_str(nonce).is_ok(),
        "invalid native MCP startup nonce"
    );
    Ok(())
}

fn sign(secret: &[u8], receipt: &Receipt) -> anyhow::Result<String> {
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(receipt)?);
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let signature =
        URL_SAFE_NO_PAD.encode(hmac::sign(&key, format!("{DOMAIN}{payload}").as_bytes()).as_ref());
    Ok(format!("{payload}.{signature}"))
}

/// Only the actual HTTP handler passes its bound listener and original response.
/// Revalidate the signed command immediately before producing this protocol receipt.
pub(super) fn attest(
    root: &Path,
    listener: SocketAddr,
    token: &str,
    nonce: &str,
    initialize: &mut Value,
) -> anyhow::Result<()> {
    validate_nonce(nonce)?;
    let before = verify_internal_command_session_token(root, token)?;
    let now = now_ms();
    let receipt = Receipt {
        nonce: nonce.to_owned(),
        command_token_hash: format!("{:x}", Sha256::digest(token.as_bytes())),
        listener_port: listener.port(),
        issued_at_ms: now,
        expires_at_ms: now.saturating_add(TTL_MS),
        initialize: initialize.clone(),
    };
    let secret = mcp_internal_session_signing_secret(root)?;
    let proof = sign(&secret, &receipt)?;
    anyhow::ensure!(
        verify_internal_command_session_token(root, token)? == before,
        "native MCP command authority changed during initialization"
    );
    initialize["capabilities"]["experimental"] = serde_json::json!({ CAPABILITY: proof });
    Ok(())
}

fn verify_proof(
    secret: &[u8],
    nonce: &str,
    token: &str,
    endpoint: &str,
    initialize: &Value,
    now: i64,
) -> anyhow::Result<()> {
    validate_nonce(nonce)?;
    let url = url::Url::parse(endpoint)?;
    let ip = url
        .host_str()
        .context("native MCP endpoint has no host")?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<std::net::IpAddr>()?;
    anyhow::ensure!(
        ip.is_loopback()
            && url.scheme() == "http"
            && url.path() == "/mcp"
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "native MCP startup requires the numeric loopback endpoint"
    );
    let proof = initialize
        .pointer("/capabilities/experimental")
        .and_then(Value::as_object)
        .filter(|caps| caps.len() == 1)
        .and_then(|caps| caps.get(CAPABILITY))
        .and_then(Value::as_str)
        .context("original MCP initialization has no native startup receipt")?;
    anyhow::ensure!(proof.len() <= 12 * 1024, "native MCP receipt exceeds bound");
    let (payload, signature) = proof
        .split_once('.')
        .context("malformed native MCP receipt")?;
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    hmac::verify(
        &key,
        format!("{DOMAIN}{payload}").as_bytes(),
        &URL_SAFE_NO_PAD.decode(signature)?,
    )
    .map_err(|_| anyhow::anyhow!("invalid native MCP startup signature"))?;
    let receipt: Receipt = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload)?)?;
    anyhow::ensure!(
        receipt.nonce == nonce
            && receipt.command_token_hash == format!("{:x}", Sha256::digest(token.as_bytes()))
            && Some(receipt.listener_port) == url.port_or_known_default()
            && receipt.issued_at_ms <= now
            && now < receipt.expires_at_ms
            && receipt.expires_at_ms == receipt.issued_at_ms.saturating_add(TTL_MS),
        "native MCP startup receipt binding expired or changed"
    );
    let mut original = initialize.clone();
    original["capabilities"]
        .as_object_mut()
        .context("native MCP capabilities are not an object")?
        .remove("experimental");
    anyhow::ensure!(
        original == receipt.initialize
            && original["protocolVersion"] == MCP_PROTOCOL_VERSION
            && original["capabilities"] == serde_json::json!({"tools":{}})
            && original["serverInfo"]
                == serde_json::json!({
                    "name":"ctox-business-os-mcp", "version":env!("CARGO_PKG_VERSION")
                }),
        "original native MCP protocol response changed"
    );
    Ok(())
}

/// Root verifies only the actual Core snapshot, never renderer-supplied metadata.
pub(crate) fn verify(
    root: &Path,
    nonce: &str,
    token: &str,
    endpoint: &str,
    expected_context: &Value,
    snapshot: &ctox_core::native_mcp_startup::NativeMcpStartupSnapshot,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        snapshot.servers().len() == 1,
        "native MCP startup contains foreign servers"
    );
    let server = &snapshot.servers()[0];
    anyhow::ensure!(
        server.server() == "ctox-business-os" && server.http_url() == Some(endpoint),
        "original native MCP connection differs from the enrolled endpoint"
    );
    let initialize = server
        .initialize()
        .context("original native MCP result is unavailable")?;
    anyhow::ensure!(
        verify_internal_command_session_token(root, token)? == *expected_context,
        "native MCP command authority changed"
    );
    verify_proof(
        &mcp_internal_session_signing_secret(root)?,
        nonce,
        token,
        endpoint,
        initialize,
        now_ms(),
    )?;
    anyhow::ensure!(
        verify_internal_command_session_token(root, token)? == *expected_context,
        "native MCP command authority changed after receipt verification"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn response() -> Value {
        serde_json::json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities":{"tools":{}},
            "serverInfo":{"name":"ctox-business-os-mcp","version":env!("CARGO_PKG_VERSION")}
        })
    }
    fn signed(secret: &[u8], nonce: &str) -> Value {
        let receipt = Receipt {
            nonce: nonce.into(),
            command_token_hash: format!("{:x}", Sha256::digest(b"command")),
            listener_port: 8788,
            issued_at_ms: 100,
            expires_at_ms: 100 + TTL_MS,
            initialize: response(),
        };
        let mut value = response();
        value["capabilities"]["experimental"] =
            serde_json::json!({CAPABILITY:sign(secret,&receipt).unwrap()});
        value
    }
    struct ListenerFixture {
        port: u16,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        task: Option<std::thread::JoinHandle<()>>,
    }
    impl ListenerFixture {
        fn start(root: &Path) -> Self {
            use std::sync::{
                atomic::{AtomicBool, Ordering},
                Arc,
            };
            let server = Server::http("127.0.0.1:0").unwrap();
            let listener = server.server_addr().to_ip().unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let ending = stop.clone();
            let root = root.to_path_buf();
            let task = std::thread::spawn(move || {
                while !ending.load(Ordering::Acquire) {
                    if let Some(request) = server
                        .recv_timeout(std::time::Duration::from_millis(50))
                        .unwrap()
                    {
                        // Actual production HTTP authorization + RPC + receipt path.
                        handle_mcp_http_request(&root, listener, request).unwrap();
                    }
                }
            });
            Self {
                port: listener.port(),
                stop,
                task: Some(task),
            }
        }
    }
    impl Drop for ListenerFixture {
        fn drop(&mut self) {
            self.stop.store(true, std::sync::atomic::Ordering::Release);
            if let Some(task) = self.task.take() {
                task.join().unwrap();
            }
        }
    }

    #[test]
    fn native_startup_actual_http_core_handshake_receipt_and_failed_verifier() -> anyhow::Result<()>
    {
        let (root, task) = workjet_confirmed_plan_service_test_fixture()?;
        let token = issue_internal_confirmed_plan_session(
            root.path(),
            &task,
            "fixture-plan-worker",
            "native-plan-workspace",
        )?
        .context("actual confirmed plan denied startup fixture")?;
        let context = verify_internal_command_session_token(root.path(), &token)?;
        let bearer = mcp_operator_auth_token(root.path())?;
        let listener = ListenerFixture::start(root.path());
        let endpoint = format!("http://127.0.0.1:{}/mcp", listener.port);
        let nonce = uuid::Uuid::new_v4().to_string();
        let servers = serde_json::json!({"ctox-business-os":{
            "url":endpoint, "http_headers":{"Authorization":format!("Bearer {bearer}"),
                "X-CTOX-Business-Command-Session":token, (HEADER):nonce},
            "enabled":true,"required":true,"startup_timeout_sec":10
        }});
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let home=root.path().join("actual-core-home");
            std::fs::create_dir_all(&home)?;
            let mut config=ctox_core::config::ConfigBuilder::default()
                .codex_home(home.clone())
                .cli_overrides(vec![("mcp_servers".into(),toml::Value::try_from(&servers)?)])
                .build().await?;
            config.cwd=root.path().to_owned();
            config.model=Some("gpt-5.1".into());
            config.model_provider.requires_openai_auth=false;
            config.model_provider.supports_websockets=false;
            let auth=std::sync::Arc::new(ctox_core::AuthManager::new(
                home,false,config.cli_auth_credentials_store_mode));
            let manager=ctox_core::ThreadManager::new(&config,auth,
                ctox_protocol::protocol::SessionSource::Exec,
                ctox_core::models_manager::collaboration_mode_presets::CollaborationModesConfig::default());
            let loaded=manager.start_thread(config).await?;
            loaded.thread.register_native_source_factory()?;
            let wrong=uuid::Uuid::new_v4().to_string();
            let failed=loaded.thread.reconcile_native_mcp_startup(|snapshot| {
                verify(root.path(),&wrong,&token,&endpoint,&context,snapshot)
                    .map_err(|error|std::io::Error::other(error.to_string()))
            }).await;
            assert!(failed.is_err(),"a later probe/foreign nonce cleared original startup");
            let core = rusqlite::Connection::open(crate::paths::core_db(root.path()))?;
            core.execute("UPDATE communication_routing_state SET lease_worker_id='replacement' WHERE message_key=?1", [&task])?;
            let revoked = loaded.thread.reconcile_native_mcp_startup(|snapshot| {
                verify(root.path(), &nonce, &token, &endpoint, &context, snapshot)
                    .map_err(|error| std::io::Error::other(error.to_string()))
            }).await;
            assert!(revoked.is_err(), "current command revocation did not fence startup publication");
            core.execute("UPDATE communication_routing_state SET lease_worker_id='fixture-plan-worker' WHERE message_key=?1", [&task])?;
            loaded.thread.reconcile_native_mcp_startup(|snapshot| {
                assert_eq!(snapshot.session_id(),loaded.thread_id);
                verify(root.path(),&nonce,&token,&endpoint,&context,snapshot)
                    .map_err(|error|std::io::Error::other(error.to_string()))
            }).await?;
            assert!(loaded.thread.reconcile_native_mcp_startup(|_| Ok(())).await.is_err());
            loaded.thread.shutdown_and_wait().await?;
            let (_,state)=loaded.thread.capture_native_state().await?;
            let report=serde_json::to_value(state.core_effect_capture().context("actual Core capture missing")?.report())?;
            assert!(!report["startupUncertainties"].as_array().unwrap().contains(&Value::String("mcp-startup".into())));
            Ok::<(),anyhow::Error>(())
        })
    }

    #[test]
    fn native_startup_receipt_binds_original_response_nonce_token_listener_and_expiry() {
        let nonce = uuid::Uuid::new_v4().to_string();
        let value = signed(b"native-secret", &nonce);
        assert!(verify_proof(
            b"native-secret",
            &nonce,
            "command",
            "http://127.0.0.1:8788/mcp",
            &value,
            101
        )
        .is_ok());
        for (secret, n, token, url, time) in [
            (
                &b"foreign"[..],
                nonce.as_str(),
                "command",
                "http://127.0.0.1:8788/mcp",
                101,
            ),
            (
                &b"native-secret"[..],
                "00000000-0000-0000-0000-000000000000",
                "command",
                "http://127.0.0.1:8788/mcp",
                101,
            ),
            (
                &b"native-secret"[..],
                nonce.as_str(),
                "other",
                "http://127.0.0.1:8788/mcp",
                101,
            ),
            (
                &b"native-secret"[..],
                nonce.as_str(),
                "command",
                "http://127.0.0.1:8789/mcp",
                101,
            ),
            (
                &b"native-secret"[..],
                nonce.as_str(),
                "command",
                "http://remote.invalid:8788/mcp",
                101,
            ),
            (
                &b"native-secret"[..],
                nonce.as_str(),
                "command",
                "http://127.0.0.1:8788/mcp",
                100 + TTL_MS,
            ),
        ] {
            assert!(verify_proof(secret, n, token, url, &value, time).is_err());
        }
        let mut changed = value;
        changed["capabilities"]["tools"]["listChanged"] = Value::Bool(true);
        assert!(verify_proof(
            b"native-secret",
            &nonce,
            "command",
            "http://127.0.0.1:8788/mcp",
            &changed,
            101
        )
        .is_err());
    }
}
