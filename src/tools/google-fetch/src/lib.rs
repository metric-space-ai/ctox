use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::header::{
    ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL, COOKIE, LOCATION, REFERER, SET_COOKIE,
    UPGRADE_INSECURE_REQUESTS, USER_AGENT,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;
use url::Url;

#[derive(Debug, Clone, Deserialize)]
pub struct FetchRequest {
    pub url: String,
    pub user_agent: String,
    pub cookie_header: String,
    pub accept_language: String,
    pub timeout_ms: u64,
    pub emulation_major: u16,
    #[serde(default)]
    pub transport: FetchTransport,
    #[serde(default)]
    pub repo_root: Option<String>,
    #[serde(default)]
    pub extra_headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FetchTransport {
    #[default]
    Native,
    BrowserClone,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FetchResponse {
    pub final_url: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct RawFetchResponse {
    pub status: u16,
    pub final_url: String,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportProfile {
    RustlsH2,
    RustlsH1,
    NativeTlsH2,
    NativeTlsH1,
}

impl TransportProfile {
    pub const fn label(self) -> &'static str {
        match self {
            Self::RustlsH2 => "rustls-h2",
            Self::RustlsH1 => "rustls-h1",
            Self::NativeTlsH2 => "native-tls-h2",
            Self::NativeTlsH1 => "native-tls-h1",
        }
    }

    pub const fn all() -> [Self; 4] {
        [
            Self::RustlsH2,
            Self::RustlsH1,
            Self::NativeTlsH2,
            Self::NativeTlsH1,
        ]
    }

    pub fn parse(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "rustls-h2" => Some(Self::RustlsH2),
            "rustls-h1" => Some(Self::RustlsH1),
            "native-tls-h2" => Some(Self::NativeTlsH2),
            "native-tls-h1" => Some(Self::NativeTlsH1),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ResponseMarkers {
    pub data_ved: bool,
    pub sorry: bool,
    pub captcha: bool,
    pub enablejs: bool,
}

pub fn fetch_google_response(
    request: &FetchRequest,
    transport: TransportProfile,
) -> Result<RawFetchResponse> {
    let _emulation_major = request.emulation_major;
    let client = build_client(request.timeout_ms, transport)?;
    let search_url =
        Url::parse(request.url.as_str()).context("failed to parse Google search request URL")?;
    let homepage_url = homepage_url(&search_url);
    let mut base_headers = browser_headers(request);

    let homepage_response = client
        .get(homepage_url.as_str())
        .headers(base_headers.clone())
        .send()
        .context("failed to warm Google homepage session")?;
    let warmed_cookie_header = merge_cookie_header(
        request.cookie_header.as_str(),
        homepage_response.headers().get_all(SET_COOKIE),
    );
    let _ = homepage_response.text();
    if let Some(cookie_header) = warmed_cookie_header {
        base_headers.insert(
            COOKIE,
            reqwest::header::HeaderValue::from_str(cookie_header.as_str())
                .context("failed to encode warmed Google cookie header")?,
        );
    }

    let response = client
        .get(search_url.as_str())
        .headers(base_headers)
        .header(REFERER, homepage_url.as_str())
        .send()
        .context("failed to query Google search endpoint")?;

    let status = response.status().as_u16();
    let final_url = response.url().clone();
    let redirect_location = response
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = response
        .text()
        .context("failed to read Google search response")?;

    detect_google_redirect(status, redirect_location.as_deref())?;
    detect_google_interstitial(&final_url, &body)?;

    Ok(RawFetchResponse {
        status,
        final_url: final_url.to_string(),
        body,
    })
}

pub fn response_markers(body: &str) -> ResponseMarkers {
    let lowered_body = body.to_ascii_lowercase();
    ResponseMarkers {
        data_ved: lowered_body.contains("data-ved"),
        sorry: lowered_body.contains("/sorry/")
            || lowered_body.contains("sorry.google.com")
            || lowered_body.contains("unusual traffic"),
        captcha: lowered_body.contains("captcha-form"),
        enablejs: lowered_body.contains("enablejs"),
    }
}

fn build_client(timeout_ms: u64, transport: TransportProfile) -> Result<Client> {
    let builder = match transport {
        TransportProfile::RustlsH2 => ClientBuilder::new(),
        TransportProfile::RustlsH1 => ClientBuilder::new().http1_only(),
        TransportProfile::NativeTlsH2 => ClientBuilder::new().use_native_tls(),
        TransportProfile::NativeTlsH1 => ClientBuilder::new().use_native_tls().http1_only(),
    };

    builder
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .with_context(|| {
            format!(
                "failed to build reqwest Google transport for {}",
                transport.label()
            )
        })
}

fn homepage_url(search_url: &Url) -> Url {
    let mut url = search_url.clone();
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    url
}

fn browser_headers(request: &FetchRequest) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        ACCEPT,
        reqwest::header::HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7",
        ),
    );
    headers.insert(
        CACHE_CONTROL,
        reqwest::header::HeaderValue::from_static("max-age=0"),
    );
    headers.insert(
        UPGRADE_INSECURE_REQUESTS,
        reqwest::header::HeaderValue::from_static("1"),
    );
    if !request.cookie_header.trim().is_empty() {
        headers.insert(
            COOKIE,
            reqwest::header::HeaderValue::from_str(request.cookie_header.as_str())
                .expect("Google cookie header should be valid"),
        );
    }
    headers.insert(
        USER_AGENT,
        reqwest::header::HeaderValue::from_str(request.user_agent.as_str())
            .expect("Google user agent should be valid"),
    );
    headers.insert(
        ACCEPT_LANGUAGE,
        reqwest::header::HeaderValue::from_str(request.accept_language.as_str())
            .expect("Google accept-language should be valid"),
    );
    for (name, value) in &request.extra_headers {
        let normalized = name.trim().to_ascii_lowercase();
        if normalized.is_empty()
            || normalized == "cookie"
            || normalized == "user-agent"
            || normalized == "accept-language"
            || normalized == "referer"
            || normalized == "host"
            || normalized.starts_with(':')
        {
            continue;
        }
        let Ok(header_name) = reqwest::header::HeaderName::from_bytes(normalized.as_bytes()) else {
            continue;
        };
        let Ok(header_value) = reqwest::header::HeaderValue::from_str(value) else {
            continue;
        };
        headers.insert(header_name, header_value);
    }
    headers
}

fn merge_cookie_header(
    base_cookie_header: &str,
    set_cookies: reqwest::header::GetAll<reqwest::header::HeaderValue>,
) -> Option<String> {
    let mut cookies = BTreeMap::new();

    for segment in base_cookie_header.split(';') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        if let Some((name, value)) = segment.split_once('=') {
            cookies.insert(name.trim().to_string(), value.trim().to_string());
        }
    }

    for header in set_cookies.iter() {
        let Ok(raw) = header.to_str() else {
            continue;
        };
        let Some(first_segment) = raw.split(';').next() else {
            continue;
        };
        let Some((name, value)) = first_segment.split_once('=') else {
            continue;
        };
        cookies.insert(name.trim().to_string(), value.trim().to_string());
    }

    if cookies.is_empty() {
        None
    } else {
        Some(
            cookies
                .into_iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

fn detect_google_redirect(status: u16, redirect_location: Option<&str>) -> Result<()> {
    if !(300..400).contains(&status) {
        return Ok(());
    }
    let Some(location) = redirect_location else {
        return Ok(());
    };
    let lowered = location.to_ascii_lowercase();
    if lowered.contains("sorry.google.com")
        || lowered.contains("/sorry/")
        || lowered.contains("/sorry/index")
    {
        bail!("Google returned a sorry/CAPTCHA redirect");
    }
    if lowered.contains("consent.google.com") {
        bail!("Google returned a consent redirect");
    }
    Ok(())
}

fn detect_google_interstitial(final_url: &Url, body: &str) -> Result<()> {
    let lowered_url = final_url.as_str().to_ascii_lowercase();
    if lowered_url.contains("sorry.google.com") || final_url.path().starts_with("/sorry") {
        bail!("Google returned a sorry/CAPTCHA page");
    }
    if lowered_url.contains("consent.google.com") {
        bail!("Google returned a consent interstitial");
    }

    let lowered_body = body.to_ascii_lowercase();
    let has_result_markers = lowered_body.contains("data-ved");
    if lowered_body.contains("captcha-form")
        || lowered_body.contains("/sorry/index")
        || lowered_body.contains("consent.google.com")
        || lowered_body.contains("before you continue to google")
        || lowered_body.contains("to continue, please type the characters below")
    {
        bail!("Google returned a consent, sorry, or CAPTCHA interstitial");
    }

    if !has_result_markers
        && (lowered_body.contains("/httpservice/retry/enablejs")
            || lowered_body.contains("enablejs")
            || lowered_body.contains("please click")
                && lowered_body.contains("if you are not redirected within a few seconds"))
    {
        bail!("Google returned a JavaScript-only enablejs interstitial");
    }

    if body.starts_with(")]}'")
        && lowered_body.contains("data-async-context=\"query:\"")
        && !lowered_body.contains("<a ")
    {
        bail!("Google returned an async bootstrap payload without result anchors");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_real_result_fixture() {
        let url = Url::parse("https://www.google.com/search?q=rust").expect("valid url");
        let html = include_str!("../../web-stack/fixtures/google_results_fixture.html");
        assert!(detect_google_interstitial(&url, html).is_ok());
    }

    #[test]
    fn rejects_sorry_and_enablejs_bodies() {
        let url = Url::parse("https://www.google.com/search?q=rust").expect("valid url");
        assert!(detect_google_interstitial(
            &url,
            "<html><body><form id=\"captcha-form\"></form></body></html>"
        )
        .is_err());
        assert!(detect_google_interstitial(
            &url,
            "<html><body><meta content=\"0;url=/httpservice/retry/enablejs?sei=abc\" http-equiv=\"refresh\"></body></html>"
        )
        .is_err());
    }

    #[test]
    fn accepts_enablejs_noscript_when_result_markers_exist() {
        let url = Url::parse("https://www.google.com/search?q=rust").expect("valid url");
        let html = "<html><body><noscript><meta content=\"0;url=/httpservice/retry/enablejs?sei=abc\" http-equiv=\"refresh\"><div>Please click <a href=\"/httpservice/retry/enablejs?sei=abc\">here</a> if you are not redirected within a few seconds.</div></noscript><a data-ved=\"123\" href=\"https://www.rust-lang.org/\">Rust</a></body></html>";
        assert!(detect_google_interstitial(&url, html).is_ok());
    }

    #[test]
    fn derives_homepage_from_search_url() {
        let search =
            Url::parse("https://www.google.com/search?q=rust&hl=en-US&start=0").expect("url");
        assert_eq!(homepage_url(&search).as_str(), "https://www.google.com/");
    }
}
