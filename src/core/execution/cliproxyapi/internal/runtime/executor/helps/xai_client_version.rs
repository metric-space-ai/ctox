// ref: internal/helps/xai_client_version.go:19-220 @ d318bcc3afb9ea8782862f5e8cdb541afdc9dfa5
// Port-Status: adapted_to_ctox — host-owned task and anonymous HTTP capability
// License: MIT (upstream); modifications AGPL-3.0-only

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

pub const DEFAULT_XAI_FALLBACK_CLIENT_VERSION: &str = "1.0.46";
pub const XAI_CLIENT_VERSION_SERVER_FLOOR: &str = "1.0.13";
pub const XAI_VERSION_REFRESH_INTERVAL: Duration = Duration::from_secs(3 * 60 * 60);
pub const XAI_VERSION_FETCH_TIMEOUT: Duration = Duration::from_secs(10);
pub const XAI_VERSION_MAX_BODY_BYTES: usize = 1 << 20;
pub const XAI_NPM_REGISTRY_URL: &str = "https://registry.npmjs.org/@xai-official/grok/latest";

pub type VersionFetchFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<u8>, VersionFetchError>> + Send + 'a>>;

/// Anonymous npm metadata only. Implementations must bound the body while
/// receiving it; no provider token, account identity or model prompt is passed.
pub trait XaiVersionFetcher: Send + Sync {
    fn fetch(&self) -> VersionFetchFuture<'_>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VersionFetchError {
    Transport,
    HttpStatus,
    BodyTooLarge,
    Malformed,
}

impl std::fmt::Display for VersionFetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Transport => "Grok client version metadata request failed",
            Self::HttpStatus => "Grok client version registry rejected the request",
            Self::BodyTooLarge => "Grok client version metadata exceeds its size limit",
            Self::Malformed => "Grok client version metadata is invalid",
        })
    }
}
impl std::error::Error for VersionFetchError {}

struct VersionState {
    version: String,
    active: Option<(Arc<()>, tokio::task::AbortHandle)>,
}

/// One cached value and one refresh owner, matching upstream's process-wide
/// reader. Separate values are used by isolated tests; production shares one.
pub struct XaiClientVersion {
    state: Mutex<VersionState>,
}

impl Default for XaiClientVersion {
    fn default() -> Self {
        Self {
            state: Mutex::new(VersionState {
                version: DEFAULT_XAI_FALLBACK_CLIENT_VERSION.to_owned(),
                active: None,
            }),
        }
    }
}

impl XaiClientVersion {
    pub fn current(&self) -> String {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .version
            .clone()
    }

    /// A new owner cancels the previous task, including an in-flight fetch.
    /// The previous owner's delayed Drop cannot cancel this replacement.
    pub fn start(self: &Arc<Self>, fetcher: Arc<dyn XaiVersionFetcher>) -> XaiVersionUpdater {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((_, previous)) = state.active.take() {
            previous.abort();
        }
        let generation = Arc::new(());
        let task_generation = Arc::clone(&generation);
        let cache = Arc::clone(self);
        let task = tokio::spawn(async move {
            refresh(&cache, &task_generation, &*fetcher).await;
            // A failed fetch keeps the last accepted value; it does not retry.
            let mut ticker = tokio::time::interval_at(
                tokio::time::Instant::now() + XAI_VERSION_REFRESH_INTERVAL,
                XAI_VERSION_REFRESH_INTERVAL,
            );
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                refresh(&cache, &task_generation, &*fetcher).await;
            }
        });
        state.active = Some((Arc::clone(&generation), task.abort_handle()));
        XaiVersionUpdater {
            cache: Arc::clone(self),
            generation,
            task: Some(task),
        }
    }
}

async fn refresh(cache: &XaiClientVersion, generation: &Arc<()>, fetcher: &dyn XaiVersionFetcher) {
    let fetched = tokio::time::timeout(XAI_VERSION_FETCH_TIMEOUT, fetcher.fetch()).await;
    if let Ok(Ok(body)) = fetched {
        if let Ok(version) = parse_version(&body) {
            let mut state = cache
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state
                .active
                .as_ref()
                .is_some_and(|(active, _)| Arc::ptr_eq(active, generation))
            {
                state.version = version;
            }
        }
    }
}

pub struct XaiVersionUpdater {
    cache: Arc<XaiClientVersion>,
    generation: Arc<()>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl XaiVersionUpdater {
    fn cancel(&self) {
        let mut state = self
            .cache
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state
            .active
            .as_ref()
            .is_some_and(|(active, _)| Arc::ptr_eq(active, &self.generation))
        {
            if let Some((_, active)) = state.active.take() {
                active.abort();
            }
        }
        // Also abort our own retired task, without touching its replacement.
        if let Some(task) = &self.task {
            task.abort();
        }
    }

    pub async fn stop(mut self) {
        self.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for XaiVersionUpdater {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn shared_cache() -> &'static Arc<XaiClientVersion> {
    static CACHE: OnceLock<Arc<XaiClientVersion>> = OnceLock::new();
    CACHE.get_or_init(|| Arc::new(XaiClientVersion::default()))
}

pub fn get_xai_client_version() -> String {
    shared_cache().current()
}

pub fn start_xai_version_updater(fetcher: Arc<dyn XaiVersionFetcher>) -> XaiVersionUpdater {
    shared_cache().start(fetcher)
}

fn strict_version(raw: &str) -> Option<[i64; 3]> {
    let parts: Vec<_> = raw.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let mut version = [0; 3];
    for (i, part) in parts.into_iter().enumerate() {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        version[i] = part.parse().ok()?;
    }
    Some(version)
}

fn parse_version(body: &[u8]) -> Result<String, VersionFetchError> {
    if body.len() > XAI_VERSION_MAX_BODY_BYTES {
        return Err(VersionFetchError::BodyTooLarge);
    }
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| VersionFetchError::Malformed)?;
    let raw = value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or(VersionFetchError::Malformed)?
        .trim();
    let parsed = strict_version(raw).ok_or(VersionFetchError::Malformed)?;
    let floor =
        strict_version(XAI_CLIENT_VERSION_SERVER_FLOOR).expect("fixed numeric server floor");
    if parsed < floor {
        return Err(VersionFetchError::Malformed);
    }
    Ok(raw.to_owned())
}

#[cfg(any(
    feature = "anthropic-fingerprint-transport",
    feature = "codex-http-transport"
))]
pub struct HttpXaiVersionFetcher {
    client: wreq::Client,
    registry_url: String,
}

#[cfg(any(
    feature = "anthropic-fingerprint-transport",
    feature = "codex-http-transport"
))]
impl HttpXaiVersionFetcher {
    /// Proxy URL comes from the existing typed host config. Invalid proxy
    /// configuration fails closed; ambient proxy variables are never used.
    pub fn new(proxy_url: Option<&str>) -> Result<Self, VersionFetchError> {
        let (transport, _) = crate::sdk::proxyutil::build_http_transport(proxy_url.unwrap_or(""))
            .map_err(|_| VersionFetchError::Transport)?;
        let mut builder = wreq::Client::builder()
            .no_proxy()
            .redirect(wreq::redirect::Policy::none())
            .timeout(XAI_VERSION_FETCH_TIMEOUT);
        if let Some(proxy) = transport.proxy_url() {
            builder = builder
                .proxy(wreq::Proxy::all(proxy.as_str()).map_err(|_| VersionFetchError::Transport)?);
        }
        Ok(Self {
            client: builder.build().map_err(|_| VersionFetchError::Transport)?,
            registry_url: XAI_NPM_REGISTRY_URL.to_owned(),
        })
    }
}

#[cfg(any(
    feature = "anthropic-fingerprint-transport",
    feature = "codex-http-transport"
))]
impl XaiVersionFetcher for HttpXaiVersionFetcher {
    fn fetch(&self) -> VersionFetchFuture<'_> {
        Box::pin(async move {
            use crate::internal::httpfetch::{HttpDoer, ResponseBody};
            let headers = [
                ("Accept".to_owned(), "application/json".to_owned()),
                ("User-Agent".to_owned(), "CLIProxyAPI".to_owned()),
            ]
            .into();
            let mut response = HttpDoer::get(&self.client, &self.registry_url, &headers)
                .await
                .map_err(|_| VersionFetchError::Transport)?;
            if response.status != 200 {
                return Err(VersionFetchError::HttpStatus);
            }
            let mut body = Vec::new();
            while let Some(chunk) = ResponseBody::next_chunk(&mut *response.body)
                .await
                .map_err(|_| VersionFetchError::Transport)?
            {
                if chunk.len() > XAI_VERSION_MAX_BODY_BYTES.saturating_sub(body.len()) {
                    return Err(VersionFetchError::BodyTooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            Ok(body)
        })
    }
}

#[cfg(test)]
#[path = "xai_client_version_test.rs"]
mod tests;
