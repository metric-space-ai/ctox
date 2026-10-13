//! Generic OAuth2 authorization-code flow (+ PKCE), generalized from the
//! provider-specific cliproxyapi authenticators (claude/codex `oauth_server.rs`,
//! `pkce.rs`, `loopback_http.rs`). Provider quirks are data, not code.
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;

use crate::http::{HttpRequest, HttpTransport};
use crate::model::OAuth2Scheme;

pub const CALLBACK_PATH: &str = "/toolhub/oauth/callback";

/// Where the provider redirects after consent. Like the cliproxyapi OAuth
/// servers (`internal/auth/*/oauth_server.rs`), the callback is always served
/// locally by the CTOX instance itself; there is no central OAuth service.
/// Headless/remote instances: open the URL anywhere, then paste the final
/// redirect URL (or `code=..&state=..`) into `ctox toolhub complete`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectTarget {
    /// Local loopback (RFC 8252 section 7.3).
    Loopback { port: u16 },
}

impl RedirectTarget {
    pub fn redirect_uri(&self) -> String {
        match self {
            Self::Loopback { port } => format!("http://127.0.0.1:{port}{CALLBACK_PATH}"),
        }
    }
}

pub struct AuthorizeParams<'a> {
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub scopes: &'a [String],
    pub state: &'a str,
    pub code_challenge: Option<&'a str>,
}

pub fn authorize_url(scheme: &OAuth2Scheme, p: &AuthorizeParams<'_>) -> Result<String> {
    let mut url = url::Url::parse(&scheme.authorize_url).context("invalid authorize_url")?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", p.client_id)
            .append_pair("redirect_uri", p.redirect_uri)
            .append_pair("state", p.state);
        if !p.scopes.is_empty() {
            q.append_pair("scope", &p.scopes.join(&scheme.scope_separator));
        }
        if let Some(challenge) = p.code_challenge {
            q.append_pair("code_challenge", challenge)
                .append_pair("code_challenge_method", "S256");
        }
    }
    Ok(url.into())
}

/// Normalized token endpoint result.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: Option<String>,
    pub expires_in: Option<u64>,
    pub scopes: Option<Vec<String>>,
}

impl std::fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenSet")
            .field("expires_in", &self.expires_in)
            .field("scopes", &self.scopes)
            .finish_non_exhaustive()
    }
}

pub struct ClientCredentials<'a> {
    pub client_id: &'a str,
    pub client_secret: Option<&'a str>,
}

pub fn exchange_code(
    transport: &dyn HttpTransport,
    scheme: &OAuth2Scheme,
    client: &ClientCredentials<'_>,
    code: &str,
    redirect_uri: &str,
    code_verifier: Option<&str>,
) -> Result<TokenSet> {
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client.client_id),
    ];
    if let Some(secret) = client.client_secret {
        form.push(("client_secret", secret));
    }
    if let Some(v) = code_verifier {
        form.push(("code_verifier", v));
    }
    token_request(transport, scheme, &form)
}

pub fn refresh_token(
    transport: &dyn HttpTransport,
    scheme: &OAuth2Scheme,
    client: &ClientCredentials<'_>,
    refresh_token: &str,
) -> Result<TokenSet> {
    let mut form = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client.client_id),
    ];
    if let Some(secret) = client.client_secret {
        form.push(("client_secret", secret));
    }
    token_request(transport, scheme, &form)
}

fn token_request(
    transport: &dyn HttpTransport,
    scheme: &OAuth2Scheme,
    form: &[(&str, &str)],
) -> Result<TokenSet> {
    let request = HttpRequest::new("POST", &scheme.token_url)
        .header("accept", "application/json")
        .form(form);
    let response = transport.send(request)?;
    let body = response.json().unwrap_or(Value::Null);
    // Some providers (GitHub) answer errors with HTTP 200 + `error`.
    if let Some(err) = body.get("error").and_then(Value::as_str) {
        bail!("oauth token endpoint error: {err}");
    }
    if !(200..300).contains(&response.status) {
        bail!("oauth token endpoint returned HTTP {}", response.status);
    }
    parse_token_response(&body, &scheme.scope_separator)
}

pub fn parse_token_response(body: &Value, scope_separator: &str) -> Result<TokenSet> {
    let access_token = body
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| anyhow!("token response without access_token"))?
        .to_string();
    let expires_in = match body.get("expires_in") {
        Some(Value::Number(n)) => n.as_u64(),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    };
    let scopes = body.get("scope").and_then(Value::as_str).map(|s| {
        s.split(|c: char| c == ',' || c == ' ' || scope_separator.contains(c))
            .filter(|x| !x.is_empty())
            .map(String::from)
            .collect()
    });
    Ok(TokenSet {
        access_token,
        refresh_token: body
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(String::from),
        token_type: body
            .get("token_type")
            .and_then(Value::as_str)
            .map(String::from),
        expires_in,
        scopes,
    })
}

/// Result of a provider redirect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallbackResult {
    Code {
        code: String,
        state: String,
    },
    Denied {
        error: String,
        state: Option<String>,
    },
}

/// Parse `?code=..&state=..` from a full callback URL or a bare query string.
pub fn parse_callback(input: &str) -> Result<CallbackResult> {
    let query = match input.split_once('?') {
        Some((_, q)) => q,
        None => input,
    };
    let query = query.split('#').next().unwrap_or("");
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            _ => {}
        }
    }
    if let Some(error) = error {
        return Ok(CallbackResult::Denied { error, state });
    }
    match (code, state) {
        (Some(code), Some(state)) if !code.is_empty() && !state.is_empty() => {
            Ok(CallbackResult::Code { code, state })
        }
        _ => bail!("callback without code/state"),
    }
}

/// Serve exactly one loopback callback request (bounded by `timeout`).
/// Returns the parsed callback; answers the browser with a bilingual page.
pub fn wait_for_loopback_callback(
    listener: TcpListener,
    timeout: Duration,
) -> Result<CallbackResult> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                let mut line = String::new();
                BufReader::new(std::io::Read::take(&stream, 8192)).read_line(&mut line)?;
                let target = line.split_whitespace().nth(1).unwrap_or("");
                if !target.starts_with(CALLBACK_PATH) {
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                    );
                    continue;
                }
                let result = parse_callback(target);
                let page = match &result {
                    Ok(CallbackResult::Code { .. }) => "<p>Verbindung hergestellt. Sie k&ouml;nnen dieses Fenster schlie&szlig;en.</p><p>Connected. You can close this window.</p>",
                    _ => "<p>Verbindung fehlgeschlagen.</p><p>Connection failed.</p>",
                };
                let html = format!("<!doctype html><meta charset=utf-8><title>CTOX</title>{page}");
                let _ = write!(stream, "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{html}", html.len());
                return result;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    bail!("timed out waiting for OAuth callback");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpResponse;
    use std::sync::Mutex;

    pub struct Recorder {
        pub requests: Mutex<Vec<HttpRequest>>,
        pub reply: Value,
    }
    impl HttpTransport for Recorder {
        fn send(&self, r: HttpRequest) -> Result<HttpResponse> {
            self.requests.lock().unwrap().push(r);
            Ok(HttpResponse {
                status: 200,
                body: serde_json::to_vec(&self.reply).unwrap(),
            })
        }
    }

    fn scheme() -> OAuth2Scheme {
        OAuth2Scheme {
            authorize_url: "https://idp.example/authorize?prompt=consent".into(),
            token_url: "https://idp.example/token".into(),
            default_scopes: vec![],
            pkce: true,
            scope_separator: " ".into(),
            refreshable: true,
        }
    }

    #[test]
    fn authorize_url_contains_pkce_and_keeps_existing_query() {
        let url = authorize_url(
            &scheme(),
            &AuthorizeParams {
                client_id: "cid",
                redirect_uri: "http://127.0.0.1:1/toolhub/oauth/callback",
                scopes: &["repo".into(), "read:org".into()],
                state: "st",
                code_challenge: Some("ch"),
            },
        )
        .unwrap();
        let parsed = url::Url::parse(&url).unwrap();
        let q: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(q["prompt"], "consent");
        assert_eq!(q["scope"], "repo read:org");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(
            q["redirect_uri"],
            "http://127.0.0.1:1/toolhub/oauth/callback"
        );
    }

    #[test]
    fn exchange_sends_verifier_and_parses_tokens() {
        let t = Recorder {
            requests: Mutex::default(),
            reply: serde_json::json!({
            "access_token": "at", "refresh_token": "rt", "expires_in": "3600", "scope": "repo,read:org", "token_type": "bearer"}),
        };
        let set = exchange_code(
            &t,
            &scheme(),
            &ClientCredentials {
                client_id: "cid",
                client_secret: Some("cs"),
            },
            "code1",
            "http://127.0.0.1:1/cb",
            Some("verif"),
        )
        .unwrap();
        assert_eq!(set.access_token, "at");
        assert_eq!(set.expires_in, Some(3600));
        assert_eq!(set.scopes, Some(vec!["repo".into(), "read:org".into()]));
        let body = String::from_utf8(t.requests.lock().unwrap()[0].body.clone().unwrap()).unwrap();
        assert!(
            body.contains("code_verifier=verif") && body.contains("grant_type=authorization_code")
        );
        assert!(!format!("{set:?}").contains("at\""));
    }

    #[test]
    fn provider_error_with_http_200_is_an_error() {
        let t = Recorder {
            requests: Mutex::default(),
            reply: serde_json::json!({"error": "bad_verification_code"}),
        };
        let err = refresh_token(
            &t,
            &scheme(),
            &ClientCredentials {
                client_id: "c",
                client_secret: None,
            },
            "r",
        )
        .unwrap_err();
        assert!(err.to_string().contains("bad_verification_code"));
    }

    #[test]
    fn callback_parsing() {
        assert_eq!(
            parse_callback("https://x/toolhub/oauth/callback?code=a%2Fb&state=s#frag").unwrap(),
            CallbackResult::Code {
                code: "a/b".into(),
                state: "s".into()
            }
        );
        assert_eq!(
            parse_callback("error=access_denied&state=s").unwrap(),
            CallbackResult::Denied {
                error: "access_denied".into(),
                state: Some("s".into())
            }
        );
        assert!(parse_callback("code=a").is_err());
        assert_eq!(
            RedirectTarget::Loopback { port: 1 }.redirect_uri(),
            "http://127.0.0.1:1/toolhub/oauth/callback"
        );
    }

    #[test]
    fn loopback_serves_one_callback() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            wait_for_loopback_callback(listener, Duration::from_secs(5))
        });
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(b"GET /toolhub/oauth/callback?code=c1&state=s1 HTTP/1.1\r\nhost: x\r\n\r\n")
            .unwrap();
        let mut resp = String::new();
        std::io::Read::read_to_string(&mut s, &mut resp).unwrap();
        assert!(resp.contains("Connected"));
        assert_eq!(
            handle.join().unwrap().unwrap(),
            CallbackResult::Code {
                code: "c1".into(),
                state: "s1".into()
            }
        );
    }
}
