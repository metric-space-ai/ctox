// ref: internal/auth/claude/utls_transport.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use wreq::header::{
    ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CACHE_CONTROL, CONNECTION, CONTENT_ENCODING,
    CONTENT_TYPE, RETRY_AFTER, USER_AGENT,
};
use wreq::tls::session::{LruTlsSessionCache, TlsSessionCache};
use wreq::tls::{ExtensionType, KeyShare, TlsOptions, TlsVersion};
use wreq::{Client, ClientBuilder, Proxy};

use super::anthropic_auth::{
    ClaudeCodeExchangeTransport, ClaudeRefreshTransport, ExchangeHttpResponse, ExchangeRequest,
    OAuthInspectHttpResponse, OAuthInspectRequest, RefreshHttpResponse, RefreshRequest,
    RefreshTransportFailure, PROFILE_URL, ROLES_URL, TOKEN_URL,
};
use super::oauth_response::decode_claude_oauth_response_body;

const RETRY_AFTER_MS: &str = "retry-after-ms";
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const CLAUDE_OAUTH_SESSION_CACHE_CAPACITY: usize = 8;
const CLAUDE_OAUTH_PROXY_CACHE_CAPACITY: usize = 64;
const AXIOS_ACCEPT: &str = "application/json, text/plain, */*";
const AXIOS_USER_AGENT: &str = "axios/1.15.2";
const AXIOS_ACCEPT_ENCODING: &str = "gzip, compress, deflate, br";

pub const CLAUDE_OAUTH_REFRESH_HEADER_ORDER: &[&str] = &[
    "Accept",
    "Content-Type",
    "User-Agent",
    "Content-Length",
    "Accept-Encoding",
    "Host",
    "Connection",
];

pub const CLAUDE_OAUTH_INSPECT_HEADER_ORDER: &[&str] = &[
    "Accept",
    "Content-Type",
    "Authorization",
    "Cache-Control",
    "User-Agent",
    "Accept-Encoding",
    "Host",
    "Connection",
];

const CLAUDE_OAUTH_CIPHER_LIST: &str = concat!(
    "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256:",
    "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256:TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256:",
    "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384:TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384:",
    "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256:TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256:",
    "TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA:TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA:",
    "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA:TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA:",
    "TLS_RSA_WITH_AES_128_GCM_SHA256:TLS_RSA_WITH_AES_256_GCM_SHA384:",
    "TLS_RSA_WITH_AES_128_CBC_SHA:TLS_RSA_WITH_AES_256_CBC_SHA"
);

const CLAUDE_OAUTH_SIGALGS: &str = concat!(
    "ecdsa_secp256r1_sha256:rsa_pss_rsae_sha256:rsa_pkcs1_sha256:",
    "ecdsa_secp384r1_sha384:rsa_pss_rsae_sha384:rsa_pkcs1_sha384:",
    "rsa_pss_rsae_sha512:rsa_pkcs1_sha512:rsa_pkcs1_sha1"
);

const CLAUDE_OAUTH_EXTENSIONS: &[ExtensionType] = &[
    ExtensionType::SERVER_NAME,
    ExtensionType::EXTENDED_MASTER_SECRET,
    ExtensionType::RENEGOTIATE,
    ExtensionType::SUPPORTED_GROUPS,
    ExtensionType::EC_POINT_FORMATS,
    ExtensionType::SESSION_TICKET,
    ExtensionType::SIGNATURE_ALGORITHMS,
    ExtensionType::KEY_SHARE,
    ExtensionType::PSK_KEY_EXCHANGE_MODES,
    ExtensionType::SUPPORTED_VERSIONS,
];
const CLAUDE_OAUTH_KEY_SHARES: &[KeyShare] = &[KeyShare::X25519];

struct ProxySessionCacheEntry {
    proxy_key: String,
    id: u64,
    cache: Arc<dyn TlsSessionCache>,
}

static PROXY_SESSION_CACHES: OnceLock<Mutex<VecDeque<ProxySessionCacheEntry>>> = OnceLock::new();
static NEXT_SESSION_CACHE_ID: AtomicU64 = AtomicU64::new(1);

fn proxy_session_cache(proxy_key: &str) -> (u64, Arc<dyn TlsSessionCache>) {
    let caches = PROXY_SESSION_CACHES.get_or_init(|| Mutex::new(VecDeque::new()));
    let mut caches = caches
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(index) = caches.iter().position(|entry| entry.proxy_key == proxy_key) {
        let entry = caches.remove(index).expect("located cache entry");
        let result = (entry.id, Arc::clone(&entry.cache));
        caches.push_back(entry);
        return result;
    }
    let id = NEXT_SESSION_CACHE_ID.fetch_add(1, Ordering::Relaxed);
    let cache: Arc<dyn TlsSessionCache> =
        Arc::new(LruTlsSessionCache::new(CLAUDE_OAUTH_SESSION_CACHE_CAPACITY));
    caches.push_back(ProxySessionCacheEntry {
        proxy_key: proxy_key.to_owned(),
        id,
        cache: Arc::clone(&cache),
    });
    while caches.len() > CLAUDE_OAUTH_PROXY_CACHE_CAPACITY {
        caches.pop_front();
    }
    (id, cache)
}

#[cfg(test)]
pub(crate) fn proxy_session_cache_count() -> usize {
    PROXY_SESSION_CACHES
        .get_or_init(|| Mutex::new(VecDeque::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .len()
}

pub(crate) fn claude_oauth_tls_options() -> TlsOptions {
    TlsOptions::builder()
        .alpn_protocols(std::iter::empty())
        .min_tls_version(TlsVersion::TLS_1_2)
        .max_tls_version(TlsVersion::TLS_1_3)
        .cipher_list(CLAUDE_OAUTH_CIPHER_LIST)
        .preserve_tls13_cipher_list(true)
        .curves_list("X25519:P-256:P-384")
        .key_shares(CLAUDE_OAUTH_KEY_SHARES)
        .sigalgs_list(CLAUDE_OAUTH_SIGALGS)
        .session_ticket(true)
        .pre_shared_key(true)
        .psk_dhe_ke(true)
        .renegotiation(true)
        .permute_extensions(false)
        .grease_enabled(false)
        .extension_permutation(CLAUDE_OAUTH_EXTENSIONS)
        .build()
}

/// Claude Code 2.1.220 Node/OpenSSL-shaped TLS/HTTP/1.1 OAuth transport.
///
/// The candidate capture advertises no ALPN, uses one fixed extension order,
/// and resumes sessions only within the same effective proxy boundary. Rust
/// expresses the same observable contract through wreq/BoringSSL options.
#[derive(Clone)]
pub struct AnthropicHttpTransport {
    client: Client,
    endpoint: String,
    profile_endpoint: Option<String>,
    roles_endpoint: Option<String>,
    proxy_mode: AnthropicProxyMode,
    session_cache_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnthropicProxyMode {
    Direct,
    Proxy,
}

impl AnthropicHttpTransport {
    /// Builds a direct transport unless a proxy URL is supplied by typed host
    /// configuration. System/environment proxy discovery is disabled.
    pub fn new(proxy_url: Option<&str>) -> Result<Self, AnthropicTransportBuildError> {
        Self::build(TOKEN_URL, Some(PROFILE_URL), Some(ROLES_URL), proxy_url)
    }

    #[cfg(test)]
    pub(crate) fn with_endpoint(
        endpoint: &str,
        proxy_url: Option<&str>,
    ) -> Result<Self, AnthropicTransportBuildError> {
        Self::build(endpoint, None, None, proxy_url)
    }

    fn build(
        endpoint: &str,
        profile_endpoint: Option<&str>,
        roles_endpoint: Option<&str>,
        proxy_url: Option<&str>,
    ) -> Result<Self, AnthropicTransportBuildError> {
        if endpoint.trim().is_empty() {
            return Err(AnthropicTransportBuildError::InvalidEndpoint);
        }

        let proxy_mode = classify_proxy_mode(proxy_url)?;
        let proxy_key = normalized_proxy_key(proxy_url)?;
        let (session_cache_id, session_cache) = proxy_session_cache(&proxy_key);
        let client = build_anthropic_client(proxy_url, session_cache)?;
        Ok(Self {
            client,
            endpoint: endpoint.to_owned(),
            profile_endpoint: profile_endpoint.map(ToOwned::to_owned),
            roles_endpoint: roles_endpoint.map(ToOwned::to_owned),
            proxy_mode,
            session_cache_id,
        })
    }

    pub fn proxy_mode(&self) -> AnthropicProxyMode {
        self.proxy_mode
    }

    pub fn session_cache_id(&self) -> u64 {
        self.session_cache_id
    }

    fn inspect_endpoint(&self, request: &OAuthInspectRequest) -> Option<&str> {
        match request.kind() {
            super::anthropic_auth::OAuthInspectKind::Profile => self.profile_endpoint.as_deref(),
            super::anthropic_auth::OAuthInspectKind::Roles => self.roles_endpoint.as_deref(),
        }
    }
}

fn normalized_proxy_key(proxy_url: Option<&str>) -> Result<String, AnthropicTransportBuildError> {
    match classify_proxy_mode(proxy_url)? {
        AnthropicProxyMode::Direct => Ok(String::new()),
        AnthropicProxyMode::Proxy => Ok(proxy_url.unwrap_or_default().trim().to_owned()),
    }
}

fn classify_proxy_mode(
    proxy_url: Option<&str>,
) -> Result<AnthropicProxyMode, AnthropicTransportBuildError> {
    let Some(proxy_url) = proxy_url.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(AnthropicProxyMode::Direct);
    };
    if proxy_url.eq_ignore_ascii_case("direct") || proxy_url.eq_ignore_ascii_case("none") {
        return Ok(AnthropicProxyMode::Direct);
    }

    let parsed =
        url::Url::parse(proxy_url).map_err(|_| AnthropicTransportBuildError::InvalidProxy)?;
    if !matches!(parsed.scheme(), "socks5" | "socks5h" | "http" | "https") {
        return Err(AnthropicTransportBuildError::InvalidProxy);
    }
    Proxy::all(proxy_url).map_err(|_| AnthropicTransportBuildError::InvalidProxy)?;
    Ok(AnthropicProxyMode::Proxy)
}

pub(crate) fn build_anthropic_client(
    proxy_url: Option<&str>,
    session_cache: Arc<dyn TlsSessionCache>,
) -> Result<Client, AnthropicTransportBuildError> {
    let mut builder = anthropic_client_builder(session_cache);

    match classify_proxy_mode(proxy_url)? {
        AnthropicProxyMode::Proxy => {
            let proxy_url = proxy_url
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or(AnthropicTransportBuildError::InvalidProxy)?;
            let proxy =
                Proxy::all(proxy_url).map_err(|_| AnthropicTransportBuildError::InvalidProxy)?;
            builder = builder.proxy(proxy);
        }
        AnthropicProxyMode::Direct => {
            builder = builder.no_proxy();
        }
    }

    builder
        .build()
        .map_err(|_| AnthropicTransportBuildError::Client)
}

fn anthropic_client_builder(session_cache: Arc<dyn TlsSessionCache>) -> ClientBuilder {
    Client::builder()
        .tls_options(claude_oauth_tls_options())
        .tls_session_cache(session_cache)
        .connect_timeout(HANDSHAKE_TIMEOUT)
        .retry(wreq::retry::Policy::never())
        .redirect(wreq::redirect::Policy::none())
}

#[cfg(test)]
pub(crate) fn build_anthropic_loopback_test_client(
    session_cache: Arc<dyn TlsSessionCache>,
) -> Result<Client, AnthropicTransportBuildError> {
    anthropic_client_builder(session_cache)
        .no_proxy()
        .tls_cert_verification(false)
        .tls_verify_hostname(false)
        .build()
        .map_err(|_| AnthropicTransportBuildError::Client)
}

/// Reuses the bounded proxy-scoped cache for Messages and OAuth traffic so a
/// configured account has one resumption identity instead of one per caller.
pub(crate) fn build_anthropic_messages_client(
    proxy_url: Option<&str>,
) -> Result<Client, AnthropicTransportBuildError> {
    let proxy_key = normalized_proxy_key(proxy_url)?;
    let (_, session_cache) = proxy_session_cache(&proxy_key);
    build_anthropic_client(proxy_url, session_cache)
}

impl fmt::Debug for AnthropicHttpTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnthropicHttpTransport")
            .field("endpoint", &self.endpoint)
            .field("profile_endpoint", &self.profile_endpoint)
            .field("roles_endpoint", &self.roles_endpoint)
            .field("proxy_mode", &self.proxy_mode)
            .field("client", &"ClaudeCode-2.1.220-Node/BoringSSL")
            .field("session_cache_id", &self.session_cache_id)
            .finish()
    }
}

fn ordered_headers(order: &[&str]) -> wreq::header::OrigHeaderMap {
    let mut headers = wreq::header::OrigHeaderMap::with_capacity(order.len());
    for name in order {
        headers.insert((*name).to_owned());
    }
    headers
}

fn content_encodings(response: &wreq::Response) -> Vec<String> {
    response
        .headers()
        .get_all(CONTENT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
        .collect()
}

struct DecodedOAuthResponse {
    status: u16,
    retry_after: Option<String>,
    retry_after_ms: Option<String>,
    body: Vec<u8>,
}

async fn decoded_response_body(
    response: wreq::Response,
) -> Result<DecodedOAuthResponse, RefreshTransportFailure> {
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let retry_after_ms = response
        .headers()
        .get(RETRY_AFTER_MS)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let encodings = content_encodings(&response);
    let body = response
        .bytes()
        .await
        .map_err(classify_transport_error)?
        .to_vec();
    let body = decode_claude_oauth_response_body(body, &encodings)
        .map_err(|_| RefreshTransportFailure::Protocol)?;
    Ok(DecodedOAuthResponse {
        status,
        retry_after,
        retry_after_ms,
        body,
    })
}

impl ClaudeCodeExchangeTransport for AnthropicHttpTransport {
    fn exchange<'a>(
        &'a self,
        request: &'a ExchangeRequest,
        timeout: Duration,
    ) -> Pin<
        Box<dyn Future<Output = Result<ExchangeHttpResponse, RefreshTransportFailure>> + Send + 'a>,
    > {
        Box::pin(async move {
            let body = request
                .json_body()
                .map_err(|_| RefreshTransportFailure::Protocol)?;
            let response = self
                .client
                .post(&self.endpoint)
                .header(ACCEPT, AXIOS_ACCEPT)
                .header(CONTENT_TYPE, "application/json")
                .header(USER_AGENT, AXIOS_USER_AGENT)
                .header(ACCEPT_ENCODING, AXIOS_ACCEPT_ENCODING)
                .header(CONNECTION, "close")
                .orig_headers(ordered_headers(CLAUDE_OAUTH_REFRESH_HEADER_ORDER))
                .timeout(timeout)
                .body(body.as_slice().to_vec())
                .send()
                .await
                .map_err(classify_transport_error)?;
            let response = decoded_response_body(response).await?;
            Ok(ExchangeHttpResponse::new(response.status, response.body))
        })
    }

    fn inspect<'a>(
        &'a self,
        request: &'a OAuthInspectRequest,
        timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<OAuthInspectHttpResponse, RefreshTransportFailure>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let endpoint = self
                .inspect_endpoint(request)
                .ok_or(RefreshTransportFailure::Protocol)?;
            let response = self
                .client
                .get(endpoint)
                .header(ACCEPT, AXIOS_ACCEPT)
                .header(CONTENT_TYPE, "application/json")
                .header(
                    AUTHORIZATION,
                    format!("Bearer {}", request.access_token().expose_secret()),
                )
                .header(CACHE_CONTROL, "no-cache")
                .header(USER_AGENT, AXIOS_USER_AGENT)
                .header(ACCEPT_ENCODING, AXIOS_ACCEPT_ENCODING)
                .header(CONNECTION, "close")
                .orig_headers(ordered_headers(CLAUDE_OAUTH_INSPECT_HEADER_ORDER))
                .timeout(timeout)
                .send()
                .await
                .map_err(classify_transport_error)?;
            let response = decoded_response_body(response).await?;
            Ok(OAuthInspectHttpResponse::new(
                response.status,
                response.body,
            ))
        })
    }
}

impl ClaudeRefreshTransport for AnthropicHttpTransport {
    fn execute<'a>(
        &'a self,
        request: &'a RefreshRequest,
        timeout: Duration,
    ) -> Pin<
        Box<dyn Future<Output = Result<RefreshHttpResponse, RefreshTransportFailure>> + Send + 'a>,
    > {
        Box::pin(async move {
            let body = request
                .json_body()
                .map_err(|_| RefreshTransportFailure::Protocol)?;
            let response = self
                .client
                .post(&self.endpoint)
                .header(ACCEPT, AXIOS_ACCEPT)
                .header(CONTENT_TYPE, "application/json")
                .header(USER_AGENT, AXIOS_USER_AGENT)
                .header(ACCEPT_ENCODING, AXIOS_ACCEPT_ENCODING)
                .header(CONNECTION, "close")
                .orig_headers(ordered_headers(CLAUDE_OAUTH_REFRESH_HEADER_ORDER))
                .timeout(timeout)
                .body(body.as_slice().to_vec())
                .send()
                .await
                .map_err(classify_transport_error)?;

            let response = decoded_response_body(response).await?;
            Ok(RefreshHttpResponse::new(
                response.status,
                response.retry_after,
                response.retry_after_ms,
                response.body,
            ))
        })
    }

    fn inspect<'a>(
        &'a self,
        request: &'a OAuthInspectRequest,
        timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<OAuthInspectHttpResponse, RefreshTransportFailure>>
                + Send
                + 'a,
        >,
    > {
        <Self as ClaudeCodeExchangeTransport>::inspect(self, request, timeout)
    }
}

fn classify_transport_error(error: wreq::Error) -> RefreshTransportFailure {
    if error.is_timeout() {
        RefreshTransportFailure::Timeout
    } else if error.is_connect() {
        RefreshTransportFailure::Connect
    } else {
        RefreshTransportFailure::Protocol
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnthropicTransportBuildError {
    InvalidEndpoint,
    InvalidProxy,
    Client,
}

impl fmt::Display for AnthropicTransportBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEndpoint => formatter.write_str("Anthropic token endpoint is invalid"),
            Self::InvalidProxy => formatter.write_str("Anthropic proxy configuration is invalid"),
            Self::Client => formatter.write_str("Anthropic HTTP transport could not be built"),
        }
    }
}

impl std::error::Error for AnthropicTransportBuildError {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;
    use crate::internal::auth::claude::{
        ClaudeAuth, ClaudeRefreshCoordinator, PkceCodes, SecretString, SystemRefreshClock,
    };

#[test]
    fn invalid_proxy_error_never_echoes_proxy_credentials() {
        let secret_proxy = "not a proxy://operator:do-not-leak";
        let error = AnthropicHttpTransport::new(Some(secret_proxy)).unwrap_err();
        let rendered = format!("{error:?} {error}");
        assert!(!rendered.contains("operator"));
        assert!(!rendered.contains("do-not-leak"));
    }

#[test]
    fn unsupported_proxy_schemes_are_rejected_without_echoing_the_url() {
        let proxy = "ftp://operator:do-not-leak@proxy.example.com";
        let error = AnthropicHttpTransport::new(Some(proxy)).unwrap_err();
        assert_eq!(error, AnthropicTransportBuildError::InvalidProxy);
        let rendered = format!("{error:?} {error}");
        assert!(!rendered.contains("operator"));
        assert!(!rendered.contains("do-not-leak"));
        assert!(!rendered.contains("proxy.example.com"));
    }

#[tokio::test]
    async fn loopback_code_exchange_uses_json_contract_and_fragment_state() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/oauth/token", listener.local_addr().unwrap());
        let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let server_capture = Arc::clone(&captured);
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 2048];
            loop {
                let read = socket.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if let Some(headers_end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers_end = headers_end + 4;
                    let headers = String::from_utf8_lossy(&request[..headers_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= headers_end + content_length {
                        break;
                    }
                }
            }
            *server_capture.lock().await = request;
            let body = br#"{
                "access_token":"loop-exchange-access",
                "refresh_token":"loop-exchange-refresh",
                "token_type":"Bearer",
                "expires_in":3600,
                "account":{"uuid":"account","email_address":"loop@example.com"},
                "organization":{"uuid":"org","name":"Loop Org"}
            }"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.write_all(body).await.unwrap();
        });

        let auth = ClaudeAuth::new(AnthropicHttpTransport::with_endpoint(&endpoint, None).unwrap());
        let pkce = PkceCodes {
            code_verifier: "loop-verifier".to_owned(),
            code_challenge: "loop-challenge".to_owned(),
        };
        let bundle = auth
            .exchange_code_for_tokens(
                &SecretString::new("loop-code#loop-fragment-state").unwrap(),
                &SecretString::new("loop-original-state").unwrap(),
                &pkce,
            )
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(
            bundle.token_data().access_token().expose_secret(),
            "loop-exchange-access"
        );
        assert_eq!(bundle.user_info().email(), "loop@example.com");

        let request = String::from_utf8(captured.lock().await.clone()).unwrap();
        let lower = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /v1/oauth/token HTTP/1.1\r\n"));
        assert!(lower.contains("content-type: application/json"));
        assert!(lower.contains("accept: application/json"));
        assert!(!lower.contains("sec-ch-ua"));
        assert!(!lower.contains("sec-fetch"));
        assert!(request.contains("\"code\":\"loop-code\""));
        assert!(request.contains("\"state\":\"loop-fragment-state\""));
        assert!(request.contains("\"code_verifier\":\"loop-verifier\""));
    }

#[tokio::test]
    async fn loopback_refresh_sends_only_upstream_json_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/oauth/token", listener.local_addr().unwrap());
        let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let server_capture = Arc::clone(&captured);
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 2048];
            loop {
                let read = socket.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if let Some(headers_end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers_end = headers_end + 4;
                    let headers = String::from_utf8_lossy(&request[..headers_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= headers_end + content_length {
                        break;
                    }
                }
            }
            *server_capture.lock().await = request;
            let body = br#"{"access_token":"loop-access","refresh_token":"loop-refresh","expires_in":3600}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.write_all(body).await.unwrap();
        });

        let transport = AnthropicHttpTransport::with_endpoint(&endpoint, None).unwrap();
        let token = ClaudeRefreshCoordinator::default()
            .refresh(
                &transport,
                &SystemRefreshClock,
                SecretString::new("loop-original").unwrap(),
                1,
            )
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(token.access_token().expose_secret(), "loop-access");

        let request = String::from_utf8(captured.lock().await.clone()).unwrap();
        let lower = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /v1/oauth/token HTTP/1.1\r\n"));
        assert!(lower.contains("content-type: application/json"));
        assert!(lower.contains("accept: application/json"));
        assert!(!lower.contains("sec-ch-ua"));
        assert!(!lower.contains("sec-fetch"));
        assert!(!lower.contains("accept-language"));
        assert!(request.contains("\"refresh_token\":\"loop-original\""));
    }
}
