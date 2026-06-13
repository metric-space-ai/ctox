use ctox_protocol::protocol::AgentStatus;
use ctox_protocol::protocol::EventMsg;

/// Derive the next agent status from a single emitted event.
/// Returns `None` when the event does not affect status tracking.
pub(crate) fn agent_status_from_event(msg: &EventMsg) -> Option<AgentStatus> {
    match msg {
        EventMsg::TurnStarted(_) => Some(AgentStatus::Running),
        EventMsg::TurnComplete(ev) => Some(AgentStatus::Completed(ev.last_agent_message.clone())),
        EventMsg::TurnAborted(ev) => match ev.reason {
            ctox_protocol::protocol::TurnAbortReason::Interrupted => Some(AgentStatus::Interrupted),
            _ => Some(AgentStatus::Errored(format!("{:?}", ev.reason))),
        },
        EventMsg::Error(ev) => Some(AgentStatus::Errored(ev.message.clone())),
        EventMsg::ShutdownComplete => Some(AgentStatus::Shutdown),
        _ => None,
    }
}

pub(crate) fn is_final(status: &AgentStatus) -> bool {
    !matches!(
        status,
        AgentStatus::PendingInit | AgentStatus::Running | AgentStatus::Interrupted
    )
}

/// Like [`is_final`] but also treats `Interrupted` as terminal.
///
/// Use ONLY at agent-job collection/finalize sites that immediately
/// `shutdown_agent` the thread (so the interrupt is actually made terminal and
/// the worker cannot be resumed afterward). Do NOT use this at the completion
/// watcher or `wait_for_final_status`: those do not shut the child down, so an
/// `Interrupted` child there is a resumable transient state and treating it as
/// terminal would emit a premature/duplicate parent edge.
pub(crate) fn is_subagent_terminal(status: &AgentStatus) -> bool {
    is_final(status) || matches!(status, AgentStatus::Interrupted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_subagent_terminal_treats_interrupted_as_terminal() {
        // The divergence the predicate introduces: an Interrupted child is
        // terminal for collection sites that shut it down, but is NOT final.
        assert!(is_subagent_terminal(&AgentStatus::Interrupted));
        assert!(!is_final(&AgentStatus::Interrupted));
        // Genuinely-final states stay terminal; a running child stays non-terminal.
        assert!(is_subagent_terminal(&AgentStatus::Shutdown));
        assert!(!is_subagent_terminal(&AgentStatus::Running));
    }
}
