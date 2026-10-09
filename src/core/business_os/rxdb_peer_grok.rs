// Origin: CTOX
// License: AGPL-3.0-only
//! Owner/Admin subscription control on the admitted native WebRTC peer.
use super::store;
use crate::execution::cliproxyapi_xai::{
    self as xai, CtoxXaiLogin, XaiDeviceLogin, XaiLoginProgress,
};
use anyhow::ensure;
use rxdb::plugins::replication_webrtc::{
    index_mod::GuardedAuxiliaryResponse, WebRTCPublicationGuard,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc, Mutex},
    time::{Duration, Instant},
};

pub(super) const METHOD: &str = "ctox.workjet.grok.v1";
pub(super) const CAPABILITY: &str = "ctox-workjet-grok-v1";
const CHECK_KEY: &str = "grok_subscription_check";
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    version: u8,
    action: String,
    operation_id: String,
    login_id: Option<String>,
    model_id: Option<String>,
}
impl Request {
    fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            self.version == 1 && uuid::Uuid::parse_str(&self.operation_id).is_ok(),
            "invalid_request"
        );
        match self.action.as_str() {
            "instance.grok.read" | "instance.grok.start" | "instance.grok.remove" => ensure!(
                self.login_id.is_none() && self.model_id.is_none(),
                "invalid_request"
            ),
            "instance.grok.poll" | "instance.grok.cancel" => ensure!(
                self.model_id.is_none()
                    && self
                        .login_id
                        .as_deref()
                        .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok()),
                "invalid_request"
            ),
            "instance.grok.check" => ensure!(
                self.login_id.is_none()
                    && self.model_id.as_deref().is_some_and(|m| !m.is_empty()
                        && m.len() <= 256
                        && !m.contains(['\n', '\r'])),
                "invalid_request"
            ),
            _ => anyhow::bail!("invalid_request"),
        }
        Ok(())
    }
}
struct Authority {
    root: PathBuf,
    token: String,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
}
impl Authority {
    fn check(&self) -> anyhow::Result<()> {
        ensure!((self.current)(), "retired");
        let claims = store::verified_webrtc_capability_claims(&self.root, &self.token)
            .ok_or_else(|| anyhow::anyhow!("denied"))?;
        ensure!(
            matches!(claims.role.as_str(), "chef" | "admin") && (self.current)(),
            "denied"
        );
        Ok(())
    }
}
impl WebRTCPublicationGuard for Authority {
    fn with_current(
        &self,
        publish: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
    ) -> rxdb::rx_error::RxResult<()> {
        // The responder holds the current peer-generation publication fence.
        let claims = store::verified_webrtc_capability_claims(&self.root, &self.token);
        if !claims.is_some_and(|c| matches!(c.role.as_str(), "chef" | "admin")) {
            return Err(rxdb::rx_error::new_rx_error("GROK_CONTROL_RETIRED", None));
        }
        publish()
    }
}
struct Retained {
    device: XaiDeviceLogin,
    expires_at: i64,
    authority: Arc<Authority>,
    operation_id: String,
}
struct Controller {
    login: CtoxXaiLogin,
    retained: Mutex<Option<Retained>>,
    slot: Arc<tokio::sync::Semaphore>,
}
impl Controller {
    fn new(root: &Path) -> anyhow::Result<Self> {
        Ok(Self {
            login: CtoxXaiLogin::new(root)?,
            retained: Mutex::new(None),
            slot: Arc::new(tokio::sync::Semaphore::new(1)),
        })
    }
    fn public_login(&self, authority: &Authority) -> anyhow::Result<Value> {
        let state = self
            .retained
            .lock()
            .map_err(|_| anyhow::anyhow!("unavailable"))?;
        let Some(retained) = state.as_ref() else {
            return Ok(Value::Null);
        };
        if retained.authority.token != authority.token {
            return Ok(Value::Null);
        }
        if retained.authority.check().is_err() {
            self.login.cancel(&retained.device.login_id)?;
            return Ok(Value::Null);
        }
        let mut phase = match self.login.poll(&retained.device.login_id)? {
            XaiLoginProgress::Pending => "pending",
            XaiLoginProgress::Accepted => "accepted",
            XaiLoginProgress::Cancelled => "cancelled",
            XaiLoginProgress::Failed => "failed",
        };
        if phase != "accepted" && chrono::Utc::now().timestamp_millis() >= retained.expires_at {
            self.login.cancel(&retained.device.login_id)?;
            phase = "expired";
        }
        Ok(
            json!({"loginId":retained.device.login_id,"phase":phase,"verificationUri":retained.device.verification_uri,"userCode":retained.device.user_code,"expiresAt":retained.expires_at}),
        )
    }
}
fn saved_check(root: &Path) -> anyhow::Result<Value> {
    let saved: Option<Value> = crate::persistence::load_json_payload(root, CHECK_KEY)?;
    let binding = xai::credential_binding(root)?;
    Ok(saved
        .filter(|s| {
            binding
                .as_deref()
                .is_some_and(|b| s["binding"].as_str() == Some(b))
        })
        .map(|s| s["check"].clone())
        .unwrap_or(Value::Null))
}
fn genuine_text(body: &[u8]) -> bool {
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    if v.get("object").and_then(Value::as_str) != Some("response")
        || v.get("status").and_then(Value::as_str) != Some("completed")
    {
        return false;
    }
    v.get("output")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.get("type").and_then(Value::as_str) == Some("message")
                    && item
                        .get("content")
                        .and_then(Value::as_array)
                        .is_some_and(|content| {
                            content.iter().any(|part| {
                                part.get("type").and_then(Value::as_str) == Some("output_text")
                                    && part
                                        .get("text")
                                        .and_then(Value::as_str)
                                        .is_some_and(|text| !text.trim().is_empty())
                            })
                        })
            })
        })
}
async fn probe(root: &Path, model: &str) -> Result<(), &'static str> {
    use ctox_cliproxyapi::sdk::api::handlers::openai::openai_responses_handlers::{
        OpenAiResponsesRouteHandler, OpenAiResponsesRouteResponse,
    };
    let router = crate::execution::cliproxyapi_host::build_instance_codex_responses_router(root)
        .map_err(|_| "request_failed")?
        .ok_or("missing_credential")?;
    let body = serde_json::to_vec(&json!({"model":model,"input":"Hi","stream":false}))
        .map_err(|_| "request_failed")?;
    match router.handle_provider_route(Some("xai"), &body).await {
        OpenAiResponsesRouteResponse::Buffered(response) if response.status() == 200 => {
            if genuine_text(response.body()) {
                Ok(())
            } else {
                Err("invalid_response")
            }
        }
        _ => Err("request_failed"),
    }
}
async fn bounded_current<T>(
    authority: &Authority,
    work: impl std::future::Future<Output = T>,
    duration: Duration,
) -> anyhow::Result<Option<T>> {
    let retirement = async {
        loop {
            authority.check()?;
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };
    tokio::select! { result=tokio::time::timeout(duration,work)=>Ok(result.ok()), result=retirement=>{result?; anyhow::bail!("retired")} }
}
async fn handle(
    controller: Arc<Controller>,
    authority: Arc<Authority>,
    params: Vec<Value>,
) -> anyhow::Result<GuardedAuxiliaryResponse> {
    ensure!(
        params.len() == 1 && serde_json::to_vec(&params)?.len() <= 2048,
        "invalid_request"
    );
    let request: Request = serde_json::from_value(params.into_iter().next().unwrap())?;
    request.validate()?;
    authority.check()?;
    let _permit = controller
        .slot
        .clone()
        .try_acquire_owned()
        .map_err(|_| anyhow::anyhow!("busy"))?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut models = Vec::new();
    let mut catalog_error = None;
    let mut returned_check = saved_check(&authority.root)?;
    if matches!(
        request.action.as_str(),
        "instance.grok.read" | "instance.grok.check" | "instance.grok.poll"
    ) && xai::subscription_installed(&authority.root)
    {
        match bounded_current(
            &authority,
            xai::discover_models(&authority.root),
            Duration::from_secs(8),
        )
        .await?
        {
            Some(Ok(live)) => models = live,
            Some(Err(_)) => catalog_error = Some("request_failed"),
            None => catalog_error = Some("timeout"),
        }
    }
    match request.action.as_str() {
        "instance.grok.start" => {
            let existing = controller
                .retained
                .lock()
                .map_err(|_| anyhow::anyhow!("unavailable"))?
                .as_ref()
                .is_some_and(|r| {
                    r.operation_id == request.operation_id
                        && r.authority.token == authority.token
                        && (r.authority.current)()
                });
            if !existing {
                let commit = authority.clone();
                let watch = authority.clone();
                let device = bounded_current(
                    &authority,
                    controller.login.start_authorized(
                        Arc::new(move || watch.check().is_ok()),
                        Arc::new(move || commit.check()),
                    ),
                    Duration::from_secs(15),
                )
                .await?
                .ok_or_else(|| anyhow::anyhow!("timeout"))??;
                authority.check()?;
                let expires_at = chrono::Utc::now().timestamp_millis() + device.expires_in * 1000;
                *controller
                    .retained
                    .lock()
                    .map_err(|_| anyhow::anyhow!("unavailable"))? = Some(Retained {
                    device,
                    expires_at,
                    authority: authority.clone(),
                    operation_id: request.operation_id.clone(),
                });
            }
        }
        "instance.grok.poll" | "instance.grok.cancel" => {
            let state = controller
                .retained
                .lock()
                .map_err(|_| anyhow::anyhow!("unavailable"))?;
            let retained = state
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("unknown_login"))?;
            ensure!(
                retained.authority.token == authority.token
                    && (retained.authority.current)()
                    && request.login_id.as_deref() == Some(retained.device.login_id.as_str()),
                "unknown_login"
            );
            if request.action == "instance.grok.cancel" {
                controller.login.cancel(&retained.device.login_id)?;
            }
        }
        "instance.grok.remove" => {
            bounded_current(
                &authority,
                controller.login.remove(),
                Duration::from_secs(20),
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("timeout"))??;
            authority.check()?;
            *controller
                .retained
                .lock()
                .map_err(|_| anyhow::anyhow!("unavailable"))? = None;
            crate::persistence::store_json_payload(&authority.root, CHECK_KEY, None::<&Value>)?;
            returned_check = Value::Null;
        }
        "instance.grok.check" => {
            let model = request.model_id.as_deref().unwrap();
            let before = xai::credential_binding(&authority.root)?;
            let started = Instant::now();
            let result = if before.is_none() {
                Err("missing_credential")
            } else if let Some(error) = catalog_error {
                Err(error)
            } else if !models.iter().any(|m| m == model) {
                Err("model_unavailable")
            } else {
                bounded_current(
                    &authority,
                    probe(&authority.root, model),
                    deadline.saturating_duration_since(Instant::now()),
                )
                .await?
                .unwrap_or(Err("timeout"))
            };
            authority.check()?;
            let after = xai::credential_binding(&authority.root)?;
            // The shared route may refresh the credential. Bind to that accepted
            // post-route version, never a renderer-supplied fingerprint.
            ensure!(after.is_some() || before.is_none(), "credential_changed");
            let check = json!({"modelId":model,"status":if result.is_ok(){"ok"}else{"error"},"checkedAt":chrono::Utc::now().timestamp_millis(),"latencyMs":if result.is_ok(){Some(started.elapsed().as_millis() as u64)}else{None},"errorCode":result.err()});
            crate::persistence::store_json_payload(
                &authority.root,
                CHECK_KEY,
                Some(&json!({"binding":after,"check":check})),
            )?;
            returned_check = check;
        }
        _ => {}
    }
    authority.check()?;
    // Discovery can refresh the credential; never publish a cached green check
    // bound to the version that existed before that request.
    if request.action != "instance.grok.check" || returned_check["status"] == "ok" {
        returned_check = saved_check(&authority.root)?;
    }
    Ok(GuardedAuxiliaryResponse {
        result: json!({"version":1,"action":request.action,"operationId":request.operation_id,"installed":xai::subscription_installed(&authority.root),"accountLabel":"Grok Build subscription","login":controller.public_login(&authority)?,"models":models,"check":returned_check}),
        publication: authority,
    })
}
pub(super) fn register(
    pool: &ctox_sync::native::NativePool,
    root: &Path,
) -> rxdb::rx_error::RxResult<()> {
    use rxdb::plugins::replication_webrtc::WebRTCConnectionHandler;
    let controller = Arc::new(
        Controller::new(root)
            .map_err(|_| rxdb::rx_error::new_rx_error("GROK_CONTROL_UNAVAILABLE", None))?,
    );
    let weak_pool = Arc::downgrade(pool);
    let root = root.to_path_buf();
    pool.register_guarded_auxiliary_request_handler(
        METHOD,
        Arc::new(move |peer, token, params| {
            let weak_pool = weak_pool.clone();
            let root = root.clone();
            let controller = controller.clone();
            Box::pin(async move {
                let token_for_check = token.clone();
                let current = Arc::new(move || {
                    weak_pool.upgrade().is_some_and(|p| {
                        !p.canceled.load(Ordering::SeqCst)
                            && p.connection_handler.is_peer_current(&peer)
                            && p.connection_handler.peer_capability_token(&peer).as_deref()
                                == Some(token_for_check.as_str())
                    })
                });
                handle(
                    controller,
                    Arc::new(Authority {
                        root,
                        token,
                        current,
                    }),
                    params,
                )
                .await
                .map_err(|_| "Grok control unavailable, denied, busy or changed".to_owned())
            })
        }),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn shared_route_requires_real_text_and_reset_invalidates_check() -> anyhow::Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for (output, expected) in [
            (
                json!([{ "type":"message","content":[{"type":"output_text","text":"Hi"}]}]),
                Value::Null,
            ),
            (json!([]), json!("invalid_response")),
            (
                json!([{ "type":"message","content":[{"type":"output_text","text":" "}]}]),
                json!("invalid_response"),
            ),
        ] {
            let root = tempfile::tempdir()?;
            let record = json!({"access":"private-fixture","refresh":null,"identity":null,"expires_at":chrono::Utc::now().timestamp()+3600,"token_endpoint":"https://auth.x.ai/token"});
            crate::secrets::write_secret_record(
                root.path(),
                "provider-subscriptions",
                "xai-instance-oauth",
                &record.to_string(),
                None,
                json!({}),
            )?;
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            xai::test_endpoint(
                root.path(),
                Some(format!("http://{}", listener.local_addr()?)),
            );
            let server = tokio::spawn(async move {
                for index in 0..3 {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut data = vec![0; 8192];
                    let n = stream.read(&mut data).await.unwrap();
                    let request = String::from_utf8_lossy(&data[..n]);
                    assert!(request
                        .to_ascii_lowercase()
                        .contains("authorization: bearer private-fixture"));
                    let body = if index < 2 {
                        assert!(request.starts_with("GET /models"));
                        json!({"data":[{"id":"grok-4.7"}]}).to_string()
                    } else {
                        assert!(request.starts_with("POST /responses"));
                        format!(
                            "data: {}\n\n",
                            json!({"type":"response.completed","response":{"object":"response","status":"completed","output":output}})
                        )
                    };
                    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
                }
            });
            let controller = Arc::new(Controller::new(root.path())?);
            let authority = auth(root.path(), "chef", Arc::new(|| true))?;
            let mut req = request("instance.grok.check");
            req["modelId"] = json!("grok-4.7");
            let response = handle(controller, authority, vec![req]).await?.result;
            server.await?;
            xai::test_endpoint(root.path(), None);
            assert_eq!(response["check"]["errorCode"], expected);
            assert_eq!(
                response["check"]["status"],
                if expected.is_null() { "ok" } else { "error" }
            );
            assert!(!response.to_string().contains("private-fixture"));
            assert!(!saved_check(root.path())?.is_null());
            crate::secrets::write_secret_record(
                root.path(),
                "provider-subscriptions",
                "xai-instance-oauth",
                "replacement-fixture",
                None,
                json!({}),
            )?;
            assert!(saved_check(root.path())?.is_null());
        }
        Ok(())
    }
    #[tokio::test]
    async fn credential_replacement_during_discovery_discards_catalog_and_old_check(
    ) -> anyhow::Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let root = tempfile::tempdir()?;
        let authority = auth(root.path(), "chef", Arc::new(|| true))?;
        let record = json!({"access":"private-before","refresh":null,"identity":null,"expires_at":chrono::Utc::now().timestamp()+3600,"token_endpoint":"https://auth.x.ai/token"});
        crate::secrets::write_secret_record(
            root.path(),
            "provider-subscriptions",
            "xai-instance-oauth",
            &record.to_string(),
            None,
            json!({}),
        )?;
        crate::persistence::store_json_payload(
            root.path(),
            CHECK_KEY,
            Some(&json!({"binding":xai::credential_binding(root.path())?,"check":{"status":"ok"}})),
        )?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        xai::test_endpoint(
            root.path(),
            Some(format!("http://{}", listener.local_addr()?)),
        );
        let server_root = root.path().to_path_buf();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut data = [0; 8192];
            let n = stream.read(&mut data).await.unwrap();
            assert!(String::from_utf8_lossy(&data[..n])
                .to_ascii_lowercase()
                .contains("authorization: bearer private-before"));
            let mut changed = record;
            changed["access"] = json!("private-after");
            crate::secrets::write_secret_record(
                &server_root,
                "provider-subscriptions",
                "xai-instance-oauth",
                &changed.to_string(),
                None,
                json!({}),
            )
            .unwrap();
            let body = json!({"data":[{"id":"grok-4.7"}]}).to_string();
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let response = handle(
            Arc::new(Controller::new(root.path())?),
            authority,
            vec![request("instance.grok.read")],
        )
        .await?
        .result;
        server.await?;
        xai::test_endpoint(root.path(), None);
        assert!(response["check"].is_null());
        assert_eq!(response["models"], json!([]));
        assert!(!response.to_string().contains("private-"));
        Ok(())
    }
    #[test]
    fn failed_response_with_text_is_not_accepted() {
        assert!(!genuine_text(br#"{"object":"response","status":"failed","output":[{"type":"message","content":[{"type":"output_text","text":"Partial"}]}]}"#));
        assert!(!genuine_text(br#"{"object":"other","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"Hi"}]}]}"#));
    }
    #[test]
    fn genuine_text_required() {
        for body in [
            b"{}".as_slice(),
            b"invalid",
            br#"{"output":[{"type":"message","content":[{"type":"output_text","text":"  "}]}]}"#,
        ] {
            assert!(!genuine_text(body));
        }
        assert!(genuine_text(
            br#"{"object":"response","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"Hi"}]}]}"#
        ));
    }
    fn auth(
        root: &Path,
        role: &str,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> anyhow::Result<Arc<Authority>> {
        let (token, _) = store::issue_business_os_capability_token_for_managed_user(
            root,
            "grok-operator",
            "Grok operator",
            role,
            chrono::Utc::now().timestamp_millis(),
        )?;
        Ok(Arc::new(Authority {
            root: root.into(),
            token,
            current,
        }))
    }
    fn request(action: &str) -> Value {
        json!({"version":1,"action":action,"operationId":uuid::Uuid::new_v4().to_string()})
    }
    #[tokio::test]
    async fn read_rejects_member_and_retired_owner() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let controller = Arc::new(Controller::new(root.path())?);
        for (role, current) in [("user", true), ("chef", false)] {
            assert!(handle(
                controller.clone(),
                auth(root.path(), role, Arc::new(move || current))?,
                vec![request("instance.grok.read")]
            )
            .await
            .is_err());
        }
        let response = handle(
            controller,
            auth(root.path(), "chef", Arc::new(|| true))?,
            vec![request("instance.grok.read")],
        )
        .await?
        .result;
        assert_eq!(response["installed"], false);
        assert!(response["check"].is_null());
        assert_eq!(response["models"], json!([]));
        Ok(())
    }
    #[tokio::test]
    async fn timeout_and_retirement_drop_work() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let current = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let c = current.clone();
        let auth = auth(
            root.path(),
            "chef",
            Arc::new(move || c.load(Ordering::SeqCst)),
        )?;
        assert!(bounded_current(
            &auth,
            std::future::pending::<()>(),
            Duration::from_millis(1)
        )
        .await?
        .is_none());
        current.store(false, Ordering::SeqCst);
        assert!(
            bounded_current(&auth, std::future::pending::<()>(), Duration::from_secs(1))
                .await
                .is_err()
        );
        Ok(())
    }
    #[test]
    fn correlation_fields_and_action_shape_are_exact() {
        let mut v = request("instance.grok.poll");
        assert!(serde_json::from_value::<Request>(v.clone())
            .unwrap()
            .validate()
            .is_err());
        v["loginId"] = json!(uuid::Uuid::new_v4().to_string());
        assert!(serde_json::from_value::<Request>(v.clone())
            .unwrap()
            .validate()
            .is_ok());
        v["secret"] = json!("private");
        assert!(serde_json::from_value::<Request>(v).is_err());
    }
}
