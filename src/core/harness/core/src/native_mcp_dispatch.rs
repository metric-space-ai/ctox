//! In-process native dispatch at the real MCP emission boundary.
//! Registrations are private live objects, never wire/configuration authority.
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use ctox_protocol::mcp::CallToolResult;
use serde_json::Value;

use crate::codex::{Session, TurnContext};

/// Constructed only by this module from the actual harness Session/TurnContext.
/// Neither JSON nor a legacy tool-begin event can construct this invocation.
pub struct NativeMcpInvocation<'a> {
    thread_id: &'a str,
    turn_id: &'a str,
    call_id: &'a str,
    server: &'a str,
    tool: &'a str,
    arguments: Option<&'a Value>,
}
impl NativeMcpInvocation<'_> {
    pub fn thread_id(&self) -> &str {
        self.thread_id
    }
    pub fn turn_id(&self) -> &str {
        self.turn_id
    }
    pub fn call_id(&self) -> &str {
        self.call_id
    }
    pub fn server(&self) -> &str {
        self.server
    }
    pub fn tool(&self) -> &str {
        self.tool
    }
    pub fn arguments(&self) -> Option<&Value> {
        self.arguments
    }
}

/// Installed by the native turn owner. None preserves ordinary MCP transport;
/// Some is a native result after the existing MCP approval/safety boundary.
/// The callback must be bounded and enforce its own live worker, actual turn,
/// policy and controller guards. It cannot treat these identifiers as permits.
pub trait NativeMcpDispatch: Send + Sync {
    fn dispatch(
        &self,
        invocation: NativeMcpInvocation<'_>,
    ) -> Option<Result<CallToolResult, String>>;
}

struct RegistryEntry {
    // Keeping a weak allocation alive prevents pointer reuse while registered.
    session: Weak<Session>,
    dispatcher: Weak<dyn NativeMcpDispatch>,
}
type Registry = HashMap<usize, RegistryEntry>;
fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Native Rust capability bound to one actual Core Session allocation.
/// Thread/turn strings, JSON and retained dispatcher Arcs cannot register it.
pub struct NativeMcpRegistration {
    session: Weak<Session>,
    dispatcher: Arc<dyn NativeMcpDispatch>,
}

pub(crate) fn register_native_mcp_dispatch(
    session: &Arc<Session>,
    dispatcher: Arc<dyn NativeMcpDispatch>,
) -> Result<NativeMcpRegistration, String> {
    let key = Arc::as_ptr(session) as usize;
    let mut entries = registry()
        .lock()
        .map_err(|_| "native MCP registry poisoned")?;
    if entries
        .get(&key)
        .and_then(|entry| entry.dispatcher.upgrade())
        .is_some()
    {
        return Err("native MCP dispatcher already registered for session".into());
    }
    let session = Arc::downgrade(session);
    entries.insert(
        key,
        RegistryEntry {
            session: session.clone(),
            dispatcher: Arc::downgrade(&dispatcher),
        },
    );
    Ok(NativeMcpRegistration {
        session,
        dispatcher,
    })
}
impl Drop for NativeMcpRegistration {
    fn drop(&mut self) {
        if let Ok(mut entries) = registry().lock() {
            let key = self.session.as_ptr() as usize;
            if entries.get(&key).is_some_and(|entry| {
                entry.session.ptr_eq(&self.session)
                    && entry.dispatcher.ptr_eq(&Arc::downgrade(&self.dispatcher))
            }) {
                entries.remove(&key);
            }
        }
    }
}

pub(crate) async fn dispatch_native_mcp(
    session: &Session,
    turn: &TurnContext,
    call_id: &str,
    server: &str,
    tool: &str,
    arguments: Option<&Value>,
) -> Option<Result<CallToolResult, String>> {
    let key = session as *const Session as usize;
    let dispatcher = match registry().lock() {
        Ok(entries) => entries
            .get(&key)
            .and_then(|entry| entry.dispatcher.upgrade()),
        Err(_) => return Some(Err("native MCP registry poisoned".into())),
    }?;
    // Release the registry lock before taking Core's lifecycle fence. Ordinary
    // unregistered MCP transport remains independent of this native check.
    let active = session.active_turn.lock().await;
    let task = active
        .as_ref()
        .and_then(|active| active.tasks.get(&turn.sub_id));
    if !task.is_some_and(|task| {
        std::ptr::eq(task.turn_context.as_ref(), turn) && !task.cancellation_token.is_cancelled()
    }) {
        return Some(Err(
            "native MCP emission requires the actual live Core turn".into(),
        ));
    }
    let thread_id = session.conversation_id.to_string();
    // Keep the lifecycle fence across the bounded synchronous effect callback:
    // finish, replacement and abort all remove tasks under this same mutex.
    let result = dispatcher.dispatch(NativeMcpInvocation {
        thread_id: &thread_id,
        turn_id: &turn.sub_id,
        call_id,
        server,
        tool,
        arguments,
    });
    drop(active);
    result
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::state::TaskKind;
    use crate::tasks::{SessionTask, SessionTaskContext};
    use ctox_protocol::user_input::UserInput;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio_util::sync::CancellationToken;

    struct LiveTask {
        finish: CancellationToken,
    }
    #[async_trait::async_trait]
    impl SessionTask for LiveTask {
        fn kind(&self) -> TaskKind {
            TaskKind::Regular
        }
        fn span_name(&self) -> &'static str {
            "native_mcp_test"
        }
        async fn run(
            self: Arc<Self>,
            _session: Arc<SessionTaskContext>,
            _ctx: Arc<TurnContext>,
            _input: Vec<UserInput>,
            cancellation: CancellationToken,
        ) -> Option<String> {
            tokio::select! {
                _ = cancellation.cancelled() => {},
                _ = self.finish.cancelled() => {},
                _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {},
            }
            None
        }
    }
    pub(crate) async fn start_live_turn(
        session: &Arc<Session>,
        turn: &Arc<TurnContext>,
    ) -> CancellationToken {
        let finish = CancellationToken::new();
        session
            .spawn_task(
                turn.clone(),
                vec![],
                LiveTask {
                    finish: finish.clone(),
                },
            )
            .await;
        finish
    }
    struct Dispatch {
        calls: Arc<AtomicUsize>,
        session: Weak<Session>,
    }
    impl NativeMcpDispatch for Dispatch {
        fn dispatch(
            &self,
            invocation: NativeMcpInvocation<'_>,
        ) -> Option<Result<CallToolResult, String>> {
            // Regression: the callback itself must run inside the lifecycle fence.
            assert!(
                self.session
                    .upgrade()
                    .unwrap()
                    .active_turn
                    .try_lock()
                    .is_err()
            );
            assert_eq!(invocation.call_id(), "actual-call");
            assert_eq!(invocation.server(), "actual-server");
            assert_eq!(invocation.tool(), "actual-tool");
            assert_eq!(
                invocation.arguments(),
                Some(&serde_json::json!({"payload": 17}))
            );
            self.calls.fetch_add(1, Ordering::SeqCst);
            Some(Ok(CallToolResult {
                content: vec![],
                structured_content: None,
                is_error: None,
                meta: None,
            }))
        }
    }
    async fn invoke(
        session: &Session,
        turn: &TurnContext,
    ) -> Option<Result<CallToolResult, String>> {
        dispatch_native_mcp(
            session,
            turn,
            "actual-call",
            "actual-server",
            "actual-tool",
            Some(&serde_json::json!({"payload": 17})),
        )
        .await
    }

    #[tokio::test]
    async fn native_mcp_registration_scopes_dispatch_and_teardown() {
        let (session, turn, _events) = crate::codex::make_session_and_context_with_rx().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let dispatcher = Arc::new(Dispatch {
            calls: calls.clone(),
            session: Arc::downgrade(&session),
        });
        let registration = register_native_mcp_dispatch(&session, dispatcher.clone()).unwrap();
        assert!(register_native_mcp_dispatch(&session, dispatcher.clone()).is_err());
        assert!(invoke(&session, &turn).await.unwrap().is_err());
        start_live_turn(&session, &turn).await;
        assert!(invoke(&session, &turn).await.unwrap().is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(session.abort_turn(&turn.sub_id).await);
        assert!(invoke(&session, &turn).await.unwrap().is_err());
        drop(registration);
        assert!(invoke(&session, &turn).await.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn native_mcp_registration_does_not_cross_sessions_with_identical_labels() {
        let (session, turn, _events) = crate::codex::make_session_and_context_with_rx().await;
        let (mut other, mut other_turn, _other_events) =
            crate::codex::make_session_and_context_with_rx().await;
        Arc::get_mut(&mut other).unwrap().conversation_id = session.conversation_id;
        Arc::get_mut(&mut other_turn).unwrap().sub_id = turn.sub_id.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let dispatcher = Arc::new(Dispatch {
            calls: calls.clone(),
            session: Arc::downgrade(&session),
        });
        let _registration = register_native_mcp_dispatch(&session, dispatcher).unwrap();
        start_live_turn(&session, &turn).await;
        start_live_turn(&other, &other_turn).await;
        assert!(invoke(&other, &other_turn).await.is_none());
        assert!(invoke(&session, &turn).await.unwrap().is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(session.abort_turn(&turn.sub_id).await);
        assert!(other.abort_turn(&other_turn.sub_id).await);
    }

    #[tokio::test]
    async fn native_mcp_rejects_finished_replaced_and_cancelled_turns() {
        let (session, turn, _events) = crate::codex::make_session_and_context_with_rx().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let dispatcher = Arc::new(Dispatch {
            calls: calls.clone(),
            session: Arc::downgrade(&session),
        });
        let _registration = register_native_mcp_dispatch(&session, dispatcher).unwrap();
        let finish = start_live_turn(&session, &turn).await;
        finish.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while session
                .turn_context_for_sub_id(&turn.sub_id)
                .await
                .is_some()
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(invoke(&session, &turn).await.unwrap().is_err());
        // Reuse the label with a distinct real context; the old context is stale.
        let (_, mut replacement, _replacement_events) =
            crate::codex::make_session_and_context_with_rx().await;
        Arc::get_mut(&mut replacement).unwrap().sub_id = turn.sub_id.clone();
        start_live_turn(&session, &replacement).await;
        assert!(invoke(&session, &turn).await.unwrap().is_err());
        assert!(invoke(&session, &replacement).await.unwrap().is_ok());
        {
            let active = session.active_turn.lock().await;
            active.as_ref().unwrap().tasks[&replacement.sub_id]
                .cancellation_token
                .cancel();
        }
        assert!(invoke(&session, &replacement).await.unwrap().is_err());
        session
            .abort_all_tasks(crate::protocol::TurnAbortReason::Interrupted)
            .await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
