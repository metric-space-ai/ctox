//! Logical host-consumer contract, not an authenticated data service.
//!
//! Decoding enforces request shape and budgets only. The native session adapter
//! must still resolve the saved target, bind current authorization, restrict the
//! query scope and enforce snapshot/event continuity before returning data.
use crate::business_data_contract::{
    NativeBusinessDataOperation as Operation, NativeBusinessDataRequest as Request,
    NativeBusinessDataScope as Scope, NativeBusinessDataSessionRef as SessionRef,
    CTOX_BUSINESS_DATA_MAX_PAGE_DOCUMENTS, CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
};
use std::io;

/// Reuse the existing local request budget. This does not open another listener
/// or change authority framing. Data response streaming is still to be bound.
pub fn decode_request(bytes: &[u8]) -> io::Result<Request> {
    if bytes.is_empty() || bytes.len() > crate::ipc::IPC_MAX_FRAME_BYTES {
        return Err(invalid("BusinessData request exceeds its frame budget"));
    }
    let request: Request = serde_json::from_slice(bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if request.version != CTOX_BUSINESS_DATA_PROTOCOL_VERSION || !id(&request.request_id) {
        return Err(invalid("invalid BusinessData protocol or request ID"));
    }
    match &request.operation {
        Operation::Open { target_id } => check(id(target_id), "invalid saved target ID")?,
        Operation::Status { session } | Operation::Close { session } => session_ref(session)?,
        Operation::Query {
            session,
            query,
            page_cursor: cursor,
        }
        | Operation::Watch {
            session,
            query,
            resume_cursor: cursor,
        } => {
            session_ref(session)?;
            check(id(&query.collection), "invalid collection")?;
            check(query.query.is_object(), "query must be a Mango object")?;
            check(
                query.page_size > 0 && query.page_size <= CTOX_BUSINESS_DATA_MAX_PAGE_DOCUMENTS,
                "query page exceeds its document budget",
            )?;
            if let Some(cursor) = cursor {
                check(
                    !cursor.is_empty() && cursor.len() <= 4096,
                    "invalid opaque cursor",
                )?;
            }
            match &query.scope {
                Scope::Instance {} => {}
                Scope::Project { project_id } => check(id(project_id), "invalid project scope")?,
                Scope::Thread {
                    project_id,
                    thread_id,
                } => {
                    check(id(project_id) && id(thread_id), "invalid thread scope")?;
                }
            }
        }
        Operation::Unwatch {
            session,
            subscription_id,
        } => {
            session_ref(session)?;
            check(id(subscription_id), "invalid subscription ID")?;
        }
        Operation::SubmitCommand { session, command } => {
            session_ref(session)?;
            check(
                id(&command.command_id) && id(&command.command_type),
                "invalid command identity",
            )?;
            check(
                command.payload.is_object(),
                "command payload must be an object",
            )?;
        }
        Operation::ObserveCommand {
            session,
            command_id,
        } => {
            session_ref(session)?;
            check(id(command_id), "invalid command ID")?;
        }
    }
    Ok(request)
}
fn session_ref(session: &SessionRef) -> io::Result<()> {
    check(
        id(&session.handle)
            && session.generation > 0
            && session.generation <= 9_007_199_254_740_991,
        "invalid native session reference",
    )
}
fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
fn check(valid: bool, reason: &str) -> io::Result<()> {
    if valid {
        Ok(())
    } else {
        Err(invalid(reason))
    }
}
fn invalid(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}
