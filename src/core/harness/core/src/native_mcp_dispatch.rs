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

type Registry = HashMap<String, Weak<dyn NativeMcpDispatch>>;
fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Only this live registration retains the dispatcher. In-flight calls must
/// additionally obey the native owner's lifetime; cloning its Arc is no permit.
pub struct NativeMcpRegistration {
    thread_id: String,
    dispatcher: Arc<dyn NativeMcpDispatch>,
}

pub fn register_native_mcp_dispatch(
    actual_thread_id: String,
    dispatcher: Arc<dyn NativeMcpDispatch>,
) -> Result<NativeMcpRegistration, String> {
    if actual_thread_id.trim().is_empty()
        || actual_thread_id.len() > 256
        || actual_thread_id.chars().any(char::is_control)
    {
        return Err("native MCP registration requires an actual thread".into());
    }
    let mut entries = registry()
        .lock()
        .map_err(|_| "native MCP registry poisoned")?;
    if entries
        .get(&actual_thread_id)
        .and_then(Weak::upgrade)
        .is_some()
    {
        return Err("native MCP dispatcher already registered for thread".into());
    }
    entries.insert(actual_thread_id.clone(), Arc::downgrade(&dispatcher));
    Ok(NativeMcpRegistration {
        thread_id: actual_thread_id,
        dispatcher,
    })
}
impl Drop for NativeMcpRegistration {
    fn drop(&mut self) {
        if let Ok(mut entries) = registry().lock() {
            if entries
                .get(&self.thread_id)
                .is_some_and(|entry| entry.ptr_eq(&Arc::downgrade(&self.dispatcher)))
            {
                entries.remove(&self.thread_id);
            }
        }
    }
}

pub(crate) fn dispatch_native_mcp(
    session: &Session,
    turn: &TurnContext,
    call_id: &str,
    server: &str,
    tool: &str,
    arguments: Option<&Value>,
) -> Option<Result<CallToolResult, String>> {
    let thread_id = session.conversation_id.to_string();
    dispatch_registered(&thread_id, &turn.sub_id, call_id, server, tool, arguments)
}

fn dispatch_registered(
    thread_id: &str,
    turn_id: &str,
    call_id: &str,
    server: &str,
    tool: &str,
    arguments: Option<&Value>,
) -> Option<Result<CallToolResult, String>> {
    let dispatcher = match registry().lock() {
        Ok(entries) => entries.get(thread_id).and_then(Weak::upgrade),
        Err(_) => return Some(Err("native MCP registry poisoned".into())),
    }?;
    // No registry lock crosses the effect callback or its native guards.
    dispatcher.dispatch(NativeMcpInvocation {
        thread_id,
        turn_id,
        call_id,
        server,
        tool,
        arguments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Dispatch {
        calls: Arc<AtomicUsize>,
        turn: String,
    }
    impl NativeMcpDispatch for Dispatch {
        fn dispatch(
            &self,
            invocation: NativeMcpInvocation<'_>,
        ) -> Option<Result<CallToolResult, String>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if invocation.turn_id() != self.turn {
                return Some(Err("foreign actual turn".into()));
            }
            assert_eq!(invocation.call_id(), "actual-call");
            assert_eq!(invocation.server(), "actual-server");
            assert_eq!(invocation.tool(), "actual-tool");
            assert_eq!(
                invocation.arguments(),
                Some(&serde_json::json!({"payload": 17}))
            );
            Some(Ok(CallToolResult {
                content: vec![],
                structured_content: None,
                is_error: None,
                meta: None,
            }))
        }
    }

    #[test]
    fn native_mcp_registration_scopes_dispatch_and_teardown() {
        let thread = uuid::Uuid::new_v4().to_string();
        let calls = Arc::new(AtomicUsize::new(0));
        let dispatcher = Arc::new(Dispatch {
            calls: calls.clone(),
            turn: "actual-turn".into(),
        });
        let registration =
            register_native_mcp_dispatch(thread.clone(), dispatcher.clone()).unwrap();
        assert!(register_native_mcp_dispatch(thread.clone(), dispatcher.clone()).is_err());
        assert!(
            dispatch_registered(
                "foreign-thread",
                "actual-turn",
                "actual-call",
                "actual-server",
                "actual-tool",
                Some(&serde_json::json!({"payload":17}))
            )
            .is_none()
        );
        assert!(
            dispatch_registered(
                &thread,
                "foreign-turn",
                "actual-call",
                "actual-server",
                "actual-tool",
                Some(&serde_json::json!({"payload":17}))
            )
            .unwrap()
            .is_err()
        );
        assert!(
            dispatch_registered(
                &thread,
                "actual-turn",
                "actual-call",
                "actual-server",
                "actual-tool",
                Some(&serde_json::json!({"payload":17}))
            )
            .unwrap()
            .is_ok()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        drop(registration);
        // Retaining a dispatcher Arc cannot keep a removed registration live.
        assert!(
            dispatch_registered(
                &thread,
                "actual-turn",
                "actual-call",
                "actual-server",
                "actual-tool",
                None
            )
            .is_none()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
