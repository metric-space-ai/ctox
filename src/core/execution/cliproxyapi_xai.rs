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

/// Private observation of the credential actually loaded by the native route.
/// It grants no execution authority and has no browser/JSON representation.
pub(crate) struct CredentialUse {
    version: String,
}
impl CredentialUse {
    pub(crate) fn into_content_version(self) -> String {
        self.version
    }
}
struct CredentialObserver {
    root: PathBuf,
    observed: Option<CredentialUse>,
    ambiguous: bool,
}
tokio::task_local! {
    static CREDENTIAL_OBSERVER: Mutex<CredentialObserver>;
}
pub(crate) async fn observe_credential_use<T>(
    root: &Path,
    work: impl std::future::Future<Output = T>,
) -> (T, Option<CredentialUse>) {
    CREDENTIAL_OBSERVER.scope(
        Mutex::new(CredentialObserver {
            root: root.to_owned(),
            observed: None,
            ambiguous: false,
        }),
        async {
            let result = work.await;
            let observed = CREDENTIAL_OBSERVER.with(|slot| {
                let mut slot = slot.lock().ok()?;
                if slot.ambiguous { None } else { slot.observed.take() }
            });
            (result, observed)
        },
    ).await
}
fn record_credential_use(root: &Path, version: &str) {
    let _ = CREDENTIAL_OBSERVER.try_with(|slot| {
        if let Ok(mut slot) = slot.lock() {
            if slot.root != root
                || slot.observed.as_ref().is_some_and(|observed| observed.version != version)
            {
                slot.ambiguous = true;
                slot.observed = None;
            } else if !slot.ambiguous {
                slot.observed = Some(CredentialUse { version: version.to_owned() });
            }
        }
    });
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
                            if authorize_commit().is_ok() && install_bundle(&root, &bundle).is_ok()
                            {
                                XaiLoginProgress::Accepted
                            } else {
                                XaiLoginProgress::Failed
                            }
                        }
                        _ if cancel.is_cancelled() || !current() => XaiLoginProgress::Cancelled,
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

/// Direct local CLI, using the existing host-operator/root-key boundary.
/// This is not a Business OS actor or a remote authorization endpoint.
pub fn handle_operator_login(root: &Path) -> anyhow::Result<()> {
    let controller = CtoxXaiLogin::new(root)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let phase = runtime.block_on(operator_login(
        &controller,
        |device| {
            use std::io::Write;
            let mut output = std::io::stdout().lock();
            writeln!(output, "{}", serde_json::to_string(device)?)?;
            output.flush()?;
            Ok(())
        },
        operator_shutdown(),
    ))?;
    let status = match phase {
        XaiLoginProgress::Pending => "pending",
        XaiLoginProgress::Accepted => "accepted",
        XaiLoginProgress::Cancelled => "cancelled",
        XaiLoginProgress::Failed => "failed",
    };
    println!(
        "{}",
        serde_json::json!({"phase":status,"account_id":ACCOUNT_ID})
    );
    anyhow::ensure!(
        phase == XaiLoginProgress::Accepted,
        "Grok login did not complete"
    );
    Ok(())
}

async fn operator_shutdown() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result,
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await
}

async fn operator_login(
    controller: &CtoxXaiLogin,
    mut publish: impl FnMut(&XaiDeviceLogin) -> anyhow::Result<()>,
    shutdown: impl std::future::Future<Output = std::io::Result<()>>,
) -> anyhow::Result<XaiLoginProgress> {
    tokio::pin!(shutdown);
    let device = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(35), controller.start()) =>
            result.map_err(|_| anyhow::anyhow!("Grok device authorization timed out"))??,
        result = &mut shutdown => {
            result?;
            controller.cancel_all();
            return Ok(XaiLoginProgress::Cancelled);
        }
    };
    publish(&device)?;
    let expires = Duration::from_secs(device.expires_in.clamp(1, 1800) as u64);
    let outcome = async {
        loop {
            let phase = controller.poll(&device.login_id)?;
            if phase != XaiLoginProgress::Pending {
                return Ok::<_, anyhow::Error>(phase);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    };
    tokio::select! {
        result = tokio::time::timeout(expires, outcome) => match result {
            Ok(result) => result,
            Err(_) => {
                controller.cancel(&device.login_id)?;
                anyhow::bail!("Grok device authorization expired");
            }
        },
        result = &mut shutdown => {
            result?;
            controller.cancel(&device.login_id)?;
            Ok(XaiLoginProgress::Cancelled)
        }
    }
}

pub fn handle_operator_models(root: &Path) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let models = runtime
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(8), discover_models(root)).await
        })
        .map_err(|_| anyhow::anyhow!("Grok catalog request timed out"))?;
    // Never print an upstream error body, credential or token.
    let models = models.map_err(|_| anyhow::anyhow!("Grok catalog request failed"))?;
    println!(
        "{}",
        serde_json::json!({"account_id":ACCOUNT_ID,"models":models})
    );
    Ok(())
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
fn stored_bundle(bundle: &AuthBundle) -> Stored {
    Stored {
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
    }
}
// Device authorization adds an account. It must never replace an account
// installed by another native controller or local operator in the meantime.
fn install_bundle(root: &Path, bundle: &AuthBundle) -> anyhow::Result<()> {
    let encoded = Zeroizing::new(serde_json::to_string(&stored_bundle(bundle))?);
    let committed = crate::secrets::compare_and_write_secret_records(
        root,
        &[(SCOPE, NAME, None)],
        &[crate::secrets::SecretRecordWrite {
            scope: SCOPE,
            name: NAME,
            value: &encoded,
            description: Some("Grok subscription OAuth"),
            metadata: serde_json::json!({"provider":"xai"}),
        }],
        &[],
    )?;
    anyhow::ensure!(committed, "Grok subscription already installed");
    Ok(())
}
fn save_bundle(root: &Path, bundle: &AuthBundle) -> anyhow::Result<()> {
    let mut record = stored_bundle(bundle);
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
async fn active_record(root: &Path, client: &native_http::Client) -> anyhow::Result<(Stored, String)> {
    let (encoded, mut version) = crate::secrets::read_secret_value_versioned(root, SCOPE, NAME)?;
    let encoded = Zeroizing::new(encoded);
    let mut record: Stored = serde_json::from_str(&encoded)?;
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
        let (refreshed, refreshed_version) =
            crate::secrets::read_secret_value_versioned(root, SCOPE, NAME)?;
        record = serde_json::from_str(&Zeroizing::new(refreshed))?;
        version = refreshed_version;
    }
    Ok((record, version))
}
pub async fn discover_models(root: &Path) -> anyhow::Result<Vec<String>> {
    use ctox_cliproxyapi::internal::runtime::executor::xai_executor_request::apply_xai_chat_headers;
    use ctox_cliproxyapi::sdk::cliproxy::auth::Auth;
    let _guard = AUTH_USE.lock().await;
    let client = native_http::Client::builder()
        .redirect(native_http::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()?;
    let (record, catalog_binding) = active_record(root, &client).await?;
    record_credential_use(root, &catalog_binding);
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
    let bytes =
        bounded_response(request.send().await?, 1024 * 1024, UpstreamStage::Catalog).await?;
    anyhow::ensure!(
        credential_binding(root)?.as_deref() == Some(catalog_binding.as_str()),
        "Grok credential changed during discovery"
    );
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
        Err(error) => Route::Buffered(route_failure(&error)),
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
    let client = native_http::Client::builder()
        .redirect(native_http::redirect::Policy::none())
        .timeout(Duration::from_secs(60))
        .build()?;
    let (record, route_binding) = active_record(root, &client).await?;
    record_credential_use(root, &route_binding);
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
    let catalog = bounded_response(
        catalog_request.send().await?,
        1024 * 1024,
        UpstreamStage::Catalog,
    )
    .await?;
    anyhow::ensure!(
        credential_binding(root)?.as_deref() == Some(route_binding.as_str()),
        "Grok credential changed during route discovery"
    );
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
    let data = bounded_response(
        upstream.send().await?,
        32 * 1024 * 1024,
        UpstreamStage::Inference,
    )
    .await;
    anyhow::ensure!(
        credential_binding(root)?.as_deref() == Some(route_binding.as_str()),
        "Grok credential changed during inference"
    );
    let data = data?;
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
/// Created only after an actual HTTP response, never for credential loading,
/// request construction, transport, body-limit or response decoding failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UpstreamStage {
    Catalog,
    Inference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpstreamHttpRejection {
    pub(crate) status: u16,
    pub(crate) stage: UpstreamStage,
}

impl std::fmt::Display for UpstreamHttpRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let stage = match self.stage {
            UpstreamStage::Catalog => "catalog",
            UpstreamStage::Inference => "inference",
        };
        write!(
            formatter,
            "Grok {stage} request rejected: HTTP {}",
            self.status
        )
    }
}
impl std::error::Error for UpstreamHttpRejection {}

fn route_failure(
    error: &anyhow::Error,
) -> ctox_cliproxyapi::sdk::api::handlers::openai::openai_responses_handlers::OpenAiResponsesHttpResponse{
    use ctox_cliproxyapi::sdk::api::handlers::openai::openai_responses_handlers::OpenAiResponsesHttpResponse as Response;
    if let Some(rejection) = error.downcast_ref::<UpstreamHttpRejection>() {
        // Do not forward the provider body, URL or credentials. This envelope is
        // produced here, rather than inferred from the generic gateway status.
        Response::json(
            rejection.status,
            serde_json::json!({"error":{
                "type":"grok_upstream_http_error",
                "message":"Grok upstream rejected the request",
                "upstream":rejection
            }})
            .to_string()
            .into_bytes(),
        )
    } else {
        Response::error(502, "Grok subscription request failed")
    }
}

pub(crate) fn route_rejection(status: u16, body: &[u8]) -> Option<UpstreamHttpRejection> {
    let envelope: serde_json::Value = serde_json::from_slice(body).ok()?;
    let error = envelope.get("error")?;
    if error.get("type")?.as_str()? != "grok_upstream_http_error" {
        return None;
    }
    let rejection: UpstreamHttpRejection =
        serde_json::from_value(error.get("upstream")?.clone()).ok()?;
    ((300..600).contains(&status) && rejection.status == status).then_some(rejection)
}

async fn bounded_response(
    response: native_http::Response,
    limit: usize,
    stage: UpstreamStage,
) -> anyhow::Result<Vec<u8>> {
    if !response.status().is_success() {
        return Err(UpstreamHttpRejection {
            status: response.status().as_u16(),
            stage,
        }
        .into());
    }
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
