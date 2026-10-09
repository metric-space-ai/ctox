// Origin: CTOX
// License: AGPL-3.0-only
//! Instance-owned Grok device login. Only public progress crosses this seam.
#[cfg(test)]
#[path = "cliproxyapi_xai_tests.rs"]
mod tests;
use ctox_cliproxyapi::internal::auth::xai::*;
use ctox_cliproxyapi::sdk::auth::LoginCancellation;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use zeroize::Zeroizing;

static AUTH_USE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const SCOPE: &str = "provider-subscriptions";
const NAME: &str = "xai-instance-oauth";
pub const ACCOUNT_ID: &str = "xai-instance-primary";
pub fn subscription_installed(root: &Path) -> bool {
    crate::secrets::secret_exists(root, SCOPE, NAME).unwrap_or(false)
}
pub(crate) fn credential_binding(root: &Path) -> anyhow::Result<Option<String>> {
    crate::secrets::secret_record_content_version(root, SCOPE, NAME)
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum XaiLoginProgress {
    Pending,
    Accepted,
    Cancelled,
    Failed,
}
#[derive(Debug, Clone, Serialize)]
pub struct XaiDeviceLogin {
    pub login_id: String,
    pub verification_uri: String,
    pub user_code: String,
    pub expires_in: i64,
}
struct Login {
    cancel: LoginCancellation,
    progress: XaiLoginProgress,
}
struct PendingStart {
    sessions: Arc<Mutex<HashMap<String, Login>>>,
    id: String,
    transferred: bool,
}
impl Drop for PendingStart {
    fn drop(&mut self) {
        if self.transferred {
            return;
        }
        if let Ok(mut sessions) = self.sessions.lock() {
            if let Some(login) = sessions.get_mut(&self.id) {
                if login.progress == XaiLoginProgress::Pending {
                    login.cancel.cancel();
                    login.progress = XaiLoginProgress::Cancelled;
                }
            }
        }
    }
}
/// Kept by the authorized native controller, never by a renderer. Drop cancels
/// all outstanding polls. At most one pending login exists per instance.
pub struct CtoxXaiLogin {
    root: PathBuf,
    auth: Arc<XaiAuth>,
    sessions: Arc<Mutex<HashMap<String, Login>>>,
}
impl CtoxXaiLogin {
    pub fn new(root: &Path) -> anyhow::Result<Self> {
        Ok(Self::with_auth(
            root,
            Arc::new(XaiAuth::new(
                Arc::new(LoginTransport(
                    native_http::Client::builder()
                        .redirect(native_http::redirect::Policy::none())
                        .build()?,
                )),
                Arc::new(SystemXaiClock),
                Arc::new(XaiRefreshCoordinator::default()),
            )),
        ))
    }
    fn with_auth(root: &Path, auth: Arc<XaiAuth>) -> Self {
        Self {
            root: root.into(),
            auth,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub async fn start(&self) -> anyhow::Result<XaiDeviceLogin> {
        self.start_authorized(Arc::new(|| true), Arc::new(|| Ok(())))
            .await
    }
    /// The native caller owns current-peer admission and must revalidate the
    /// admitted token immediately before the encrypted credential commit.
    pub async fn start_authorized(
        &self,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
        authorize_commit: Arc<dyn Fn() -> anyhow::Result<()> + Send + Sync>,
    ) -> anyhow::Result<XaiDeviceLogin> {
        authorize_commit()?;
        anyhow::ensure!(
            !crate::secrets::secret_exists(&self.root, SCOPE, NAME)?,
            "Grok subscription already installed"
        );
        // Serial admission includes discovery; overlapping starts cannot orphan a poll.
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = LoginCancellation::default();
        {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| anyhow::anyhow!("login unavailable"))?;
            anyhow::ensure!(
                !sessions
                    .values()
                    .any(|s| s.progress == XaiLoginProgress::Pending),
                "Grok login already pending"
            );
            sessions.clear();
            sessions.insert(
                id.clone(),
                Login {
                    cancel: cancel.clone(),
                    progress: XaiLoginProgress::Pending,
                },
            );
        }
        let mut pending = PendingStart {
            sessions: self.sessions.clone(),
            id: id.clone(),
            transferred: false,
        };
        let code = match self.auth.start_device_flow(&cancel).await {
            Ok(code) => code,
            Err(_) => {
                self.set_progress(&id, XaiLoginProgress::Failed);
                anyhow::bail!("Grok device authorization failed")
            }
        };
        let public = XaiDeviceLogin {
            login_id: id.clone(),
            verification_uri: if code.verification_uri_complete.is_empty() {
                code.verification_uri.clone()
            } else {
                code.verification_uri_complete.clone()
            },
            user_code: code.user_code.clone(),
            expires_in: code.expires_in.clamp(0, 1800),
        };
        let auth = self.auth.clone();
        let sessions = self.sessions.clone();
        let root = self.root.clone();
        tokio::spawn(async move {
            let expires = Duration::from_secs(code.expires_in.clamp(1, 1800) as u64);
            let retirement = async {
                loop {
                    if !current() {
                        cancel.cancel();
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            };
            let result = tokio::select! {
                result = tokio::time::timeout(expires, auth.wait_for_authorization(&cancel, &code)) => Some(result),
                () = retirement => None,
            };
            if let Ok(mut state) = sessions.lock() {
                if let Some(login) = state.get_mut(&id) {
                    if login.progress != XaiLoginProgress::Pending {
                        return;
                    }
                    login.progress = match result {
                        Some(Ok(Ok(bundle))) if !cancel.is_cancelled() && current() => {
                            if authorize_commit().is_ok() && save_bundle(&root, &bundle).is_ok() {
                                XaiLoginProgress::Accepted
                            } else {
                                XaiLoginProgress::Failed
                            }
                        }
                        None => XaiLoginProgress::Cancelled,
                        _ => XaiLoginProgress::Failed,
                    };
                }
            }
        });
        pending.transferred = true;
        Ok(public)
    }
    pub fn cancel_all(&self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            for login in sessions.values_mut() {
                login.cancel.cancel();
                if login.progress == XaiLoginProgress::Pending {
                    login.progress = XaiLoginProgress::Cancelled;
                }
            }
        }
    }
    pub async fn remove(&self) -> anyhow::Result<()> {
        self.cancel_all();
        let _guard = AUTH_USE.lock().await;
        crate::secrets::delete_secret_record(&self.root, SCOPE, NAME)
    }
    fn set_progress(&self, id: &str, value: XaiLoginProgress) {
        if let Ok(mut sessions) = self.sessions.lock() {
            if let Some(login) = sessions.get_mut(id) {
                login.progress = value;
            }
        }
    }
    pub fn poll(&self, id: &str) -> anyhow::Result<XaiLoginProgress> {
        self.sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("login unavailable"))?
            .get(id)
            .map(|s| s.progress.clone())
            .ok_or_else(|| anyhow::anyhow!("unknown Grok login"))
    }
    pub fn cancel(&self, id: &str) -> anyhow::Result<XaiLoginProgress> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("login unavailable"))?;
        let login = sessions
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("unknown Grok login"))?;
        if login.progress == XaiLoginProgress::Pending {
            login.cancel.cancel();
            login.progress = XaiLoginProgress::Cancelled;
        }
        Ok(login.progress.clone())
    }
}
impl Drop for CtoxXaiLogin {
    fn drop(&mut self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            for login in sessions.values_mut() {
                login.cancel.cancel();
                if login.progress == XaiLoginProgress::Pending {
                    login.progress = XaiLoginProgress::Cancelled;
                }
            }
        }
    }
}
struct LoginTransport(native_http::Client);
impl XaiHttpTransport for LoginTransport {
    fn execute<'a>(
        &'a self,
        request: &'a XaiHttpRequest,
        timeout: Duration,
        cancellation: &'a LoginCancellation,
    ) -> XaiHttpFuture<'a> {
        Box::pin(async move {
            let operation = async {
                let mut builder = self
                    .0
                    .request(
                        match request.method {
                            XaiHttpMethod::Get => native_http::Method::GET,
                            XaiHttpMethod::Post => native_http::Method::POST,
                        },
                        &request.url,
                    )
                    .timeout(timeout)
                    .body(request.body.to_vec());
                for (key, value) in &request.headers {
                    builder = builder.header(key, value);
                }
                let response = builder
                    .send()
                    .await
                    .map_err(|_| XaiTransportFailure::Protocol)?;
                let status = response.status().as_u16();
                use futures_util::StreamExt as _;
                let mut response = response.bytes_stream();
                let mut body = Vec::new();
                while let Some(chunk) = response
                    .next()
                    .await
                    .transpose()
                    .map_err(|_| XaiTransportFailure::Protocol)?
                {
                    if body.len().saturating_add(chunk.len()) > 1024 * 1024 {
                        return Err(XaiTransportFailure::Protocol);
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(XaiHttpResponse::new(status, body))
            };
            tokio::select! { result = operation => result, () = cancellation.cancelled() => Err(XaiTransportFailure::Cancelled) }
        })
    }
}
// This entire record is encrypted; even identity/refresh metadata never enters
// a replicated collection or debug formatter.
#[derive(Serialize, Deserialize)]
struct Stored {
    access: String,
    refresh: Option<String>,
    identity: Option<String>,
    expires_at: Option<u64>,
    token_endpoint: String,
}
fn save_bundle(root: &Path, bundle: &AuthBundle) -> anyhow::Result<()> {
    let mut record = Stored {
        access: bundle.token_data.access_token().expose_secret().into(),
        refresh: bundle
            .token_data
            .refresh_token()
            .map(|v| v.expose_secret().into()),
        identity: bundle
            .token_data
            .id_token()
            .map(|v| v.expose_secret().into()),
        expires_at: bundle
            .token_data
            .expires_at()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs()),
        token_endpoint: bundle.token_endpoint.clone(),
    };
    // Refresh responses may omit an unchanged refresh/identity token.
    if let Ok(previous) = crate::secrets::read_secret_value(root, SCOPE, NAME) {
        let previous = Zeroizing::new(previous);
        if let Ok(prior) = serde_json::from_str::<Stored>(&previous) {
            if record.refresh.is_none() {
                record.refresh = prior.refresh.clone();
            }
            if record.identity.is_none() {
                record.identity = prior.identity.clone();
            }
        }
    }
    let encoded = Zeroizing::new(serde_json::to_string(&record)?);
    crate::secrets::write_secret_record(
        root,
        SCOPE,
        NAME,
        &encoded,
        Some("Grok subscription OAuth".into()),
        serde_json::json!({"provider":"xai"}),
    )?;
    Ok(())
}

/// Fetch the actual account catalog for native settings/control callers.
#[cfg(test)]
static TEST_ENDPOINTS: Mutex<Option<HashMap<PathBuf, String>>> = Mutex::new(None);
#[cfg(test)]
pub(crate) fn test_endpoint(root: &Path, endpoint: Option<String>) {
    let mut endpoints = TEST_ENDPOINTS.lock().unwrap();
    let map = endpoints.get_or_insert_with(HashMap::new);
    if let Some(endpoint) = endpoint {
        map.insert(root.into(), endpoint);
    } else {
        map.remove(root);
    }
}
fn endpoint(root: &Path) -> String {
    #[cfg(test)]
    if let Some(endpoint) = TEST_ENDPOINTS
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(root))
        .cloned()
    {
        return endpoint;
    }
    let _ = root;
    CLI_CHAT_PROXY_BASE_URL.into()
}
pub async fn discover_models(root: &Path) -> anyhow::Result<Vec<String>> {
    use ctox_cliproxyapi::internal::runtime::executor::xai_executor_request::apply_xai_chat_headers;
    use ctox_cliproxyapi::sdk::cliproxy::auth::Auth;
    let encoded = Zeroizing::new(crate::secrets::read_secret_value(root, SCOPE, NAME)?);
    let record: Stored = serde_json::from_str(&encoded)?;
    let client = native_http::Client::builder()
        .redirect(native_http::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()?;
    let mut auth = Auth::default();
    auth.attributes.insert("auth_kind".into(), "oauth".into());
    auth.attributes
        .insert("base_url".into(), CLI_CHAT_PROXY_BASE_URL.into());
    let mut headers = std::collections::BTreeMap::new();
    apply_xai_chat_headers(&mut headers, Some(&auth), &record.access, false, "");
    let mut request = client.get(format!("{}/models", endpoint(root)));
    for (key, values) in headers {
        for value in values {
            request = request.header(&key, value);
        }
    }
    let bytes = bounded_response(request.send().await?, 1024 * 1024).await?;
    let catalog: serde_json::Value = serde_json::from_slice(&bytes)?;
    let rows = catalog
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("invalid Grok catalog"))?;
    Ok(rows
        .iter()
        .filter_map(|row| row.get("id").and_then(serde_json::Value::as_str))
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Explicit subscription route; default provider configuration is never changed.
/// Model membership is checked against the authenticated live catalog before use.
pub async fn handle_route(root: &Path, body: &[u8]) -> ctox_cliproxyapi::sdk::api::handlers::openai::openai_responses_handlers::OpenAiResponsesRouteResponse{
    use ctox_cliproxyapi::sdk::api::handlers::openai::openai_responses_handlers::{
        OpenAiResponsesHttpResponse as Response, OpenAiResponsesRouteResponse as Route,
    };
    match execute_route(root, body).await {
        Ok((stream, data)) => Route::Buffered(if stream {
            Response::event_stream(200, data)
        } else {
            Response::json(200, data)
        }),
        Err(_) => Route::Buffered(Response::error(502, "Grok subscription request failed")),
    }
}
async fn execute_route(root: &Path, body: &[u8]) -> anyhow::Result<(bool, Vec<u8>)> {
    execute_route_at(root, body, &endpoint(root)).await
}
async fn execute_route_at(
    root: &Path,
    body: &[u8],
    endpoint: &str,
) -> anyhow::Result<(bool, Vec<u8>)> {
    use ctox_cliproxyapi::internal::runtime::executor::xai_executor_request::{
        apply_xai_chat_headers, prepare_xai_responses_body, XaiRequestPolicy,
    };
    use ctox_cliproxyapi::sdk::cliproxy::auth::Auth;
    // Serialize native refresh/use, including encrypted writeback, so two calls
    // cannot rotate the same refresh token concurrently.
    let _guard = AUTH_USE.lock().await;
    let requested = ctox_cliproxyapi::internal::api::account_selection::requested_account();
    anyhow::ensure!(
        requested.as_deref().is_none_or(|id| id == ACCOUNT_ID),
        "requested Grok account unavailable"
    );
    ctox_cliproxyapi::internal::api::account_selection::record_selected(ACCOUNT_ID);
    let encoded = Zeroizing::new(crate::secrets::read_secret_value(root, SCOPE, NAME)?);
    let mut record: Stored = serde_json::from_str(&encoded)?;
    let client = native_http::Client::builder()
        .redirect(native_http::redirect::Policy::none())
        .timeout(Duration::from_secs(60))
        .build()?;
    if record.expires_at.is_some_and(|expiry| {
        expiry
            <= std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                + 300
    }) {
        let auth = XaiAuth::new(
            Arc::new(LoginTransport(client.clone())),
            Arc::new(SystemXaiClock),
            Arc::new(XaiRefreshCoordinator::default()),
        );
        let token = auth
            .refresh_tokens(
                SecretString::new(
                    record
                        .refresh
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("refresh unavailable"))?,
                )?,
                Some(&record.token_endpoint),
            )
            .await
            .map_err(|_| anyhow::anyhow!("Grok refresh failed"))?;
        let bundle = AuthBundle {
            token_data: token,
            last_refresh: std::time::SystemTime::now(),
            base_url: CLI_CHAT_PROXY_BASE_URL.into(),
            redirect_uri: String::new(),
            token_endpoint: record.token_endpoint.clone(),
        };
        save_bundle(root, &bundle)?;
        record = serde_json::from_str(&Zeroizing::new(crate::secrets::read_secret_value(
            root, SCOPE, NAME,
        )?))?;
    }
    let request: serde_json::Value = serde_json::from_slice(body)?;
    let model = request
        .get("model")
        .and_then(serde_json::Value::as_str)
        .filter(|m| !m.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("model required"))?;
    let mut auth = Auth::default();
    auth.attributes.insert("auth_kind".into(), "oauth".into());
    auth.attributes
        .insert("base_url".into(), CLI_CHAT_PROXY_BASE_URL.into());
    let mut headers = std::collections::BTreeMap::new();
    apply_xai_chat_headers(&mut headers, Some(&auth), &record.access, false, "");
    let mut catalog_request = client.get(format!("{endpoint}/models"));
    for (key, values) in &headers {
        for value in values {
            catalog_request = catalog_request.header(key, value);
        }
    }
    let catalog = bounded_response(catalog_request.send().await?, 1024 * 1024).await?;
    let catalog: serde_json::Value = serde_json::from_slice(&catalog)?;
    anyhow::ensure!(
        catalog
            .get("data")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|rows| rows
                .iter()
                .any(|row| row.get("id").and_then(serde_json::Value::as_str) == Some(model))),
        "model absent from authenticated catalog"
    );
    let stream = request
        .get("stream")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let prepared = prepare_xai_responses_body(
        body,
        XaiRequestPolicy {
            model,
            stream: true,
            ..XaiRequestPolicy::default()
        },
    )
    .map_err(|_| anyhow::anyhow!("invalid Grok request"))?;
    let mut headers = std::collections::BTreeMap::new();
    apply_xai_chat_headers(
        &mut headers,
        Some(&auth),
        &record.access,
        true,
        &prepared.session_id,
    );
    let mut upstream = client
        .post(format!("{endpoint}/responses"))
        .body(prepared.body);
    for (key, values) in headers {
        for value in values {
            upstream = upstream.header(&key, value);
        }
    }
    let data = bounded_response(upstream.send().await?, 32 * 1024 * 1024).await?;
    let data = if stream {
        data
    } else {
        completed_response(&data)?
    };
    Ok((stream, data))
}
fn completed_response(body: &[u8]) -> anyhow::Result<Vec<u8>> {
    for line in body.split(|byte| *byte == b'\n') {
        if let Some(data) = line.strip_prefix(b"data:") {
            if let Ok(event) = serde_json::from_slice::<serde_json::Value>(data) {
                if event.get("type").and_then(serde_json::Value::as_str)
                    == Some("response.completed")
                {
                    let response = event
                        .get("response")
                        .filter(|value| value.is_object())
                        .ok_or_else(|| anyhow::anyhow!("invalid Grok response"))?;
                    return Ok(serde_json::to_vec(response)?);
                }
            }
        }
    }
    anyhow::bail!("Grok response did not complete")
}
async fn bounded_response(
    response: native_http::Response,
    limit: usize,
) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        response.status().is_success(),
        "Grok upstream rejected request"
    );
    use futures_util::StreamExt as _;
    let mut response = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = response.next().await.transpose()? {
        anyhow::ensure!(
            body.len().saturating_add(chunk.len()) <= limit,
            "Grok response too large"
        );
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
impl Drop for Stored {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.access.zeroize();
        self.refresh.zeroize();
        self.identity.zeroize();
    }
}
