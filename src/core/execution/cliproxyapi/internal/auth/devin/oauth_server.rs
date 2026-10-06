// ref: internal/auth/devin/devin_auth.go:200-384
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — owner-bound state and bounded, owned async listener
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::auth::loopback_http::{
    read_request_head, response, write_response, HttpResponse, RequestHead, IO_TIMEOUT,
    MAX_CONCURRENT_CONNECTIONS, MAX_TOTAL_CONNECTIONS,
};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    net::{Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinSet,
    time::Instant,
};
use zeroize::Zeroizing;

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

/// The flow owner supplies its unguessable state and retains the matching PKCE
/// verifier. Only the matching callback may yield a code or provider error.
/// This object owns no spawned task until its consuming wait is polled. Dropping
/// it, or cancelling that wait, releases the listener and owned connections.
pub struct OAuthServer {
    listener: TcpListener,
    expected_state_digest: [u8; 32],
    expires_at: Instant,
}
impl fmt::Debug for OAuthServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinOAuthServer").finish_non_exhaustive()
    }
}

impl OAuthServer {
    pub async fn start(port: u16, expected_state: &str) -> Result<Self, OAuthServerError> {
        Self::start_with_timeout(port, expected_state, CALLBACK_TIMEOUT).await
    }
    async fn start_with_timeout(
        port: u16,
        expected_state: &str,
        timeout: Duration,
    ) -> Result<Self, OAuthServerError> {
        if expected_state.is_empty() || expected_state.trim() != expected_state {
            return Err(OAuthServerError::InvalidExpectedState);
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|_| OAuthServerError::Bind)?;
        Ok(Self {
            listener,
            expected_state_digest: Sha256::digest(expected_state.as_bytes()).into(),
            expires_at: Instant::now() + timeout,
        })
    }
    pub fn local_addr(&self) -> Result<SocketAddr, OAuthServerError> {
        self.listener
            .local_addr()
            .map_err(|_| OAuthServerError::Bind)
    }
    pub fn redirect_uri(&self) -> Result<String, OAuthServerError> {
        Ok(format!("http://{}/callback", self.local_addr()?))
    }
    pub async fn wait_for_callback(self) -> Result<OAuthResult, OAuthServerError> {
        let Self {
            listener,
            expected_state_digest,
            expires_at,
        } = self;
        let mut connections = JoinSet::new();
        let result = tokio::time::timeout_at(
            expires_at,
            wait_for_callback(&listener, expected_state_digest, &mut connections),
        )
        .await
        .unwrap_or(Err(OAuthServerError::Timeout))
        .and_then(|result| {
            if Instant::now() >= expires_at {
                Err(OAuthServerError::Timeout)
            } else {
                Ok(result)
            }
        });
        drop(listener);
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        result
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OAuthServerError {
    InvalidExpectedState,
    Bind,
    Accept,
    ConnectionLimit,
    ConnectionFailed,
    Timeout,
}
impl fmt::Display for OAuthServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidExpectedState => "Devin OAuth requires a nonempty owner-bound state",
            Self::Bind => "failed to bind local Devin OAuth server",
            Self::Accept => "local Devin OAuth listener failed",
            Self::ConnectionLimit => "local Devin OAuth connection limit reached",
            Self::ConnectionFailed => "local Devin OAuth callback task failed",
            Self::Timeout => "devin authentication timed out",
        })
    }
}
impl std::error::Error for OAuthServerError {}

#[derive(Clone, PartialEq, Eq)]
pub struct OAuthResult {
    code: Option<Zeroizing<String>>,
    state: Option<Zeroizing<String>>,
    error: Option<String>,
}
impl OAuthResult {
    pub fn code(&self) -> Option<&str> {
        self.code.as_deref().map(String::as_str)
    }
    pub fn state(&self) -> Option<&str> {
        self.state.as_deref().map(String::as_str)
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn is_success(&self) -> bool {
        self.code.is_some() && self.state.is_some() && self.error.is_none()
    }
}
impl fmt::Debug for OAuthResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinOAuthResult")
            .field("has_code", &self.code.is_some())
            .field("has_state", &self.state.is_some())
            .field("has_error", &self.error.is_some())
            .finish()
    }
}

async fn wait_for_callback(
    listener: &TcpListener,
    expected: [u8; 32],
    connections: &mut JoinSet<Option<OAuthResult>>,
) -> Result<OAuthResult, OAuthServerError> {
    let claimed = Arc::new(AtomicBool::new(false));
    let mut accepted = 0;
    loop {
        if accepted == MAX_TOTAL_CONNECTIONS && connections.is_empty() {
            return Err(OAuthServerError::ConnectionLimit);
        }
        tokio::select! {
            finished = connections.join_next(), if !connections.is_empty() => {
                match finished {
                    Some(Ok(Some(result))) => return Ok(result),
                    Some(Err(_)) => return Err(OAuthServerError::ConnectionFailed),
                    _ => {},
                }
            },
            incoming = listener.accept(), if accepted < MAX_TOTAL_CONNECTIONS => {
                let (stream, peer) = incoming.map_err(|_| OAuthServerError::Accept)?;
                accepted += 1;
                if !peer.ip().is_loopback() || connections.len() >= MAX_CONCURRENT_CONNECTIONS {
                    drop(stream);
                    continue;
                }
                let claimed = Arc::clone(&claimed);
                connections.spawn(handle_connection(stream, expected, claimed));
            },
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    expected: [u8; 32],
    claimed: Arc<AtomicBool>,
) -> Option<OAuthResult> {
    let (reply, result) =
        match tokio::time::timeout(IO_TIMEOUT, read_request_head(&mut stream)).await {
            Ok(Ok(request)) => route_request(&request, &expected, &claimed),
            Ok(Err(_)) => (plain(400, "Bad Request", "Bad request"), None),
            Err(_) => (plain(408, "Request Timeout", "Request timeout"), None),
        };
    // A disconnected browser must not discard a valid, state-checked code.
    let _ = write_response(&mut stream, reply).await;
    result
}

fn route_request(
    request: &RequestHead,
    expected: &[u8; 32],
    claimed: &AtomicBool,
) -> (HttpResponse, Option<OAuthResult>) {
    if request.method != "GET" {
        return (
            plain(405, "Method Not Allowed", "Method not allowed").with_header("Allow", "GET"),
            None,
        );
    }
    if !request.target.starts_with('/') || request.target.starts_with("//") {
        return (plain(400, "Bad Request", "Bad request"), None);
    }
    let url = match url::Url::parse(&format!("http://localhost{}", request.target)) {
        Ok(url) if url.host_str() == Some("localhost") && url.fragment().is_none() => url,
        _ => return (plain(400, "Bad Request", "Bad request"), None),
    };
    if url.path() != "/callback" {
        return (plain(404, "Not Found", "Not found"), None);
    }
    let query = |name: &str| {
        url.query_pairs()
            .find_map(|(key, value)| (key == name).then(|| value.trim().to_owned()))
            .unwrap_or_default()
    };
    let state = query("state");
    let received: [u8; 32] = Sha256::digest(state.as_bytes()).into();
    if state.is_empty() || !bool::from(expected.ct_eq(&received)) {
        // An unrelated browser request must not consume the active login.
        return (plain(400, "Bad Request", "Invalid OAuth state"), None);
    }
    if claimed
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return (
            plain(409, "Conflict", "OAuth callback already received"),
            None,
        );
    }
    let code = query("code");
    let error = query("error");
    let description = query("error_description");
    if !error.is_empty() || code.is_empty() {
        let message = if !description.is_empty() {
            format!("{error}: {description}")
        } else if !error.is_empty() {
            error
        } else {
            "missing authorization code".into()
        };
        let body = format!("<!DOCTYPE html><html lang=\"en\"><meta charset=\"UTF-8\"><title>Authentication Failed - Devin</title><h2>Authentication Failed</h2><p>Devin authentication encountered an error: {}</p><p>Please check your terminal and try again.</p></html>", html_escape(&message));
        return (
            response(
                400,
                "Bad Request",
                "text/html; charset=utf-8",
                body.into_bytes(),
            ),
            Some(OAuthResult {
                code: None,
                state: None,
                error: Some(message),
            }),
        );
    }
    let body = b"<!DOCTYPE html><html lang=\"en\"><meta charset=\"UTF-8\"><title>Authentication Successful - Devin</title><h2>Authentication Complete</h2><p>You have successfully logged in to Devin via CLIProxyAPI.</p><p>You may safely close this window and return to your terminal.</p></html>";
    (
        response(200, "OK", "text/html; charset=utf-8", body.to_vec()),
        Some(OAuthResult {
            code: Some(Zeroizing::new(code)),
            state: Some(Zeroizing::new(state)),
            error: None,
        }),
    )
}
fn plain(status: u16, reason: &'static str, body: &str) -> HttpResponse {
    response(
        status,
        reason,
        "text/plain; charset=utf-8",
        body.as_bytes().to_vec(),
    )
}
fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('>', "&gt;")
        .replace('<', "&lt;")
        .replace('"', "&#34;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
#[path = "oauth_server_test.rs"]
mod tests;
