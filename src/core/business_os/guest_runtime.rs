// Origin: CTOX
// License: AGPL-3.0-only

//! Guest-side desktop effects. This adapter owns no identity, VM provisioner,
//! lease, transport, scheduler or persistent state. A native authority must
//! execute input at its effect boundary and publish observations through its
//! revocation-aware delivery path. There is intentionally no permissive default
//! implementation and no registered browser/VM operation until that connector exists.

use anyhow::{ensure, Result};
use serde::Deserialize;
use std::future::Future;

mod qmp;
mod x11;
pub(super) use x11::{X11GuestConfig, X11GuestDriver};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct GuestScope {
    pub instance_id: String,
    pub user_id: String,
    pub project_id: String,
    pub thread_id: String,
    pub worker_profile_id: String,
    pub guest_id: String,
}

/// Supplied by the authenticated native caller, never deserialized from a tool payload.
pub(super) enum GuestCaller {
    Human {
        session_id: String,
    },
    Worker {
        execution_id: String,
        provider_session_id: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GuestRequest {
    pub guest_id: String,
    pub action: GuestAction,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum GuestAction {
    Observe,
    Input { frame_id: String, input: GuestInput },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum GuestInput {
    Click {
        x: u32,
        y: u32,
        button: MouseButton,
    },
    Type {
        text: String,
    },
    Scroll {
        x: u32,
        y: u32,
        direction: ScrollDirection,
        steps: u8,
    },
    Key {
        key: GuestKey,
    },
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum GuestKey {
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    SelectAll,
    Copy,
    Paste,
}

pub(super) struct GuestFrame {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Opaque receipt from the existing authority/stream publisher, not raw frame
/// bytes that a caller could deliver later after takeover without revalidation.
pub(super) enum GuestOutcome {
    ObservationPublished { frame_id: String },
    InputApplied,
}

pub(super) trait GuestDriver {
    fn guest_id(&self) -> &str;
    fn capture(&self) -> impl Future<Output = Result<GuestFrame>> + Send;
    fn input(&self, input: &GuestInput) -> impl Future<Output = Result<()>> + Send;
}

pub(super) trait GuestAuthorization {
    type Observation: Send;

    fn begin_observation(
        &self,
        scope: &GuestScope,
        caller: &GuestCaller,
    ) -> impl Future<Output = Result<Self::Observation>> + Send;

    /// Must revalidate the same binding/epoch after capture and atomically
    /// enqueue via existing delivery ownership. Pending frames are invalidated
    /// on takeover/revocation by that delivery path, not a second queue here.
    fn publish_observation(
        &self,
        scope: &GuestScope,
        caller: &GuestCaller,
        observation: Self::Observation,
        frame: GuestFrame,
    ) -> impl Future<Output = Result<String>> + Send;

    /// The callback may be polled only within the current input authority.
    /// This must serialize or cancel against takeover, expiry and revocation at
    /// the actual effect; a successful earlier check is not a permit.
    /// frame_id must identify a current observation of this exact guest.
    fn apply_input<F, Fut>(
        &self,
        scope: &GuestScope,
        caller: &GuestCaller,
        frame_id: &str,
        effect: F,
    ) -> impl Future<Output = Result<()>> + Send
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<()>> + Send;
}

fn identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

impl GuestInput {
    fn validate(&self) -> Result<()> {
        match self {
            Self::Click { x, y, .. } | Self::Scroll { x, y, .. } => {
                ensure!(
                    *x < 4096 && *y < 4096,
                    "guest coordinates exceed the bounded display"
                );
            }
            Self::Type { text } => {
                ensure!(
                    !text.is_empty() && text.len() <= 16_384,
                    "guest text length is invalid"
                );
                ensure!(!text.contains('\0'), "guest text contains a null byte");
            }
            Self::Key { .. } => {}
        }
        if let Self::Scroll { steps, .. } = self {
            ensure!(
                (1..=20).contains(steps),
                "guest scroll steps are outside the limit"
            );
        }
        Ok(())
    }
}

pub(super) async fn dispatch_guest<D: GuestDriver + Sync, A: GuestAuthorization + Sync>(
    driver: &D,
    authorization: &A,
    scope: &GuestScope,
    caller: &GuestCaller,
    request: GuestRequest,
) -> Result<GuestOutcome> {
    for id in [
        &scope.instance_id,
        &scope.user_id,
        &scope.project_id,
        &scope.thread_id,
        &scope.worker_profile_id,
        &scope.guest_id,
        &request.guest_id,
    ] {
        ensure!(identifier(id), "guest scope contains an invalid identifier");
    }
    match caller {
        GuestCaller::Human { session_id } => {
            ensure!(identifier(session_id), "guest caller session is invalid")
        }
        GuestCaller::Worker {
            execution_id,
            provider_session_id,
        } => ensure!(
            identifier(execution_id) && identifier(provider_session_id),
            "guest execution identity is invalid"
        ),
    }
    ensure!(
        request.guest_id == scope.guest_id && driver.guest_id() == scope.guest_id,
        "guest target does not match the bound driver"
    );
    match request.action {
        GuestAction::Observe => {
            let observation = authorization.begin_observation(scope, caller).await?;
            let frame = driver.capture().await?;
            let frame_id = authorization
                .publish_observation(scope, caller, observation, frame)
                .await?;
            ensure!(
                identifier(&frame_id),
                "guest publisher returned an invalid frame identity"
            );
            Ok(GuestOutcome::ObservationPublished { frame_id })
        }
        GuestAction::Input { frame_id, input } => {
            ensure!(
                identifier(&frame_id),
                "guest input requires a frame identity"
            );
            input.validate()?;
            authorization
                .apply_input(scope, caller, &frame_id, || driver.input(&input))
                .await?;
            Ok(GuestOutcome::InputApplied)
        }
    }
}

#[cfg(test)]
mod tests;
