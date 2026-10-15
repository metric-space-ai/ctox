// Origin: CTOX
// Port-Status: adapted_to_ctox
// Port-Note: replaces in-process loading with a supervised child process
// License: AGPL-3.0-only

use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};
use zeroize::Zeroizing;

use crate::sdk::pluginabi::{
    Envelope, METHOD_EXECUTOR_IDENTIFIER, METHOD_PLUGIN_REGISTER, METHOD_PLUGIN_SHUTDOWN,
    SCHEMA_VERSION,
};

use super::process_transport::{
    read_process_message, write_process_message, InflightRequests, ProcessEvent, RequestMode,
};
use super::rpc_schema::{
    decode_upstream_json, encode_upstream_json, ProcessMessage, RpcEmptyResponse,
    RpcIdentifierResponse, RpcLifecycleRequest, RpcRegistration, PROCESS_PROTOCOL_VERSION,
};
#[cfg(unix)]
use super::transport_unix::{
    UnixPluginConnection as PlatformPluginConnection, UnixPluginEndpoint as PlatformPluginEndpoint,
};
#[cfg(windows)]
use super::transport_windows::{
    WindowsPluginConnection as PlatformPluginConnection,
    WindowsPluginEndpoint as PlatformPluginEndpoint,
};

const CONTROL_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct PluginProcessConfig {
    pub executable: PathBuf,
    pub runtime_root: PathBuf,
    pub instance_id: String,
    pub plugin_id: String,
    pub restart_policy: RestartPolicy,
}

impl fmt::Debug for PluginProcessConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginProcessConfig")
            .field("executable", &self.executable)
            .field("runtime_root", &self.runtime_root)
            .field("instance_id", &self.instance_id)
            .field("plugin_id", &self.plugin_id)
            .field("restart_policy", &self.restart_policy)
            .finish()
    }
}

impl PluginProcessConfig {
    fn validate(&self) -> Result<(), PluginSupervisorError> {
        if !self.executable.is_absolute() || !self.runtime_root.is_absolute() {
            return Err(PluginSupervisorError::InvalidConfig);
        }
        let executable =
            fs::metadata(&self.executable).map_err(|_| PluginSupervisorError::InvalidConfig)?;
        if !executable.is_file() {
            return Err(PluginSupervisorError::InvalidConfig);
        }
        #[cfg(unix)]
        if executable.permissions().mode() & 0o111 == 0 {
            return Err(PluginSupervisorError::InvalidConfig);
        }
        let root =
            fs::metadata(&self.runtime_root).map_err(|_| PluginSupervisorError::InvalidConfig)?;
        if !root.is_dir() {
            return Err(PluginSupervisorError::InvalidConfig);
        }
        self.restart_policy.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestartPolicy {
    pub max_restarts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_restarts: 4,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
        }
    }
}

impl RestartPolicy {
    fn validate(&self) -> Result<(), PluginSupervisorError> {
        if self.max_restarts == 0
            || self.base_delay.is_zero()
            || self.max_delay < self.base_delay
            || self.max_delay > Duration::from_secs(30)
        {
            return Err(PluginSupervisorError::InvalidConfig);
        }
        Ok(())
    }

    pub fn delay_for_failure(&self, failure: u32) -> Duration {
        let shift = failure.saturating_sub(1).min(31);
        self.base_delay
            .saturating_mul(1_u32 << shift)
            .min(self.max_delay)
    }
}

pub struct PluginSupervisor {
    config: PluginProcessConfig,
    active: Option<ActivePluginProcess>,
    consecutive_failures: u32,
}

impl PluginSupervisor {
    pub fn new(config: PluginProcessConfig) -> Result<Self, PluginSupervisorError> {
        config.validate()?;
        Ok(Self {
            config,
            active: None,
            consecutive_failures: 0,
        })
    }

    pub fn is_ready(&self) -> bool {
        self.active.is_some()
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    pub fn is_registered(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.registration.is_some())
    }

    pub async fn start(&mut self) -> Result<(), PluginSupervisorError> {
        if self.active.is_some() {
            return Err(PluginSupervisorError::AlreadyRunning);
        }
        match ActivePluginProcess::launch(&self.config).await {
            Ok(active) => {
                self.active = Some(active);
                Ok(())
            }
            Err(error) => {
                self.consecutive_failures = self.consecutive_failures.saturating_add(1);
                Err(error)
            }
        }
    }

    pub fn mark_stable(&mut self) {
        if self.active.is_some() {
            self.consecutive_failures = 0;
        }
    }

    pub fn begin_request(
        &mut self,
        request_id: String,
        mode: RequestMode,
        deadline_unix_ms: Option<u64>,
        now_unix_ms: u64,
    ) -> Result<(), PluginSupervisorError> {
        self.active
            .as_mut()
            .ok_or(PluginSupervisorError::NotRunning)?
            .inflight
            .begin(request_id, mode, deadline_unix_ms, now_unix_ms)
            .map_err(|_| PluginSupervisorError::RequestState)
    }

    pub async fn register(
        &mut self,
        config_yaml: Vec<u8>,
    ) -> Result<RpcRegistration, PluginSupervisorError> {
        let active = self
            .active
            .as_mut()
            .ok_or(PluginSupervisorError::NotRunning)?;
        if active.registration.is_some() {
            return Err(PluginSupervisorError::AlreadyRegistered);
        }
        let payload = encode_upstream_json(&RpcLifecycleRequest {
            config_yaml,
            schema_version: SCHEMA_VERSION,
        })
        .map_err(|_| PluginSupervisorError::Protocol)?;
        let envelope = active
            .call_unary(METHOD_PLUGIN_REGISTER, payload, None)
            .await?;
        if !envelope.ok || envelope.error.is_some() {
            return Err(PluginSupervisorError::PluginRejected);
        }
        let result = envelope.result.ok_or(PluginSupervisorError::Protocol)?;
        let registration: RpcRegistration =
            decode_upstream_json(&result).map_err(|_| PluginSupervisorError::Protocol)?;
        if registration.schema_version != SCHEMA_VERSION
            || registration.metadata.name.trim().is_empty()
            || registration.metadata.name.len() > 128
        {
            return Err(PluginSupervisorError::InvalidRegistration);
        }
        active.registration = Some(registration.clone());
        Ok(registration)
    }

    pub async fn executor_identifier(&mut self) -> Result<String, PluginSupervisorError> {
        let active = self
            .active
            .as_mut()
            .ok_or(PluginSupervisorError::NotRunning)?;
        let supports_executor = active
            .registration
            .as_ref()
            .is_some_and(|registration| registration.capabilities.executor);
        if !supports_executor {
            return Err(if active.registration.is_some() {
                PluginSupervisorError::UnsupportedCapability
            } else {
                PluginSupervisorError::NotRegistered
            });
        }
        let payload = encode_upstream_json(&RpcEmptyResponse {})
            .map_err(|_| PluginSupervisorError::Protocol)?;
        let envelope = active
            .call_unary(METHOD_EXECUTOR_IDENTIFIER, payload, None)
            .await?;
        if !envelope.ok || envelope.error.is_some() {
            return Err(PluginSupervisorError::PluginRejected);
        }
        let result = envelope.result.ok_or(PluginSupervisorError::Protocol)?;
        let response: RpcIdentifierResponse =
            decode_upstream_json(&result).map_err(|_| PluginSupervisorError::Protocol)?;
        let identifier = response.identifier.trim();
        if identifier.is_empty() || identifier.len() > 128 {
            return Err(PluginSupervisorError::Protocol);
        }
        Ok(identifier.to_owned())
    }

    pub async fn wait_for_exit(&mut self) -> Result<ExitReport, PluginSupervisorError> {
        let status = self
            .active
            .as_mut()
            .ok_or(PluginSupervisorError::NotRunning)?
            .child
            .wait()
            .await
            .map_err(|_| PluginSupervisorError::Wait)?;
        let mut active = self.active.take().expect("active process checked above");
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        Ok(ExitReport {
            success: status.success(),
            code: status.code(),
            aborted_request_ids: active.inflight.abort_all(),
        })
    }

    pub async fn restart(&mut self) -> Result<Duration, PluginSupervisorError> {
        if self.active.is_some() {
            return Err(PluginSupervisorError::AlreadyRunning);
        }
        if self.consecutive_failures == 0 {
            return Err(PluginSupervisorError::RestartWithoutFailure);
        }
        if self.consecutive_failures > self.config.restart_policy.max_restarts {
            return Err(PluginSupervisorError::RestartExhausted);
        }
        let delay = self
            .config
            .restart_policy
            .delay_for_failure(self.consecutive_failures);
        sleep(delay).await;
        self.start().await?;
        Ok(delay)
    }

    pub(super) async fn call_process(
        &mut self,
        call: super::abi::PluginCall,
        plugin_id: &str,
        callbacks: &super::host_callbacks::HostCallbackRouter,
    ) -> Result<Envelope, super::abi::PluginClientError> {
        let active = self
            .active
            .as_mut()
            .ok_or(super::abi::PluginClientError::Closed)?;
        super::process_client::exchange_unary(
            active.connection.stream_mut(),
            call,
            plugin_id,
            callbacks,
        )
        .await
    }

    pub(super) async fn call_process_stream(
        &mut self,
        call: super::abi::PluginCall,
        plugin_id: &str,
        callbacks: &super::host_callbacks::HostCallbackRouter,
        chunks: &tokio::sync::mpsc::Sender<
            Result<Box<serde_json::value::RawValue>, super::abi::PluginClientError>,
        >,
    ) -> Result<Option<crate::sdk::pluginabi::Error>, super::abi::PluginClientError> {
        let active = self
            .active
            .as_mut()
            .ok_or(super::abi::PluginClientError::Closed)?;
        super::process_client_stream::exchange_stream(
            active.connection.stream_mut(),
            call,
            plugin_id,
            callbacks,
            chunks,
        )
        .await
    }

    pub(super) async fn abort_process(&mut self) {
        if let Some(mut active) = self.active.take() {
            active.kill_and_wait().await;
        }
    }

    pub async fn shutdown(&mut self) -> Result<ExitReport, PluginSupervisorError> {
        let mut active = self
            .active
            .take()
            .ok_or(PluginSupervisorError::NotRunning)?;
        let result = active.graceful_shutdown().await;
        match result {
            Ok(status) => Ok(ExitReport {
                success: status.success(),
                code: status.code(),
                aborted_request_ids: active.inflight.abort_all(),
            }),
            Err(error) => {
                active.kill_and_wait().await;
                Err(error)
            }
        }
    }
}

impl Drop for PluginSupervisor {
    fn drop(&mut self) {
        if let Some(active) = self.active.as_mut() {
            let _ = active.child.start_kill();
        }
    }
}

struct ActivePluginProcess {
    child: Child,
    connection: PlatformPluginConnection,
    _endpoint: PlatformPluginEndpoint,
    inflight: InflightRequests,
    registration: Option<RpcRegistration>,
}

impl ActivePluginProcess {
    async fn launch(config: &PluginProcessConfig) -> Result<Self, PluginSupervisorError> {
        #[cfg(unix)]
        let endpoint = PlatformPluginEndpoint::bind(&config.runtime_root, &config.instance_id)
            .map_err(|_| PluginSupervisorError::Endpoint)?;
        #[cfg(windows)]
        let mut endpoint = PlatformPluginEndpoint::bind(&config.runtime_root, &config.instance_id)
            .map_err(|_| PluginSupervisorError::Endpoint)?;
        let mut token_bytes = [0_u8; 32];
        getrandom::fill(&mut token_bytes).map_err(|_| PluginSupervisorError::Randomness)?;
        let token_bytes = Zeroizing::new(token_bytes);
        let token = Zeroizing::new(URL_SAFE_NO_PAD.encode(token_bytes.as_ref()));

        let mut command = Command::new(&config.executable);
        command
            .arg("--ctox-plugin-child")
            .arg("--socket")
            .arg(endpoint.endpoint_argument())
            .arg("--plugin-id")
            .arg(&config.plugin_id)
            .current_dir(&config.runtime_root)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|_| PluginSupervisorError::Spawn)?;
        let mut stdin = child.stdin.take().ok_or(PluginSupervisorError::Spawn)?;
        let write_token = async {
            stdin.write_all(token.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.shutdown().await
        };
        let token_written = matches!(timeout(CONTROL_TIMEOUT, write_token).await, Ok(Ok(())));
        drop(stdin);
        let connection = if token_written {
            tokio::select! {
                connection = endpoint.accept_verified(&config.plugin_id, token.as_bytes()) => {
                    connection.ok()
                }
                status = child.wait() => {
                    status.map_err(|_| PluginSupervisorError::Wait)?;
                    return Err(PluginSupervisorError::ExitedBeforeReady);
                }
            }
        } else {
            None
        };
        let Some(connection) = connection else {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err(PluginSupervisorError::Handshake);
        };
        if child
            .try_wait()
            .map_err(|_| PluginSupervisorError::Wait)?
            .is_some()
        {
            return Err(PluginSupervisorError::ExitedBeforeReady);
        }
        Ok(Self {
            child,
            connection,
            _endpoint: endpoint,
            inflight: InflightRequests::new(),
            registration: None,
        })
    }

    async fn call_unary(
        &mut self,
        method: &str,
        payload: Box<serde_json::value::RawValue>,
        deadline_unix_ms: Option<u64>,
    ) -> Result<Envelope, PluginSupervisorError> {
        let request_id = format!("call-{}", uuid::Uuid::new_v4().simple());
        let now_unix_ms = unix_time_ms();
        self.inflight
            .begin(
                request_id.clone(),
                RequestMode::Unary,
                deadline_unix_ms,
                now_unix_ms,
            )
            .map_err(|_| PluginSupervisorError::RequestState)?;
        let request = ProcessMessage::Request {
            protocol_version: PROCESS_PROTOCOL_VERSION,
            request_id: request_id.clone(),
            method: method.to_owned(),
            deadline_unix_ms,
            payload,
        };
        let exchange = async {
            write_process_message(self.connection.stream_mut(), &request).await?;
            read_process_message(self.connection.stream_mut())
                .await?
                .ok_or(super::process_transport::ProcessTransportError::TruncatedFrame)
        };
        let message = match timeout(CONTROL_TIMEOUT, exchange).await {
            Ok(Ok(message)) => message,
            _ => {
                self.cancel_inflight(&request_id);
                return Err(PluginSupervisorError::Protocol);
            }
        };
        match self.inflight.observe(message, unix_time_ms()) {
            Ok(ProcessEvent::UnaryResponse {
                request_id: response_id,
                envelope,
            }) if response_id == request_id => Ok(envelope),
            _ => {
                self.cancel_inflight(&request_id);
                Err(PluginSupervisorError::Protocol)
            }
        }
    }

    fn cancel_inflight(&mut self, request_id: &str) {
        let _ = self.inflight.observe(
            ProcessMessage::Cancel {
                protocol_version: PROCESS_PROTOCOL_VERSION,
                request_id: request_id.to_owned(),
            },
            unix_time_ms(),
        );
    }

    async fn graceful_shutdown(
        &mut self,
    ) -> Result<std::process::ExitStatus, PluginSupervisorError> {
        let request_id = format!("shutdown-{}", uuid::Uuid::new_v4().simple());
        let request = ProcessMessage::Request {
            protocol_version: PROCESS_PROTOCOL_VERSION,
            request_id: request_id.clone(),
            method: METHOD_PLUGIN_SHUTDOWN.into(),
            deadline_unix_ms: None,
            payload: serde_json::value::to_raw_value(&serde_json::json!({}))
                .map_err(|_| PluginSupervisorError::Shutdown)?,
        };
        timeout(
            CONTROL_TIMEOUT,
            write_process_message(self.connection.stream_mut(), &request),
        )
        .await
        .map_err(|_| PluginSupervisorError::Shutdown)?
        .map_err(|_| PluginSupervisorError::Shutdown)?;
        let response = timeout(
            CONTROL_TIMEOUT,
            read_process_message(self.connection.stream_mut()),
        )
        .await
        .map_err(|_| PluginSupervisorError::Shutdown)?
        .map_err(|_| PluginSupervisorError::Shutdown)?
        .ok_or(PluginSupervisorError::Shutdown)?;
        let ProcessMessage::Response {
            request_id: response_id,
            envelope: Envelope { ok: true, .. },
            ..
        } = response
        else {
            return Err(PluginSupervisorError::Shutdown);
        };
        if response_id != request_id {
            return Err(PluginSupervisorError::Shutdown);
        }
        let status = timeout(CONTROL_TIMEOUT, self.child.wait())
            .await
            .map_err(|_| PluginSupervisorError::Shutdown)?
            .map_err(|_| PluginSupervisorError::Shutdown)?;
        if !status.success() {
            return Err(PluginSupervisorError::Shutdown);
        }
        Ok(status)
    }

    async fn kill_and_wait(&mut self) {
        let _ = self.child.start_kill();
        let _ = timeout(CONTROL_TIMEOUT, self.child.wait()).await;
    }
}

pub struct ExitReport {
    pub success: bool,
    pub code: Option<i32>,
    pub aborted_request_ids: Vec<String>,
}

impl fmt::Debug for ExitReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExitReport")
            .field("success", &self.success)
            .field("code", &self.code)
            .field("aborted_request_count", &self.aborted_request_ids.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginSupervisorError {
    InvalidConfig,
    AlreadyRunning,
    NotRunning,
    Endpoint,
    Randomness,
    Spawn,
    Handshake,
    ExitedBeforeReady,
    Wait,
    RequestState,
    NotRegistered,
    AlreadyRegistered,
    UnsupportedCapability,
    InvalidRegistration,
    PluginRejected,
    Protocol,
    RestartWithoutFailure,
    RestartExhausted,
    Shutdown,
}

impl fmt::Display for PluginSupervisorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidConfig => "plugin process configuration is invalid",
            Self::AlreadyRunning => "plugin process is already running",
            Self::NotRunning => "plugin process is not running",
            Self::Endpoint => "plugin process endpoint could not be created",
            Self::Randomness => "plugin process randomness is unavailable",
            Self::Spawn => "plugin process could not be started",
            Self::Handshake => "plugin process handshake failed",
            Self::ExitedBeforeReady => "plugin process exited before readiness",
            Self::Wait => "plugin process exit could not be observed",
            Self::RequestState => "plugin process request state rejected the operation",
            Self::NotRegistered => "plugin process is not registered",
            Self::AlreadyRegistered => "plugin process is already registered",
            Self::UnsupportedCapability => "plugin process did not register this capability",
            Self::InvalidRegistration => "plugin process registration is invalid",
            Self::PluginRejected => "plugin process rejected the request",
            Self::Protocol => "plugin process returned an invalid protocol response",
            Self::RestartWithoutFailure => "plugin process restart has no preceding failure",
            Self::RestartExhausted => "plugin process restart budget is exhausted",
            Self::Shutdown => "plugin process did not shut down cleanly",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for PluginSupervisorError {}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
