// Origin: CTOX
// License: AGPL-3.0-only

//! Business OS command connector for guest observe/input.
//!
//! The command plane injects a native owner/driver through
//! [`GuestRuntimeInjection`]. There is no permissive default and no process
//! global: a missing owner fails closed. Payload strings are claims that must
//! match the owner's canonical [`GuestScope`]; [`GuestCaller`] comes from the
//! authenticated session plus owner, never from the command body.

use super::guest_runtime::{
    dispatch_guest, GuestAction, GuestAuthorization, GuestCaller, GuestDriver, GuestInput,
    GuestOutcome, GuestRequest, GuestScope,
};
use super::session::{session_user_id, BusinessOsSession};
use super::store::BusinessCommand;
use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::future::Future;
use std::sync::Arc;

pub(super) const GUEST_OBSERVE_COMMAND_TYPE: &str = "ctox.guest.observe";
pub(super) const GUEST_INPUT_COMMAND_TYPE: &str = "ctox.guest.input";

const MAX_GUEST_COMMAND_PAYLOAD_BYTES: usize = 20_480;
const MAX_IDENTIFIER_BYTES: usize = 256;

/// Owner/driver slot carried by the command plane.
///
/// Public RxDB intake always passes [`Self::Unregistered`]. A registered owner
/// is supplied only through
/// `accept_rxdb_business_command_with_guest_runtime` via
/// [`Self::Registered`]. There is no process-global registry and no production
/// owner in this slice.
#[derive(Clone, Default)]
pub(super) enum GuestRuntimeInjection {
    #[default]
    Unregistered,
    Registered(GuestCommandExecutor),
}

/// Type-erased native owner/driver used by [`GuestRuntimeInjection::Registered`].
#[derive(Clone)]
pub(super) struct GuestCommandExecutor {
    dispatch: Arc<dyn Fn(&BusinessOsSession, &BusinessCommand) -> Result<Value> + Send + Sync>,
}

impl GuestCommandExecutor {
    #[allow(dead_code)]
    pub(super) fn from_owner<O>(owner: O) -> Self
    where
        O: GuestCommandOwner + Send + Sync + 'static,
    {
        Self {
            dispatch: Arc::new(move |session, command| execute(&owner, session, command)),
        }
    }

    fn run(&self, session: &BusinessOsSession, command: &BusinessCommand) -> Result<Value> {
        (self.dispatch)(session, command)
    }
}

pub(super) fn is_guest_command(command_type: &str) -> bool {
    matches!(
        command_type,
        GUEST_OBSERVE_COMMAND_TYPE | GUEST_INPUT_COMMAND_TYPE
    )
}

pub(super) fn injection_from_runtime() -> GuestRuntimeInjection {
    GuestRuntimeInjection::Unregistered
}

/// Native authority/driver owner. Tests supply a fake; production has none
/// until the unowned lifecycle connector registers a real owner.
#[allow(dead_code)]
pub(super) trait GuestCommandOwner: Send + Sync {
    type Driver: GuestDriver + Sync;
    type Authorization: GuestAuthorization + Sync;

    fn driver(&self, guest_id: &str) -> Result<&Self::Driver>;
    fn authorization(&self) -> &Self::Authorization;
    fn scope(&self, guest_id: &str) -> Result<GuestScope>;
    fn caller(&self, session: &BusinessOsSession, scope: &GuestScope) -> Result<GuestCaller>;
}

pub(super) fn execute_injected(
    injection: &GuestRuntimeInjection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> Result<Value> {
    match injection {
        GuestRuntimeInjection::Unregistered => {
            let _request = parse_guest_command(command)?;
            bail!("guest command owner is not registered for this runtime")
        }
        GuestRuntimeInjection::Registered(executor) => executor.run(session, command),
    }
}

#[allow(dead_code)]
pub(super) fn execute<O: GuestCommandOwner>(
    owner: &O,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> Result<Value> {
    let request = parse_guest_command(command)?;
    dispatch_authorized(owner, session, command, request)
}

#[allow(dead_code)]
fn dispatch_authorized<O: GuestCommandOwner>(
    owner: &O,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    request: GuestRequest,
) -> Result<Value> {
    let scope = owner.scope(&request.guest_id)?;
    apply_scope_claims(&scope, command)?;
    let caller = owner.caller(session, &scope)?;
    let driver = owner.driver(&request.guest_id)?;
    let outcome = block_on_guest(dispatch_guest(
        driver,
        owner.authorization(),
        &scope,
        &caller,
        request,
    ))?;
    Ok(receipt(command, &scope, outcome))
}

fn parse_guest_command(command: &BusinessCommand) -> Result<GuestRequest> {
    let payload_bytes =
        serde_json::to_vec(&command.payload).context("guest command payload is not JSON")?;
    ensure!(
        payload_bytes.len() <= MAX_GUEST_COMMAND_PAYLOAD_BYTES,
        "guest command payload is oversized"
    );
    match command.command_type.as_str() {
        GUEST_OBSERVE_COMMAND_TYPE => {
            let payload: GuestObserveCommandPayload =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.guest.observe payload")?;
            validate_identifier(&payload.guest_id, "guest_id")?;
            Ok(GuestRequest {
                guest_id: payload.guest_id,
                action: GuestAction::Observe,
            })
        }
        GUEST_INPUT_COMMAND_TYPE => {
            let payload: GuestInputCommandPayload = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.guest.input payload")?;
            validate_identifier(&payload.guest_id, "guest_id")?;
            validate_identifier(&payload.frame_id, "frame_id")?;
            Ok(GuestRequest {
                guest_id: payload.guest_id,
                action: GuestAction::Input {
                    frame_id: payload.frame_id,
                    input: payload.input,
                },
            })
        }
        other => bail!("unsupported guest command type: {other}"),
    }
}

fn apply_scope_claims(scope: &GuestScope, command: &BusinessCommand) -> Result<()> {
    let claims: GuestScopeClaims = match command.command_type.as_str() {
        GUEST_OBSERVE_COMMAND_TYPE => {
            serde_json::from_value::<GuestObserveCommandPayload>(command.payload.clone())
                .map(GuestScopeClaims::from)?
        }
        GUEST_INPUT_COMMAND_TYPE => {
            serde_json::from_value::<GuestInputCommandPayload>(command.payload.clone())
                .map(GuestScopeClaims::from)?
        }
        other => bail!("unsupported guest command type: {other}"),
    };
    claim_matches(
        claims.instance_id.as_deref(),
        &scope.instance_id,
        "instance",
    )?;
    claim_matches(claims.project_id.as_deref(), &scope.project_id, "project")?;
    claim_matches(claims.thread_id.as_deref(), &scope.thread_id, "thread")?;
    claim_matches(
        claims.worker_profile_id.as_deref(),
        &scope.worker_profile_id,
        "worker",
    )?;
    claim_matches(Some(claims.guest_id.as_str()), &scope.guest_id, "guest")?;
    Ok(())
}

fn claim_matches(claim: Option<&str>, canonical: &str, field: &str) -> Result<()> {
    if let Some(claim) = claim.filter(|value| !value.is_empty()) {
        ensure!(
            claim == canonical,
            "guest {field} does not match the bound guest"
        );
    }
    Ok(())
}

fn validate_identifier(value: &str, field: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= MAX_IDENTIFIER_BYTES
            && !value.chars().any(char::is_control),
        "guest {field} is invalid"
    );
    Ok(())
}

fn receipt(command: &BusinessCommand, scope: &GuestScope, outcome: GuestOutcome) -> Value {
    let command_id = command.id.clone().unwrap_or_default();
    match outcome {
        GuestOutcome::ObservationPublished { frame_id } => json!({
            "ok": true,
            "command_id": command_id,
            "guest_id": scope.guest_id,
            "outcome": "observation_published",
            "frame_id": frame_id,
        }),
        GuestOutcome::InputApplied => json!({
            "ok": true,
            "command_id": command_id,
            "guest_id": scope.guest_id,
            "outcome": "input_applied",
        }),
    }
}

#[allow(dead_code)]
fn block_on_guest<T>(future: impl Future<Output = Result<T>>) -> Result<T> {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => match handle.runtime_flavor() {
            tokio::runtime::RuntimeFlavor::CurrentThread => {
                bail!("guest command dispatch cannot nest on a current-thread runtime")
            }
            _ => tokio::task::block_in_place(|| handle.block_on(future)),
        },
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to start guest command runtime")?
            .block_on(future),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GuestObserveCommandPayload {
    guest_id: String,
    #[serde(default)]
    instance_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    thread_id: Option<String>,
    #[serde(default)]
    worker_profile_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GuestInputCommandPayload {
    guest_id: String,
    frame_id: String,
    input: GuestInput,
    #[serde(default)]
    instance_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    thread_id: Option<String>,
    #[serde(default)]
    worker_profile_id: Option<String>,
}

struct GuestScopeClaims {
    guest_id: String,
    instance_id: Option<String>,
    project_id: Option<String>,
    thread_id: Option<String>,
    worker_profile_id: Option<String>,
}

impl From<GuestObserveCommandPayload> for GuestScopeClaims {
    fn from(payload: GuestObserveCommandPayload) -> Self {
        Self {
            guest_id: payload.guest_id,
            instance_id: payload.instance_id,
            project_id: payload.project_id,
            thread_id: payload.thread_id,
            worker_profile_id: payload.worker_profile_id,
        }
    }
}

impl From<GuestInputCommandPayload> for GuestScopeClaims {
    fn from(payload: GuestInputCommandPayload) -> Self {
        Self {
            guest_id: payload.guest_id,
            instance_id: payload.instance_id,
            project_id: payload.project_id,
            thread_id: payload.thread_id,
            worker_profile_id: payload.worker_profile_id,
        }
    }
}

pub(super) fn guest_record_scope_id(command: &BusinessCommand) -> String {
    command
        .payload
        .get("guest_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|guest_id| format!("guest/{guest_id}"))
        .unwrap_or_else(|| "guest".to_string())
}

pub(super) fn require_authenticated_actor(session: &BusinessOsSession) -> Result<&str> {
    session_user_id(session).context("guest command is missing a user identity")
}

#[cfg(test)]
pub(super) fn test_guest_runtime(revoke_during_capture: bool) -> GuestRuntimeInjection {
    GuestRuntimeInjection::Registered(GuestCommandExecutor::from_owner(tests::test_owner(
        revoke_during_capture,
    )))
}

#[cfg(test)]
mod tests;
