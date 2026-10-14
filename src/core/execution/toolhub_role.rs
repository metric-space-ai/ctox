// Origin: CTOX
// License: AGPL-3.0-only
//! Who may raise user-facing cards (credential / connect / confirm).
//!
//! DE: Nur der CTOX-Supervisor darf Karten im Chat anlegen. Persistente und
//! One-Shot-Worker geben stattdessen ein strukturiertes `needs_user` an ihren
//! Supervisor zurück. EN: only the supervisor raises cards; workers return a
//! structured `needs_user` result instead.
//!
//! Role resolution (fail towards "worker", never towards "supervisor"):
//! 1. The session (`--session` / `CTOX_SESSION_ID`, a CodeThread id) is looked
//!    up in the instance bindings: `workjet_supervisor_bindings` → supervisor,
//!    `workjet_persistent_worker_bindings` → persistent worker, any other known
//!    session → one-shot worker.
//! 2. `CTOX_TEAM_ROLE` (set by the spawner: `supervisor`, `persistent_worker`,
//!    `one_shot_worker`) can only restrict, never elevate: a session bound as a
//!    worker stays a worker even if the env claims `supervisor`, and an env
//!    worker role wins over a supervisor binding.
//! 3. No session and no role: a human/operator shell → allowed.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamRole {
    Supervisor,
    PersistentWorker,
    OneShotWorker,
    /// Interactive shell without a team session (admin / developer).
    Operator,
}

impl TeamRole {
    pub fn may_raise_cards(self) -> bool {
        matches!(self, TeamRole::Supervisor | TeamRole::Operator)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            TeamRole::Supervisor => "supervisor",
            TeamRole::PersistentWorker => "persistent_worker",
            TeamRole::OneShotWorker => "one_shot_worker",
            TeamRole::Operator => "operator",
        }
    }
}

fn parse_env_role(v: &str) -> Option<TeamRole> {
    match v.trim() {
        "supervisor" => Some(TeamRole::Supervisor),
        "persistent_worker" => Some(TeamRole::PersistentWorker),
        "one_shot_worker" | "worker" => Some(TeamRole::OneShotWorker),
        "" => None,
        // Unknown values are treated as the most restricted role.
        _ => Some(TeamRole::OneShotWorker),
    }
}

fn table_has(conn: &Connection, table: &str, thread: &str) -> bool {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            params![table],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if !exists {
        return false;
    }
    conn.query_row(
        &format!("SELECT 1 FROM {table} WHERE thread_id=?1 LIMIT 1"),
        params![thread],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

/// Pure resolution (testable with an in-memory connection).
pub fn resolve_with(conn: Option<&Connection>, session: Option<&str>, env_role: Option<&str>) -> TeamRole {
    let env = env_role.and_then(parse_env_role);
    let bound = match (conn, session) {
        (Some(c), Some(s)) if !s.trim().is_empty() => Some(if table_has(c, "workjet_supervisor_bindings", s) {
            TeamRole::Supervisor
        } else if table_has(c, "workjet_persistent_worker_bindings", s) {
            TeamRole::PersistentWorker
        } else {
            TeamRole::OneShotWorker
        }),
        (None, Some(s)) if !s.trim().is_empty() => Some(TeamRole::OneShotWorker),
        _ => None,
    };
    match (bound, env) {
        (None, None) => TeamRole::Operator,
        (Some(b), None) => b,
        (None, Some(TeamRole::Supervisor)) => TeamRole::OneShotWorker, // claim without binding
        (None, Some(e)) => e,
        (Some(TeamRole::Supervisor), Some(e)) => e, // env may restrict
        (Some(b), Some(_)) => b,                    // a worker binding cannot be elevated
    }
}

pub fn resolve(root: &Path, session: Option<&str>) -> TeamRole {
    let env = std::env::var("CTOX_TEAM_ROLE").ok();
    let conn = if session.is_some() {
        crate::business_os::store::open_store(root).ok()
    } else {
        None
    };
    resolve_with(conn.as_ref(), session, env.as_deref())
}

/// Settings keys the native harness uses to carry the team identity from the
/// session start into the composed system prompt and the shell environment.
pub const SETTINGS_SESSION_KEY: &str = "ctox.team.session_id";
pub const SETTINGS_ROLE_KEY: &str = "ctox.team.role";

/// Single source of truth for the harness prompt section (see the skill).
pub const TOOLHUB_SKILL: &str =
    include_str!("../../skills/system/integrations/ctox-toolhub/SKILL.md");

fn prompt_block(marker: &str) -> Option<&'static str> {
    let open = format!("<!-- toolhub-prompt:{marker} -->");
    let start = TOOLHUB_SKILL.find(&open)? + open.len();
    let rest = &TOOLHUB_SKILL[start..];
    let end = rest.find("<!-- toolhub-prompt:")?;
    Some(rest[..end].trim())
}

/// The Toolhub/UI-card system-prompt section for one role: common block plus
/// exactly that role's rule. Panics only if the embedded skill lost a marker,
/// which the tests catch at build time.
pub fn prompt_section(role: TeamRole) -> String {
    let common = prompt_block("common").expect("ctox-toolhub skill: common block");
    let rule = prompt_block(&format!("role:{}", role.as_str())).expect("ctox-toolhub skill: role block");
    format!("{common}\n{rule}")
}

/// Role carried in session settings (set by the session start); absent means
/// an operator session without a team binding.
pub fn role_from_settings(settings: &std::collections::BTreeMap<String, String>) -> TeamRole {
    settings
        .get(SETTINGS_ROLE_KEY)
        .and_then(|v| parse_env_role(v))
        .unwrap_or(TeamRole::Operator)
}

/// Environment every shell command of that session gets, so `ctox ui request`
/// and `ctox toolhub exec` resolve the same role (closes the operator-shell gap).
pub fn team_env(settings: &std::collections::BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut env = vec![(
        "CTOX_TEAM_ROLE".to_string(),
        role_from_settings(settings).as_str().to_string(),
    )];
    if let Some(s) = settings.get(SETTINGS_SESSION_KEY).filter(|s| !s.trim().is_empty()) {
        env.push(("CTOX_SESSION_ID".to_string(), s.clone()));
    }
    env
}

/// Settings for a session bound to Workjet thread `thread` (None: unbound).
pub fn with_team(
    root: &Path,
    settings: &std::collections::BTreeMap<String, String>,
    thread: Option<&str>,
    explicit_role: Option<TeamRole>,
) -> std::collections::BTreeMap<String, String> {
    let mut out = settings.clone();
    out.remove(SETTINGS_SESSION_KEY);
    out.remove(SETTINGS_ROLE_KEY);
    let role = match (thread, explicit_role) {
        (_, Some(r)) => Some(r),
        (Some(t), None) => Some(resolve(root, Some(t))),
        (None, None) => None,
    };
    if let Some(t) = thread {
        out.insert(SETTINGS_SESSION_KEY.into(), t.to_string());
    }
    if let Some(r) = role {
        out.insert(SETTINGS_ROLE_KEY.into(), r.as_str().into());
    }
    out
}

/// Structured hand-off from a worker to its supervisor. One-shot workers
/// terminate with it; persistent workers may park until the supervisor resolves.
pub fn needs_user(role: TeamRole, kind: &str, toolkit: &str, reason: &str, fields: Value) -> Value {
    let next = match role {
        TeamRole::PersistentWorker => {
            "Return this needs_user to your supervisor and park; resume when it reports the request resolved."
        }
        _ => "Stop here and return this needs_user as your result; the supervisor asks the user and re-dispatches you.",
    };
    json!({
        "needs_user": {
            "kind": kind,
            "toolkit": toolkit,
            "reason": reason,
            "fields": fields,
            "role": role.as_str(),
        },
        "next": next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE workjet_supervisor_bindings (project_id TEXT, thread_id TEXT, owner_user_id TEXT, record_json TEXT);
             CREATE TABLE workjet_persistent_worker_bindings (thread_id TEXT PRIMARY KEY, owner_user_id TEXT, project_id TEXT, record_json TEXT);
             INSERT INTO workjet_supervisor_bindings VALUES ('p','sup','u','{}');
             INSERT INTO workjet_persistent_worker_bindings VALUES ('pw','u','p','{}');",
        )
        .unwrap();
        c
    }

    #[test]
    fn bindings_decide_the_role() {
        let c = db();
        assert_eq!(resolve_with(Some(&c), Some("sup"), None), TeamRole::Supervisor);
        assert_eq!(resolve_with(Some(&c), Some("pw"), None), TeamRole::PersistentWorker);
        assert_eq!(resolve_with(Some(&c), Some("other"), None), TeamRole::OneShotWorker);
        assert_eq!(resolve_with(None, None, None), TeamRole::Operator);
    }

    #[test]
    fn env_can_restrict_but_never_elevate() {
        let c = db();
        assert_eq!(resolve_with(Some(&c), Some("sup"), Some("one_shot_worker")), TeamRole::OneShotWorker);
        assert_eq!(resolve_with(Some(&c), Some("pw"), Some("supervisor")), TeamRole::PersistentWorker);
        assert_eq!(resolve_with(Some(&c), Some("other"), Some("supervisor")), TeamRole::OneShotWorker);
        assert_eq!(resolve_with(None, None, Some("supervisor")), TeamRole::OneShotWorker);
        assert_eq!(resolve_with(None, None, Some("persistent_worker")), TeamRole::PersistentWorker);
        assert_eq!(resolve_with(None, None, Some("bogus")), TeamRole::OneShotWorker);
    }

    #[test]
    fn only_supervisor_and_operator_raise_cards() {
        assert!(TeamRole::Supervisor.may_raise_cards());
        assert!(TeamRole::Operator.may_raise_cards());
        assert!(!TeamRole::PersistentWorker.may_raise_cards());
        assert!(!TeamRole::OneShotWorker.may_raise_cards());
    }

    #[test]
    fn prompt_section_has_common_block_and_only_its_own_role_rule() {
        let roles = [
            (TeamRole::Supervisor, "Role rule (Supervisor)"),
            (TeamRole::PersistentWorker, "Role rule (Persistent worker)"),
            (TeamRole::OneShotWorker, "Role rule (One-shot worker)"),
            (TeamRole::Operator, "Role rule (Operator shell)"),
        ];
        for (role, own) in roles {
            let s = prompt_section(role);
            for needle in ["## CTOX Toolhub", "ctox toolhub list", "ctox toolhub catalog", "ctox toolhub exec", "3 credential_required", "--confirm", own] {
                assert!(s.contains(needle), "{role:?} misses {needle}");
            }
            for (other, rule) in roles {
                if other != role {
                    assert!(!s.contains(rule), "{role:?} leaks {rule}");
                }
            }
        }
        assert!(prompt_section(TeamRole::Supervisor).contains("ctox ui request"));
        assert!(prompt_section(TeamRole::OneShotWorker).contains("never wait"));
        assert!(prompt_section(TeamRole::PersistentWorker).contains("park"));
    }

    #[test]
    fn team_env_and_settings_roundtrip() {
        let mut s = std::collections::BTreeMap::new();
        assert_eq!(role_from_settings(&s), TeamRole::Operator);
        s.insert(SETTINGS_ROLE_KEY.into(), "persistent_worker".into());
        s.insert(SETTINGS_SESSION_KEY.into(), "t1".into());
        let env = team_env(&s);
        assert!(env.contains(&("CTOX_TEAM_ROLE".into(), "persistent_worker".into())));
        assert!(env.contains(&("CTOX_SESSION_ID".into(), "t1".into())));
        let cleared = with_team(Path::new("/nonexistent"), &s, None, None);
        assert!(!cleared.contains_key(SETTINGS_ROLE_KEY));
        let explicit = with_team(Path::new("/nonexistent"), &s, Some("t2"), Some(TeamRole::OneShotWorker));
        assert_eq!(role_from_settings(&explicit), TeamRole::OneShotWorker);
    }

    #[test]
    fn needs_user_tells_one_shot_to_stop_and_persistent_to_park() {
        let o = needs_user(TeamRole::OneShotWorker, "credential", "jira", "no connection", json!([]));
        assert_eq!(o["needs_user"]["toolkit"], "jira");
        assert!(o["next"].as_str().unwrap().contains("re-dispatches"));
        let p = needs_user(TeamRole::PersistentWorker, "connect", "slack", "x", json!([]));
        assert!(p["next"].as_str().unwrap().contains("park"));
    }
}
