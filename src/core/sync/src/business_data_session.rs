//! Trusted-host Open/Status/Close lifecycle for native BusinessData.
//!
//! A service instance is bound to one private IPC connection. Handles, WebRTC
//! sessions and credential requesters are therefore never shared between
//! clients. The host supplies saved-target pins/options and current principal;
//! renderer requests name an already-enrolled target only. Query, watch and
//! command operations are intentionally unsupported by this first lifecycle
//! service and fail closed.
use crate::{
    business_data_contract::{
        NativeBusinessDataBinding as Binding, NativeBusinessDataErrorCode as ErrorCode,
        NativeBusinessDataOperation as Operation, NativeBusinessDataPrincipal as Principal,
        NativeBusinessDataRequest as Request, NativeBusinessDataResponse as Response,
        NativeBusinessDataResult as Result, NativeBusinessDataSessionRef as SessionRef,
        NativeBusinessDataSessionState as State,
    },
    business_data_ipc::{
        BusinessDataDispatchFuture, BusinessDataDispatcher, BusinessDataShutdownFuture,
    },
    credential_ipc::CredentialRequester,
    native::{
        NativeCredentialBinding, NativeSessionTarget, NativeSessionTargetProvider,
        NativeSyncOptions, NativeSyncSession,
    },
};
use async_trait::async_trait;
use ring::rand::{SecureRandom, SystemRandom};
use rxdb::plugins::replication_webrtc::WebRTCConnectionHandler;
use std::{
    collections::HashMap,
    io,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::task::JoinHandle;

const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
const READY_TIMEOUT: Duration = Duration::from_secs(10);

/// Independently enrolled target identity and the host's local account epoch.
/// The epoch fences credentials and is deliberately distinct from the server
/// authorization epoch inside [`Principal`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedBusinessDataTarget {
    pub public_identity: String,
    pub instance_id: String,
    pub account_epoch: u64,
}

/// The trusted host integration. `native_options` must return query-only
/// options without a local session provider; this service installs the only
/// provider and independently proves the target before credential use.
#[async_trait]
pub trait BusinessDataSessionHost: Send + Sync {
    async fn saved_target(&self, target_id: &str) -> io::Result<Option<SavedBusinessDataTarget>>;
    async fn current_principal(&self, target_id: &str) -> io::Result<Option<Principal>>;
    async fn native_options(&self, target_id: &str) -> io::Result<NativeSyncOptions>;
}

type StartupResult = io::Result<Arc<NativeSyncSession>>;

/// Resolve result: handle, target, valid-generation metadata, lifecycle and
/// optional revocation.
type SessionSnapshot = (
    String,
    String,
    Option<OwnedSession>,
    Lifecycle,
    Option<(u64, String)>,
);

/// A startup task remains owned by the handle table until it has completed and
/// its successful transport has moved into the one owned-transport slot.
#[derive(Clone)]
struct OwnedStartup {
    result: Arc<tokio::sync::Mutex<Option<StartupResult>>>,
    task: Arc<tokio::sync::Mutex<Option<JoinHandle<()>>>>,
    drain_deadline: Duration,
}

enum OwnedCleanup {
    Starting(OwnedStartup),
    Session(Arc<NativeSyncSession>),
}

impl OwnedStartup {
    fn start(options: NativeSyncOptions) -> Self {
        // Native bring-up is bounded by the host-supplied deadline and its own
        // failure path drains resources before returning. Allow one cleanup
        // budget after that boundary.
        let drain_deadline = options.bringup_timeout + CLEANUP_TIMEOUT + Duration::from_secs(1);
        let result: Arc<tokio::sync::Mutex<Option<StartupResult>>> = Arc::default();
        let stored_result = result.clone();
        let task = tokio::spawn(async move {
            let outcome = NativeSyncSession::start_data_client(options)
                .await
                .map(Arc::new);
            *stored_result.lock().await = Some(outcome);
        });
        Self {
            result,
            task: Arc::new(tokio::sync::Mutex::new(Some(task))),
            drain_deadline,
        }
    }

    async fn take_session(&self) -> io::Result<Arc<NativeSyncSession>> {
        if let Some(task) = self.task.lock().await.take() {
            let _ = task.await;
        }
        self.result
            .lock()
            .await
            .take()
            .ok_or_else(|| unavailable("BusinessData startup result was already owned"))?
    }

    async fn shutdown(self) -> io::Result<()> {
        let drained = tokio::time::timeout(self.drain_deadline, async {
            if let Some(task) = self.task.lock().await.take() {
                let _ = task.await;
            }
            if let Some(Ok(session)) = self.result.lock().await.take() {
                shutdown_bounded(&session).await?;
            }
            Ok::<(), io::Error>(())
        })
        .await;
        match drained {
            Ok(result) => result,
            Err(_) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "BusinessData startup cleanup timed out",
            )),
        }
    }
}

/// Metadata for the one currently valid handle generation. The native session
/// itself is stored only once beside it.
#[derive(Clone)]
struct OwnedSession {
    generation: u64,
    target_id: String,
    saved: SavedBusinessDataTarget,
    binding: Binding,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    Connecting,
    Ready,
    Revoked,
}

struct SessionEntry {
    lifecycle: Lifecycle,
    target_id: String,
    startup: Option<OwnedStartup>,
    transport: Option<Arc<NativeSyncSession>>,
    session: Option<OwnedSession>,
    revocation: Option<(u64, String)>,
}

/// One connection's handle table and owned native sessions.
pub struct BusinessDataService {
    host: Arc<dyn BusinessDataSessionHost>,
    sessions: Mutex<HashMap<String, SessionEntry>>,
}

impl BusinessDataService {
    pub fn new(host: Arc<dyn BusinessDataSessionHost>) -> Self {
        Self {
            host,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Number of handle-table slots still owning startup or transport cleanup.
    pub fn owned_cleanup_count(&self) -> usize {
        self.sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .filter(|entry| entry.startup.is_some() || entry.transport.is_some())
            .count()
    }

    /// The dispatcher is private to one IPC connection. `BusinessDataIpc`
    /// awaits its shutdown after disconnect, even for pending authentication.
    pub fn dispatcher(
        self: &Arc<Self>,
        credentials: CredentialRequester,
    ) -> BusinessDataServiceDispatcher {
        BusinessDataServiceDispatcher {
            service: self.clone(),
            credentials,
        }
    }

    /// Stop every owned transport once. Startup and ready transports are
    /// deliberately represented by mutually exclusive slots so a successful
    /// startup cannot be drained twice.
    pub async fn shutdown(&self) -> io::Result<()> {
        let cleanups: Vec<_> = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values_mut()
            .filter_map(|entry| {
                if let Some(startup) = entry.startup.take() {
                    entry.transport.take();
                    Some(OwnedCleanup::Starting(startup))
                } else {
                    entry.transport.take().map(OwnedCleanup::Session)
                }
            })
            .collect();
        let mut failure = None;
        for cleanup in cleanups {
            let result = match cleanup {
                OwnedCleanup::Starting(startup) => startup.shutdown().await,
                OwnedCleanup::Session(session) => shutdown_bounded(&session).await,
            };
            if let Err(error) = result {
                failure.get_or_insert(error);
            }
        }
        self.sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        failure.map_or(Ok(()), Err)
    }

    async fn open(
        &self,
        request_id: &str,
        credentials: CredentialRequester,
        target_id: &str,
    ) -> io::Result<Response> {
        let handle = fresh_handle()?;
        let saved = self
            .host
            .saved_target(target_id)
            .await?
            .ok_or_else(|| unauthorized("saved BusinessData target is unavailable"))?;
        validate_saved(&saved)?;
        let mut options = self.host.native_options(target_id).await?;
        if options.local_session_provider.is_some()
            || !options.collections.is_empty()
            || !options.database.collections.lock().is_empty()
        {
            return Err(invalid("host supplied non-query-only BusinessData options"));
        }
        options.local_session_provider = Some(local_provider(
            self.host.clone(),
            credentials,
            target_id.to_owned(),
            saved.clone(),
        ));

        // Bring-up is spawned only after this handle owns a Connecting slot.
        // Dropping the IPC request therefore cannot drop a native startup that
        // shutdown still needs to await and drain.
        let startup = OwnedStartup::start(options);
        let entry = SessionEntry {
            lifecycle: Lifecycle::Connecting,
            target_id: target_id.to_owned(),
            startup: Some(startup.clone()),
            transport: None,
            session: None,
            revocation: None,
        };
        self.sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(handle.clone(), entry);
        let session = match startup.take_session().await {
            Ok(session) => session,
            Err(error) => {
                self.sessions
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .remove(&handle);
                return Err(io::Error::other(format!(
                    "BusinessData startup failed: {error}"
                )));
            }
        };
        // Move the session into the table before any further await. Startup and
        // Ready now share the same canonical owned-transport slot, never two.
        let registered = {
            let mut sessions = self
                .sessions
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            match sessions.get_mut(&handle) {
                Some(entry) => {
                    entry.startup = None;
                    entry.transport = Some(session.clone());
                    true
                }
                None => false,
            }
        };
        if !registered {
            shutdown_bounded(&session).await?;
            return Err(unavailable("BusinessData startup handle was invalidated"));
        }
        match self.activate(&session, target_id, &saved).await {
            Ok((_connection, principal)) => {
                let generation = next_generation();
                let owned = OwnedSession {
                    generation,
                    target_id: target_id.to_owned(),
                    saved: saved.clone(),
                    binding: Binding {
                        target_id: target_id.to_owned(),
                        instance_id: saved.instance_id.clone(),
                        user_id: principal.user_id.clone(),
                    },
                };
                let registered = {
                    let mut sessions = self
                        .sessions
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    match sessions.get_mut(&handle) {
                        Some(entry) => {
                            entry.lifecycle = Lifecycle::Ready;
                            entry.session = Some(owned);
                            entry.revocation = None;
                            true
                        }
                        None => false,
                    }
                };
                if !registered {
                    shutdown_bounded(&session).await?;
                    return Err(unavailable("BusinessData startup handle was invalidated"));
                }
                Ok(response_session(
                    request_id,
                    State::Ready {
                        session: ref_of(&handle, generation),
                        binding: Binding {
                            target_id: target_id.to_owned(),
                            instance_id: saved.instance_id,
                            user_id: principal.user_id,
                        },
                    },
                ))
            }
            Err(error) => {
                self.sessions
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .remove(&handle);
                let drained = shutdown_bounded(&session).await.is_ok();
                if !drained {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "BusinessData startup cleanup timed out",
                    ));
                }
                Err(error)
            }
        }
    }

    async fn activate(
        &self,
        session: &Arc<NativeSyncSession>,
        target_id: &str,
        saved: &SavedBusinessDataTarget,
    ) -> io::Result<(
        rxdb::plugins::replication_webrtc::WebRTCRsConnection,
        Principal,
    )> {
        let connection = wait_connection(session, READY_TIMEOUT).await?;
        wait_peer_ready(session, &connection, READY_TIMEOUT).await?;
        // Readiness is only transport plus source admission. Independently
        // re-prove the same channel now that reciprocal readiness is required.
        let proof = session
            .peer_identity_proof(
                connection.clone(),
                &saved.public_identity,
                &saved.instance_id,
            )
            .await?;
        let expected = self
            .host
            .current_principal(target_id)
            .await?
            .ok_or_else(|| unauthorized("BusinessData principal is unavailable"))?;
        let actual = proof
            .principal
            .ok_or_else(|| unauthorized("BusinessData source did not attest a principal"))?;
        if actual != expected {
            return Err(unauthorized("BusinessData principal is stale or wrong"));
        }
        self.assert_current(target_id, saved, &expected).await?;
        Ok((connection, actual))
    }

    async fn assert_current(
        &self,
        target_id: &str,
        saved: &SavedBusinessDataTarget,
        principal: &Principal,
    ) -> io::Result<()> {
        let current = self
            .host
            .saved_target(target_id)
            .await?
            .filter(|current| current == saved)
            .ok_or_else(|| unauthorized("saved BusinessData target or account is stale"))?;
        let current_principal = self
            .host
            .current_principal(target_id)
            .await?
            .filter(|current| current == principal)
            .ok_or_else(|| unauthorized("BusinessData principal is stale"))?;
        validate_saved(&current)?;
        if current_principal.user_id != principal.user_id {
            return Err(unauthorized("BusinessData principal changed"));
        }
        Ok(())
    }

    fn resolve(&self, session: &SessionRef) -> io::Result<SessionSnapshot> {
        let sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let Some(entry) = sessions.get(&session.handle) else {
            return Err(unknown("unknown BusinessData session"));
        };
        if entry.lifecycle == Lifecycle::Connecting {
            return Ok((
                session.handle.clone(),
                entry.target_id.clone(),
                None,
                Lifecycle::Connecting,
                None,
            ));
        }
        if let Some(revocation) = entry.revocation.clone() {
            if revocation.0 != session.generation {
                return Err(rejected(
                    ErrorCode::StaleGeneration,
                    "stale BusinessData session generation",
                ));
            }
            return Ok((
                session.handle.clone(),
                entry.target_id.clone(),
                None,
                Lifecycle::Revoked,
                Some(revocation),
            ));
        }
        let owned = entry
            .session
            .as_ref()
            .ok_or_else(|| invalid("BusinessData session has no owned transport"))?;
        if owned.generation != session.generation {
            return Err(rejected(
                ErrorCode::StaleGeneration,
                "stale BusinessData session generation",
            ));
        }
        Ok((
            session.handle.clone(),
            entry.target_id.clone(),
            Some(owned.clone()),
            entry.lifecycle,
            None,
        ))
    }

    async fn status(&self, request_id: &str, session: &SessionRef) -> io::Result<Response> {
        let (handle, target_id, owned, lifecycle, revocation) = self.resolve(session)?;
        let Some(owned) = owned else {
            if let Some((generation, reason)) = revocation {
                return Ok(response_session(
                    request_id,
                    State::Revoked {
                        session: ref_of(&handle, generation),
                        reason,
                    },
                ));
            }
            return Ok(response_session(
                request_id,
                State::Resolving {
                    session: session.clone(),
                    target_id,
                },
            ));
        };
        match self.host.saved_target(&owned.target_id).await? {
            Some(current) if current == owned.saved => (),
            _ => {
                return self
                    .revoke(
                        request_id,
                        &handle,
                        owned.generation,
                        "saved target was invalidated",
                    )
                    .await
            }
        }
        let saved = owned.saved.clone();
        let principal = match self.host.current_principal(&owned.target_id).await {
            Ok(Some(principal)) => principal,
            _ => {
                return self
                    .revoke(
                        request_id,
                        &handle,
                        owned.generation,
                        "principal was invalidated",
                    )
                    .await
            }
        };
        match self
            .assert_current(&owned.target_id, &saved, &principal)
            .await
        {
            Ok(()) if lifecycle == Lifecycle::Ready => Ok(response_session(
                request_id,
                State::Ready {
                    session: ref_of(&handle, owned.generation),
                    binding: owned.binding.clone(),
                },
            )),
            _ => {
                self.revoke(
                    request_id,
                    &handle,
                    owned.generation,
                    "target or principal was invalidated",
                )
                .await
            }
        }
    }

    async fn revoke(
        &self,
        request_id: &str,
        handle: &str,
        generation: u64,
        reason: &str,
    ) -> io::Result<Response> {
        let transport = {
            let mut sessions = self
                .sessions
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let Some(entry) = sessions.get_mut(handle) else {
                return Err(unknown("unknown BusinessData session"));
            };
            entry.lifecycle = Lifecycle::Revoked;
            entry.revocation = Some((generation, reason.to_owned()));
            entry.transport.take()
        };
        if let Some(session) = transport {
            shutdown_bounded(&session).await?;
        }
        Ok(response_session(
            request_id,
            State::Revoked {
                session: ref_of(handle, generation),
                reason: reason.into(),
            },
        ))
    }

    async fn close(&self, request_id: &str, session: &SessionRef) -> io::Result<Response> {
        let (handle, _target_id, owned, _lifecycle, revocation) = self.resolve(session)?;
        let Some(owned) = owned else {
            if let Some((generation, reason)) = revocation {
                return Ok(response_session(
                    request_id,
                    State::Revoked {
                        session: ref_of(&handle, generation),
                        reason,
                    },
                ));
            }
            return Err(unknown("BusinessData session is still resolving"));
        };
        let native = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&handle)
            .and_then(|entry| entry.transport);
        let Some(native) = native else {
            return Err(invalid("BusinessData session has no owned transport"));
        };
        shutdown_bounded(&native).await?;
        Ok(response_session(
            request_id,
            State::Disconnected {
                session: session.clone(),
                binding: owned.binding,
            },
        ))
    }
}

/// The first dispatcher intentionally owns lifecycle only. Every data-bearing
/// operation fails closed until policy-scoped transport operations land.
impl BusinessDataDispatcher for BusinessDataServiceDispatcher {
    fn dispatch(&self, request: Request) -> BusinessDataDispatchFuture {
        let service = self.service.clone();
        let credentials = self.credentials.clone();
        Box::pin(async move {
            let result = match &request.operation {
                Operation::Open { target_id } => {
                    service
                        .open(&request.request_id, credentials, target_id)
                        .await
                }
                Operation::Status { session } => service.status(&request.request_id, session).await,
                Operation::Close { session } => service.close(&request.request_id, session).await,
                _ => Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "BusinessData operation is unsupported",
                )),
            };
            Ok(result.unwrap_or_else(|error| response_rejection(&request.request_id, &error)))
        })
    }

    fn shutdown(&self) -> BusinessDataShutdownFuture<'_> {
        let service = self.service.clone();
        Box::pin(async move { service.shutdown().await })
    }
}

/// One private IPC connection's dispatcher and credential requester.
pub struct BusinessDataServiceDispatcher {
    service: Arc<BusinessDataService>,
    credentials: CredentialRequester,
}

fn response_rejection(request_id: &str, error: &io::Error) -> Response {
    let (code, retryable) = if error
        .get_ref()
        .is_some_and(|value| value.is::<StaleGeneration>())
    {
        (ErrorCode::StaleGeneration, false)
    } else {
        match error.kind() {
            io::ErrorKind::InvalidInput => (ErrorCode::InvalidRequest, false),
            io::ErrorKind::PermissionDenied => (ErrorCode::Unauthorized, false),
            io::ErrorKind::NotFound => (ErrorCode::UnknownSession, false),
            io::ErrorKind::Unsupported => (ErrorCode::Unsupported, false),
            io::ErrorKind::TimedOut => (ErrorCode::LimitExceeded, true),
            _ => (ErrorCode::Disconnected, true),
        }
    };
    let message = rejection_message(&code);
    Response {
        version: crate::business_data_contract::CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
        request_id: request_id.to_owned(),
        result: Result::Rejected {
            code,
            message,
            retryable,
        },
    }
}

fn rejection_message(code: &ErrorCode) -> String {
    match code {
        ErrorCode::InvalidRequest => "invalid BusinessData lifecycle request".into(),
        ErrorCode::Unauthorized => "target, account or principal was not current".into(),
        ErrorCode::UnknownSession => "BusinessData session handle is unknown".into(),
        ErrorCode::StaleGeneration => "BusinessData session generation is stale".into(),
        ErrorCode::Disconnected => "BusinessData session is unavailable".into(),
        ErrorCode::Unsupported => "BusinessData operation is unsupported".into(),
        ErrorCode::LimitExceeded => "BusinessData operation exceeded its cleanup deadline".into(),
        _ => "BusinessData request was rejected".into(),
    }
}

fn local_provider(
    host: Arc<dyn BusinessDataSessionHost>,
    credentials: CredentialRequester,
    target_id: String,
    expected: SavedBusinessDataTarget,
) -> NativeSessionTargetProvider {
    Arc::new(move |connection| {
        let host = host.clone();
        let credentials = credentials.clone();
        let target_id = target_id.clone();
        let expected = expected.clone();
        Box::pin(async move {
            // The target proof has completed when this provider is invoked.
            // A target/account change still fences credential release here.
            let saved = host
                .saved_target(&target_id)
                .await
                .map_err(|_| credential_failure())?
                .filter(|saved| saved == &expected)
                .ok_or_else(credential_failure)?;
            validate_saved(&saved).map_err(|_| credential_failure())?;
            let binding = NativeCredentialBinding {
                target_id: target_id.clone(),
                connection_id: format!(
                    "business-data-{}-{}",
                    connection.peer_id(),
                    connection.generation()
                ),
                account_epoch: saved.account_epoch,
                connection,
            };
            Ok(NativeSessionTarget::with_ipc_credentials(
                saved.public_identity,
                saved.instance_id,
                binding,
                credentials,
            ))
        })
    })
}

async fn wait_connection(
    session: &NativeSyncSession,
    timeout: Duration,
) -> io::Result<rxdb::plugins::replication_webrtc::WebRTCRsConnection> {
    // Discovery establishes the one connection before the target provider can
    // run. Poll the current exact handle rather than subscribing after the fact.
    let deadline = tokio::time::Instant::now() + timeout;
    let mut delay = Duration::from_millis(20);
    loop {
        let session = session.pool();
        if let Some(connection) = session
            .connection_handler
            .current_connections()
            .into_iter()
            .next()
        {
            return Ok(connection);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(unavailable("BusinessData connection was not established"));
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(Duration::from_millis(100));
    }
}

async fn wait_peer_ready(
    session: &NativeSyncSession,
    connection: &rxdb::plugins::replication_webrtc::WebRTCRsConnection,
    timeout: Duration,
) -> io::Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if session.pool().is_peer_ready_for_control(connection) {
            return Ok(());
        }
        if !session
            .pool()
            .connection_handler
            .is_peer_current(connection)
        {
            return Err(unavailable(
                "BusinessData connection retired before readiness",
            ));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(unavailable("BusinessData connection did not become ready"));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn shutdown_bounded(session: &NativeSyncSession) -> io::Result<()> {
    tokio::time::timeout(CLEANUP_TIMEOUT, session.shutdown())
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "BusinessData cleanup timed out"))
}

fn validate_saved(saved: &SavedBusinessDataTarget) -> io::Result<()> {
    if saved.public_identity.is_empty() || saved.instance_id.is_empty() || saved.account_epoch == 0
    {
        return Err(invalid("incomplete saved BusinessData target"));
    }
    Ok(())
}

fn response_session(request_id: &str, state: State) -> Response {
    Response {
        version: crate::business_data_contract::CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
        request_id: request_id.to_owned(),
        result: Result::Session { state },
    }
}

fn ref_of(handle: &str, generation: u64) -> SessionRef {
    SessionRef {
        handle: handle.to_owned(),
        generation,
    }
}

fn next_generation() -> u64 {
    static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

fn fresh_handle() -> io::Result<String> {
    let mut bytes = [0; 16];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| io::Error::other("BusinessData handle generation failed"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn unauthorized(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
fn unavailable(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, message)
}
fn unknown(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, message)
}
fn rejected(code: ErrorCode, message: &str) -> io::Error {
    let payload: Box<dyn std::error::Error + Send + Sync> = if code == ErrorCode::StaleGeneration {
        Box::new(StaleGeneration)
    } else {
        message.into()
    };
    io::Error::new(io::ErrorKind::InvalidInput, payload)
}

#[derive(Debug)]
struct StaleGeneration;

impl std::fmt::Display for StaleGeneration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("stale BusinessData session generation")
    }
}

impl std::error::Error for StaleGeneration {}
fn credential_failure() -> rxdb::rx_error::RxError {
    rxdb::rx_error::new_rx_error(
        "RC_WEBRTC_PEER",
        Some(serde_json::json!({
            "code": "business_data_target_stale",
            "message": "saved BusinessData target or account is stale"
        })),
    )
}
