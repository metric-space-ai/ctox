//! In-process app-server runtime host for local embedders.
//!
//! This module runs the existing [`MessageProcessor`] and outbound routing logic
//! on Tokio tasks, but replaces socket/stdio transports with bounded in-memory
//! channels. The intent is to preserve app-server semantics while avoiding a
//! process boundary for CLI surfaces that run in the same process.
//!
//! # Lifecycle
//!
//! 1. Construct runtime state with [`InProcessStartArgs`].
//! 2. Call [`start`], which performs the `initialize` / `initialized` handshake
//!    internally and returns a ready-to-use [`InProcessClientHandle`].
//! 3. Send requests via [`InProcessClientHandle::request`], notifications via
//!    [`InProcessClientHandle::notify`], and consume events via
//!    [`InProcessClientHandle::next_event`].
//! 4. Terminate with [`InProcessClientHandle::shutdown`].
//!
//! # Transport model
//!
//! The runtime is transport-local but not protocol-free. Incoming requests are
//! typed [`ClientRequest`] values, yet responses still come back through the
//! same JSON-RPC result envelope that `MessageProcessor` uses for stdio and
//! websocket transports. This keeps in-process behavior aligned with
//! app-server rather than creating a second execution contract.
//!
//! # Backpressure
//!
//! Command submission uses `try_send` and can return `WouldBlock`, while event
//! fanout may drop notifications under saturation. Server requests are never
//! silently abandoned: if they cannot be queued they are failed back into
//! `MessageProcessor` with overload or internal errors so approval flows do
//! not hang indefinitely.
//!
//! # Relationship to `ctox-app-server-client`
//!
//! This module provides the low-level runtime handle ([`InProcessClientHandle`]).
//! Higher-level callers (TUI, exec) should go through `ctox-app-server-client`,
//! which wraps this module behind a worker task with async request/response
//! helpers, surface-specific startup policy, and bounded shutdown.

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::collections::hash_map::Entry;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::error_code::OVERLOADED_ERROR_CODE;
use crate::message_processor::ConnectionSessionState;
use crate::message_processor::MessageProcessor;
use crate::message_processor::MessageProcessorArgs;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::OutgoingEnvelope;
use crate::outgoing_message::OutgoingMessage;
use crate::outgoing_message::OutgoingMessageSender;
use crate::transport::CHANNEL_CAPACITY;
use crate::transport::OutboundConnectionState;
use crate::transport::route_outgoing_envelope;
use ctox_app_server_protocol::ClientNotification;
use ctox_app_server_protocol::ClientRequest;
use ctox_app_server_protocol::ConfigWarningNotification;
use ctox_app_server_protocol::InitializeParams;
use ctox_app_server_protocol::JSONRPCErrorError;
use ctox_app_server_protocol::JSONRPCNotification;
use ctox_app_server_protocol::RequestId;
use ctox_app_server_protocol::Result;
use ctox_app_server_protocol::ServerNotification;
use ctox_app_server_protocol::ServerRequest;
use ctox_arg0::Arg0DispatchPaths;
use ctox_core::AuthManager;
use ctox_core::ThreadManager;
use ctox_core::config::Config;
use ctox_core::config_loader::CloudRequirementsLoader;
use ctox_core::config_loader::LoaderOverrides;
use ctox_feedback::CodexFeedback;
use ctox_protocol::protocol::SessionSource;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::timeout;
use toml::Value as TomlValue;
use tracing::warn;

const IN_PROCESS_CONNECTION_ID: ConnectionId = ConnectionId(0);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
/// Default bounded channel capacity for in-process runtime queues.
pub const DEFAULT_IN_PROCESS_CHANNEL_CAPACITY: usize = CHANNEL_CAPACITY;

type PendingClientRequestResponse = std::result::Result<Result, JSONRPCErrorError>;

fn server_notification_requires_delivery(notification: &ServerNotification) -> bool {
    matches!(notification, ServerNotification::TurnCompleted(_))
}

fn legacy_notification_requires_delivery(notification: &JSONRPCNotification) -> bool {
    matches!(
        notification
            .method
            .strip_prefix("codex/event/")
            .unwrap_or(&notification.method),
        "task_complete" | "turn_aborted" | "shutdown_complete"
    )
}

/// Forward one event toward the client without ever blocking the runtime
/// select loop. Delivery-required events that do not fit into the client
/// queue are buffered in `pending` and flushed by the loop's `reserve()`
/// arm; droppable events behind a non-empty buffer are discarded so buffered
/// completion events are never overtaken. Returns `true` when the loop
/// should exit because the event consumer is gone.
fn enqueue_in_process_event(
    event_tx: &mpsc::Sender<InProcessServerEvent>,
    pending: &mut VecDeque<InProcessServerEvent>,
    consumer_gone: &mut bool,
    event: InProcessServerEvent,
    requires_delivery: bool,
    kind: &'static str,
) -> bool {
    if *consumer_gone {
        return true;
    }
    if !pending.is_empty() {
        if requires_delivery {
            pending.push_back(event);
        } else {
            warn!("dropping in-process {kind} behind buffered delivery-required events");
        }
        return false;
    }
    match event_tx.try_send(event) {
        Ok(()) => false,
        Err(mpsc::error::TrySendError::Full(event)) => {
            if requires_delivery {
                pending.push_back(event);
            } else {
                warn!("dropping in-process {kind} (queue full)");
            }
            false
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            *consumer_gone = true;
            true
        }
    }
}

/// Input needed to start an in-process app-server runtime.
///
/// These fields mirror the pieces of ambient process state that stdio and
/// websocket transports normally assemble before `MessageProcessor` starts.
#[derive(Clone)]
pub struct InProcessStartArgs {
    /// Resolved argv0 dispatch paths used by command execution internals.
    pub arg0_paths: Arg0DispatchPaths,
    /// Shared base config used to initialize core components.
    pub config: Arc<Config>,
    /// CLI config overrides that are already parsed into TOML values.
    pub cli_overrides: Vec<(String, TomlValue)>,
    /// Loader override knobs used by config API paths.
    pub loader_overrides: LoaderOverrides,
    /// Preloaded cloud requirements provider.
    pub cloud_requirements: CloudRequirementsLoader,
    /// Optional prebuilt auth manager reused by an embedding caller.
    pub auth_manager: Option<Arc<AuthManager>>,
    /// Optional prebuilt thread manager reused by an embedding caller.
    pub thread_manager: Option<Arc<ThreadManager>>,
    /// Feedback sink used by app-server/core telemetry and logs.
    pub feedback: CodexFeedback,
    /// Startup warnings emitted after initialize succeeds.
    pub config_warnings: Vec<ConfigWarningNotification>,
    /// Session source stamped into thread/session metadata.
    pub session_source: SessionSource,
    /// Whether auth loading should honor the `CODEX_API_KEY` environment variable.
    pub enable_ctox_api_key_env: bool,
    /// Initialize params used for initial handshake.
    pub initialize: InitializeParams,
    /// Capacity used for all runtime queues (clamped to at least 1).
    pub channel_capacity: usize,
}

/// Event emitted from the app-server to the in-process client.
///
/// The stream carries three event families because CLI surfaces are mid-migration
/// from the legacy `ctox_protocol::Event` model to the typed app-server
/// notification model. Once all surfaces consume only [`ServerNotification`],
/// [`LegacyNotification`](Self::LegacyNotification) can be removed.
///
/// [`Lagged`](Self::Lagged) is a transport health marker, not an application
/// event — it signals that the consumer fell behind and some events were dropped.
#[derive(Debug, Clone)]
pub enum InProcessServerEvent {
    /// Server request that requires client response/rejection.
    ServerRequest(ServerRequest),
    /// App-server notification directed to the embedded client.
    ServerNotification(ServerNotification),
    /// Legacy JSON-RPC notification from core event bridge.
    LegacyNotification(JSONRPCNotification),
    /// Indicates one or more events were dropped due to backpressure.
    Lagged { skipped: usize },
}

/// Internal message sent from [`InProcessClientHandle`] methods to the runtime task.
///
/// Requests carry a oneshot sender for the response; notifications and server-request
/// replies are fire-and-forget from the caller's perspective (transport errors are
/// caught by `try_send` on the outer channel).
enum InProcessClientMessage {
    Request {
        request: Box<ClientRequest>,
        response_tx: oneshot::Sender<PendingClientRequestResponse>,
    },
    Notification {
        notification: ClientNotification,
    },
    ServerRequestResponse {
        request_id: RequestId,
        result: Result,
    },
    ServerRequestError {
        request_id: RequestId,
        error: JSONRPCErrorError,
    },
    Shutdown {
        done_tx: oneshot::Sender<()>,
    },
}

enum ProcessorCommand {
    Request(Box<ClientRequest>),
    Notification(ClientNotification),
}

#[derive(Clone)]
pub struct InProcessClientSender {
    client_tx: mpsc::Sender<InProcessClientMessage>,
}

impl InProcessClientSender {
    pub async fn request(&self, request: ClientRequest) -> IoResult<PendingClientRequestResponse> {
        let (response_tx, response_rx) = oneshot::channel();
        self.try_send_client_message(InProcessClientMessage::Request {
            request: Box::new(request),
            response_tx,
        })?;
        response_rx.await.map_err(|err| {
            IoError::new(
                ErrorKind::BrokenPipe,
                format!("in-process request response channel closed: {err}"),
            )
        })
    }

    pub fn notify(&self, notification: ClientNotification) -> IoResult<()> {
        self.try_send_client_message(InProcessClientMessage::Notification { notification })
    }

    pub fn respond_to_server_request(&self, request_id: RequestId, result: Result) -> IoResult<()> {
        self.try_send_client_message(InProcessClientMessage::ServerRequestResponse {
            request_id,
            result,
        })
    }

    pub fn fail_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> IoResult<()> {
        self.try_send_client_message(InProcessClientMessage::ServerRequestError {
            request_id,
            error,
        })
    }

    fn try_send_client_message(&self, message: InProcessClientMessage) -> IoResult<()> {
        match self.client_tx.try_send(message) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => Err(IoError::new(
                ErrorKind::WouldBlock,
                "in-process app-server client queue is full",
            )),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(IoError::new(
                ErrorKind::BrokenPipe,
                "in-process app-server runtime is closed",
            )),
        }
    }
}

/// Handle used by an in-process client to call app-server and consume events.
///
/// This is the low-level runtime handle. Higher-level callers should usually go
/// through `ctox-app-server-client`, which adds worker-task buffering,
/// request/response helpers, and surface-specific startup policy.
pub struct InProcessClientHandle {
    client: InProcessClientSender,
    event_rx: mpsc::Receiver<InProcessServerEvent>,
    runtime_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for InProcessClientHandle {
    fn drop(&mut self) {
        if let Some(handle) = self.runtime_handle.take() {
            handle.abort();
        }
    }
}

impl InProcessClientHandle {
    /// Sends a typed client request into the in-process runtime.
    ///
    /// The returned value is a transport-level `IoResult` containing either a
    /// JSON-RPC success payload or JSON-RPC error payload. Callers must keep
    /// request IDs unique among concurrent requests; reusing an in-flight ID
    /// produces an `INVALID_REQUEST` response and can make request routing
    /// ambiguous in the caller.
    pub async fn request(&self, request: ClientRequest) -> IoResult<PendingClientRequestResponse> {
        self.client.request(request).await
    }

    /// Sends a typed client notification into the in-process runtime.
    ///
    /// Notifications do not have an application-level response. Transport
    /// errors indicate queue saturation or closed runtime.
    pub fn notify(&self, notification: ClientNotification) -> IoResult<()> {
        self.client.notify(notification)
    }

    /// Resolves a pending [`ServerRequest`](InProcessServerEvent::ServerRequest).
    ///
    /// This should be used only with request IDs received from the current
    /// runtime event stream; sending arbitrary IDs has no effect on app-server
    /// state and can mask a stuck approval flow in the caller.
    pub fn respond_to_server_request(&self, request_id: RequestId, result: Result) -> IoResult<()> {
        self.client.respond_to_server_request(request_id, result)
    }

    /// Rejects a pending [`ServerRequest`](InProcessServerEvent::ServerRequest).
    ///
    /// Use this when the embedder cannot satisfy a server request; leaving
    /// requests unanswered can stall turn progress.
    pub fn fail_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> IoResult<()> {
        self.client.fail_server_request(request_id, error)
    }

    /// Receives the next server event from the in-process runtime.
    ///
    /// Returns `None` when the runtime task exits and no more events are
    /// available.
    pub async fn next_event(&mut self) -> Option<InProcessServerEvent> {
        self.event_rx.recv().await
    }

    /// Requests runtime shutdown and waits for worker termination.
    ///
    /// Shutdown is bounded by internal timeouts and may abort background tasks
    /// if graceful drain does not complete in time.
    pub async fn shutdown(mut self) -> IoResult<()> {
        let mut runtime_handle = match self.runtime_handle.take() {
            Some(handle) => handle,
            None => return Ok(()),
        };
        let (done_tx, done_rx) = oneshot::channel();

        if timeout(
            SHUTDOWN_TIMEOUT,
            self.client
                .client_tx
                .send(InProcessClientMessage::Shutdown { done_tx }),
        )
        .await
        .is_ok_and(|send_result| send_result.is_ok())
        {
            let _ = timeout(SHUTDOWN_TIMEOUT, done_rx).await;
        }

        if let Err(_elapsed) = timeout(SHUTDOWN_TIMEOUT, &mut runtime_handle).await {
            runtime_handle.abort();
            let _ = runtime_handle.await;
        }
        Ok(())
    }

    pub fn sender(&self) -> InProcessClientSender {
        self.client.clone()
    }
}

/// Starts an in-process app-server runtime and performs initialize handshake.
///
/// This function sends `initialize` followed by `initialized` before returning
/// the handle, so callers receive a ready-to-use runtime. If initialize fails,
/// the runtime is shut down and an `InvalidData` error is returned.
pub async fn start(args: InProcessStartArgs) -> IoResult<InProcessClientHandle> {
    let initialize = args.initialize.clone();
    let client = start_uninitialized(args);

    let initialize_response = client
        .request(ClientRequest::Initialize {
            request_id: RequestId::Integer(0),
            params: initialize,
        })
        .await?;
    if let Err(error) = initialize_response {
        let _ = client.shutdown().await;
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!("in-process initialize failed: {}", error.message),
        ));
    }
    client.notify(ClientNotification::Initialized)?;

    Ok(client)
}

fn start_uninitialized(args: InProcessStartArgs) -> InProcessClientHandle {
    let channel_capacity = args.channel_capacity.max(1);
    let (client_tx, mut client_rx) = mpsc::channel::<InProcessClientMessage>(channel_capacity);
    let (event_tx, event_rx) = mpsc::channel::<InProcessServerEvent>(channel_capacity);

    let runtime_handle = tokio::spawn(async move {
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<OutgoingEnvelope>(channel_capacity);
        let outgoing_message_sender = Arc::new(OutgoingMessageSender::new(outgoing_tx));

        let (writer_tx, mut writer_rx) = mpsc::channel::<OutgoingMessage>(channel_capacity);
        let outbound_initialized = Arc::new(AtomicBool::new(false));
        let outbound_experimental_api_enabled = Arc::new(AtomicBool::new(false));
        let outbound_opted_out_notification_methods = Arc::new(RwLock::new(HashSet::new()));

        let mut outbound_connections = HashMap::<ConnectionId, OutboundConnectionState>::new();
        outbound_connections.insert(
            IN_PROCESS_CONNECTION_ID,
            OutboundConnectionState::new(
                writer_tx,
                Arc::clone(&outbound_initialized),
                Arc::clone(&outbound_experimental_api_enabled),
                Arc::clone(&outbound_opted_out_notification_methods),
                /*allow_legacy_notifications*/ true,
                /*disconnect_sender*/ None,
            ),
        );
        let mut outbound_handle = tokio::spawn(async move {
            while let Some(envelope) = outgoing_rx.recv().await {
                route_outgoing_envelope(&mut outbound_connections, envelope).await;
            }
        });

        let processor_outgoing = Arc::clone(&outgoing_message_sender);
        let (processor_tx, mut processor_rx) = mpsc::channel::<ProcessorCommand>(channel_capacity);
        let mut processor_handle = tokio::spawn(async move {
            let mut processor = MessageProcessor::new(MessageProcessorArgs {
                outgoing: Arc::clone(&processor_outgoing),
                arg0_paths: args.arg0_paths,
                config: args.config,
                cli_overrides: args.cli_overrides,
                loader_overrides: args.loader_overrides,
                cloud_requirements: args.cloud_requirements,
                auth_manager: args.auth_manager,
                thread_manager: args.thread_manager,
                feedback: args.feedback,
                log_db: None,
                config_warnings: args.config_warnings,
                session_source: args.session_source,
                enable_ctox_api_key_env: args.enable_ctox_api_key_env,
            });
            let mut thread_created_rx = processor.thread_created_receiver();
            let mut session = ConnectionSessionState::default();
            let mut listen_for_threads = true;

            loop {
                tokio::select! {
                    command = processor_rx.recv() => {
                        match command {
                            Some(ProcessorCommand::Request(request)) => {
                                let was_initialized = session.initialized;
                                processor
                                    .process_client_request(
                                        IN_PROCESS_CONNECTION_ID,
                                        *request,
                                        &mut session,
                                        &outbound_initialized,
                                    )
                                    .await;
                                if let Ok(mut opted_out_notification_methods) =
                                    outbound_opted_out_notification_methods.write()
                                {
                                    *opted_out_notification_methods =
                                        session.opted_out_notification_methods.clone();
                                } else {
                                    warn!("failed to update outbound opted-out notifications");
                                }
                                outbound_experimental_api_enabled.store(
                                    session.experimental_api_enabled,
                                    Ordering::Release,
                                );
                                if !was_initialized && session.initialized {
                                    processor.send_initialize_notifications().await;
                                }
                            }
                            Some(ProcessorCommand::Notification(notification)) => {
                                processor.process_client_notification(notification).await;
                            }
                            None => {
                                break;
                            }
                        }
                    }
                    created = thread_created_rx.recv(), if listen_for_threads => {
                        match created {
                            Ok(thread_id) => {
                                let connection_ids = if session.initialized {
                                    vec![IN_PROCESS_CONNECTION_ID]
                                } else {
                                    Vec::<ConnectionId>::new()
                                };
                                processor
                                    .try_attach_thread_listener(thread_id, connection_ids)
                                    .await;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                warn!("thread_created receiver lagged; skipping resync");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                listen_for_threads = false;
                            }
                        }
                    }
                }
            }

            processor.clear_runtime_references();
            processor.connection_closed(IN_PROCESS_CONNECTION_ID).await;
            processor.clear_all_thread_listeners().await;
            processor.drain_background_tasks().await;
            processor.shutdown_threads().await;
        });
        let mut pending_request_responses =
            HashMap::<RequestId, oneshot::Sender<PendingClientRequestResponse>>::new();
        let mut shutdown_ack = None;
        // Delivery-required events (turn completion class) that could not be
        // flushed to the client queue immediately. They drain through the
        // `event_tx.reserve()` select arm below instead of a blocking
        // `send().await` inside this loop: a paused or slow event consumer
        // must never stall the request path, otherwise `turn/interrupt` can
        // never land once the event queue is full and the whole session
        // wedges (ctox#21). `writer_rx` is gated on the buffer bound so
        // upstream backpressure still applies without unbounded memory.
        let mut pending_delivery_events: VecDeque<InProcessServerEvent> = VecDeque::new();
        let mut event_consumer_gone = false;
        let max_pending_delivery_events = channel_capacity.max(1024);

        loop {
            tokio::select! {
                permit = event_tx.reserve(), if !pending_delivery_events.is_empty() && !event_consumer_gone => {
                    match permit {
                        Ok(permit) => {
                            if let Some(event) = pending_delivery_events.pop_front() {
                                permit.send(event);
                            }
                        }
                        Err(_) => {
                            event_consumer_gone = true;
                            pending_delivery_events.clear();
                        }
                    }
                }
                message = client_rx.recv() => {
                    match message {
                        Some(InProcessClientMessage::Request { request, response_tx }) => {
                            let request = *request;
                            let request_id = request.id().clone();
                            match pending_request_responses.entry(request_id.clone()) {
                                Entry::Vacant(entry) => {
                                    entry.insert(response_tx);
                                }
                                Entry::Occupied(_) => {
                                    let _ = response_tx.send(Err(JSONRPCErrorError {
                                        code: INVALID_REQUEST_ERROR_CODE,
                                        message: format!("duplicate request id: {request_id:?}"),
                                        data: None,
                                    }));
                                    continue;
                                }
                            }

                            match processor_tx.try_send(ProcessorCommand::Request(Box::new(request))) {
                                Ok(()) => {}
                                Err(mpsc::error::TrySendError::Full(_)) => {
                                    if let Some(response_tx) =
                                        pending_request_responses.remove(&request_id)
                                    {
                                        let _ = response_tx.send(Err(JSONRPCErrorError {
                                            code: OVERLOADED_ERROR_CODE,
                                            message: "in-process app-server request queue is full"
                                                .to_string(),
                                            data: None,
                                        }));
                                    }
                                }
                                Err(mpsc::error::TrySendError::Closed(_)) => {
                                    if let Some(response_tx) =
                                        pending_request_responses.remove(&request_id)
                                    {
                                        let _ = response_tx.send(Err(JSONRPCErrorError {
                                            code: INTERNAL_ERROR_CODE,
                                            message:
                                                "in-process app-server request processor is closed"
                                                    .to_string(),
                                            data: None,
                                        }));
                                    }
                                    break;
                                }
                            }
                        }
                        Some(InProcessClientMessage::Notification { notification }) => {
                            match processor_tx.try_send(ProcessorCommand::Notification(notification)) {
                                Ok(()) => {}
                                Err(mpsc::error::TrySendError::Full(_)) => {
                                    warn!("dropping in-process client notification (queue full)");
                                }
                                Err(mpsc::error::TrySendError::Closed(_)) => {
                                    break;
                                }
                            }
                        }
                        Some(InProcessClientMessage::ServerRequestResponse { request_id, result }) => {
                            outgoing_message_sender
                                .notify_client_response(request_id, result)
                                .await;
                        }
                        Some(InProcessClientMessage::ServerRequestError { request_id, error }) => {
                            outgoing_message_sender
                                .notify_client_error(request_id, error)
                                .await;
                        }
                        Some(InProcessClientMessage::Shutdown { done_tx }) => {
                            shutdown_ack = Some(done_tx);
                            break;
                        }
                        None => {
                            break;
                        }
                    }
                }
                outgoing_message = writer_rx.recv(), if pending_delivery_events.len() < max_pending_delivery_events => {
                    let Some(outgoing_message) = outgoing_message else {
                        break;
                    };
                    match outgoing_message {
                        OutgoingMessage::Response(response) => {
                            if let Some(response_tx) = pending_request_responses.remove(&response.id) {
                                let _ = response_tx.send(Ok(response.result));
                            } else {
                                warn!(
                                    request_id = ?response.id,
                                    "dropping unmatched in-process response"
                                );
                            }
                        }
                        OutgoingMessage::Error(error) => {
                            if let Some(response_tx) = pending_request_responses.remove(&error.id) {
                                let _ = response_tx.send(Err(error.error));
                            } else {
                                warn!(
                                    request_id = ?error.id,
                                    "dropping unmatched in-process error response"
                                );
                            }
                        }
                        OutgoingMessage::Request(request) => {
                            // Send directly to avoid cloning; on failure the
                            // original value is returned inside the error.
                            if let Err(send_error) = event_tx
                                .try_send(InProcessServerEvent::ServerRequest(request))
                            {
                                let (code, message, inner) = match send_error {
                                    mpsc::error::TrySendError::Full(inner) => (
                                        OVERLOADED_ERROR_CODE,
                                        "in-process server request queue is full",
                                        inner,
                                    ),
                                    mpsc::error::TrySendError::Closed(inner) => (
                                        INTERNAL_ERROR_CODE,
                                        "in-process server request consumer is closed",
                                        inner,
                                    ),
                                };
                                let request_id = match inner {
                                    InProcessServerEvent::ServerRequest(req) => req.id().clone(),
                                    _ => unreachable!("we just sent a ServerRequest variant"),
                                };
                                outgoing_message_sender
                                    .notify_client_error(
                                        request_id,
                                        JSONRPCErrorError {
                                            code,
                                            message: message.to_string(),
                                            data: None,
                                        },
                                    )
                                    .await;
                            }
                        }
                        OutgoingMessage::AppServerNotification(notification) => {
                            let requires_delivery =
                                server_notification_requires_delivery(&notification);
                            let event = InProcessServerEvent::ServerNotification(notification);
                            if enqueue_in_process_event(
                                &event_tx,
                                &mut pending_delivery_events,
                                &mut event_consumer_gone,
                                event,
                                requires_delivery,
                                "server notification",
                            ) {
                                break;
                            }
                        }
                        OutgoingMessage::Notification(notification) => {
                            let notification = JSONRPCNotification {
                                method: notification.method,
                                params: notification.params,
                            };
                            let requires_delivery =
                                legacy_notification_requires_delivery(&notification);
                            let event = InProcessServerEvent::LegacyNotification(notification);
                            if enqueue_in_process_event(
                                &event_tx,
                                &mut pending_delivery_events,
                                &mut event_consumer_gone,
                                event,
                                requires_delivery,
                                "legacy notification",
                            ) {
                                break;
                            }
                        }
                    }
                }
            }
        }

        drop(writer_rx);
        drop(processor_tx);
        outgoing_message_sender
            .cancel_all_requests(Some(JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: "in-process app-server runtime is shutting down".to_string(),
                data: None,
            }))
            .await;
        // Drop the runtime's last sender before awaiting the router task so
        // `outgoing_rx.recv()` can observe channel closure and exit cleanly.
        drop(outgoing_message_sender);
        for (_, response_tx) in pending_request_responses {
            let _ = response_tx.send(Err(JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: "in-process app-server runtime is shutting down".to_string(),
                data: None,
            }));
        }

        if let Err(_elapsed) = timeout(SHUTDOWN_TIMEOUT, &mut processor_handle).await {
            processor_handle.abort();
            let _ = processor_handle.await;
        }
        if let Err(_elapsed) = timeout(SHUTDOWN_TIMEOUT, &mut outbound_handle).await {
            outbound_handle.abort();
            let _ = outbound_handle.await;
        }

        if let Some(done_tx) = shutdown_ack {
            let _ = done_tx.send(());
        }
    });

    InProcessClientHandle {
        client: InProcessClientSender { client_tx },
        event_rx,
        runtime_handle: Some(runtime_handle),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_app_server_protocol::ClientInfo;
    use ctox_app_server_protocol::ConfigRequirementsReadResponse;
    use ctox_app_server_protocol::SessionSource as ApiSessionSource;
    use ctox_app_server_protocol::ThreadStartParams;
    use ctox_app_server_protocol::ThreadStartResponse;
    use ctox_app_server_protocol::Turn;
    use ctox_app_server_protocol::TurnCompletedNotification;
    use ctox_app_server_protocol::TurnStatus;
    use ctox_core::config::ConfigBuilder;
    use pretty_assertions::assert_eq;

    async fn build_test_config() -> Config {
        match ConfigBuilder::default().build().await {
            Ok(config) => config,
            Err(_) => Config::load_default_with_cli_overrides(Vec::new())
                .expect("default config should load"),
        }
    }

    async fn start_test_client_with_capacity(
        session_source: SessionSource,
        channel_capacity: usize,
    ) -> InProcessClientHandle {
        let args = InProcessStartArgs {
            arg0_paths: Arg0DispatchPaths::default(),
            config: Arc::new(build_test_config().await),
            cli_overrides: Vec::new(),
            loader_overrides: LoaderOverrides::default(),
            cloud_requirements: CloudRequirementsLoader::default(),
            auth_manager: None,
            thread_manager: None,
            feedback: CodexFeedback::new(),
            config_warnings: Vec::new(),
            session_source,
            enable_ctox_api_key_env: false,
            initialize: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-in-process-test".to_string(),
                    title: None,
                    version: "0.0.0".to_string(),
                },
                capabilities: None,
            },
            channel_capacity,
        };
        start(args).await.expect("in-process runtime should start")
    }

    async fn start_test_client(session_source: SessionSource) -> InProcessClientHandle {
        start_test_client_with_capacity(session_source, DEFAULT_IN_PROCESS_CHANNEL_CAPACITY).await
    }

    #[tokio::test]
    async fn in_process_start_initializes_and_handles_typed_v2_request() {
        let client = start_test_client(SessionSource::Cli).await;
        let response = client
            .request(ClientRequest::ConfigRequirementsRead {
                request_id: RequestId::Integer(1),
                params: None,
            })
            .await
            .expect("request transport should work")
            .expect("request should succeed");
        assert!(response.is_object());

        let _parsed: ConfigRequirementsReadResponse =
            serde_json::from_value(response).expect("response should match v2 schema");
        client
            .shutdown()
            .await
            .expect("in-process runtime should shutdown cleanly");
    }

    #[tokio::test]
    async fn in_process_start_uses_requested_session_source_for_thread_start() {
        for (requested_source, expected_source) in [
            (SessionSource::Cli, ApiSessionSource::Cli),
            (SessionSource::Exec, ApiSessionSource::Exec),
        ] {
            let client = start_test_client(requested_source).await;
            let response = client
                .request(ClientRequest::ThreadStart {
                    request_id: RequestId::Integer(2),
                    params: ThreadStartParams {
                        ephemeral: Some(true),
                        ..ThreadStartParams::default()
                    },
                })
                .await
                .expect("request transport should work")
                .expect("thread/start should succeed");
            let parsed: ThreadStartResponse =
                serde_json::from_value(response).expect("thread/start response should parse");
            assert_eq!(parsed.thread.source, expected_source);
            client
                .shutdown()
                .await
                .expect("in-process runtime should shutdown cleanly");
        }
    }

    #[tokio::test]
    async fn in_process_start_clamps_zero_channel_capacity() {
        let client = start_test_client_with_capacity(SessionSource::Cli, 0).await;
        let response = loop {
            match client
                .request(ClientRequest::ConfigRequirementsRead {
                    request_id: RequestId::Integer(4),
                    params: None,
                })
                .await
            {
                Ok(response) => break response.expect("request should succeed"),
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    tokio::task::yield_now().await;
                }
                Err(err) => panic!("request transport should work: {err}"),
            }
        };
        let _parsed: ConfigRequirementsReadResponse =
            serde_json::from_value(response).expect("response should match v2 schema");
        client
            .shutdown()
            .await
            .expect("in-process runtime should shutdown cleanly");
    }

    #[test]
    fn guaranteed_delivery_helpers_cover_terminal_notifications() {
        assert!(server_notification_requires_delivery(
            &ServerNotification::TurnCompleted(TurnCompletedNotification {
                thread_id: "thread-1".to_string(),
                turn: Turn {
                    id: "turn-1".to_string(),
                    items: Vec::new(),
                    status: TurnStatus::Completed,
                    error: None,
                },
            })
        ));

        assert!(legacy_notification_requires_delivery(
            &JSONRPCNotification {
                method: "codex/event/task_complete".to_string(),
                params: None,
            }
        ));
        assert!(legacy_notification_requires_delivery(
            &JSONRPCNotification {
                method: "codex/event/turn_aborted".to_string(),
                params: None,
            }
        ));
        assert!(legacy_notification_requires_delivery(
            &JSONRPCNotification {
                method: "codex/event/shutdown_complete".to_string(),
                params: None,
            }
        ));
        assert!(!legacy_notification_requires_delivery(
            &JSONRPCNotification {
                method: "codex/event/item_started".to_string(),
                params: None,
            }
        ));
    }

    #[test]
    fn enqueue_event_buffers_delivery_required_and_never_blocks() {
        let (event_tx, mut event_rx) = mpsc::channel::<InProcessServerEvent>(1);
        let mut pending = VecDeque::new();
        let mut consumer_gone = false;
        let delivery_event = || {
            InProcessServerEvent::LegacyNotification(JSONRPCNotification {
                method: "codex/event/task_complete".to_string(),
                params: None,
            })
        };
        let droppable_event = || {
            InProcessServerEvent::LegacyNotification(JSONRPCNotification {
                method: "codex/event/item_started".to_string(),
                params: None,
            })
        };

        // First event fits the queue directly.
        assert!(!enqueue_in_process_event(
            &event_tx,
            &mut pending,
            &mut consumer_gone,
            delivery_event(),
            true,
            "test",
        ));
        assert!(pending.is_empty());

        // Queue is now full: a delivery-required event buffers instead of
        // blocking; a droppable event is discarded.
        assert!(!enqueue_in_process_event(
            &event_tx,
            &mut pending,
            &mut consumer_gone,
            delivery_event(),
            true,
            "test",
        ));
        assert_eq!(pending.len(), 1);
        assert!(!enqueue_in_process_event(
            &event_tx,
            &mut pending,
            &mut consumer_gone,
            droppable_event(),
            false,
            "test",
        ));
        assert_eq!(pending.len(), 1);

        // Ordering: with a non-empty buffer, further delivery-required
        // events append behind it even if the queue has room again.
        assert!(event_rx.try_recv().is_ok());
        assert!(!enqueue_in_process_event(
            &event_tx,
            &mut pending,
            &mut consumer_gone,
            delivery_event(),
            true,
            "test",
        ));
        assert_eq!(pending.len(), 2);

        // A dropped receiver reports consumer-gone.
        drop(event_rx);
        pending.clear();
        assert!(enqueue_in_process_event(
            &event_tx,
            &mut pending,
            &mut consumer_gone,
            delivery_event(),
            true,
            "test",
        ));
        assert!(consumer_gone);
    }

    /// Regression test for ctox#21: a paused event consumer must never stall
    /// the request path. Before the fix, a delivery-required notification
    /// that did not fit the (capacity-1) event queue blocked the runtime
    /// select loop with `event_tx.send().await`, so no further client
    /// request — including `turn/interrupt` — was ever processed and the
    /// session wedged permanently.
    #[tokio::test]
    async fn paused_event_consumer_does_not_block_request_path() {
        use app_test_support::create_mock_responses_server_repeating_assistant;
        use app_test_support::write_mock_responses_config_toml;
        use ctox_app_server_protocol::ThreadStartParams;
        use ctox_app_server_protocol::ThreadStartResponse;
        use ctox_app_server_protocol::TurnStartParams;
        use ctox_app_server_protocol::TurnStartResponse;
        use ctox_app_server_protocol::UserInput;
        use std::collections::BTreeMap;

        let server = create_mock_responses_server_repeating_assistant("done").await;
        let codex_home = tempfile::TempDir::new().expect("tempdir");
        write_mock_responses_config_toml(
            codex_home.path(),
            &server.uri(),
            &BTreeMap::new(),
            8_192,
            Some(false),
            "mock_provider",
            "compact",
        )
        .expect("mock config should write");
        let config = ConfigBuilder::default()
            .codex_home(codex_home.path().to_path_buf())
            .build()
            .await
            .expect("test config should build");

        let args = InProcessStartArgs {
            arg0_paths: Arg0DispatchPaths::default(),
            config: Arc::new(config),
            cli_overrides: Vec::new(),
            loader_overrides: LoaderOverrides::default(),
            cloud_requirements: CloudRequirementsLoader::default(),
            auth_manager: None,
            thread_manager: None,
            feedback: CodexFeedback::new(),
            // Emitted right after initialize as droppable notifications:
            // the first one occupies the single event-queue slot, so the
            // turn's delivery-required completion event can never fit
            // directly and MUST take the pending buffer instead of the old
            // blocking send.
            config_warnings: vec![ConfigWarningNotification {
                summary: "ctox#21 regression: pre-fill event queue".to_string(),
                details: None,
                path: None,
                range: None,
            }],
            session_source: SessionSource::Exec,
            enable_ctox_api_key_env: false,
            initialize: InitializeParams {
                client_info: ClientInfo {
                    name: "ctox-21-regression".to_string(),
                    title: None,
                    version: "0.0.0".to_string(),
                },
                capabilities: None,
            },
            // Smallest legal queue: a single unread event saturates it.
            channel_capacity: 1,
        };
        let mut client = start(args).await.expect("in-process runtime should start");

        // The capacity-1 client command queue can transiently reject with
        // WouldBlock (same as the zero-capacity clamp test); retry those.
        // A WEDGED loop never returns at all — that case is caught by the
        // liveness timeout below, not by this retry.
        async fn request_with_retry(
            client: &InProcessClientHandle,
            request: ClientRequest,
        ) -> Result {
            loop {
                match client.request(request.clone()).await {
                    Ok(response) => return response.expect("request should succeed"),
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        tokio::task::yield_now().await;
                    }
                    Err(err) => panic!("request transport should work: {err}"),
                }
            }
        }

        let thread_response = request_with_retry(
            &client,
            ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    ephemeral: Some(true),
                    ..ThreadStartParams::default()
                },
            },
        )
        .await;
        let thread: ThreadStartResponse =
            serde_json::from_value(thread_response).expect("thread/start response");

        let turn_response = request_with_retry(
            &client,
            ClientRequest::TurnStart {
                request_id: RequestId::Integer(2),
                params: TurnStartParams {
                    thread_id: thread.thread.id,
                    input: vec![UserInput::Text {
                        text: "hello".to_string(),
                        text_elements: Vec::new(),
                    }],
                    developer_instructions: None,
                    cwd: None,
                    approval_policy: None,
                    sandbox_policy: None,
                    approvals_reviewer: None,
                    model: None,
                    service_tier: None,
                    effort: None,
                    summary: None,
                    personality: None,
                    output_schema: None,
                    collaboration_mode: None,
                },
            },
        )
        .await;
        let _turn: TurnStartResponse =
            serde_json::from_value(turn_response).expect("turn/start response");

        // Nobody reads a single event during this phase, and the config
        // warning above keeps the capacity-1 queue occupied the whole time.
        // The turn reaches its terminal state within a few seconds
        // (regardless of whether the mock round trip succeeds — a failed
        // turn also emits a delivery-required terminal event), so its
        // completion event hits the full queue while these requests run.
        // Before the fix that blocked the runtime loop and the next request
        // hung forever.
        let liveness_deadline = tokio::time::Instant::now() + Duration::from_secs(45);
        let mut request_id = 3;
        while tokio::time::Instant::now() < liveness_deadline {
            let response = tokio::time::timeout(
                Duration::from_secs(10),
                request_with_retry(
                    &client,
                    ClientRequest::ConfigRequirementsRead {
                        request_id: RequestId::Integer(request_id),
                        params: None,
                    },
                ),
            )
            .await
            .expect("request path must stay live while events are unread");
            assert!(response.is_object());
            request_id += 1;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // The buffered terminal event must still be delivered once the
        // consumer resumes — buffering must not become event loss.
        let drain_deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        let mut saw_turn_terminal_event = false;
        while tokio::time::Instant::now() < drain_deadline {
            let event =
                match tokio::time::timeout(Duration::from_secs(10), client.next_event()).await {
                    Ok(Some(event)) => event,
                    Ok(None) | Err(_) => break,
                };
            match &event {
                InProcessServerEvent::ServerNotification(ServerNotification::TurnCompleted(_)) => {
                    saw_turn_terminal_event = true;
                    break;
                }
                InProcessServerEvent::LegacyNotification(notification)
                    if legacy_notification_requires_delivery(notification) =>
                {
                    saw_turn_terminal_event = true;
                    break;
                }
                _ => {}
            }
        }
        assert!(
            saw_turn_terminal_event,
            "delivery-required turn terminal event must survive the pending buffer"
        );

        client
            .shutdown()
            .await
            .expect("in-process runtime should shutdown cleanly");
    }
}
