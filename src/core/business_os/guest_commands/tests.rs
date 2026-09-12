// Origin: CTOX
// License: AGPL-3.0-only

use super::super::guest_runtime::{GuestFrame, GuestInput};
use super::super::session::BusinessOsSessionUser;
use super::super::store::CommandOrigin;
use super::*;
use anyhow::ensure;
use serde_json::json;
use std::future::Future;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Arc,
};

struct State {
    observe: AtomicBool,
    input: AtomicBool,
    epoch: AtomicU64,
    captures: AtomicUsize,
    inputs: AtomicUsize,
    published: AtomicUsize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            observe: AtomicBool::new(true),
            input: AtomicBool::new(true),
            epoch: AtomicU64::new(1),
            captures: AtomicUsize::new(0),
            inputs: AtomicUsize::new(0),
            published: AtomicUsize::new(0),
        }
    }
}

fn scope() -> GuestScope {
    GuestScope {
        instance_id: "instance".into(),
        user_id: "owner".into(),
        project_id: "project".into(),
        thread_id: "chat".into(),
        worker_profile_id: "profile".into(),
        guest_id: "guest-a".into(),
    }
}

fn session_for(user_id: &str) -> BusinessOsSession {
    BusinessOsSession {
        ok: true,
        authenticated: true,
        auth_required: false,
        user: Some(BusinessOsSessionUser {
            id: user_id.to_string(),
            display_name: user_id.to_string(),
            role: "admin".into(),
            is_admin: true,
        }),
        login_url: None,
        reason: None,
    }
}

struct Driver {
    state: Arc<State>,
    revoke_during_capture: bool,
}

impl GuestDriver for Driver {
    fn guest_id(&self) -> &str {
        "guest-a"
    }

    async fn capture(&self) -> Result<GuestFrame> {
        self.state.captures.fetch_add(1, Ordering::SeqCst);
        if self.revoke_during_capture {
            self.state.epoch.fetch_add(1, Ordering::SeqCst);
        }
        Ok(GuestFrame {
            png: vec![1, 2, 3],
            width: 1,
            height: 1,
        })
    }

    async fn input(&self, _: &GuestInput) -> Result<()> {
        self.state.inputs.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct Authority {
    state: Arc<State>,
}

impl GuestAuthorization for Authority {
    type Observation = u64;

    async fn begin_observation(&self, requested: &GuestScope, _: &GuestCaller) -> Result<u64> {
        ensure!(
            requested == &scope() && self.state.observe.load(Ordering::SeqCst),
            "observation denied"
        );
        Ok(self.state.epoch.load(Ordering::SeqCst))
    }

    async fn publish_observation(
        &self,
        requested: &GuestScope,
        _: &GuestCaller,
        epoch: u64,
        frame: GuestFrame,
    ) -> Result<String> {
        ensure!(
            requested == &scope()
                && self.state.observe.load(Ordering::SeqCst)
                && self.state.epoch.load(Ordering::SeqCst) == epoch,
            "observation revoked"
        );
        ensure!(
            !frame.png.is_empty() && frame.width > 0 && frame.height > 0,
            "empty frame"
        );
        self.state.published.fetch_add(1, Ordering::SeqCst);
        Ok("frame-current".into())
    }

    async fn apply_input<F, Fut>(
        &self,
        requested: &GuestScope,
        _: &GuestCaller,
        frame_id: &str,
        effect: F,
    ) -> Result<()>
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<()>> + Send,
    {
        ensure!(
            requested == &scope()
                && frame_id == "frame-current"
                && self.state.input.load(Ordering::SeqCst),
            "input denied"
        );
        effect().await
    }
}

pub(super) struct Owner {
    state: Arc<State>,
    driver: Driver,
    authority: Authority,
    bound_user_id: String,
}

pub(super) fn test_owner(revoke_during_capture: bool) -> Owner {
    Owner::new(revoke_during_capture)
}

impl Owner {
    fn new(revoke_during_capture: bool) -> Self {
        let state = Arc::new(State::default());
        Self {
            driver: Driver {
                state: state.clone(),
                revoke_during_capture,
            },
            authority: Authority {
                state: state.clone(),
            },
            state,
            bound_user_id: "owner".into(),
        }
    }
}

impl GuestCommandOwner for Owner {
    type Driver = Driver;
    type Authorization = Authority;

    fn driver(&self, guest_id: &str) -> Result<&Self::Driver> {
        ensure!(guest_id == "guest-a", "unknown guest");
        Ok(&self.driver)
    }

    fn authorization(&self) -> &Self::Authorization {
        &self.authority
    }

    fn scope(&self, guest_id: &str) -> Result<GuestScope> {
        ensure!(guest_id == "guest-a", "unknown guest");
        Ok(scope())
    }

    fn caller(&self, session: &BusinessOsSession, bound: &GuestScope) -> Result<GuestCaller> {
        let user_id = session_user_id(session).context("missing session user")?;
        ensure!(
            user_id == self.bound_user_id && user_id == bound.user_id,
            "guest actor does not match the bound guest"
        );
        Ok(GuestCaller::Human {
            session_id: user_id.to_string(),
        })
    }
}

fn observe_command(command_id: &str, payload: Value) -> BusinessCommand {
    BusinessCommand {
        origin: CommandOrigin::TrustedLocal,
        id: Some(command_id.to_string()),
        module: "ctox".into(),
        command_type: GUEST_OBSERVE_COMMAND_TYPE.into(),
        record_id: Some("guest-a".into()),
        payload,
        client_context: json!({"actor": {"id": "owner", "role": "admin"}}),
    }
}

fn input_command(command_id: &str, payload: Value) -> BusinessCommand {
    BusinessCommand {
        origin: CommandOrigin::TrustedLocal,
        id: Some(command_id.to_string()),
        module: "ctox".into(),
        command_type: GUEST_INPUT_COMMAND_TYPE.into(),
        record_id: Some("guest-a".into()),
        payload,
        client_context: json!({"actor": {"id": "owner", "role": "admin"}}),
    }
}

fn click_payload() -> Value {
    json!({
        "guest_id": "guest-a",
        "frame_id": "frame-current",
        "input": {"kind": "click", "x": 2, "y": 3, "button": "left"}
    })
}

fn assert_metadata_only(result: &Value) {
    let encoded = result.to_string();
    assert!(!encoded.contains("png"));
    assert!(!encoded.contains("xdotool"));
    assert!(!encoded.contains("maim"));
    assert!(result.get("png").is_none());
}

#[test]
fn unregistered_injection_parses_then_fails_closed() {
    let command = observe_command("cmd-observe", json!({"guest_id": "guest-a"}));
    let error = execute_injected(
        &GuestRuntimeInjection::Unregistered,
        &session_for("owner"),
        &command,
    )
    .expect_err("missing owner must fail closed");
    assert!(
        error
            .to_string()
            .contains("guest command owner is not registered for this runtime"),
        "{error}"
    );
}

#[test]
fn malformed_and_unknown_fields_are_rejected_before_dispatch() {
    let owner = Owner::new(false);
    let session = session_for("owner");
    for command in [
        observe_command(
            "cmd-actor",
            json!({"guest_id": "guest-a", "actor": "admin"}),
        ),
        observe_command(
            "cmd-shell",
            json!({"guest_id": "guest-a", "action": {"kind": "shell", "command": "id"}}),
        ),
        input_command(
            "cmd-key",
            json!({
                "guest_id": "guest-a",
                "frame_id": "frame-current",
                "input": {"kind": "key", "key": "arbitrary-command"}
            }),
        ),
        input_command(
            "cmd-helper",
            json!({
                "guest_id": "guest-a",
                "frame_id": "frame-current",
                "input": {"kind": "click", "x": 1, "y": 1, "button": "left", "argv": ["id"]}
            }),
        ),
    ] {
        assert!(
            execute(&owner, &session, &command).is_err(),
            "{}",
            command.command_type
        );
    }
    assert_eq!(owner.state.captures.load(Ordering::SeqCst), 0);
    assert_eq!(owner.state.inputs.load(Ordering::SeqCst), 0);
}

#[test]
fn oversized_and_unbounded_payloads_are_rejected() {
    let owner = Owner::new(false);
    let session = session_for("owner");
    let oversized = input_command(
        "cmd-text",
        json!({
            "guest_id": "guest-a",
            "frame_id": "frame-current",
            "input": {"kind": "type", "text": "x".repeat(16_385)}
        }),
    );
    assert!(execute(&owner, &session, &oversized).is_err());
    let huge = observe_command(
        "cmd-huge",
        json!({"guest_id": "guest-a", "project_id": "p".repeat(21_000)}),
    );
    assert!(execute(&owner, &session, &huge).is_err());
    assert_eq!(owner.state.inputs.load(Ordering::SeqCst), 0);
}

#[test]
fn observe_publishes_canonical_frame_id_without_raw_bytes() -> Result<()> {
    let owner = Owner::new(false);
    let result = execute(
        &owner,
        &session_for("owner"),
        &observe_command("cmd-observe-ok", json!({"guest_id": "guest-a"})),
    )?;
    assert_eq!(result["ok"], true);
    assert_eq!(result["command_id"], "cmd-observe-ok");
    assert_eq!(result["guest_id"], "guest-a");
    assert_eq!(result["outcome"], "observation_published");
    assert_eq!(result["frame_id"], "frame-current");
    assert_metadata_only(&result);
    assert_eq!(owner.state.published.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn input_applies_under_current_ownership_and_correlates_command_id() -> Result<()> {
    let owner = Owner::new(false);
    execute(
        &owner,
        &session_for("owner"),
        &observe_command("cmd-observe-first", json!({"guest_id": "guest-a"})),
    )?;
    let result = execute(
        &owner,
        &session_for("owner"),
        &input_command("cmd-input-ok", click_payload()),
    )?;
    assert_eq!(result["ok"], true);
    assert_eq!(result["command_id"], "cmd-input-ok");
    assert_eq!(result["outcome"], "input_applied");
    assert!(result.get("frame_id").is_none());
    assert_metadata_only(&result);
    assert_eq!(owner.state.inputs.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn wrong_guest_project_thread_worker_actor_or_frame_never_reach_the_driver() {
    let owner = Owner::new(false);
    let session = session_for("owner");
    let cases = [
        observe_command("cmd-guest", json!({"guest_id": "guest-b"})),
        observe_command(
            "cmd-project",
            json!({"guest_id": "guest-a", "project_id": "other-project"}),
        ),
        observe_command(
            "cmd-thread",
            json!({"guest_id": "guest-a", "thread_id": "other-thread"}),
        ),
        observe_command(
            "cmd-worker",
            json!({"guest_id": "guest-a", "worker_profile_id": "other-worker"}),
        ),
        input_command(
            "cmd-frame",
            json!({
                "guest_id": "guest-a",
                "frame_id": "stale-frame",
                "input": {"kind": "key", "key": "enter"}
            }),
        ),
    ];
    for command in cases {
        assert!(
            execute(&owner, &session, &command).is_err(),
            "{}",
            command.id.as_deref().unwrap_or("")
        );
    }
    assert!(execute(
        &owner,
        &session_for("intruder"),
        &observe_command("cmd-actor", json!({"guest_id": "guest-a"}))
    )
    .is_err());
    assert_eq!(owner.state.captures.load(Ordering::SeqCst), 0);
    assert_eq!(owner.state.inputs.load(Ordering::SeqCst), 0);
}

#[test]
fn observe_and_input_rights_are_rechecked_and_revocation_blocks_publication() -> Result<()> {
    let owner = Owner::new(false);
    owner.state.input.store(false, Ordering::SeqCst);
    execute(
        &owner,
        &session_for("owner"),
        &observe_command("cmd-observe-rights", json!({"guest_id": "guest-a"})),
    )?;
    assert!(execute(
        &owner,
        &session_for("owner"),
        &input_command("cmd-input-denied", click_payload())
    )
    .is_err());
    assert_eq!(owner.state.inputs.load(Ordering::SeqCst), 0);

    let revoked = Owner::new(true);
    assert!(execute(
        &revoked,
        &session_for("owner"),
        &observe_command("cmd-observe-revoked", json!({"guest_id": "guest-a"}))
    )
    .is_err());
    assert_eq!(revoked.state.captures.load(Ordering::SeqCst), 1);
    assert_eq!(revoked.state.published.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn injection_from_runtime_is_unregistered() {
    assert!(matches!(
        injection_from_runtime(),
        GuestRuntimeInjection::Unregistered
    ));
    assert!(is_guest_command(GUEST_OBSERVE_COMMAND_TYPE));
    assert!(is_guest_command(GUEST_INPUT_COMMAND_TYPE));
    assert!(!is_guest_command("ctox.workjet.session.create"));
}
