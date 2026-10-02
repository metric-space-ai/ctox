// ref: internal/api/server.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::io;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::internal::api::middleware::request_logging::{
    decode_captured_request_body_for_log_with_limit, should_log_request,
    should_skip_method_for_request_logging, RequestMetadata, MAX_DEFERRED_ERROR_REQUEST_BODY_BYTES,
};
use crate::internal::api::middleware::response_writer::{RequestInfo, ResponseWriterWrapper};
use crate::internal::api::server_middleware::RequestLoggingPolicy;
use crate::internal::api::server_routes::{
    resolve_server_route, AuxiliaryRouteHandler, AuxiliaryRouteRequest, AuxiliaryRouteResponse,
    ServerRoute,
};
use crate::sdk::api::handlers::claude::code_handlers::{
    ClaudeMessagesHttpResponse, ClaudeMessagesRouteHandler, ClaudeMessagesRouteResponse,
};
use crate::sdk::api::handlers::openai::openai_responses_handlers::{
    OpenAiResponsesHttpResponse, OpenAiResponsesRouteHandler, OpenAiResponsesRouteResponse,
};
use crate::sdk::api::handlers::request_body::read_request_body;

const MAX_HEADER_BYTES: usize = 32 * 1024;
const MAX_REQUEST_BODY_BYTES: usize = 2 * 1024 * 1024;
const PROVIDER_CONNECTION_HEADER_TIMEOUT: Duration = Duration::from_secs(10);

/// Accepts and serves one connection. CTOX owns listener supervision; this
/// primitive only owns HTTP framing and the ported route dispatch.
pub async fn serve_one_responses_connection<H>(
    listener: &TcpListener,
    handler: &H,
) -> io::Result<()>
where
    H: OpenAiResponsesRouteHandler + ?Sized,
{
    let (mut stream, _) = listener.accept().await?;
    serve_responses_connection(&mut stream, handler).await
}

pub async fn serve_responses_connection<S, H>(stream: &mut S, handler: &H) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    H: OpenAiResponsesRouteHandler + ?Sized,
{
    serve_responses_connection_inner(stream, handler, None).await
}

/// Serves one Claude Messages connection on a listener owned and supervised by
/// CTOX. This remains a separate typed route surface from `/v1/responses`.
pub async fn serve_one_messages_connection<H>(listener: &TcpListener, handler: &H) -> io::Result<()>
where
    H: ClaudeMessagesRouteHandler + ?Sized,
{
    let (mut stream, _) = listener.accept().await?;
    serve_messages_connection(&mut stream, handler).await
}

pub async fn serve_messages_connection<S, H>(stream: &mut S, handler: &H) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    H: ClaudeMessagesRouteHandler + ?Sized,
{
    let mut response = match read_request(stream).await {
        Ok(request) => dispatch_messages_request(request, handler).await,
        Err(error) => ClaudeMessagesRouteResponse::Buffered(ClaudeMessagesHttpResponse::error(
            error.status,
            error.message,
        )),
    };
    write_messages_route_response(stream, &mut response, None).await
}

/// Serves both provider-independent Responses and Claude Messages on one CTOX
/// loopback listener while retaining route-specific handlers and envelopes.
pub async fn serve_provider_connection_with_logging<S, R, C>(
    stream: &mut S,
    responses_handler: &R,
    messages_handler: Option<&C>,
    models_response: &ClaudeMessagesHttpResponse,
    policy: &RequestLoggingPolicy,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    R: OpenAiResponsesRouteHandler + ?Sized,
    C: ClaudeMessagesRouteHandler + ?Sized,
{
    serve_provider_connection_with_timeout(
        stream,
        responses_handler,
        messages_handler,
        models_response,
        policy,
        PROVIDER_CONNECTION_HEADER_TIMEOUT,
    )
    .await
}

/// Serves the core provider routes plus host-authorized auxiliary routes on
/// the same connection. The legacy entry point remains unchanged and simply
/// runs without an auxiliary handler.
pub async fn serve_provider_connection_with_auxiliary_logging<S, R, C>(
    stream: &mut S,
    responses_handler: &R,
    messages_handler: Option<&C>,
    models_response: &ClaudeMessagesHttpResponse,
    auxiliary_handler: Option<&dyn AuxiliaryRouteHandler>,
    policy: &RequestLoggingPolicy,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    R: OpenAiResponsesRouteHandler + ?Sized,
    C: ClaudeMessagesRouteHandler + ?Sized,
{
    serve_provider_connection_with_auxiliary_timeout(
        stream,
        responses_handler,
        messages_handler,
        models_response,
        auxiliary_handler,
        policy,
        PROVIDER_CONNECTION_HEADER_TIMEOUT,
    )
    .await
}

async fn serve_provider_connection_with_timeout<S, R, C>(
    stream: &mut S,
    responses_handler: &R,
    messages_handler: Option<&C>,
    models_response: &ClaudeMessagesHttpResponse,
    policy: &RequestLoggingPolicy,
    header_timeout: Duration,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    R: OpenAiResponsesRouteHandler + ?Sized,
    C: ClaudeMessagesRouteHandler + ?Sized,
{
    serve_provider_connection_with_auxiliary_timeout(
        stream,
        responses_handler,
        messages_handler,
        models_response,
        None,
        policy,
        header_timeout,
    )
    .await
}

async fn serve_provider_connection_with_auxiliary_timeout<S, R, C>(
    stream: &mut S,
    responses_handler: &R,
    messages_handler: Option<&C>,
    models_response: &ClaudeMessagesHttpResponse,
    auxiliary_handler: Option<&dyn AuxiliaryRouteHandler>,
    policy: &RequestLoggingPolicy,
    header_timeout: Duration,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    R: OpenAiResponsesRouteHandler + ?Sized,
    C: ClaudeMessagesRouteHandler + ?Sized,
{
    let request = match tokio::time::timeout(header_timeout, read_request(stream)).await {
        Err(_) => return Ok(()),
        Ok(Ok(request)) => request,
        Ok(Err(error)) => {
            let response = OpenAiResponsesHttpResponse::error(error.status, error.message);
            return write_response(stream, &response).await;
        }
    };
    let mut response_writer = response_writer_for_request(&request, policy);
    let route = resolve_server_route(&request.target);
    let write_result = if route == ServerRoute::Models {
        let mut response = ClaudeMessagesRouteResponse::Buffered(if request.method == "GET" {
            models_response.clone()
        } else {
            ClaudeMessagesHttpResponse::error(405, "method not allowed")
        });
        prepare_messages_response_writer(response_writer.as_mut(), &response);
        write_messages_route_response(stream, &mut response, response_writer.as_mut()).await
    } else if route == ServerRoute::Messages {
        let mut response = match messages_handler {
            Some(handler) => dispatch_messages_request(request, handler).await,
            None => ClaudeMessagesRouteResponse::Buffered(ClaudeMessagesHttpResponse::error(
                404,
                "route not found",
            )),
        };
        prepare_messages_response_writer(response_writer.as_mut(), &response);
        write_messages_route_response(stream, &mut response, response_writer.as_mut()).await
    } else if matches!(
        route,
        ServerRoute::CountTokens
            | ServerRoute::AlphaSearch
            | ServerRoute::Live
            | ServerRoute::Realtime
    ) {
        let response = if !route.allows_method(&request.method) {
            AuxiliaryRouteResponse::json_error(405, "method not allowed")
        } else if let Some(handler) = auxiliary_handler {
            let auxiliary_request = AuxiliaryRouteRequest {
                route,
                method: request.method,
                target: request.target,
                provider: request.provider,
                headers: request.headers,
                body: request.body,
            };
            handler
                .handle(auxiliary_request)
                .await
                .unwrap_or_else(|| AuxiliaryRouteResponse::json_error(404, "route not found"))
        } else {
            AuxiliaryRouteResponse::json_error(404, "route not found")
        };
        prepare_auxiliary_response_writer(response_writer.as_mut(), &response);
        write_auxiliary_response(stream, &response, response_writer.as_mut()).await
    } else {
        let mut response = dispatch_request(request, responses_handler).await;
        prepare_response_writer(response_writer.as_mut(), &response);
        write_route_response(stream, &mut response, response_writer.as_mut()).await
    };
    if let Some(writer) = response_writer {
        let logging_result = tokio::task::spawn_blocking(move || writer.finalize_with_outcome())
            .await
            .map_err(|_| io::Error::other("request logger worker panicked"))
            .and_then(|result| result);
        policy.metrics().record(&logging_result);
    }
    write_result
}

pub async fn serve_responses_connection_with_logging<S, H>(
    stream: &mut S,
    handler: &H,
    policy: &RequestLoggingPolicy,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    H: OpenAiResponsesRouteHandler + ?Sized,
{
    serve_responses_connection_inner(stream, handler, Some(policy)).await
}

async fn serve_responses_connection_inner<S, H>(
    stream: &mut S,
    handler: &H,
    policy: Option<&RequestLoggingPolicy>,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    H: OpenAiResponsesRouteHandler + ?Sized,
{
    let mut response = match read_request(stream).await {
        Ok(request) => {
            let mut response_writer =
                policy.and_then(|policy| response_writer_for_request(&request, policy));
            let mut response = dispatch_request(request, handler).await;
            prepare_response_writer(response_writer.as_mut(), &response);
            let write_result =
                write_route_response(stream, &mut response, response_writer.as_mut()).await;
            if let (Some(policy), Some(writer)) = (policy, response_writer) {
                let logging_result =
                    tokio::task::spawn_blocking(move || writer.finalize_with_outcome())
                        .await
                        .map_err(|_| io::Error::other("request logger worker panicked"))
                        .and_then(|result| result);
                policy.metrics().record(&logging_result);
            }
            return write_result;
        }
        Err(error) => OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
            error.status,
            error.message,
        )),
    };
    write_route_response(stream, &mut response, None).await
}

pub(super) struct ParsedRequest {
    pub(super) method: String,
    pub(super) target: String,
    pub(super) provider: Option<String>,
    pub(super) headers: BTreeMap<String, Vec<String>>,
    pub(super) body: Vec<u8>,
}

pub(super) struct RequestReadError {
    pub(super) status: u16,
    pub(super) message: &'static str,
}

async fn dispatch_request<H>(request: ParsedRequest, handler: &H) -> OpenAiResponsesRouteResponse
where
    H: OpenAiResponsesRouteHandler + ?Sized,
{
    let path = request.target.split('?').next().unwrap_or(&request.target);
    if path != "/v1/responses" {
        return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
            404,
            "route not found",
        ));
    }
    if request.method != "POST" {
        return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
            405,
            "method not allowed",
        ));
    }
    handler
        .handle_provider_route(request.provider.as_deref(), &request.body)
        .await
}

async fn dispatch_messages_request<H>(
    request: ParsedRequest,
    handler: &H,
) -> ClaudeMessagesRouteResponse
where
    H: ClaudeMessagesRouteHandler + ?Sized,
{
    let path = request.target.split('?').next().unwrap_or(&request.target);
    if path != "/v1/messages" {
        return ClaudeMessagesRouteResponse::Buffered(ClaudeMessagesHttpResponse::error(
            404,
            "route not found",
        ));
    }
    if request.method != "POST" {
        return ClaudeMessagesRouteResponse::Buffered(ClaudeMessagesHttpResponse::error(
            405,
            "method not allowed",
        ));
    }
    handler
        .handle_provider_route(request.provider.as_deref(), &request.body)
        .await
}

pub(super) async fn read_request<S>(stream: &mut S) -> Result<ParsedRequest, RequestReadError>
where
    S: AsyncRead + Unpin,
{
    let mut buffer = Vec::with_capacity(4096);
    let header_end = loop {
        if buffer.len() >= MAX_HEADER_BYTES {
            return Err(RequestReadError {
                status: 431,
                message: "request headers are too large",
            });
        }
        let mut chunk = [0_u8; 4096];
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|_| RequestReadError {
                status: 400,
                message: "request could not be read",
            })?;
        if read == 0 {
            return Err(RequestReadError {
                status: 400,
                message: "request ended before headers were complete",
            });
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(index) = find_header_end(&buffer) {
            break index;
        }
    };

    let headers = std::str::from_utf8(&buffer[..header_end]).map_err(|_| RequestReadError {
        status: 400,
        message: "request headers are not valid UTF-8",
    })?;
    let mut lines = headers.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().split_whitespace();
    let method = request_line.next().unwrap_or_default().to_owned();
    let target = request_line.next().unwrap_or_default().to_owned();
    let version = request_line.next().unwrap_or_default();
    if method.is_empty()
        || target.is_empty()
        || !matches!(version, "HTTP/1.0" | "HTTP/1.1")
        || request_line.next().is_some()
    {
        return Err(RequestReadError {
            status: 400,
            message: "invalid HTTP request line",
        });
    }

    let mut content_length = None;
    let mut provider = None;
    let mut parsed_headers = BTreeMap::<String, Vec<String>>::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(RequestReadError {
                status: 400,
                message: "invalid HTTP header",
            });
        };
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(RequestReadError {
                status: 400,
                message: "transfer encoding is not supported",
            });
        }
        if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(RequestReadError {
                    status: 400,
                    message: "duplicate content length",
                });
            }
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| RequestReadError {
                        status: 400,
                        message: "invalid content length",
                    })?,
            );
        }
        if name.eq_ignore_ascii_case("x-ctox-provider") {
            if provider.is_some() || value.trim().is_empty() {
                return Err(RequestReadError {
                    status: 400,
                    message: "invalid provider selection",
                });
            }
            provider = Some(value.trim().to_owned());
        }
        parsed_headers
            .entry(name.trim().to_owned())
            .or_default()
            .push(value.trim().to_owned());
    }
    let content_length = match content_length {
        Some(content_length) => content_length,
        None if matches!(method.as_str(), "GET" | "HEAD" | "DELETE") => 0,
        None => {
            return Err(RequestReadError {
                status: 411,
                message: "content length is required",
            })
        }
    };
    if content_length > MAX_REQUEST_BODY_BYTES {
        return Err(RequestReadError {
            status: 413,
            message: "request body is too large",
        });
    }

    let body_start = header_end + 4;
    let available = buffer.len().saturating_sub(body_start);
    if available < content_length {
        buffer.resize(body_start + content_length, 0);
        stream
            .read_exact(&mut buffer[body_start + available..body_start + content_length])
            .await
            .map_err(|_| RequestReadError {
                status: 400,
                message: "request body ended early",
            })?;
    }
    let raw_body = &buffer[body_start..body_start + content_length];
    let content_encoding = header_value(&parsed_headers, "content-encoding").map(str::to_owned);
    let body = if raw_body.is_empty() {
        Vec::new()
    } else {
        read_request_body(raw_body, content_encoding.as_deref()).map_err(|_| RequestReadError {
            status: 400,
            message: "request content encoding is invalid",
        })?
    };
    if content_encoding
        .as_deref()
        .is_some_and(|encoding| !encoding.trim().is_empty())
    {
        parsed_headers.retain(|key, _| !key.eq_ignore_ascii_case("content-encoding"));
    }
    Ok(ParsedRequest {
        method,
        target,
        provider,
        headers: parsed_headers,
        body,
    })
}

fn response_writer_for_request(
    request: &ParsedRequest,
    policy: &RequestLoggingPolicy,
) -> Option<ResponseWriterWrapper> {
    let path = request.target.split('?').next().unwrap_or(&request.target);
    let metadata = RequestMetadata {
        method: request.method.clone(),
        path: path.to_owned(),
        headers: request.headers.clone(),
        content_length: request.body.len() as i64,
        has_body: !request.body.is_empty(),
    };
    if should_skip_method_for_request_logging(Some(&metadata)) || !should_log_request(path) {
        return None;
    }
    let encoding = header_value(&request.headers, "content-encoding").unwrap_or_default();
    let body = decode_captured_request_body_for_log_with_limit(
        &request.body,
        encoding,
        MAX_DEFERRED_ERROR_REQUEST_BODY_BYTES as u64,
    );
    let request_id = header_value(&request.headers, "x-request-id")
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut writer = ResponseWriterWrapper::new(
        policy.logger(),
        RequestInfo {
            url: request.target.clone(),
            method: request.method.clone(),
            headers: request.headers.clone(),
            body,
            request_id,
        },
    );
    writer.set_log_on_error_only(policy.log_on_error_only());
    Some(writer)
}

fn header_value<'a>(headers: &'a BTreeMap<String, Vec<String>>, name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, values)| values.first())
        .map(String::as_str)
}

fn prepare_response_writer(
    writer: Option<&mut ResponseWriterWrapper>,
    response: &OpenAiResponsesRouteResponse,
) {
    let Some(writer) = writer else {
        return;
    };
    let (status, content_type) = match response {
        OpenAiResponsesRouteResponse::Buffered(response) => {
            (response.status(), response.content_type())
        }
        OpenAiResponsesRouteResponse::Stream(_)
        | OpenAiResponsesRouteResponse::CodexStream(_)
        | OpenAiResponsesRouteResponse::AntigravityStream(_) => (200, "text/event-stream"),
    };
    writer.write_header(
        status,
        BTreeMap::from([("Content-Type".to_owned(), vec![content_type.to_owned()])]),
    );
}

fn prepare_messages_response_writer(
    writer: Option<&mut ResponseWriterWrapper>,
    response: &ClaudeMessagesRouteResponse,
) {
    let Some(writer) = writer else {
        return;
    };
    let (status, content_type) = match response {
        ClaudeMessagesRouteResponse::Buffered(response) => {
            (response.status(), response.content_type())
        }
        ClaudeMessagesRouteResponse::Stream(_) => (200, "text/event-stream"),
    };
    writer.write_header(
        status,
        BTreeMap::from([("Content-Type".to_owned(), vec![content_type.to_owned()])]),
    );
}

fn prepare_auxiliary_response_writer(
    writer: Option<&mut ResponseWriterWrapper>,
    response: &AuxiliaryRouteResponse,
) {
    let Some(writer) = writer else {
        return;
    };
    writer.write_header(response.status, response.headers.clone());
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

async fn write_response<S>(stream: &mut S, response: &OpenAiResponsesHttpResponse) -> io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status(),
        reason_phrase(response.status()),
        response.content_type(),
        response.body().len(),
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(response.body()).await?;
    stream.shutdown().await
}

async fn write_response_with_capture<S>(
    stream: &mut S,
    response: &OpenAiResponsesHttpResponse,
    capture: Option<&mut ResponseWriterWrapper>,
) -> io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status(),
        reason_phrase(response.status()),
        response.content_type(),
        response.body().len(),
    );
    stream.write_all(head.as_bytes()).await?;
    let body_result = stream.write_all(response.body()).await;
    if let Some(capture) = capture {
        capture.write(response.body());
    }
    body_result?;
    stream.shutdown().await
}

async fn write_route_response<S>(
    stream: &mut S,
    response: &mut OpenAiResponsesRouteResponse,
    mut capture: Option<&mut ResponseWriterWrapper>,
) -> io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    match response {
        OpenAiResponsesRouteResponse::Buffered(response) => {
            if capture.is_some() {
                write_response_with_capture(stream, response, capture).await
            } else {
                write_response(stream, response).await
            }
        }
        OpenAiResponsesRouteResponse::Stream(stream_response) => {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
                )
                .await?;
            while let Some(chunk) = stream_response.next_chunk().await {
                let chunk_result = stream.write_all(&chunk).await;
                if let Some(capture) = capture.as_deref_mut() {
                    capture.write(&chunk);
                }
                chunk_result?;
                // Translator chunks contain one SSE event without its blank-line
                // record terminator. Preserve event boundaries on the wire.
                let delimiter_result = stream.write_all(b"\n\n").await;
                if let Some(capture) = capture.as_deref_mut() {
                    capture.write(b"\n\n");
                }
                delimiter_result?;
            }
            stream.shutdown().await
        }
        OpenAiResponsesRouteResponse::CodexStream(stream_response) => {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
                )
                .await?;
            while let Some(chunk) = stream_response.next_chunk().await {
                let chunk_result = stream.write_all(&chunk).await;
                if let Some(capture) = capture.as_deref_mut() {
                    capture.write(&chunk);
                }
                chunk_result?;
            }
            stream.shutdown().await
        }
        OpenAiResponsesRouteResponse::AntigravityStream(stream_response) => {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
                )
                .await?;
            while let Some(chunk) = stream_response.next_chunk().await {
                let chunk_result = stream.write_all(&chunk).await;
                if let Some(capture) = capture.as_deref_mut() {
                    capture.write(&chunk);
                }
                chunk_result?;
                let delimiter_result = stream.write_all(b"\n\n").await;
                if let Some(capture) = capture.as_deref_mut() {
                    capture.write(b"\n\n");
                }
                delimiter_result?;
            }
            stream.shutdown().await
        }
    }
}

async fn write_messages_route_response<S>(
    stream: &mut S,
    response: &mut ClaudeMessagesRouteResponse,
    mut capture: Option<&mut ResponseWriterWrapper>,
) -> io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    match response {
        ClaudeMessagesRouteResponse::Buffered(response) => {
            let head = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response.status(),
                reason_phrase(response.status()),
                response.content_type(),
                response.body().len(),
            );
            stream.write_all(head.as_bytes()).await?;
            let body_result = stream.write_all(response.body()).await;
            if let Some(capture) = capture.as_deref_mut() {
                capture.write(response.body());
            }
            body_result?;
        }
        ClaudeMessagesRouteResponse::Stream(stream_response) => {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
                )
                .await?;
            while let Some(chunk) = stream_response.next_chunk().await {
                let chunk_result = stream.write_all(&chunk).await;
                if let Some(capture) = capture.as_deref_mut() {
                    capture.write(&chunk);
                }
                chunk_result?;
            }
        }
    }
    stream.shutdown().await
}

async fn write_auxiliary_response<S>(
    stream: &mut S,
    response: &AuxiliaryRouteResponse,
    capture: Option<&mut ResponseWriterWrapper>,
) -> io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    let content_type =
        header_value(&response.headers, "content-type").unwrap_or("application/octet-stream");
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        reason_phrase(response.status),
        content_type,
        response.body.len(),
    );
    for (name, values) in &response.headers {
        if name.eq_ignore_ascii_case("content-type")
            || name.eq_ignore_ascii_case("content-length")
            || name.eq_ignore_ascii_case("connection")
            || name.eq_ignore_ascii_case("transfer-encoding")
            || !valid_http_header_component(name)
        {
            continue;
        }
        for value in values {
            if valid_http_header_component(value) {
                head.push_str(name);
                head.push_str(": ");
                head.push_str(value);
                head.push_str("\r\n");
            }
        }
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&response.body).await?;
    if let Some(capture) = capture {
        capture.write(&response.body);
    }
    stream.shutdown().await
}

fn valid_http_header_component(value: &str) -> bool {
    !value.is_empty()
        && !value
            .as_bytes()
            .iter()
            .any(|byte| matches!(byte, b'\r' | b'\n'))
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        413 => "Content Too Large",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime};

    use serde_json::Value;
    use tokio::net::TcpStream;
    use tokio::sync::{mpsc, oneshot};

    use super::*;
    use crate::internal::api::server_routes::{
        ClaudeCountTokensRouteHandler, CodexAlphaSearchAuthSelector, CodexAlphaSearchClient,
        CodexAlphaSearchFuture, CodexAlphaSearchRouteHandler, CodexAlphaSearchSelectionFuture,
        CodexAlphaSearchTransport, CodexAlphaSearchTransportRequest,
    };
    use crate::internal::auth::antigravity::{
        AntigravityCredentialHandles, AntigravityRefreshCoordinator,
        AntigravityRefreshHttpResponse, AntigravityRefreshRequest, AntigravityRefreshTransport,
        AntigravityRefreshTransportFailure, AntigravitySecretHandle, AntigravitySecretKind,
        AntigravitySecretStore, AntigravityStoredCredentials, AntigravityTokenError,
        SecretString as AntigravitySecretString,
    };
    use crate::internal::auth::claude::{
        ClaudeCredentialHandles, ClaudeRefreshCoordinator, ClaudeRefreshTransport,
        ClaudeSecretHandle, ClaudeSecretKind, ClaudeSecretStore, ClaudeStoredCredentials,
        RefreshClock, RefreshHttpResponse, RefreshRequest, RefreshTransportFailure,
        SecretStoreError, SecretString,
    };
    use crate::internal::auth::codex::{
        CodexCredentialHandles, CodexRefreshCoordinator, CodexRefreshHttpResponse,
        CodexRefreshRequest, CodexRefreshTransport, CodexRefreshTransportFailure,
        CodexSecretHandle, CodexSecretKind, CodexSecretStore, CodexStoredCredentials,
        RefreshClock as CodexRefreshClock, SecretStoreError as CodexSecretStoreError,
        SecretString as CodexSecretString,
    };
    use crate::internal::runtime::executor::{
        AccountStateClock, AntigravityAuthClock, AntigravityGenerateRequest,
        AntigravityGenerateResponse, AntigravityGenerateStreamResponse,
        AntigravityGenerateStreamingTransport, AntigravityGenerateTransport,
        AntigravityGenerateTransportFailure, AntigravitySubscriptionAccountPool,
        AntigravitySubscriptionAuth, AntigravitySubscriptionExecutor, AntigravityUpstreamTarget,
        ClaudeMessagesRequest, ClaudeMessagesResponse, ClaudeMessagesStreamResponse,
        ClaudeMessagesStreamingTransport, ClaudeMessagesTransport, ClaudeMessagesTransportFailure,
        ClaudeSubscriptionAuth, ClaudeSubscriptionMessagesExecutor, ClaudeUpstreamTarget,
        CodexResponsesRequest, CodexResponsesResponse, CodexResponsesStreamResponse,
        CodexResponsesStreamingTransport, CodexResponsesTransport, CodexResponsesTransportFailure,
        CodexSubscriptionAccountPool, CodexSubscriptionAuth, CodexSubscriptionResponsesExecutor,
        CodexUpstreamTarget,
    };
    use crate::sdk::api::handlers::claude::code_handlers::{
        claude_models_response, ClaudeMessagesAntigravityHandler,
    };
    use crate::sdk::api::handlers::openai::openai_responses_handlers::{
        OpenAiResponsesAntigravityHandler, OpenAiResponsesClaudeHandler,
        OpenAiResponsesCodexHandler, OpenAiResponsesProviderRouter,
    };
    use crate::sdk::cliproxy::auth::{
        AccountCandidate, AccountRouter, Auth, CooldownConductor, CooldownStateRecord,
        CooldownStateStore, CooldownStoreError,
    };
    use crate::sdk::cliproxy::executor::Headers;

    #[derive(Default)]
    struct AlphaRouteSelectorProbe(Mutex<Vec<(String, Headers, Vec<u8>)>>);

    impl CodexAlphaSearchAuthSelector for AlphaRouteSelectorProbe {
        fn select<'a>(
            &'a self,
            model: &'a str,
            headers: &'a Headers,
            original_body: &'a [u8],
        ) -> CodexAlphaSearchSelectionFuture<'a> {
            self.0.lock().unwrap().push((
                model.to_owned(),
                headers.clone(),
                original_body.to_vec(),
            ));
            Box::pin(async {
                let mut auth = Auth::default();
                auth.id = "selected-alpha-account".to_owned();
                Ok(auth)
            })
        }
    }

    #[derive(Default)]
    struct AlphaRouteTransportProbe(Mutex<Vec<CodexAlphaSearchTransportRequest>>);

    impl CodexAlphaSearchTransport for AlphaRouteTransportProbe {
        fn execute<'a>(
            &'a self,
            request: CodexAlphaSearchTransportRequest,
        ) -> CodexAlphaSearchFuture<'a> {
            self.0.lock().unwrap().push(request);
            Box::pin(async {
                Ok(
                    crate::internal::api::server_routes::CodexAlphaSearchResponse {
                        status: 200,
                        headers: Headers::from_iter([
                            (
                                "Content-Type".to_owned(),
                                vec!["application/json".to_owned()],
                            ),
                            ("X-Search-Backend".to_owned(), vec!["codex".to_owned()]),
                            ("Content-Length".to_owned(), vec!["999999".to_owned()]),
                        ]),
                        body: br#"{"results":["rust"]}"#.to_vec(),
                    },
                )
            })
        }
    }

    struct MemorySecretStore(Mutex<ClaudeStoredCredentials>);

    impl ClaudeSecretStore for MemorySecretStore {
        fn load_credentials(
            &self,
            _handles: &ClaudeCredentialHandles,
        ) -> Result<ClaudeStoredCredentials, SecretStoreError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn store_credentials(
            &self,
            _handles: &ClaudeCredentialHandles,
            credentials: &ClaudeStoredCredentials,
        ) -> Result<(), SecretStoreError> {
            *self.0.lock().unwrap() = credentials.clone();
            Ok(())
        }
    }

    struct UnusedRefreshTransport;

    impl ClaudeRefreshTransport for UnusedRefreshTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a RefreshRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<RefreshHttpResponse, RefreshTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Err(RefreshTransportFailure::Protocol) })
        }
    }

    struct FixedRefreshClock;

    impl RefreshClock for FixedRefreshClock {
        fn now(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_000)
        }

        fn sleep(
            &self,
            _duration: Duration,
        ) -> Pin<Box<dyn Future<Output = Result<(), RefreshTransportFailure>> + Send + '_>>
        {
            Box::pin(async { Ok(()) })
        }
    }

    #[derive(Default)]
    struct MemoryCooldownStore(Mutex<Vec<CooldownStateRecord>>);

    impl CooldownStateStore for MemoryCooldownStore {
        fn load(&self) -> Result<Vec<CooldownStateRecord>, CooldownStoreError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn save(&self, records: &[CooldownStateRecord]) -> Result<(), CooldownStoreError> {
            *self.0.lock().unwrap() = records.to_vec();
            Ok(())
        }
    }

    struct FixedAccountClock;

    impl AccountStateClock for FixedAccountClock {
        fn now_ms(&self) -> i64 {
            10_000
        }
    }

    struct CodexMemorySecretStore(Mutex<CodexStoredCredentials>);

    impl CodexSecretStore for CodexMemorySecretStore {
        fn load_credentials(
            &self,
            _handles: &CodexCredentialHandles,
        ) -> Result<CodexStoredCredentials, CodexSecretStoreError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn store_credentials(
            &self,
            _handles: &CodexCredentialHandles,
            credentials: &CodexStoredCredentials,
        ) -> Result<(), CodexSecretStoreError> {
            *self.0.lock().unwrap() = credentials.clone();
            Ok(())
        }
    }

    struct UnusedCodexRefreshTransport;

    impl CodexRefreshTransport for UnusedCodexRefreshTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a CodexRefreshRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<CodexRefreshHttpResponse, CodexRefreshTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Err(CodexRefreshTransportFailure::Protocol) })
        }
    }

    struct FixedCodexRefreshClock;

    impl CodexRefreshClock for FixedCodexRefreshClock {
        fn now(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_000)
        }

        fn sleep(
            &self,
            _duration: Duration,
        ) -> Pin<Box<dyn Future<Output = Result<(), CodexRefreshTransportFailure>> + Send + '_>>
        {
            Box::pin(async { Ok(()) })
        }
    }

    #[derive(Default)]
    struct CodexResponseTransport(Mutex<Vec<Vec<u8>>>);

    impl CodexResponsesTransport for CodexResponseTransport {
        fn execute<'a>(
            &'a self,
            request: &'a CodexResponsesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<CodexResponsesResponse, CodexResponsesTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.0.lock().unwrap().push(request.body().to_vec());
                Ok(CodexResponsesResponse::new(
                    200,
                    None,
                    b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_codex\",\"object\":\"response\",\"status\":\"completed\",\"output\":[]}}\n\n".to_vec(),
                ))
            })
        }
    }

    impl CodexResponsesStreamingTransport for CodexResponseTransport {
        fn execute_stream<'a>(
            &'a self,
            request: &'a CodexResponsesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            CodexResponsesStreamResponse,
                            CodexResponsesTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.0.lock().unwrap().push(request.body().to_vec());
                let (sender, receiver) = mpsc::channel(8);
                tokio::spawn(async move {
                    let chunks = [
                        b"data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_codex_stream\",\"status\":\"in_progress\"}}\n\n".to_vec(),
                        b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_codex_stream\",\"object\":\"response\",\"status\":\"completed\",\"output\":[]}}\n\n".to_vec(),
                    ];
                    for chunk in chunks {
                        if sender.send(Ok(chunk)).await.is_err() {
                            break;
                        }
                    }
                });
                Ok(CodexResponsesStreamResponse::new(200, None, receiver))
            })
        }
    }

    struct AntigravityMemorySecretStore(Mutex<AntigravityStoredCredentials>);

    impl AntigravitySecretStore for AntigravityMemorySecretStore {
        fn load_credentials(
            &self,
            _handles: &AntigravityCredentialHandles,
        ) -> Result<AntigravityStoredCredentials, AntigravityTokenError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn store_credentials(
            &self,
            _handles: &AntigravityCredentialHandles,
            credentials: &AntigravityStoredCredentials,
        ) -> Result<(), AntigravityTokenError> {
            *self.0.lock().unwrap() = credentials.clone();
            Ok(())
        }
    }

    struct UnusedAntigravityRefreshTransport;

    impl AntigravityRefreshTransport for UnusedAntigravityRefreshTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a AntigravityRefreshRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            AntigravityRefreshHttpResponse,
                            AntigravityRefreshTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Err(AntigravityRefreshTransportFailure::Protocol) })
        }
    }

    struct FixedAntigravityClock;

    impl AntigravityAuthClock for FixedAntigravityClock {
        fn now(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_000)
        }
    }

    struct AntigravityResponseTransport(Mutex<Vec<Vec<u8>>>, u16, Option<Vec<u8>>);

    impl AntigravityResponseTransport {
        fn rejected(status: u16, body: Vec<u8>) -> Self {
            Self(Mutex::new(Vec::new()), status, Some(body))
        }
    }

    impl Default for AntigravityResponseTransport {
        fn default() -> Self {
            Self(Mutex::new(Vec::new()), 200, None)
        }
    }

    impl AntigravityGenerateTransport for AntigravityResponseTransport {
        fn execute<'a>(
            &'a self,
            request: &'a AntigravityGenerateRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            AntigravityGenerateResponse,
                            AntigravityGenerateTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.0.lock().unwrap().push(request.body().to_vec());
                Ok(AntigravityGenerateResponse::new(
                    self.1,
                    None,
                    self.2.clone().unwrap_or_else(|| br#"{"response":{"candidates":[{"content":{"parts":[{"text":"hello from Antigravity"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":2,"candidatesTokenCount":3,"totalTokenCount":5}}}"#.to_vec()),
                ))
            })
        }
    }

    impl AntigravityGenerateStreamingTransport for AntigravityResponseTransport {
        fn execute_stream<'a>(
            &'a self,
            request: &'a AntigravityGenerateRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            AntigravityGenerateStreamResponse,
                            AntigravityGenerateTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.0.lock().unwrap().push(request.body().to_vec());
                let (sender, receiver) = mpsc::channel(8);
                if (200..300).contains(&self.1) {
                    tokio::spawn(async move {
                        let chunks = [
                            b"data: {\"response\":{\"responseId\":\"resp_antigravity_stream\",\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hello \"}]}}]}}\n\n".to_vec(),
                            b"data: {\"response\":{\"responseId\":\"resp_antigravity_stream\",\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"stream\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":2,\"candidatesTokenCount\":3,\"totalTokenCount\":5}}}\n\n".to_vec(),
                        ];
                        for chunk in chunks {
                            if sender.send(Ok(chunk)).await.is_err() {
                                break;
                            }
                        }
                    });
                }
                Ok(AntigravityGenerateStreamResponse::new(
                    self.1, None, receiver,
                ))
            })
        }
    }

    type CapturedCountRequest = (Vec<u8>, String, String, Vec<String>);

    struct SseTransport {
        requests: Mutex<Vec<(Vec<u8>, bool)>>,
        count_requests: Mutex<Vec<CapturedCountRequest>>,
        status: u16,
        body: Vec<u8>,
    }

    type ClaudeTestPool = Arc<crate::internal::runtime::executor::ClaudeSubscriptionAccountPool>;
    type ClaudeTestHandler = (Arc<OpenAiResponsesClaudeHandler>, Arc<SseTransport>);

    impl SseTransport {
        fn new(status: u16, body: Vec<u8>) -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
                count_requests: Mutex::new(Vec::new()),
                status,
                body,
            }
        }
    }

    impl ClaudeMessagesTransport for SseTransport {
        fn execute<'a>(
            &'a self,
            request: &'a ClaudeMessagesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<ClaudeMessagesResponse, ClaudeMessagesTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.requests
                    .lock()
                    .unwrap()
                    .push((request.body().to_vec(), request.stream()));
                Ok(ClaudeMessagesResponse::new(self.status, self.body.clone()))
            })
        }

        fn execute_count_tokens<'a>(
            &'a self,
            request: &'a ClaudeMessagesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<ClaudeMessagesResponse, ClaudeMessagesTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.count_requests.lock().unwrap().push((
                    request.body().to_vec(),
                    request.fingerprint().session_id().to_owned(),
                    request.fingerprint().device().user_agent().to_owned(),
                    request.betas().to_vec(),
                ));
                Ok(
                    ClaudeMessagesResponse::new(self.status, self.body.clone()).with_headers(
                        Headers::from_iter([
                            (
                                "Content-Type".to_owned(),
                                vec!["application/json".to_owned()],
                            ),
                            ("X-Upstream-Count".to_owned(), vec!["native".to_owned()]),
                        ]),
                    ),
                )
            })
        }
    }

    impl ClaudeMessagesStreamingTransport for SseTransport {
        fn execute_stream<'a>(
            &'a self,
            request: &'a ClaudeMessagesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            ClaudeMessagesStreamResponse,
                            ClaudeMessagesTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.requests
                    .lock()
                    .unwrap()
                    .push((request.body().to_vec(), request.stream()));
                let (sender, receiver) = mpsc::channel(64);
                if (200..300).contains(&self.status) {
                    let body = self.body.clone();
                    tokio::spawn(async move {
                        for chunk in body.chunks(17) {
                            if sender.send(Ok(chunk.to_vec())).await.is_err() {
                                return;
                            }
                        }
                    });
                }
                Ok(ClaudeMessagesStreamResponse::new(
                    self.status,
                    None,
                    receiver,
                ))
            })
        }
    }

    fn claude_sse() -> Vec<u8> {
        b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_http\",\"usage\":{\"input_tokens\":3,\"output_tokens\":0}}}\n\n\
          data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
          data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello from Claude\"}}\n\n\
          data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
          data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\n\n\
          data: {\"type\":\"message_stop\"}\n\n"
            .to_vec()
    }

    fn handler() -> ClaudeTestHandler {
        handler_with_response(200, claude_sse())
    }

    fn handler_with_response(status: u16, response_body: Vec<u8>) -> ClaudeTestHandler {
        let (pool, transport) = claude_pool_with_response(status, response_body);
        (Arc::new(OpenAiResponsesClaudeHandler::new(pool)), transport)
    }

    fn claude_pool_with_response(
        status: u16,
        response_body: Vec<u8>,
    ) -> (ClaudeTestPool, Arc<SseTransport>) {
        let handles = ClaudeCredentialHandles::new(
            ClaudeSecretHandle::new("subscriptions", "access-a", ClaudeSecretKind::AccessToken)
                .unwrap(),
            ClaudeSecretHandle::new("subscriptions", "refresh-a", ClaudeSecretKind::RefreshToken)
                .unwrap(),
        )
        .unwrap();
        let secret_store = Arc::new(MemorySecretStore(Mutex::new(ClaudeStoredCredentials::new(
            SecretString::new("access-secret").unwrap(),
            SecretString::new("refresh-secret").unwrap(),
        ))));
        let auth = Arc::new(ClaudeSubscriptionAuth::new(
            handles,
            secret_store,
            Arc::new(UnusedRefreshTransport),
            Arc::new(FixedRefreshClock),
            Arc::new(ClaudeRefreshCoordinator::default()),
        ));
        let cooldowns = Arc::new(MemoryCooldownStore::default());
        let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
        let transport = Arc::new(SseTransport::new(status, response_body));
        let executor = Arc::new(
            ClaudeSubscriptionMessagesExecutor::new(
                auth,
                transport.clone(),
                Duration::from_secs(30),
            )
            .with_stream_transport(transport.clone())
            .with_account_state_clock("account-a", conductor, Arc::new(FixedAccountClock))
            .unwrap(),
        );
        let pool = crate::internal::runtime::executor::ClaudeSubscriptionAccountPool::with_clock(
            Arc::new(AccountRouter::new(cooldowns)),
            vec![AccountCandidate {
                auth_id: "account-a".to_owned(),
                provider: "claude".to_owned(),
                priority: 0,
                weight: 1,
                websocket_enabled: false,
                supported_models: Vec::new(),
                disabled: false,
            }],
            HashMap::from([("account-a".to_owned(), executor)]),
            Arc::new(FixedAccountClock),
        )
        .unwrap()
        .with_targets(HashMap::from([(
            "account-a".to_owned(),
            ClaudeUpstreamTarget::new("https", "api.anthropic.com").unwrap(),
        )]))
        .unwrap();
        (Arc::new(pool), transport)
    }

    fn codex_handler() -> (
        Arc<OpenAiResponsesCodexHandler>,
        Arc<CodexResponseTransport>,
    ) {
        let handles = CodexCredentialHandles::new(
            CodexSecretHandle::new("subscriptions", "codex-id", CodexSecretKind::IdToken).unwrap(),
            CodexSecretHandle::new(
                "subscriptions",
                "codex-access",
                CodexSecretKind::AccessToken,
            )
            .unwrap(),
            CodexSecretHandle::new(
                "subscriptions",
                "codex-refresh",
                CodexSecretKind::RefreshToken,
            )
            .unwrap(),
        )
        .unwrap();
        let secret_store = Arc::new(CodexMemorySecretStore(Mutex::new(
            CodexStoredCredentials::new(
                CodexSecretString::new("invalid-jwt").unwrap(),
                CodexSecretString::new("codex-access-secret").unwrap(),
                CodexSecretString::new("codex-refresh-secret").unwrap(),
            ),
        )));
        let auth = Arc::new(CodexSubscriptionAuth::new(
            handles,
            secret_store,
            Arc::new(UnusedCodexRefreshTransport),
            Arc::new(FixedCodexRefreshClock),
            Arc::new(CodexRefreshCoordinator::default()),
        ));
        let cooldowns = Arc::new(MemoryCooldownStore::default());
        let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
        let transport = Arc::new(CodexResponseTransport::default());
        let executor = Arc::new(
            CodexSubscriptionResponsesExecutor::new(
                auth,
                transport.clone(),
                Duration::from_secs(30),
            )
            .unwrap()
            .with_stream_transport(transport.clone()),
        );
        let pool = CodexSubscriptionAccountPool::with_clock(
            Arc::new(AccountRouter::new(cooldowns)),
            conductor,
            vec![AccountCandidate {
                auth_id: "codex-a".to_owned(),
                provider: "codex".to_owned(),
                priority: 0,
                weight: 1,
                websocket_enabled: false,
                supported_models: Vec::new(),
                disabled: false,
            }],
            HashMap::from([("codex-a".to_owned(), executor)]),
            HashMap::from([(
                "codex-a".to_owned(),
                CodexUpstreamTarget::new("https://chatgpt.example/backend-api/codex").unwrap(),
            )]),
            Arc::new(FixedAccountClock),
        )
        .unwrap();
        (
            Arc::new(OpenAiResponsesCodexHandler::new(Arc::new(pool))),
            transport,
        )
    }

    fn antigravity_pool() -> (
        Arc<AntigravitySubscriptionAccountPool>,
        Arc<AntigravityResponseTransport>,
    ) {
        let transport = Arc::new(AntigravityResponseTransport::default());
        let pool = antigravity_pool_with_transport(Arc::clone(&transport));
        (pool, transport)
    }

    fn antigravity_pool_with_transport(
        transport: Arc<AntigravityResponseTransport>,
    ) -> Arc<AntigravitySubscriptionAccountPool> {
        let handle =
            |name, kind| AntigravitySecretHandle::new("subscriptions", name, kind).unwrap();
        let handles = AntigravityCredentialHandles::new(
            handle("antigravity-access", AntigravitySecretKind::AccessToken),
            handle("antigravity-refresh", AntigravitySecretKind::RefreshToken),
            handle("antigravity-state", AntigravitySecretKind::State),
        )
        .unwrap();
        let credentials = AntigravityStoredCredentials::new(
            AntigravitySecretString::new("antigravity-access-secret").unwrap(),
            AntigravitySecretString::new("antigravity-refresh-secret").unwrap(),
            SystemTime::UNIX_EPOCH + Duration::from_secs(3_600),
            "project-a",
        )
        .unwrap();
        let auth = Arc::new(AntigravitySubscriptionAuth::new(
            handles,
            Arc::new(AntigravityMemorySecretStore(Mutex::new(credentials))),
            Arc::new(UnusedAntigravityRefreshTransport),
            Arc::new(FixedAntigravityClock),
            Arc::new(AntigravityRefreshCoordinator::default()),
        ));
        let cooldowns = Arc::new(MemoryCooldownStore::default());
        let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
        let executor = Arc::new(
            AntigravitySubscriptionExecutor::new(auth, transport.clone(), Duration::from_secs(30))
                .unwrap()
                .with_stream_transport(transport.clone()),
        );
        let pool = AntigravitySubscriptionAccountPool::with_clock(
            Arc::new(AccountRouter::new(cooldowns)),
            conductor,
            vec![AccountCandidate {
                auth_id: "antigravity-a".to_owned(),
                provider: "antigravity".to_owned(),
                priority: 0,
                weight: 1,
                websocket_enabled: false,
                supported_models: Vec::new(),
                disabled: false,
            }],
            HashMap::from([("antigravity-a".to_owned(), executor)]),
            HashMap::from([(
                "antigravity-a".to_owned(),
                AntigravityUpstreamTarget::new("https://daily-cloudcode-pa.googleapis.com")
                    .unwrap(),
            )]),
            Arc::new(FixedAccountClock),
        )
        .unwrap();
        Arc::new(pool)
    }

    fn antigravity_handler() -> (
        Arc<OpenAiResponsesAntigravityHandler>,
        Arc<AntigravityResponseTransport>,
    ) {
        let (pool, transport) = antigravity_pool();
        (
            Arc::new(OpenAiResponsesAntigravityHandler::new(pool)),
            transport,
        )
    }

    #[tokio::test]
    async fn real_http_loopback_runs_responses_through_claude_pool() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (handler, transport) = handler();
        let server = tokio::spawn(async move {
            serve_one_responses_connection(&listener, &handler)
                .await
                .unwrap();
        });

        let body = br#"{"model":"claude-sonnet-4-5","input":[{"role":"user","content":"hello"}]}"#;
        let mut client = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        client.write_all(request.as_bytes()).await.unwrap();
        client.write_all(body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let split = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap();
        let head = std::str::from_utf8(&response[..split]).unwrap();
        let response_body: Value = serde_json::from_slice(&response[split + 4..]).unwrap();
        assert!(head.starts_with("HTTP/1.1 200 OK\r\n"));
        assert_eq!(response_body["object"], "response");
        assert_eq!(response_body["status"], "completed");
        assert_eq!(
            response_body["output"][0]["content"][0]["text"],
            "hello from Claude"
        );
        assert_eq!(response_body["model"], "claude-sonnet-4-5");
        assert_eq!(response_body["usage"]["input_tokens"], 3);
        assert_eq!(response_body["usage"]["output_tokens"], 4);

        let requests = transport.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].1,
            "translated non-stream route must aggregate upstream SSE"
        );
        let upstream: Value = serde_json::from_slice(&requests[0].0).unwrap();
        assert_eq!(upstream["model"], "claude-sonnet-4-5");
        assert_eq!(upstream["stream"], true);
        let first_content = upstream["messages"][0]["content"]
            .as_array()
            .expect("Candidate cloaking wraps the first user content in blocks");
        assert!(first_content.iter().any(|block| block["text"] == "hello"));
        assert!(first_content.iter().any(|block| {
            block["text"]
                .as_str()
                .is_some_and(|text| text.contains("# currentDate"))
        }));
        assert!(!String::from_utf8_lossy(&requests[0].0).contains("access-secret"));
    }

    #[tokio::test]
    async fn provider_header_selects_codex_independently_of_model_name() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (claude, claude_transport) = handler();
        let (codex, codex_transport) = codex_handler();
        let router = Arc::new(
            OpenAiResponsesProviderRouter::new("claude", Some(claude), Some(codex), None).unwrap(),
        );
        let server = tokio::spawn(async move {
            serve_one_responses_connection(&listener, &router)
                .await
                .unwrap();
        });

        let body = br#"{"model":"claude-sonnet-4-5","input":"use the selected subscription"}"#;
        let mut client = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: localhost\r\nX-CTOX-Provider: codex\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        client.write_all(request.as_bytes()).await.unwrap();
        client.write_all(body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let split = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap();
        assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
        let payload: Value = serde_json::from_slice(&response[split + 4..]).unwrap();
        assert_eq!(payload["id"], "resp_codex");
        assert!(claude_transport.requests.lock().unwrap().is_empty());
        let requests = codex_transport.0.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let upstream: Value = serde_json::from_slice(&requests[0]).unwrap();
        assert_eq!(upstream["model"], "claude-sonnet-4-5");
        assert_eq!(upstream["stream"], true);
        assert!(!String::from_utf8_lossy(&response).contains("codex-access-secret"));
    }

    #[tokio::test]
    async fn provider_header_selects_antigravity_independently_of_model_name() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (claude, claude_transport) = handler();
        let (antigravity, antigravity_transport) = antigravity_handler();
        let router = Arc::new(
            OpenAiResponsesProviderRouter::new("claude", Some(claude), None, Some(antigravity))
                .unwrap(),
        );
        let server = tokio::spawn(async move {
            serve_one_responses_connection(&listener, &router)
                .await
                .unwrap();
        });

        let body = br#"{"model":"claude-sonnet-4-5","input":"use the selected subscription"}"#;
        let mut client = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: localhost\r\nX-CTOX-Provider: antigravity\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        client.write_all(request.as_bytes()).await.unwrap();
        client.write_all(body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let split = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap();
        assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
        let payload: Value = serde_json::from_slice(&response[split + 4..]).unwrap();
        assert_eq!(payload["status"], "completed");
        assert_eq!(
            payload["output"][0]["content"][0]["text"],
            "hello from Antigravity"
        );
        assert!(claude_transport.requests.lock().unwrap().is_empty());
        let requests = antigravity_transport.0.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let upstream: Value = serde_json::from_slice(&requests[0]).unwrap();
        assert_eq!(upstream["model"], "claude-sonnet-4-5");
        assert!(!String::from_utf8_lossy(&response).contains("antigravity-access-secret"));
    }

    #[tokio::test]
    async fn antigravity_provider_stream_forwards_ordered_responses_events() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (antigravity, _) = antigravity_handler();
        let router = Arc::new(
            OpenAiResponsesProviderRouter::new("antigravity", None, None, Some(antigravity))
                .unwrap(),
        );
        let server = tokio::spawn(async move {
            serve_one_responses_connection(&listener, &router)
                .await
                .unwrap();
        });

        let body = br#"{"model":"gemini-3-flash-agent","stream":true,"session_id":"server-stream","input":"hello"}"#;
        let mut client = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: localhost\r\nX-CTOX-Provider: antigravity\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        client.write_all(request.as_bytes()).await.unwrap();
        client.write_all(body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let text = String::from_utf8(response).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Type: text/event-stream\r\n"));
        assert!(!text.contains("Content-Length:"));
        let created = text.find("event: response.created").unwrap();
        let delta = text.find("event: response.output_text.delta").unwrap();
        let completed = text.find("event: response.completed").unwrap();
        assert!(created < delta && delta < completed);
        assert!(!text.contains("antigravity-access-secret"));
    }

    #[tokio::test]
    async fn real_http_loopback_runs_claude_messages_through_antigravity_pool() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (pool, transport) = antigravity_pool();
        let handler = Arc::new(ClaudeMessagesAntigravityHandler::new(
            pool,
            None,
            Arc::new(|_, _| false),
        ));
        let server = tokio::spawn(async move {
            serve_one_messages_connection(&listener, &handler)
                .await
                .unwrap();
        });

        let body =
            br#"{"model":"claude-sonnet-4-5","messages":[{"role":"user","content":"hello"}]}"#;
        let mut client = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "POST /v1/messages HTTP/1.1\r\nHost: localhost\r\nX-CTOX-Provider: antigravity\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        client.write_all(request.as_bytes()).await.unwrap();
        client.write_all(body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let split = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap();
        assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
        let payload: Value = serde_json::from_slice(&response[split + 4..]).unwrap();
        assert_eq!(payload["type"], "message");
        assert_eq!(payload["content"][0]["text"], "hello from Antigravity");
        let requests = transport.0.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let upstream: Value = serde_json::from_slice(&requests[0]).unwrap();
        assert_eq!(upstream["model"], "claude-sonnet-4-5");
        assert_eq!(
            upstream["request"]["contents"][0]["parts"][0]["text"],
            "hello"
        );
        assert!(!String::from_utf8_lossy(&response).contains("antigravity-access-secret"));
    }

    #[tokio::test]
    async fn real_http_loopback_decodes_zstd_before_messages_routing() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (pool, transport) = antigravity_pool();
        let handler = Arc::new(ClaudeMessagesAntigravityHandler::new(
            pool,
            None,
            Arc::new(|_, _| false),
        ));
        let server = tokio::spawn(async move {
            serve_one_messages_connection(&listener, &handler)
                .await
                .unwrap();
        });

        let raw =
            br#"{"model":"claude-sonnet-4-5","messages":[{"role":"user","content":"hello zstd"}]}"#;
        let body = zstd::stream::encode_all(raw.as_slice(), 1).unwrap();
        let mut client = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "POST /v1/messages HTTP/1.1\r\nHost: localhost\r\nContent-Encoding: zstd\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        client.write_all(request.as_bytes()).await.unwrap();
        client.write_all(&body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
        let requests = transport.0.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let upstream: Value = serde_json::from_slice(&requests[0]).unwrap();
        assert_eq!(
            upstream["request"]["contents"][0]["parts"][0]["text"],
            "hello zstd"
        );
    }

    #[tokio::test]
    async fn claude_messages_http_stream_preserves_anthropic_event_order() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (pool, _) = antigravity_pool();
        let handler = Arc::new(ClaudeMessagesAntigravityHandler::new(
            pool,
            None,
            Arc::new(|_, _| false),
        ));
        let server = tokio::spawn(async move {
            serve_one_messages_connection(&listener, &handler)
                .await
                .unwrap();
        });

        let body = br#"{"model":"claude-sonnet-4-5","stream":true,"messages":[{"role":"user","content":"hello"}]}"#;
        let mut client = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "POST /v1/messages HTTP/1.1\r\nHost: localhost\r\nX-CTOX-Provider: antigravity\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        client.write_all(request.as_bytes()).await.unwrap();
        client.write_all(body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let text = String::from_utf8(response).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Type: text/event-stream\r\n"));
        assert!(!text.contains("Content-Length:"));
        let start = text.find("event: message_start").unwrap();
        let delta = text.find("event: content_block_delta").unwrap();
        let stop = text.find("event: message_stop").unwrap();
        assert!(start < delta && delta < stop);
        assert!(!text.contains("event: response.created"));
        assert!(!text.contains("antigravity-access-secret"));
    }

    #[tokio::test]
    async fn pending_claude_stream_error_remains_buffered_before_http_commit() {
        let transport = Arc::new(AntigravityResponseTransport::rejected(
            400,
            br#"{"error":{"type":"invalid_request_error","message":"upstream token antigravity-access-secret"}}"#.to_vec(),
        ));
        let pool = antigravity_pool_with_transport(transport);
        let handler = ClaudeMessagesAntigravityHandler::new(pool, None, Arc::new(|_, _| false));

        let response = handler
            .handle_route(br#"{"model":"claude-sonnet-4-5","stream":true,"messages":[]}"#)
            .await;
        let ClaudeMessagesRouteResponse::Buffered(response) = response else {
            panic!("upstream error before bootstrap must not commit Claude SSE")
        };
        assert_eq!(response.status(), 400);
        assert_eq!(response.content_type(), "application/json");
        let body: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body["type"], "error");
        assert_eq!(body["error"]["type"], "invalid_request_error");
        assert!(!String::from_utf8_lossy(response.body()).contains("access-secret"));
    }

    #[tokio::test]
    async fn combined_provider_listener_dispatches_messages_with_logging_policy() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (pool, _) = antigravity_pool();
        let responses = Arc::new(
            OpenAiResponsesProviderRouter::new(
                "antigravity",
                None,
                None,
                Some(Arc::new(OpenAiResponsesAntigravityHandler::new(
                    Arc::clone(&pool),
                ))),
            )
            .unwrap(),
        );
        let messages = Arc::new(ClaudeMessagesAntigravityHandler::new(
            pool,
            None,
            Arc::new(|_, _| false),
        ));
        let logs_dir = std::env::temp_dir().join(format!(
            "ctox-provider-combined-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let metrics_registry =
            crate::internal::api::server_middleware::RequestLoggingMetricsRegistry::default();
        let policy = Arc::new(
            crate::internal::api::server_middleware::RequestLoggingPolicy::error_only_scoped(
                &metrics_registry,
                &logs_dir,
                "combined-provider-test",
                logs_dir.join("logs"),
                2,
            ),
        );
        let models = claude_models_response(&[], false);
        let server_policy = Arc::clone(&policy);
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            serve_provider_connection_with_logging(
                &mut stream,
                responses.as_ref(),
                Some(messages.as_ref()),
                &models,
                server_policy.as_ref(),
            )
            .await
            .unwrap();
        });

        let body =
            br#"{"model":"claude-sonnet-4-5","messages":[{"role":"user","content":"hello"}]}"#;
        let mut client = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "POST /v1/messages HTTP/1.1\r\nHost: localhost\r\nX-CTOX-Provider: antigravity\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        client.write_all(request.as_bytes()).await.unwrap();
        client.write_all(body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let split = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap();
        let payload: Value = serde_json::from_slice(&response[split + 4..]).unwrap();
        assert_eq!(payload["type"], "message");
        assert_eq!(policy.metrics().snapshot().logger_failures, 0);
        let _ = fs::remove_dir_all(logs_dir);
    }

    #[tokio::test]
    async fn combined_provider_listener_serves_bounded_claude_model_snapshot() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (pool, _) = antigravity_pool();
        let responses = Arc::new(
            OpenAiResponsesProviderRouter::new(
                "antigravity",
                None,
                None,
                Some(Arc::new(OpenAiResponsesAntigravityHandler::new(
                    Arc::clone(&pool),
                ))),
            )
            .unwrap(),
        );
        let messages = Arc::new(ClaudeMessagesAntigravityHandler::new(
            pool,
            None,
            Arc::new(|_, _| false),
        ));
        let catalog = vec![serde_json::json!({
            "id":"gpt-4o",
            "object":"model",
            "owned_by":"ctox",
            "display_name":"GPT-4o",
            "providers":["antigravity","codex"]
        })
        .as_object()
        .unwrap()
        .clone()];
        let models = claude_models_response(&catalog, false);
        let logs_dir = std::env::temp_dir().join(format!(
            "ctox-provider-models-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let metrics_registry =
            crate::internal::api::server_middleware::RequestLoggingMetricsRegistry::default();
        let policy = Arc::new(
            crate::internal::api::server_middleware::RequestLoggingPolicy::error_only_scoped(
                &metrics_registry,
                &logs_dir,
                "combined-provider-models-test",
                logs_dir.join("logs"),
                2,
            ),
        );
        let server_policy = Arc::clone(&policy);
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            serve_provider_connection_with_logging(
                &mut stream,
                responses.as_ref(),
                Some(messages.as_ref()),
                &models,
                server_policy.as_ref(),
            )
            .await
            .unwrap();
        });

        let mut client = TcpStream::connect(address).await.unwrap();
        client
            .write_all(b"GET /v1/models HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let split = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap();
        assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
        let payload: Value = serde_json::from_slice(&response[split + 4..]).unwrap();
        assert_eq!(payload["data"][0]["id"], "claude-fable-5-dd-o4-tpg");
        assert_eq!(
            payload["data"][0]["providers"],
            serde_json::json!(["antigravity", "codex"])
        );
        assert!(!String::from_utf8_lossy(&response).contains("access-secret"));
        let _ = fs::remove_dir_all(logs_dir);
    }

    #[tokio::test]
    async fn host_style_accept_loop_is_not_blocked_by_idle_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (pool, _) = antigravity_pool();
        let responses = Arc::new(
            OpenAiResponsesProviderRouter::new(
                "antigravity",
                None,
                None,
                Some(Arc::new(OpenAiResponsesAntigravityHandler::new(
                    Arc::clone(&pool),
                ))),
            )
            .unwrap(),
        );
        let messages = Arc::new(ClaudeMessagesAntigravityHandler::new(
            pool,
            None,
            Arc::new(|_, _| false),
        ));
        let models = claude_models_response(&[], false);
        let logs_dir = std::env::temp_dir().join(format!(
            "ctox-provider-idle-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let metrics_registry =
            crate::internal::api::server_middleware::RequestLoggingMetricsRegistry::default();
        let policy = Arc::new(
            crate::internal::api::server_middleware::RequestLoggingPolicy::error_only_scoped(
                &metrics_registry,
                &logs_dir,
                "idle-connection-test",
                logs_dir.join("logs"),
                2,
            ),
        );
        let (first_accepted, first_accepted_rx) = oneshot::channel();
        let (idle_done, idle_done_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut idle_stream, _) = listener.accept().await.unwrap();
            let idle_responses = Arc::clone(&responses);
            let idle_messages = Arc::clone(&messages);
            let idle_models = models.clone();
            let idle_policy = Arc::clone(&policy);
            tokio::spawn(async move {
                let result = serve_provider_connection_with_timeout(
                    &mut idle_stream,
                    idle_responses.as_ref(),
                    Some(idle_messages.as_ref()),
                    &idle_models,
                    idle_policy.as_ref(),
                    Duration::from_millis(25),
                )
                .await;
                let _ = idle_done.send(result);
            });
            let _ = first_accepted.send(());

            let (mut live_stream, _) = listener.accept().await.unwrap();
            serve_provider_connection_with_logging(
                &mut live_stream,
                responses.as_ref(),
                Some(messages.as_ref()),
                &models,
                policy.as_ref(),
            )
            .await
            .unwrap();
        });

        let idle_client = TcpStream::connect(address).await.unwrap();
        first_accepted_rx.await.unwrap();
        let mut live_client = TcpStream::connect(address).await.unwrap();
        live_client
            .write_all(b"GET /v1/models HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        tokio::time::timeout(
            Duration::from_secs(1),
            live_client.read_to_end(&mut response),
        )
        .await
        .expect("idle peer must not block the next accepted HTTP connection")
        .unwrap();
        assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
        tokio::time::timeout(Duration::from_secs(1), idle_done_rx)
            .await
            .expect("idle connection must be released after its header deadline")
            .unwrap()
            .unwrap();
        drop(idle_client);
        server.await.unwrap();
        let _ = fs::remove_dir_all(logs_dir);
    }

    #[tokio::test]
    async fn codex_provider_stream_commits_and_forwards_incremental_responses_events() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (codex, _) = codex_handler();
        let router =
            Arc::new(OpenAiResponsesProviderRouter::new("codex", None, Some(codex), None).unwrap());
        let server = tokio::spawn(async move {
            serve_one_responses_connection(&listener, &router)
                .await
                .unwrap();
        });

        let body = br#"{"model":"gpt-5.5","stream":true,"input":"hello"}"#;
        let mut client = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: localhost\r\nX-CTOX-Provider: codex\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        client.write_all(request.as_bytes()).await.unwrap();
        client.write_all(body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let text = String::from_utf8(response).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Type: text/event-stream\r\n"));
        assert!(!text.contains("Content-Length:"));
        let created = text.find("response.created").unwrap();
        let completed = text.find("response.completed").unwrap();
        assert!(created < completed);
        assert!(!text.contains("codex-access-secret"));
    }

    #[tokio::test]
    async fn real_http_stream_commits_only_after_bootstrap_and_preserves_event_order() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (handler, transport) = handler();
        let server = tokio::spawn(async move {
            serve_one_responses_connection(&listener, &handler)
                .await
                .unwrap();
        });

        let body = br#"{"model":"claude-sonnet-4-5","stream":true,"input":[{"role":"user","content":"hello"}]}"#;
        let mut client = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        client.write_all(request.as_bytes()).await.unwrap();
        client.write_all(body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let text = String::from_utf8(response).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Type: text/event-stream\r\n"));
        assert!(!text.contains("Content-Length:"));
        let created = text.find("event: response.created").unwrap();
        let delta = text.find("event: response.output_text.delta").unwrap();
        let completed = text.find("event: response.completed").unwrap();
        assert!(created < delta && delta < completed);
        assert!(!text.contains("access-secret"));
        assert_eq!(transport.requests.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn post_commit_provider_error_is_redacted_terminal_sse() {
        let error_sse = b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_failed\"}}\n\n\
            data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"token access-secret\"}}\n\n"
            .to_vec();
        let (handler, _) = handler_with_response(200, error_sse);
        let response = handler
            .handle_route(br#"{"model":"claude-sonnet-4-5","stream":true,"input":[]}"#)
            .await;
        let OpenAiResponsesRouteResponse::Stream(mut stream) = response else {
            panic!("expected committed stream bootstrap");
        };
        let mut chunks = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            chunks.push(String::from_utf8(chunk).unwrap());
        }
        let joined = chunks.join("\n\n");
        assert!(joined.contains("event: response.failed"));
        assert!(joined.contains("overloaded_error"));
        assert!(joined.contains("Claude upstream stream failed"));
        assert!(!joined.contains("access-secret"));
        assert!(!joined.contains("response.completed"));
    }

    #[tokio::test]
    async fn upstream_failure_remains_http_error_before_stream_commit() {
        let (handler, _) = handler_with_response(429, b"token access-secret".to_vec());
        let response = handler
            .handle_route(br#"{"model":"claude-sonnet-4-5","stream":true,"input":[]}"#)
            .await;
        let OpenAiResponsesRouteResponse::Buffered(response) = response else {
            panic!("upstream failure must not commit SSE headers");
        };
        assert_eq!(response.status(), 429);
        assert!(!String::from_utf8_lossy(response.body()).contains("access-secret"));
    }

    #[tokio::test]
    async fn typed_error_logging_policy_is_consumed_by_real_http_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (handler, _) = handler_with_response(429, b"token access-secret".to_vec());
        let logs_dir = std::env::temp_dir().join(format!(
            "ctox-server-logging-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let registry_root = logs_dir.join("root");
        let metrics_registry =
            crate::internal::api::server_middleware::RequestLoggingMetricsRegistry::default();
        let policy = Arc::new(
            crate::internal::api::server_middleware::RequestLoggingPolicy::error_only_scoped(
                &metrics_registry,
                &registry_root,
                "test-listener",
                &logs_dir,
                2,
            ),
        );
        let server_policy = Arc::clone(&policy);
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            serve_responses_connection_with_logging(
                &mut stream,
                handler.as_ref(),
                server_policy.as_ref(),
            )
            .await
            .unwrap();
        });

        let body = br#"{"model":"claude-sonnet-4-5","stream":true,"input":[]}"#;
        let mut client = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "POST /v1/responses?token=client-secret-token HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer client-authorization-secret\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        client.write_all(request.as_bytes()).await.unwrap();
        client.write_all(body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        assert!(response.starts_with(b"HTTP/1.1 429 Too Many Requests\r\n"));
        let metrics = policy.metrics().snapshot();
        assert_eq!(metrics.finalized_logs, 1);
        assert_eq!(metrics.forced_error_logs, 1);
        assert_eq!(metrics.logger_failures, 0);
        let root_metrics = metrics_registry.snapshot_for_root(&registry_root);
        assert_eq!(root_metrics.total, metrics);
        assert_eq!(root_metrics.scopes["test-listener"], metrics);
        let log_path = fs::read_dir(&logs_dir)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let log = fs::read_to_string(log_path).unwrap();
        assert!(log.contains("token=clie...oken"));
        assert!(log.contains("Authorization: Bearer clie...cret"));
        assert!(!log.contains("client-secret-token"));
        assert!(!log.contains("client-authorization-secret"));
        assert!(fs::remove_dir_all(logs_dir).is_ok());
    }

    #[tokio::test]
    async fn real_http_loopback_reaches_native_claude_count_tokens_route() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (responses, _) = handler();
        let (count_pool, count_transport) =
            claude_pool_with_response(200, br#"{"input_tokens":37}"#.to_vec());
        let auxiliary = Arc::new(ClaudeCountTokensRouteHandler::new(count_pool));
        let models = claude_models_response(&[], false);
        let logs_dir = std::env::temp_dir().join(format!(
            "ctox-claude-count-route-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let metrics_registry =
            crate::internal::api::server_middleware::RequestLoggingMetricsRegistry::default();
        let policy = Arc::new(
            crate::internal::api::server_middleware::RequestLoggingPolicy::error_only_scoped(
                &metrics_registry,
                &logs_dir,
                "claude-count-route-test",
                logs_dir.join("logs"),
                2,
            ),
        );
        let server_policy = policy.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            serve_provider_connection_with_auxiliary_logging(
                &mut stream,
                responses.as_ref(),
                Option::<&ClaudeMessagesAntigravityHandler>::None,
                &models,
                Some(auxiliary.as_ref()),
                server_policy.as_ref(),
            )
            .await
            .unwrap();
        });

        let body = br#"{"model":"claude-sonnet-4-5","messages":[{"role":"user","content":"count me"}],"metadata":{"user_id":"session_01H00000000000000000000000_account__session_01H00000000000000000000000"}}"#;
        let mut client = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "POST /v1/messages/count_tokens HTTP/1.1\r\nHost: localhost\r\nX-Ctox-Provider: claude\r\nX-App: cli\r\nUser-Agent: claude-cli/2.1.220\r\nAnthropic-Beta: claude-code-20250219\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        client.write_all(request.as_bytes()).await.unwrap();
        client.write_all(body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let text = String::from_utf8(response).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("X-Upstream-Count: native\r\n"));
        assert!(text.ends_with(r#"{"input_tokens":37}"#));
        let requests = count_transport.count_requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(!requests[0].1.is_empty());
        assert_eq!(requests[0].2, "claude-cli/2.1.280 (external, cli)");
        assert!(requests[0]
            .3
            .iter()
            .any(|beta| beta == "token-counting-2024-11-01"));
        let prepared: Value = serde_json::from_slice(&requests[0].0).unwrap();
        assert_eq!(prepared["model"], "claude-sonnet-4-5");
        assert!(prepared.get("metadata").is_none());
        drop(requests);
        if logs_dir.exists() {
            assert!(fs::remove_dir_all(logs_dir).is_ok());
        }
    }

    #[tokio::test]
    async fn real_http_loopback_reaches_host_injected_codex_alpha_search() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (responses, _) = handler();
        let selector = Arc::new(AlphaRouteSelectorProbe::default());
        let transport = Arc::new(AlphaRouteTransportProbe::default());
        let auxiliary = Arc::new(CodexAlphaSearchRouteHandler::new(
            selector.clone(),
            Arc::new(CodexAlphaSearchClient::new(transport.clone())),
        ));
        let models = claude_models_response(&[], false);
        let logs_dir = std::env::temp_dir().join(format!(
            "ctox-alpha-route-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let metrics_registry =
            crate::internal::api::server_middleware::RequestLoggingMetricsRegistry::default();
        let policy = Arc::new(
            crate::internal::api::server_middleware::RequestLoggingPolicy::error_only_scoped(
                &metrics_registry,
                &logs_dir,
                "alpha-route-test",
                logs_dir.join("logs"),
                2,
            ),
        );
        let server_policy = policy.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            serve_provider_connection_with_auxiliary_logging(
                &mut stream,
                responses.as_ref(),
                Option::<&ClaudeMessagesAntigravityHandler>::None,
                &models,
                Some(auxiliary.as_ref()),
                server_policy.as_ref(),
            )
            .await
            .unwrap();
        });

        let body = br#"{"id":"session-search","model":"gpt-5-search","query":"rust","prompt_cache_key":"private","prompt_cache_retention":"24h"}"#;
        let mut client = TcpStream::connect(address).await.unwrap();
        let request = format!(
            "POST /backend-api/codex/alpha/search?mode=fast HTTP/1.1\r\nHost: localhost\r\nUser-Agent: codex-test/1\r\nX-Client-Request-Id: req-alpha\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        client.write_all(request.as_bytes()).await.unwrap();
        client.write_all(body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let text = String::from_utf8(response).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("X-Search-Backend: codex\r\n"));
        assert!(text.contains("Content-Length: 20\r\n"));
        assert!(!text.contains("999999"));
        assert!(text.ends_with(r#"{"results":["rust"]}"#));

        let selections = selector.0.lock().unwrap();
        assert_eq!(selections.len(), 1);
        assert_eq!(selections[0].0, "gpt-5-search");
        assert_eq!(
            selections[0].1.get("X-Session-ID"),
            Some(&vec!["session-search".to_owned()])
        );
        assert_eq!(selections[0].2, body);
        drop(selections);

        let requests = transport.0.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].auth_id, "selected-alpha-account");
        assert_eq!(requests[0].model, "gpt-5-search");
        assert_eq!(
            requests[0].url,
            "https://chatgpt.com/backend-api/codex/alpha/search"
        );
        let upstream: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(upstream["query"], "rust");
        assert!(upstream.get("prompt_cache_key").is_none());
        assert!(upstream.get("prompt_cache_retention").is_none());
        assert_eq!(
            requests[0].headers.get("User-Agent"),
            Some(&vec!["codex-test/1".to_owned()])
        );
        assert_eq!(policy.metrics().snapshot().logger_failures, 0);
        let _ = fs::remove_dir_all(logs_dir);
    }

    #[tokio::test]
    async fn real_http_loopback_rejects_alpha_search_method_before_authority_selection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (responses, _) = handler();
        let selector = Arc::new(AlphaRouteSelectorProbe::default());
        let auxiliary = Arc::new(CodexAlphaSearchRouteHandler::new(
            selector.clone(),
            Arc::new(CodexAlphaSearchClient::new(Arc::new(
                AlphaRouteTransportProbe::default(),
            ))),
        ));
        let models = claude_models_response(&[], false);
        let logs_dir = std::env::temp_dir().join(format!(
            "ctox-alpha-method-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let metrics_registry =
            crate::internal::api::server_middleware::RequestLoggingMetricsRegistry::default();
        let policy = Arc::new(
            crate::internal::api::server_middleware::RequestLoggingPolicy::error_only_scoped(
                &metrics_registry,
                &logs_dir,
                "alpha-method-test",
                logs_dir.join("logs"),
                2,
            ),
        );
        let server_policy = policy.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            serve_provider_connection_with_auxiliary_logging(
                &mut stream,
                responses.as_ref(),
                Option::<&ClaudeMessagesAntigravityHandler>::None,
                &models,
                Some(auxiliary.as_ref()),
                server_policy.as_ref(),
            )
            .await
            .unwrap();
        });

        let mut client = TcpStream::connect(address).await.unwrap();
        client
            .write_all(b"GET /v1/alpha/search HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        assert!(response.starts_with(b"HTTP/1.1 405 Method Not Allowed\r\n"));
        assert!(selector.0.lock().unwrap().is_empty());
        let _ = fs::remove_dir_all(logs_dir);
    }
}
