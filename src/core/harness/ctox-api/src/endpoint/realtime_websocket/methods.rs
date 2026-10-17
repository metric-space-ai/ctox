use crate::endpoint::realtime_websocket::methods_common::conversation_handoff_append_message;
use crate::endpoint::realtime_websocket::methods_common::conversation_item_create_message;
use crate::endpoint::realtime_websocket::methods_common::normalized_session_mode;
use crate::endpoint::realtime_websocket::methods_common::session_update_session;
use crate::endpoint::realtime_websocket::methods_common::websocket_intent;
use crate::endpoint::realtime_websocket::protocol::RealtimeAudioFrame;
use crate::endpoint::realtime_websocket::protocol::RealtimeEvent;
use crate::endpoint::realtime_websocket::protocol::RealtimeEventParser;
use crate::endpoint::realtime_websocket::protocol::RealtimeOutboundMessage;
use crate::endpoint::realtime_websocket::protocol::RealtimeSessionConfig;
use crate::endpoint::realtime_websocket::protocol::RealtimeSessionMode;
use crate::endpoint::realtime_websocket::protocol::RealtimeTranscriptDelta;
use crate::endpoint::realtime_websocket::protocol::RealtimeTranscriptEntry;
use crate::endpoint::realtime_websocket::protocol::parse_realtime_event;
use crate::error::ApiError;
use crate::provider::Provider;
use ctox_client::maybe_build_rustls_client_config_with_custom_ca;
use ctox_utils_rustls_provider::ensure_rustls_crypto_provider;
use futures::SinkExt;
use futures::StreamExt;
use http::HeaderMap;
use http::HeaderValue;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::trace;
use tungstenite::protocol::WebSocketConfig;
use url::Url;

struct WsStream {
    tx_command: mpsc::Sender<WsCommand>,
    pump_task: tokio::task::JoinHandle<()>,
}

enum WsCommand {
    Send {
        message: Message,
        tx_result: oneshot::Sender<Result<(), WsError>>,
    },
    Close {
        tx_result: oneshot::Sender<Result<(), WsError>>,
    },
}

impl WsStream {
    fn new(
        inner: WebSocketStream<MaybeTlsStream<TcpStream>>,
    ) -> (Self, mpsc::UnboundedReceiver<Result<Message, WsError>>) {
        let (tx_command, mut rx_command) = mpsc::channel::<WsCommand>(32);
        let (tx_message, rx_message) = mpsc::unbounded_channel::<Result<Message, WsError>>();

        let pump_task = tokio::spawn(async move {
            let mut inner = inner;
            loop {
                tokio::select! {
                    command = rx_command.recv() => {
                        let Some(command) = command else {
                            break;
                        };
                        match command {
                            WsCommand::Send { message, tx_result } => {
                                debug!("realtime websocket sending message");
                                let result = inner.send(message).await;
                                let should_break = result.is_err();
                                if let Err(err) = &result {
                                    error!("realtime websocket send failed: {err}");
                                }
                                let _ = tx_result.send(result);
                                if should_break {
                                    break;
                                }
                            }
                            WsCommand::Close { tx_result } => {
                                info!("realtime websocket sending close");
                                let result = inner.close(None).await;
                                if let Err(err) = &result {
                                    error!("realtime websocket close failed: {err}");
                                }
                                let _ = tx_result.send(result);
                                break;
                            }
                        }
                    }
                    message = inner.next() => {
                        let Some(message) = message else {
                            break;
                        };
                        match message {
                            Ok(Message::Ping(payload)) => {
                                trace!(payload_len = payload.len(), "realtime websocket received ping");
                                if let Err(err) = inner.send(Message::Pong(payload)).await {
                                    error!("realtime websocket failed to send pong: {err}");
                                    let _ = tx_message.send(Err(err));
                                    break;
                                }
                            }
                            Ok(Message::Pong(_)) => {}
                            Ok(message @ (Message::Text(_)
                                | Message::Binary(_)
                                | Message::Close(_)
                                | Message::Frame(_))) => {
                                let is_close = matches!(message, Message::Close(_));
                                match &message {
                                    Message::Text(_) => trace!("realtime websocket received text frame"),
                                    Message::Binary(binary) => {
                                        error!(
                                            payload_len = binary.len(),
                                            "realtime websocket received unexpected binary frame"
                                        );
                                    }
                                    Message::Close(frame) => info!(
                                        "realtime websocket received close frame: code={:?} reason={:?}",
                                        frame.as_ref().map(|frame| frame.code),
                                        frame.as_ref().map(|frame| frame.reason.as_str())
                                    ),
                                    Message::Frame(_) => {
                                        trace!("realtime websocket received raw frame");
                                    }
                                    Message::Ping(_) | Message::Pong(_) => {}
                                }
                                if tx_message.send(Ok(message)).is_err() {
                                    break;
                                }
                                if is_close {
                                    break;
                                }
                            }
                            Err(err) => {
                                error!("realtime websocket receive failed: {err}");
                                let _ = tx_message.send(Err(err));
                                break;
                            }
                        }
                    }
                }
            }
            info!("realtime websocket pump exiting");
        });

        (
            Self {
                tx_command,
                pump_task,
            },
            rx_message,
        )
    }

    async fn request(
        &self,
        make_command: impl FnOnce(oneshot::Sender<Result<(), WsError>>) -> WsCommand,
    ) -> Result<(), WsError> {
        let (tx_result, rx_result) = oneshot::channel();
        if self.tx_command.send(make_command(tx_result)).await.is_err() {
            return Err(WsError::ConnectionClosed);
        }
        rx_result.await.unwrap_or(Err(WsError::ConnectionClosed))
    }

    async fn send(&self, message: Message) -> Result<(), WsError> {
        self.request(|tx_result| WsCommand::Send { message, tx_result })
            .await
    }

    async fn close(&self) -> Result<(), WsError> {
        self.request(|tx_result| WsCommand::Close { tx_result })
            .await
    }
}

impl Drop for WsStream {
    fn drop(&mut self) {
        self.pump_task.abort();
    }
}

pub struct RealtimeWebsocketConnection {
    writer: RealtimeWebsocketWriter,
    events: RealtimeWebsocketEvents,
}

#[derive(Clone)]
pub struct RealtimeWebsocketWriter {
    stream: Arc<WsStream>,
    is_closed: Arc<AtomicBool>,
    event_parser: RealtimeEventParser,
}

#[derive(Clone)]
pub struct RealtimeWebsocketEvents {
    rx_message: Arc<Mutex<mpsc::UnboundedReceiver<Result<Message, WsError>>>>,
    active_transcript: Arc<Mutex<ActiveTranscriptState>>,
    event_parser: RealtimeEventParser,
    is_closed: Arc<AtomicBool>,
}

#[derive(Default)]
struct ActiveTranscriptState {
    entries: Vec<RealtimeTranscriptEntry>,
}

impl RealtimeWebsocketConnection {
    pub async fn send_audio_frame(&self, frame: RealtimeAudioFrame) -> Result<(), ApiError> {
        self.writer.send_audio_frame(frame).await
    }

    pub async fn send_conversation_item_create(&self, text: String) -> Result<(), ApiError> {
        self.writer.send_conversation_item_create(text).await
    }

    pub async fn send_conversation_handoff_append(
        &self,
        handoff_id: String,
        output_text: String,
    ) -> Result<(), ApiError> {
        self.writer
            .send_conversation_handoff_append(handoff_id, output_text)
            .await
    }

    pub async fn close(&self) -> Result<(), ApiError> {
        self.writer.close().await
    }

    pub async fn next_event(&self) -> Result<Option<RealtimeEvent>, ApiError> {
        self.events.next_event().await
    }

    pub fn writer(&self) -> RealtimeWebsocketWriter {
        self.writer.clone()
    }

    pub fn events(&self) -> RealtimeWebsocketEvents {
        self.events.clone()
    }

    fn new(
        stream: WsStream,
        rx_message: mpsc::UnboundedReceiver<Result<Message, WsError>>,
        event_parser: RealtimeEventParser,
    ) -> Self {
        let stream = Arc::new(stream);
        let is_closed = Arc::new(AtomicBool::new(false));
        Self {
            writer: RealtimeWebsocketWriter {
                stream: Arc::clone(&stream),
                is_closed: Arc::clone(&is_closed),
                event_parser,
            },
            events: RealtimeWebsocketEvents {
                rx_message: Arc::new(Mutex::new(rx_message)),
                active_transcript: Arc::new(Mutex::new(ActiveTranscriptState::default())),
                event_parser,
                is_closed,
            },
        }
    }
}

impl RealtimeWebsocketWriter {
    pub async fn send_audio_frame(&self, frame: RealtimeAudioFrame) -> Result<(), ApiError> {
        self.send_json(&RealtimeOutboundMessage::InputAudioBufferAppend { audio: frame.data })
            .await
    }

    pub async fn send_conversation_item_create(&self, text: String) -> Result<(), ApiError> {
        self.send_json(&conversation_item_create_message(self.event_parser, text))
            .await
    }

    pub async fn send_conversation_handoff_append(
        &self,
        handoff_id: String,
        output_text: String,
    ) -> Result<(), ApiError> {
        self.send_json(&conversation_handoff_append_message(
            self.event_parser,
            handoff_id,
            output_text,
        ))
        .await
    }

    pub async fn send_response_create(&self) -> Result<(), ApiError> {
        self.send_json(&RealtimeOutboundMessage::ResponseCreate)
            .await
    }

    pub async fn send_session_update(
        &self,
        instructions: String,
        session_mode: RealtimeSessionMode,
    ) -> Result<(), ApiError> {
        let session_mode = normalized_session_mode(self.event_parser, session_mode);
        let session = session_update_session(self.event_parser, instructions, session_mode);
        self.send_json(&RealtimeOutboundMessage::SessionUpdate { session })
            .await
    }

    pub async fn close(&self) -> Result<(), ApiError> {
        if self.is_closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        if let Err(err) = self.stream.close().await
            && !matches!(err, WsError::ConnectionClosed | WsError::AlreadyClosed)
        {
            return Err(ApiError::Stream(format!(
                "failed to close websocket: {err}"
            )));
        }
        Ok(())
    }

    async fn send_json(&self, message: &RealtimeOutboundMessage) -> Result<(), ApiError> {
        let payload = serde_json::to_string(message)
            .map_err(|err| ApiError::Stream(format!("failed to encode realtime request: {err}")))?;
        debug!(?message, "realtime websocket request");
        self.send_payload(payload).await
    }

    pub async fn send_payload(&self, payload: String) -> Result<(), ApiError> {
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(ApiError::Stream(
                "realtime websocket connection is closed".to_string(),
            ));
        }

        self.stream
            .send(Message::Text(payload.into()))
            .await
            .map_err(|err| ApiError::Stream(format!("failed to send realtime request: {err}")))?;
        Ok(())
    }
}

impl RealtimeWebsocketEvents {
    pub async fn next_event(&self) -> Result<Option<RealtimeEvent>, ApiError> {
        if self.is_closed.load(Ordering::SeqCst) {
            return Ok(None);
        }

        loop {
            let msg = match self.rx_message.lock().await.recv().await {
                Some(Ok(msg)) => msg,
                Some(Err(err)) => {
                    self.is_closed.store(true, Ordering::SeqCst);
                    error!("realtime websocket read failed: {err}");
                    return Err(ApiError::Stream(format!(
                        "failed to read websocket message: {err}"
                    )));
                }
                None => {
                    self.is_closed.store(true, Ordering::SeqCst);
                    info!("realtime websocket event stream ended");
                    return Ok(None);
                }
            };

            match msg {
                Message::Text(text) => {
                    if let Some(mut event) = parse_realtime_event(&text, self.event_parser) {
                        self.update_active_transcript(&mut event).await;
                        debug!(?event, "realtime websocket parsed event");
                        return Ok(Some(event));
                    }
                    debug!("realtime websocket ignored unsupported text frame");
                }
                Message::Close(frame) => {
                    self.is_closed.store(true, Ordering::SeqCst);
                    info!(
                        "realtime websocket closed: code={:?} reason={:?}",
                        frame.as_ref().map(|frame| frame.code),
                        frame.as_ref().map(|frame| frame.reason.as_str())
                    );
                    return Ok(None);
                }
                Message::Binary(_) => {
                    return Ok(Some(RealtimeEvent::Error(
                        "unexpected binary realtime websocket event".to_string(),
                    )));
                }
                Message::Frame(_) | Message::Ping(_) | Message::Pong(_) => {}
            }
        }
    }

    async fn update_active_transcript(&self, event: &mut RealtimeEvent) {
        let mut active_transcript = self.active_transcript.lock().await;
        match event {
            RealtimeEvent::InputAudioSpeechStarted(_) => {}
            RealtimeEvent::InputTranscriptDelta(RealtimeTranscriptDelta { delta }) => {
                append_transcript_delta(&mut active_transcript.entries, "user", delta);
            }
            RealtimeEvent::OutputTranscriptDelta(RealtimeTranscriptDelta { delta }) => {
                append_transcript_delta(&mut active_transcript.entries, "assistant", delta);
            }
            RealtimeEvent::HandoffRequested(handoff) => {
                handoff.active_transcript = std::mem::take(&mut active_transcript.entries);
            }
            RealtimeEvent::SessionUpdated { .. }
            | RealtimeEvent::AudioOut(_)
            | RealtimeEvent::ResponseCancelled(_)
            | RealtimeEvent::ConversationItemAdded(_)
            | RealtimeEvent::ConversationItemDone { .. }
            | RealtimeEvent::Error(_) => {}
        }
    }
}

fn append_transcript_delta(entries: &mut Vec<RealtimeTranscriptEntry>, role: &str, delta: &str) {
    if delta.is_empty() {
        return;
    }

    if let Some(last_entry) = entries.last_mut()
        && last_entry.role == role
    {
        last_entry.text.push_str(delta);
        return;
    }

    entries.push(RealtimeTranscriptEntry {
        role: role.to_string(),
        text: delta.to_string(),
    });
}

pub struct RealtimeWebsocketClient {
    provider: Provider,
}

impl RealtimeWebsocketClient {
    pub fn new(provider: Provider) -> Self {
        Self { provider }
    }

    pub async fn connect(
        &self,
        config: RealtimeSessionConfig,
        extra_headers: HeaderMap,
        default_headers: HeaderMap,
    ) -> Result<RealtimeWebsocketConnection, ApiError> {
        ensure_rustls_crypto_provider();
        let ws_url = websocket_url_from_api_url(
            self.provider.base_url.as_str(),
            self.provider.query_params.as_ref(),
            config.model.as_deref(),
            config.event_parser,
            config.session_mode,
        )?;

        let mut request = ws_url
            .as_str()
            .into_client_request()
            .map_err(|err| ApiError::Stream(format!("failed to build websocket request: {err}")))?;
        let headers = merge_request_headers(
            &self.provider.headers,
            with_session_id_header(extra_headers, config.session_id.as_deref())?,
            default_headers,
        );
        request.headers_mut().extend(headers);

        info!("connecting realtime websocket: {ws_url}");
        // Realtime websocket TLS should honor the same custom-CA env vars as the rest of Codex's
        // outbound HTTPS and websocket traffic.
        let connector = maybe_build_rustls_client_config_with_custom_ca()
            .map_err(|err| ApiError::Stream(format!("failed to configure websocket TLS: {err}")))?
            .map(tokio_tungstenite::Connector::Rustls);
        let (stream, response) = tokio_tungstenite::connect_async_tls_with_config(
            request,
            Some(websocket_config()),
            false,
            connector,
        )
        .await
        .map_err(|err| ApiError::Stream(format!("failed to connect realtime websocket: {err}")))?;
        info!(
            ws_url = %ws_url,
            status = %response.status(),
            "realtime websocket connected"
        );

        let (stream, rx_message) = WsStream::new(stream);
        let connection = RealtimeWebsocketConnection::new(stream, rx_message, config.event_parser);
        debug!(
            session_id = config.session_id.as_deref().unwrap_or("<none>"),
            "realtime websocket sending session.update"
        );
        connection
            .writer
            .send_session_update(config.instructions, config.session_mode)
            .await?;
        Ok(connection)
    }
}

fn merge_request_headers(
    provider_headers: &HeaderMap,
    extra_headers: HeaderMap,
    default_headers: HeaderMap,
) -> HeaderMap {
    let mut headers = provider_headers.clone();
    headers.extend(extra_headers);
    for (name, value) in &default_headers {
        if let http::header::Entry::Vacant(entry) = headers.entry(name) {
            entry.insert(value.clone());
        }
    }
    headers
}

fn with_session_id_header(
    mut headers: HeaderMap,
    session_id: Option<&str>,
) -> Result<HeaderMap, ApiError> {
    let Some(session_id) = session_id else {
        return Ok(headers);
    };
    headers.insert(
        "x-session-id",
        HeaderValue::from_str(session_id).map_err(|err| {
            ApiError::Stream(format!("invalid realtime session id header: {err}"))
        })?,
    );
    Ok(headers)
}

fn websocket_config() -> WebSocketConfig {
    WebSocketConfig::default()
}

fn websocket_url_from_api_url(
    api_url: &str,
    query_params: Option<&HashMap<String, String>>,
    model: Option<&str>,
    event_parser: RealtimeEventParser,
    _session_mode: RealtimeSessionMode,
) -> Result<Url, ApiError> {
    let mut url = Url::parse(api_url)
        .map_err(|err| ApiError::Stream(format!("failed to parse realtime api_url: {err}")))?;

    normalize_realtime_path(&mut url);

    match url.scheme() {
        "ws" | "wss" => {}
        "http" | "https" => {
            let scheme = if url.scheme() == "http" { "ws" } else { "wss" };
            let _ = url.set_scheme(scheme);
        }
        scheme => {
            return Err(ApiError::Stream(format!(
                "unsupported realtime api_url scheme: {scheme}"
            )));
        }
    }

    let intent = websocket_intent(event_parser);
    let has_extra_query_params = query_params.is_some_and(|query_params| {
        query_params
            .iter()
            .any(|(key, _)| key != "intent" && !(key == "model" && model.is_some()))
    });
    if intent.is_some() || model.is_some() || has_extra_query_params {
        let mut query = url.query_pairs_mut();
        if let Some(intent) = intent {
            query.append_pair("intent", intent);
        }
        if let Some(model) = model {
            query.append_pair("model", model);
        }
        if let Some(query_params) = query_params {
            for (key, value) in query_params {
                if key == "intent" || (key == "model" && model.is_some()) {
                    continue;
                }
                query.append_pair(key, value);
            }
        }
    }

    Ok(url)
}

fn normalize_realtime_path(url: &mut Url) {
    let path = url.path().to_string();
    if path.is_empty() || path == "/" {
        url.set_path("/v1/realtime");
        return;
    }

    if path.ends_with("/realtime") {
        return;
    }

    if path.ends_with("/realtime/") {
        url.set_path(path.trim_end_matches('/'));
        return;
    }

    if path.ends_with("/v1") {
        url.set_path(&format!("{path}/realtime"));
        return;
    }

    if path.ends_with("/v1/") {
        url.set_path(&format!("{path}realtime"));
    }
}
