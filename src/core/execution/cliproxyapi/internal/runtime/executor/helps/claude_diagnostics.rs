// ref: internal/runtime/executor/helps/claude_diagnostics.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// Candidate delta evidence: internal/runtime/executor/helps/claude_diagnostics.go
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

const CLAUDE_DIAGNOSTICS_TTL: Duration = Duration::from_secs(60 * 60);
const CLAUDE_DIAGNOSTICS_CLEANUP_PERIOD: Duration = Duration::from_secs(15 * 60);
pub(super) const CLAUDE_DIAGNOSTICS_MAX_ENTRIES: usize = 4096;
const CLAUDE_DIAGNOSTICS_EVICT_BATCH_SIZE: usize = 256;

#[derive(Clone, Debug)]
struct ClaudeDiagnosticsEntry {
    previous_message_id: String,
    pinned_date: String,
    minimum_sequence: u64,
    committed_sequence: u64,
    last_access: u64,
    expires_at: Instant,
}

#[derive(Debug, Default)]
struct ClaudeDiagnosticsState {
    entries: HashMap<String, ClaudeDiagnosticsEntry>,
    last_cleanup: Option<Instant>,
    next_sequence: u64,
    next_access: u64,
}

static CLAUDE_DIAGNOSTICS_STATE: OnceLock<Mutex<ClaudeDiagnosticsState>> = OnceLock::new();

fn diagnostics_state() -> &'static Mutex<ClaudeDiagnosticsState> {
    CLAUDE_DIAGNOSTICS_STATE.get_or_init(|| Mutex::new(ClaudeDiagnosticsState::default()))
}

/// Starts one request generation for a stable credential identity and Claude
/// conversation. Only a SHA-256 digest of both inputs is retained as the key.
pub fn begin_claude_diagnostics(
    credential_identity: &str,
    session_id: &str,
) -> (String, u64, String) {
    let credential_identity = credential_identity.trim();
    let session_id = session_id.trim();
    if credential_identity.is_empty() || session_id.is_empty() {
        return (String::new(), 0, String::new());
    }

    let mut hasher = Sha256::new();
    hasher.update(credential_identity.as_bytes());
    hasher.update([0]);
    hasher.update(session_id.as_bytes());
    let key = format!("{:x}", hasher.finalize());
    let now = Instant::now();
    let mut state = diagnostics_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    cleanup_claude_diagnostics_locked(&mut state, now);

    let found = state.entries.contains_key(&key);
    let new_generation = state
        .entries
        .get(&key)
        .is_none_or(|entry| now > entry.expires_at);
    if !found {
        evict_claude_diagnostics_locked(&mut state);
    }
    state.next_sequence = state.next_sequence.wrapping_add(1);
    let sequence = state.next_sequence;
    state.next_access = state.next_access.wrapping_add(1);
    let last_access = state.next_access;
    if new_generation {
        state.entries.insert(
            key.clone(),
            ClaudeDiagnosticsEntry {
                previous_message_id: String::new(),
                pinned_date: String::new(),
                minimum_sequence: sequence,
                committed_sequence: 0,
                last_access,
                expires_at: now + CLAUDE_DIAGNOSTICS_TTL,
            },
        );
    }
    let entry = state
        .entries
        .get_mut(&key)
        .expect("new or existing diagnostics generation must be present");
    entry.last_access = last_access;
    entry.expires_at = now + CLAUDE_DIAGNOSTICS_TTL;
    let previous_message_id = entry.previous_message_id.clone();
    (key, sequence, previous_message_id)
}

// ref: internal/runtime/executor/helps/claude_diagnostics.go:271-298 @ 2044a01f
/// Pins the first resolved credential-local date to the existing session.
/// Missing continuity (including a process restart) preserves the supplied date.
pub fn pin_claude_session_date(key: &str, date: &str) -> String {
    let key = key.trim();
    let date = date.trim();
    if key.is_empty() || date.is_empty() {
        return date.to_owned();
    }
    let mut state = diagnostics_state().lock().unwrap_or_else(|error| error.into_inner());
    let Some(entry) = state.entries.get_mut(key) else {
        return date.to_owned();
    };
    if entry.pinned_date.is_empty() {
        entry.pinned_date = date.to_owned();
    }
    entry.pinned_date.clone()
}

/// Advances continuity only after a response completes. Late responses from
/// older concurrently-started generations cannot overwrite newer continuity.
pub fn commit_claude_diagnostics(key: &str, sequence: u64, message_id: &str) {
    let key = key.trim();
    let message_id = message_id.trim();
    if key.is_empty() || sequence == 0 || message_id.is_empty() {
        return;
    }

    let now = Instant::now();
    let mut state = diagnostics_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let Some(entry) = state.entries.get(key) else {
        return;
    };
    if sequence < entry.minimum_sequence || sequence < entry.committed_sequence {
        return;
    }
    state.next_access = state.next_access.wrapping_add(1);
    let last_access = state.next_access;
    let entry = state
        .entries
        .get_mut(key)
        .expect("entry existence was checked while holding the lock");
    entry.previous_message_id = message_id.to_owned();
    entry.committed_sequence = sequence;
    entry.last_access = last_access;
    entry.expires_at = now + CLAUDE_DIAGNOSTICS_TTL;
}

fn cleanup_claude_diagnostics_locked(state: &mut ClaudeDiagnosticsState, now: Instant) {
    if state.last_cleanup.is_some_and(|last_cleanup| {
        now.duration_since(last_cleanup) < CLAUDE_DIAGNOSTICS_CLEANUP_PERIOD
    }) {
        return;
    }
    state.entries.retain(|_, entry| now <= entry.expires_at);
    state.last_cleanup = Some(now);
}

fn evict_claude_diagnostics_locked(state: &mut ClaudeDiagnosticsState) {
    if state.entries.len() < CLAUDE_DIAGNOSTICS_MAX_ENTRIES {
        return;
    }
    let mut candidates = state
        .entries
        .iter()
        .map(|(key, entry)| (key.clone(), entry.last_access))
        .collect::<Vec<_>>();
    candidates.sort_unstable_by_key(|(_, last_access)| *last_access);
    for (key, _) in candidates
        .into_iter()
        .take(CLAUDE_DIAGNOSTICS_EVICT_BATCH_SIZE)
    {
        state.entries.remove(&key);
    }
}

#[cfg(test)]
pub(super) fn reset_claude_diagnostics_for_test() {
    *diagnostics_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = ClaudeDiagnosticsState::default();
}

#[cfg(test)]
pub(super) fn expire_claude_diagnostics_for_test(key: &str) {
    let mut state = diagnostics_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(entry) = state.entries.get_mut(key) {
        entry.expires_at = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
    }
}

#[cfg(test)]
pub(super) fn claude_diagnostics_cache_state_for_test(
    first_key: &str,
    newest_key: &str,
) -> (usize, bool, bool) {
    let state = diagnostics_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    (
        state.entries.len(),
        state.entries.contains_key(first_key),
        state.entries.contains_key(newest_key),
    )
}
