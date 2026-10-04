// Origin: CTOX
// License: AGPL-3.0-only

//! Turn-scoped native coding bridge for Anthropic plans and inherited Responses.
//!
//! Pi talks Anthropic Messages to this loopback-only endpoint using a public
//! sentinel. The native owner replaces that sentinel with the selected
//! account's x-api-key and streams the provider response back. The secret is
//! never placed in the sidecar process, its environment or the turn payload.
//! The same owner boundary carries MiniMax Coding Plan and Kimi Coding Plan;
//! provider-specific account configuration still owns the fixed upstream URL,
//! model allow-list and secret handle.
//! Inherited Responses routes reuse the same capability and lifecycle boundary;
//! native runtime resolution owns their endpoint, model and Bearer credential.

use anyhow::Context;
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use tiny_http::{Header, Response, Server, StatusCode};
use zeroize::Zeroizing;

const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const BRIDGE_TOKEN_HEADER: &str = "X-CTOX-Bridge-Token";

#[derive(Clone)]
enum CodingProtocol {
    Anthropic,
    Responses { model: String },
    ChatCompletions { model: String },
}

pub(crate) struct AnthropicCodingBridge {
    base_url: String,
    capability_token: Zeroizing<String>,
    stop: Arc<AtomicBool>,
    server: Arc<Server>,
    worker: Option<JoinHandle<()>>,
}

impl AnthropicCodingBridge {
    pub(crate) fn spawn(api_key: String, upstream_base_url: &str) -> anyhow::Result<Self> {
        Self::spawn_protocol(api_key, upstream_base_url, CodingProtocol::Anthropic)
    }

    /// The inherited main route keeps its credential in this native owner too.
    /// It is a per-turn capability, not the retired persistent :12434 gateway.
    pub(crate) fn spawn_responses(
        api_key: String,
        upstream_base_url: &str,
        model: &str,
    ) -> anyhow::Result<Self> {
        Self::spawn_protocol(
            api_key,
            upstream_base_url,
            CodingProtocol::Responses {
                model: model.to_owned(),
            },
        )
    }

    pub(crate) fn spawn_chat_completions(
        api_key: String,
        upstream_base_url: &str,
        model: &str,
    ) -> anyhow::Result<Self> {
        Self::spawn_protocol(
            api_key,
            upstream_base_url,
            CodingProtocol::ChatCompletions {
                model: model.to_owned(),
            },
        )
    }

    fn spawn_protocol(
        api_key: String,
        upstream_base_url: &str,
        protocol: CodingProtocol,
    ) -> anyhow::Result<Self> {
        let mut token_bytes = [0u8; 32];
        SystemRandom::new()
            .fill(&mut token_bytes)
            .map_err(|_| anyhow::anyhow!("generate coding bridge capability token"))?;
        let capability_token = Zeroizing::new(
            token_bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        );
        let server = Arc::new(
            Server::http("127.0.0.1:0")
                .map_err(|error| anyhow::anyhow!(error.to_string()))
                .context("bind Anthropic-compatible coding bridge")?,
        );
        let address = server
            .server_addr()
            .to_ip()
            .context("coding bridge did not bind an IP address")?;
        let base_url = format!("http://{address}");
        let stop = Arc::new(AtomicBool::new(false));
        let worker_server = Arc::clone(&server);
        let worker_stop = Arc::clone(&stop);
        let upstream_base_url = upstream_base_url.trim_end_matches('/').to_owned();
        let worker_capability_token = capability_token.clone();
        let api_key = Zeroizing::new(api_key);
        let worker = std::thread::Builder::new()
            .name("ctox-anthropic-coding-bridge".to_owned())
            .spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    let request = match worker_server.recv_timeout(Duration::from_millis(100)) {
                        Ok(Some(request)) => request,
                        Ok(None) => continue,
                        Err(_) if worker_stop.load(Ordering::Acquire) => break,
                        Err(_) => continue,
                    };
                    handle_request(
                        request,
                        &upstream_base_url,
                        api_key.as_str(),
                        worker_capability_token.as_str(),
                        &protocol,
                    );
                }
            })
            .context("spawn Anthropic-compatible coding bridge")?;
        Ok(Self {
            base_url,
            capability_token,
            stop,
            server,
            worker: Some(worker),
        })
    }

    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    pub(crate) fn capability_token(&self) -> &str {
        &self.capability_token
    }
}

impl Drop for AnthropicCodingBridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.server.unblock();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn handle_request(
    mut request: tiny_http::Request,
    upstream_base_url: &str,
    api_key: &str,
    capability_token: &str,
    protocol: &CodingProtocol,
) {
    let path = match protocol {
        CodingProtocol::Anthropic => "/v1/messages",
        CodingProtocol::Responses { .. } => "/v1/responses",
        CodingProtocol::ChatCompletions { .. } => "/v1/chat/completions",
    };
    if request.method().as_str() != "POST" || request.url() != path {
        let _ = request.respond(Response::from_string("not found").with_status_code(404));
        return;
    }
    let mut presented_tokens = request
        .headers()
        .iter()
        .filter(|header| {
            header
                .field
                .as_str()
                .as_str()
                .eq_ignore_ascii_case(BRIDGE_TOKEN_HEADER)
        })
        .map(|header| header.value.as_str());
    let presented_token = presented_tokens.next();
    let authorized = presented_tokens.next().is_none()
        && presented_token.is_some_and(|presented| {
            let key = hmac::Key::new(hmac::HMAC_SHA256, capability_token.as_bytes());
            let expected = hmac::sign(&key, capability_token.as_bytes());
            hmac::verify(&key, presented.as_bytes(), expected.as_ref()).is_ok()
        });
    if !authorized {
        let _ = request.respond(Response::from_string("unauthorized").with_status_code(401));
        return;
    }
    let declared_length = request.body_length().unwrap_or(0);
    if declared_length > MAX_REQUEST_BYTES {
        let _ = request.respond(Response::from_string("request too large").with_status_code(413));
        return;
    }
    let mut body = Vec::with_capacity(declared_length.min(MAX_REQUEST_BYTES));
    if request
        .as_reader()
        .take((MAX_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .is_err()
        || body.len() > MAX_REQUEST_BYTES
    {
        let _ = request.respond(Response::from_string("invalid request").with_status_code(400));
        return;
    }

    if let CodingProtocol::Responses { model } | CodingProtocol::ChatCompletions { model } =
        protocol
    {
        // A turn capability cannot select a different upstream model.
        let allowed = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .is_some_and(|body| body.get("model").and_then(|value| value.as_str()) == Some(model));
        if !allowed {
            let _ = request
                .respond(Response::from_string("model is outside this turn").with_status_code(403));
            return;
        }
    }

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_write(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(300))
        .timeout(Duration::from_secs(330))
        .redirects(0)
        .build();
    let url = match protocol {
        CodingProtocol::Anthropic => format!("{upstream_base_url}/v1/messages"),
        CodingProtocol::Responses { .. } => format!("{upstream_base_url}/responses"),
        CodingProtocol::ChatCompletions { .. } => format!("{upstream_base_url}/chat/completions"),
    };
    let mut upstream = agent
        .post(&url)
        .set("content-type", "application/json")
        .set("accept", "application/json, text/event-stream");
    upstream = match protocol {
        CodingProtocol::Anthropic => upstream.set("x-api-key", api_key),
        CodingProtocol::Responses { .. } | CodingProtocol::ChatCompletions { .. } => {
            upstream.set("authorization", &format!("Bearer {api_key}"))
        }
    };
    let mut saw_version = false;
    for header in request.headers() {
        let name = header.field.as_str().as_str();
        if matches!(protocol, CodingProtocol::Anthropic)
            && name.eq_ignore_ascii_case("anthropic-version")
        {
            saw_version = true;
            upstream = upstream.set("anthropic-version", header.value.as_str());
        } else if matches!(protocol, CodingProtocol::Anthropic)
            && name.eq_ignore_ascii_case("anthropic-beta")
        {
            upstream = upstream.set("anthropic-beta", header.value.as_str());
        }
    }
    if matches!(protocol, CodingProtocol::Anthropic) && !saw_version {
        upstream = upstream.set("anthropic-version", "2023-06-01");
    }

    let upstream_response = match upstream.send_bytes(&body) {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(_) => {
            let _ = request.respond(
                Response::from_string("coding-plan upstream unavailable").with_status_code(502),
            );
            return;
        }
    };
    let status = StatusCode(upstream_response.status());
    let mut headers = Vec::new();
    for name in ["content-type", "request-id", "x-request-id"] {
        if let Some(value) = upstream_response.header(name) {
            if let Ok(header) = Header::from_bytes(name.as_bytes(), value.as_bytes()) {
                headers.push(header);
            }
        }
    }
    let response = Response::new(status, headers, upstream_response.into_reader(), None, None);
    let _ = request.respond(response);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherited_chat_bridge_uses_selected_provider_endpoint_and_bearer() -> anyhow::Result<()> {
        let upstream =
            Server::http("127.0.0.1:0").map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let address = upstream
            .server_addr()
            .to_ip()
            .context("fake chat upstream")?;
        let worker = std::thread::spawn(move || {
            let mut request = upstream
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .expect("chat request");
            assert_eq!(request.url(), "/v1/chat/completions");
            assert!(request.headers().iter().any(|header| header
                .field
                .as_str()
                .as_str()
                .eq_ignore_ascii_case("authorization")
                && header.value.as_str() == "Bearer native-minimax-key"));
            let mut body = String::new();
            request.as_reader().read_to_string(&mut body).unwrap();
            let body: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(body["model"], "MiniMax-M3");
            assert_eq!(body["messages"][0]["content"], "bounded fixture");
            request.respond(Response::from_string("data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\ndata: [DONE]\n\n").with_header(Header::from_bytes("content-type", "text/event-stream").unwrap())).unwrap();
        });
        let bridge = AnthropicCodingBridge::spawn_chat_completions(
            "native-minimax-key".to_owned(),
            &format!("http://{address}/v1"),
            "MiniMax-M3",
        )?;
        let response = ureq::post(&format!("{}/v1/chat/completions", bridge.base_url()))
            .set(BRIDGE_TOKEN_HEADER, bridge.capability_token())
            .send_string(r#"{"model":"MiniMax-M3","messages":[{"role":"user","content":"bounded fixture"}],"stream":true}"#)?;
        assert!(response.into_string()?.contains("[DONE]"));
        worker.join().expect("chat upstream assertions");
        drop(bridge);
        Ok(())
    }

    #[test]
    fn inherited_responses_bridge_authenticates_scopes_streams_and_stops() -> anyhow::Result<()> {
        const BODY: &str = r#"{"model":"MiniMax-M3","stream":true,"input":[],"tools":[]}"#;
        const SSE: &str = "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"output\":[]}}\n\n";
        let upstream =
            Server::http("127.0.0.1:0").map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let address = upstream.server_addr().to_ip().context("fake upstream")?;
        let worker =
            std::thread::spawn(move || {
                let mut request = upstream
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .expect("one authorized request");
                assert_eq!(request.url(), "/v1/responses");
                let header = |name: &str| {
                    request
                        .headers()
                        .iter()
                        .find(|header| header.field.as_str().as_str().eq_ignore_ascii_case(name))
                        .map(|header| header.value.as_str().to_owned())
                };
                assert_eq!(
                    header("authorization").as_deref(),
                    Some("Bearer native-secret")
                );
                assert!(header(BRIDGE_TOKEN_HEADER).is_none());
                assert!(header("X-CTOX-Provider").is_none());
                assert!(header("anthropic-version").is_none());
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                assert_eq!(body, BODY);
                request
                    .respond(Response::from_string(SSE).with_header(
                        Header::from_bytes("content-type", "text/event-stream").unwrap(),
                    ))
                    .unwrap();
            });
        let bridge = AnthropicCodingBridge::spawn_responses(
            "native-secret".to_owned(),
            &format!("http://{address}/v1"),
            "MiniMax-M3",
        )?;
        let endpoint = format!("{}/v1/responses", bridge.base_url());
        let no_capability = ureq::post(&endpoint).send_string(BODY).unwrap_err();
        assert!(matches!(no_capability, ureq::Error::Status(401, _)));
        let wrong_model = ureq::post(&endpoint)
            .set(BRIDGE_TOKEN_HEADER, bridge.capability_token())
            .send_string(r#"{"model":"other"}"#)
            .unwrap_err();
        assert!(matches!(wrong_model, ureq::Error::Status(403, _)));
        let response = ureq::post(&endpoint)
            .set(BRIDGE_TOKEN_HEADER, bridge.capability_token())
            .set("authorization", "Bearer sidecar-sentinel")
            .set("X-CTOX-Provider", "other-account")
            .send_string(BODY)?;
        assert_eq!(response.header("content-type"), Some("text/event-stream"));
        assert_eq!(response.into_string()?, SSE);
        worker.join().expect("fake upstream assertions");
        let base_url = bridge.base_url().to_owned();
        drop(bridge);
        assert!(
            ureq::get(&base_url).call().is_err(),
            "bridge is reaped with its owner"
        );
        Ok(())
    }

    #[test]
    fn bridge_replaces_the_public_sentinel_without_exposing_the_secret_to_pi() -> anyhow::Result<()>
    {
        let upstream =
            Server::http("127.0.0.1:0").map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let upstream_address = upstream.server_addr().to_ip().context("upstream IP")?;
        let upstream_worker = std::thread::spawn(move || {
            let request = upstream.recv().expect("upstream request");
            assert_eq!(request.url(), "/v1/messages");
            let header = |name: &str| {
                request
                    .headers()
                    .iter()
                    .find(|header| header.field.as_str().as_str().eq_ignore_ascii_case(name))
                    .map(|header| header.value.as_str().to_owned())
            };
            assert_eq!(header("x-api-key").as_deref(), Some("owner-secret"));
            assert_eq!(header("anthropic-version").as_deref(), Some("2023-06-01"));
            assert_eq!(header(BRIDGE_TOKEN_HEADER), None);
            request
                .respond(
                    Response::from_string(r#"{"type":"message","content":[]}"#).with_header(
                        Header::from_bytes("content-type", "application/json").unwrap(),
                    ),
                )
                .unwrap();
        });

        let bridge = AnthropicCodingBridge::spawn(
            "owner-secret".to_owned(),
            &format!("http://{upstream_address}"),
        )?;
        let endpoint = format!("{}/v1/messages", bridge.base_url());
        for presented in [None, Some("wrong-token")] {
            let mut request = ureq::post(&endpoint)
                .set("x-api-key", "ctox-loopback")
                .set("content-type", "application/json");
            if let Some(presented) = presented {
                request = request.set(BRIDGE_TOKEN_HEADER, presented);
            }
            let error = request
                .send_string(r#"{"model":"MiniMax-M3","messages":[]}"#)
                .unwrap_err();
            assert!(matches!(error, ureq::Error::Status(401, _)));
        }
        let response = ureq::post(&format!("{}/v1/messages", bridge.base_url()))
            .set("x-api-key", "ctox-loopback")
            .set(BRIDGE_TOKEN_HEADER, bridge.capability_token())
            .set("content-type", "application/json")
            .send_string(r#"{"model":"MiniMax-M3","messages":[]}"#)?;
        assert_eq!(response.status(), 200);
        assert!(response.into_string()?.contains("message"));
        drop(bridge);
        upstream_worker.join().expect("upstream worker");
        Ok(())
    }

    #[test]
    fn bridge_preserves_anthropic_tools_thinking_usage_and_sse_bytes() -> anyhow::Result<()> {
        const REQUEST_BODY: &str = r#"{"model":"MiniMax-M3","stream":true,"system":"be exact","messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"plan","signature":"sig"},{"type":"tool_use","id":"tool-1","name":"read_file","input":{"path":"a.rs"}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":"ok"}]}],"tools":[{"name":"read_file","description":"read","input_schema":{"type":"object"}}]}"#;
        const RESPONSE_BODY: &str = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"usage\":{\"input_tokens\":12,\"output_tokens\":0}}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"plan\"}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tool-2\",\"name\":\"write_file\",\"input\":{}}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":8}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";

        let upstream =
            Server::http("127.0.0.1:0").map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let upstream_address = upstream.server_addr().to_ip().context("upstream IP")?;
        let upstream_worker = std::thread::spawn(move || {
            let mut request = upstream.recv().expect("upstream request");
            let mut body = String::new();
            request.as_reader().read_to_string(&mut body).unwrap();
            assert_eq!(body, REQUEST_BODY);
            assert!(request.headers().iter().any(|header| {
                header
                    .field
                    .as_str()
                    .as_str()
                    .eq_ignore_ascii_case("anthropic-beta")
                    && header.value.as_str() == "interleaved-thinking-2025-05-14"
            }));
            request
                .respond(
                    Response::from_string(RESPONSE_BODY)
                        .with_header(
                            Header::from_bytes("content-type", "text/event-stream").unwrap(),
                        )
                        .with_header(Header::from_bytes("x-request-id", "request-1").unwrap()),
                )
                .unwrap();
        });

        let bridge = AnthropicCodingBridge::spawn(
            "format-secret".to_owned(),
            &format!("http://{upstream_address}"),
        )?;
        let response = ureq::post(&format!("{}/v1/messages", bridge.base_url()))
            .set(BRIDGE_TOKEN_HEADER, bridge.capability_token())
            .set("anthropic-beta", "interleaved-thinking-2025-05-14")
            .set("content-type", "application/json")
            .send_string(REQUEST_BODY)?;
        assert_eq!(response.status(), 200);
        assert_eq!(response.header("content-type"), Some("text/event-stream"));
        assert_eq!(response.header("x-request-id"), Some("request-1"));
        assert_eq!(response.into_string()?, RESPONSE_BODY);
        drop(bridge);
        upstream_worker.join().expect("upstream worker");
        Ok(())
    }

    #[test]
    fn bridge_preserves_provider_error_status_body_and_request_id() -> anyhow::Result<()> {
        const ERROR_BODY: &str =
            r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#;
        let upstream =
            Server::http("127.0.0.1:0").map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let upstream_address = upstream.server_addr().to_ip().context("upstream IP")?;
        let upstream_worker = std::thread::spawn(move || {
            let request = upstream.recv().expect("upstream request");
            request
                .respond(
                    Response::from_string(ERROR_BODY)
                        .with_status_code(429)
                        .with_header(
                            Header::from_bytes("content-type", "application/json").unwrap(),
                        )
                        .with_header(Header::from_bytes("request-id", "rate-1").unwrap()),
                )
                .unwrap();
        });

        let bridge = AnthropicCodingBridge::spawn(
            "error-secret".to_owned(),
            &format!("http://{upstream_address}"),
        )?;
        let error = ureq::post(&format!("{}/v1/messages", bridge.base_url()))
            .set(BRIDGE_TOKEN_HEADER, bridge.capability_token())
            .set("content-type", "application/json")
            .send_string(r#"{"model":"MiniMax-M3","messages":[]}"#)
            .unwrap_err();
        let ureq::Error::Status(status, response) = error else {
            anyhow::bail!("expected provider status response")
        };
        assert_eq!(status, 429);
        assert_eq!(response.header("request-id"), Some("rate-1"));
        assert_eq!(response.into_string()?, ERROR_BODY);
        drop(bridge);
        upstream_worker.join().expect("upstream worker");
        Ok(())
    }

    #[test]
    fn bridge_never_follows_an_upstream_redirect_with_the_provider_key() -> anyhow::Result<()> {
        let redirect_target =
            Server::http("127.0.0.1:0").map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let redirect_address = redirect_target
            .server_addr()
            .to_ip()
            .context("redirect target IP")?;
        let upstream =
            Server::http("127.0.0.1:0").map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let upstream_address = upstream.server_addr().to_ip().context("upstream IP")?;
        let upstream_worker = std::thread::spawn(move || {
            let request = upstream.recv().expect("upstream request");
            assert!(request.headers().iter().any(|header| {
                header
                    .field
                    .as_str()
                    .as_str()
                    .eq_ignore_ascii_case("x-api-key")
                    && header.value.as_str() == "redirect-secret"
            }));
            request
                .respond(
                    Response::empty(307).with_header(
                        Header::from_bytes(
                            "location",
                            format!("http://{redirect_address}/steal").as_bytes(),
                        )
                        .unwrap(),
                    ),
                )
                .unwrap();
        });

        let bridge = AnthropicCodingBridge::spawn(
            "redirect-secret".to_owned(),
            &format!("http://{upstream_address}"),
        )?;
        let response = ureq::post(&format!("{}/v1/messages", bridge.base_url()))
            .set(BRIDGE_TOKEN_HEADER, bridge.capability_token())
            .set("content-type", "application/json")
            .send_string(r#"{"model":"MiniMax-M3","messages":[]}"#)?;
        assert_eq!(response.status(), 307);
        assert!(redirect_target
            .recv_timeout(Duration::from_millis(200))?
            .is_none());
        drop(bridge);
        upstream_worker.join().expect("upstream worker");
        Ok(())
    }
}
