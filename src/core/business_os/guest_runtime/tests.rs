// Origin: CTOX
// License: AGPL-3.0-only

use super::*;
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
fn caller() -> GuestCaller {
    GuestCaller::Worker {
        execution_id: "execution".into(),
        provider_session_id: "provider-session".into(),
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
fn fixture(revoke_during_capture: bool) -> (Arc<State>, Driver, Authority) {
    let state = Arc::new(State::default());
    (
        state.clone(),
        Driver {
            state: state.clone(),
            revoke_during_capture,
        },
        Authority { state },
    )
}
fn request(action: GuestAction) -> GuestRequest {
    GuestRequest {
        guest_id: "guest-a".into(),
        action,
    }
}
fn click() -> GuestAction {
    GuestAction::Input {
        frame_id: "frame-current".into(),
        input: GuestInput::Click {
            x: 2,
            y: 3,
            button: MouseButton::Left,
        },
    }
}

#[tokio::test]
async fn capture_is_denied_before_driver_or_revoked_before_publication() -> Result<()> {
    let (state, driver, auth) = fixture(false);
    state.observe.store(false, Ordering::SeqCst);
    assert!(dispatch_guest(
        &driver,
        &auth,
        &scope(),
        &caller(),
        request(GuestAction::Observe)
    )
    .await
    .is_err());
    assert_eq!(state.captures.load(Ordering::SeqCst), 0);

    let (state, driver, auth) = fixture(true);
    assert!(dispatch_guest(
        &driver,
        &auth,
        &scope(),
        &caller(),
        request(GuestAction::Observe)
    )
    .await
    .is_err());
    assert_eq!(state.captures.load(Ordering::SeqCst), 1);
    assert_eq!(state.published.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn observe_and_input_rights_are_separate_and_input_is_rechecked() -> Result<()> {
    let (state, driver, auth) = fixture(false);
    state.input.store(false, Ordering::SeqCst);
    dispatch_guest(
        &driver,
        &auth,
        &scope(),
        &caller(),
        request(GuestAction::Observe),
    )
    .await?;
    assert_eq!(state.published.load(Ordering::SeqCst), 1);
    assert!(
        dispatch_guest(&driver, &auth, &scope(), &caller(), request(click()))
            .await
            .is_err()
    );
    assert_eq!(state.inputs.load(Ordering::SeqCst), 0);
    state.input.store(true, Ordering::SeqCst);
    dispatch_guest(&driver, &auth, &scope(), &caller(), request(click())).await?;
    state.input.store(false, Ordering::SeqCst);
    assert!(
        dispatch_guest(&driver, &auth, &scope(), &caller(), request(click()))
            .await
            .is_err()
    );
    assert_eq!(state.inputs.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn target_scope_and_frame_mismatches_never_reach_the_driver() -> Result<()> {
    let (state, driver, auth) = fixture(false);
    let mut other_guest = request(click());
    other_guest.guest_id = "guest-b".into();
    assert!(
        dispatch_guest(&driver, &auth, &scope(), &caller(), other_guest)
            .await
            .is_err()
    );
    let mut other_project = scope();
    other_project.project_id = "other-project".into();
    assert!(
        dispatch_guest(&driver, &auth, &other_project, &caller(), request(click()))
            .await
            .is_err()
    );
    let old_frame = GuestAction::Input {
        frame_id: "stale-frame".into(),
        input: GuestInput::Key {
            key: GuestKey::Enter,
        },
    };
    assert!(
        dispatch_guest(&driver, &auth, &scope(), &caller(), request(old_frame))
            .await
            .is_err()
    );
    assert_eq!(state.inputs.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn payloads_cannot_supply_authority_commands_or_unbounded_actions() {
    for raw in [
        r#"{"guest_id":"guest-a","actor":"admin","action":{"kind":"observe"}}"#,
        r#"{"guest_id":"guest-a","action":{"kind":"shell","command":"id"}}"#,
        r#"{"guest_id":"guest-a","action":{"kind":"input","frame_id":"frame-current","input":{"kind":"key","key":"arbitrary-command"}}}"#,
    ] {
        assert!(serde_json::from_str::<GuestRequest>(raw).is_err());
    }
    assert!(GuestInput::Type {
        text: "x".repeat(16_385)
    }
    .validate()
    .is_err());
    assert!(GuestInput::Click {
        x: 4096,
        y: 0,
        button: MouseButton::Left
    }
    .validate()
    .is_err());
    assert!(GuestInput::Scroll {
        x: 0,
        y: 0,
        direction: ScrollDirection::Down,
        steps: 0
    }
    .validate()
    .is_err());
}
