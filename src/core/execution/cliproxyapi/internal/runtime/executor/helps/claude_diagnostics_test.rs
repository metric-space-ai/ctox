// ref: internal/runtime/executor/helps/claude_diagnostics_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// Candidate delta evidence: internal/runtime/executor/helps/claude_diagnostics_test.go
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Mutex, MutexGuard, OnceLock};

use super::claude_diagnostics::{
    begin_claude_diagnostics, claude_diagnostics_cache_state_for_test, commit_claude_diagnostics,
    expire_claude_diagnostics_for_test, pin_claude_session_date, reset_claude_diagnostics_for_test,
    CLAUDE_DIAGNOSTICS_MAX_ENTRIES,
};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct ResetDiagnostics {
    _guard: MutexGuard<'static, ()>,
}

impl ResetDiagnostics {
    fn new() -> Self {
        let guard = TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        reset_claude_diagnostics_for_test();
        Self { _guard: guard }
    }
}

impl Drop for ResetDiagnostics {
    fn drop(&mut self) {
        reset_claude_diagnostics_for_test();
    }
}

#[test]
fn pins_date_per_credential_session_and_preserves_continuity() {
    let _reset = ResetDiagnostics::new();
    let (key, sequence, _) = begin_claude_diagnostics("date-credential", "date-session");
    assert_eq!(pin_claude_session_date(&key, " 2026-10-02 "), "2026-10-02");
    commit_claude_diagnostics(&key, sequence, "msg_before_midnight");
    let (same_key, _, previous) = begin_claude_diagnostics("date-credential", "date-session");
    assert_eq!(
        pin_claude_session_date(&same_key, "2026-10-03"),
        "2026-10-02"
    );
    assert_eq!(previous, "msg_before_midnight");
    for (credential, session) in [
        ("date-other", "date-session"),
        ("date-credential", "date-other"),
    ] {
        let (other_key, _, _) = begin_claude_diagnostics(credential, session);
        assert_eq!(
            pin_claude_session_date(&other_key, "2026-10-03"),
            "2026-10-03"
        );
    }
}

#[test]
fn pin_date_falls_back_for_missing_empty_and_expired_sessions() {
    let _reset = ResetDiagnostics::new();
    assert_eq!(pin_claude_session_date("", "2026-10-03"), "2026-10-03");
    assert_eq!(
        pin_claude_session_date("missing", "2026-10-03"),
        "2026-10-03"
    );
    let (key, _, _) = begin_claude_diagnostics("date-expired", "session");
    assert_eq!(pin_claude_session_date(&key, " "), "");
    assert_eq!(pin_claude_session_date(&key, "2026-10-02"), "2026-10-02");
    expire_claude_diagnostics_for_test(&key);
    let (same_key, _, _) = begin_claude_diagnostics("date-expired", "session");
    assert_eq!(
        pin_claude_session_date(&same_key, "2026-10-03"),
        "2026-10-03"
    );
}

#[test]
fn tracks_completed_message_per_credential_session() {
    let _reset = ResetDiagnostics::new();
    let (key, sequence, previous) = begin_claude_diagnostics("credential-a", "session-a");
    assert!(!key.is_empty());
    assert_eq!(sequence, 1);
    assert!(previous.is_empty());
    commit_claude_diagnostics(&key, sequence, "msg_first");

    let (_, second_sequence, previous) = begin_claude_diagnostics("credential-a", "session-a");
    assert_eq!(second_sequence, 2);
    assert_eq!(previous, "msg_first");

    let (_, _, other_session) = begin_claude_diagnostics("credential-a", "session-b");
    let (_, _, other_credential) = begin_claude_diagnostics("credential-b", "session-a");
    assert!(other_session.is_empty());
    assert!(other_credential.is_empty());
}

#[test]
fn rejects_expired_generation_commit() {
    let _reset = ResetDiagnostics::new();
    let (key, expired_sequence, _) = begin_claude_diagnostics("credential", "session");
    expire_claude_diagnostics_for_test(&key);

    let (new_key, current_sequence, previous) = begin_claude_diagnostics("credential", "session");
    assert_eq!(new_key, key);
    assert!(current_sequence > expired_sequence);
    assert!(previous.is_empty());
    commit_claude_diagnostics(&new_key, current_sequence, "msg_current");
    commit_claude_diagnostics(&key, expired_sequence, "msg_expired");
    let (_, _, previous) = begin_claude_diagnostics("credential", "session");
    assert_eq!(previous, "msg_current");
}

#[test]
fn cache_evicts_oldest_entries_within_capacity() {
    let _reset = ResetDiagnostics::new();
    let (first_key, first_sequence, _) = begin_claude_diagnostics("credential", "session-0");
    let mut newest_key = String::new();
    for index in 1..=CLAUDE_DIAGNOSTICS_MAX_ENTRIES {
        (newest_key, _, _) = begin_claude_diagnostics("credential", &format!("session-{index}"));
    }

    let (entry_count, first_found, newest_found) =
        claude_diagnostics_cache_state_for_test(&first_key, &newest_key);
    assert!(entry_count <= CLAUDE_DIAGNOSTICS_MAX_ENTRIES);
    assert!(!first_found);
    assert!(newest_found);

    let (new_key, new_sequence, _) = begin_claude_diagnostics("credential", "session-0");
    assert_eq!(new_key, first_key);
    assert!(new_sequence > first_sequence);
    commit_claude_diagnostics(&new_key, new_sequence, "msg_recreated");
    commit_claude_diagnostics(&first_key, first_sequence, "msg_evicted");
    let (_, _, previous) = begin_claude_diagnostics("credential", "session-0");
    assert_eq!(previous, "msg_recreated");
}

#[test]
fn rejects_late_older_commit() {
    let _reset = ResetDiagnostics::new();
    let (key, first, _) = begin_claude_diagnostics("credential", "session");
    let (_, second, _) = begin_claude_diagnostics("credential", "session");
    commit_claude_diagnostics(&key, second, "msg_newer");
    commit_claude_diagnostics(&key, first, "msg_older");
    let (_, _, previous) = begin_claude_diagnostics("credential", "session");
    assert_eq!(previous, "msg_newer");
}

#[test]
fn empty_identity_inputs_and_commits_are_noops() {
    let _reset = ResetDiagnostics::new();
    assert_eq!(
        begin_claude_diagnostics(" ", "session"),
        (String::new(), 0, String::new())
    );
    assert_eq!(
        begin_claude_diagnostics("credential", " "),
        (String::new(), 0, String::new())
    );
    commit_claude_diagnostics("", 1, "message");
    commit_claude_diagnostics("missing", 0, "message");
    commit_claude_diagnostics("missing", 1, " ");
}
