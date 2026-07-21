use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HarnessSubagentSpawnContract {
    pub pattern: &'static str,
    pub parent_entity_type: &'static str,
    pub child_entity_type: &'static str,
    pub ranking_function: &'static str,
    pub finite_bound: &'static str,
    pub recursion_guard: &'static str,
    pub worker_tool_surface: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HarnessSubagentSpawnModelReport {
    pub ok: bool,
    pub contracts: Vec<HarnessSubagentSpawnContract>,
    pub violations: Vec<String>,
    pub proof: String,
    pub recommended_gate: String,
}

pub fn analyze_harness_subagent_spawn_model() -> HarnessSubagentSpawnModelReport {
    let contracts = harness_subagent_spawn_contracts().to_vec();
    let mut violations = Vec::new();

    let tools_spec = include_str!("tools/spec.rs");
    let tool_router = include_str!("tools/router.rs");
    let direct_session = include_str!("../../../execution/agent/direct_session.rs");
    let fork_record = include_str!("../../FORK.md");

    if !tools_spec.contains("let include_collab_tools = false;")
        || !tools_spec.contains("let include_agent_jobs = false;")
        || !tools_spec.contains("let include_agent_job_worker_tools = false;")
        || !tools_spec.contains("const FREE_SUBAGENT_TOOLS_REMOVED: bool = true;")
        || !tools_spec.contains("if !FREE_SUBAGENT_TOOLS_REMOVED && config.collab_tools")
        || !tools_spec.contains("if !FREE_SUBAGENT_TOOLS_REMOVED && config.agent_jobs_tools")
    {
        violations.push("tool_builder_must_remove_free_subagent_tools".to_string());
    }
    if !tool_router.contains("fn is_removed_free_subagent_tool")
        || !tool_router.contains("free subagent tools are removed from CTOX")
    {
        violations.push("tool_router_must_reject_free_subagent_calls".to_string());
    }
    for required in [
        "(\"features.multi_agent\", false)",
        "(\"features.enable_fanout\", false)",
        "(\"features.memory_tool\", false)",
    ] {
        if !direct_session.contains(required) {
            violations.push(format!("managed_session_missing_override:{required}"));
        }
    }
    if !direct_session.contains("let session_source = SessionSource::Exec")
        || direct_session.contains("SessionSource::SubAgent")
        || !direct_session.contains("SandboxPolicy::new_read_only_policy()")
    {
        violations.push("completion_review_must_be_server_owned_read_only_exec".to_string());
    }
    if !fork_record.contains("Free Subagents Removed")
        || !fork_record.contains("Coding")
        || !fork_record.contains("sole external-agent exception")
    {
        violations.push("fork_record_must_state_no_free_subagents".to_string());
    }

    HarnessSubagentSpawnModelReport {
        ok: violations.is_empty(),
        contracts,
        violations,
        proof: "CTOX-managed sessions force multi_agent, enable_fanout, and memories off. The tool builder does not register free collaboration or fanout tools, the model-visible projection filters their reserved names, and the router rejects any stale or injected call. Completion review runs as a server-owned read-only Exec session, never as a child session. Durable CTOX work decomposition and the separate policy-checked Coding Agents provider channel are the only allowed execution paths."
            .to_string(),
        recommended_gate: "Run as a release/CI gate together with cargo test, cargo check, and process-mining spawn-liveness. Keep the static analyzer as a unit test so normal test builds fail on invariant drift; do not put this into every rustc compile via build.rs because it is a repository-conformance proof, not type checking."
            .to_string(),
    }
}

/// x-subagent-liveness: forensic backstop over the running harness's actual
/// `threads` rows. The source-string proof above confirms the code SHAPE; this
/// confirms no row the runtime actually persisted (migration 0021:
/// `subagent_depth`, `subagent_parent_thread_id`) violates the config-INDEPENDENT
/// structural invariants a forked Codex delta or out-of-band writer could break.
/// Detective evidence, not a preventive gate: vacuously ok on a fresh/CI host (no
/// deep rows), and the config-relative `agents.max_depth` is reported only as
/// `max_observed_subagent_depth`, never used as a critical gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HarnessSubagentForensicReport {
    pub ok: bool,
    pub state_db_present: bool,
    pub threads_inspected: i64,
    pub max_observed_subagent_depth: i64,
    pub violations: Vec<String>,
    pub proof: String,
}

const FORENSIC_PROOF: &str = "Detective backstop over persisted threads rows: (a) every non-root subagent thread's subagent_depth is exactly one greater than its parent's, and (b) no subagent thread is the parent of a thread that itself has a child (the leaf/path-shape invariant). Both are config-independent structural facts. max_observed_subagent_depth is reported for evidence only; the config-relative agents.max_depth is never a critical gate here, because lowering it later would not retroactively make a validly-spawned historical row illegitimate. Absence of the state db, of the threads table, or of rows is vacuously ok.";

/// Read-only forensic analysis of the harness `threads` ledger. Resolves the
/// state db the same way the runtime does and tolerates its absence (no db / no
/// threads table / no rows == vacuously ok), so it never errors a process-mining
/// run on a fresh host.
pub fn analyze_harness_subagent_thread_forensics() -> HarnessSubagentForensicReport {
    match resolve_state_db_path().filter(|path| path.exists()) {
        Some(path) => match rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        ) {
            Ok(conn) => subagent_thread_forensics_from_conn(&conn, true),
            // The file exists but can't be opened (permissions, lock, corruption):
            // host-state, not a liveness breach. Degrade to vacuously ok, but keep
            // state_db_present=true so an operator can tell "present-but-unread"
            // apart from "absent".
            Err(_) => vacuous_forensic_report(true),
        },
        None => vacuous_forensic_report(false),
    }
}

fn resolve_state_db_path() -> Option<PathBuf> {
    // Locate the SAME state db the runtime uses, within the scope the verifier
    // specified (find_codex_home / state_db_path). Honour a CODEX_SQLITE_HOME
    // override exactly as the runtime's resolve_sqlite_home_env does — an absolute
    // value as-is, a relative value resolved against the cwd — else fall back to
    // the codex home. Reading the same env the runtime reads to LOCATE the db is
    // path resolution, not a new runtime toggle.
    //
    // Scope note: a `sqlite_home` set in config.toml (rare, advanced) is NOT
    // consulted here, because that would require loading the async harness Config
    // into this synchronous detective path. In that case this backstop targets the
    // conventional codex-home db; it never writes and only ever degrades to a soft
    // warning, so the worst case is reduced coverage, never a false critical gate.
    let home = match std::env::var(ctox_state::SQLITE_HOME_ENV) {
        Ok(raw) if !raw.trim().is_empty() => {
            let candidate = PathBuf::from(raw.trim());
            if candidate.is_absolute() {
                candidate
            } else {
                std::env::current_dir().ok()?.join(candidate)
            }
        }
        _ => crate::config::find_codex_home().ok()?,
    };
    Some(ctox_state::state_db_path(&home))
}

fn vacuous_forensic_report(state_db_present: bool) -> HarnessSubagentForensicReport {
    HarnessSubagentForensicReport {
        ok: true,
        state_db_present,
        threads_inspected: 0,
        max_observed_subagent_depth: 0,
        violations: Vec::new(),
        proof: FORENSIC_PROOF.to_string(),
    }
}

fn subagent_thread_forensics_from_conn(
    conn: &rusqlite::Connection,
    state_db_present: bool,
) -> HarnessSubagentForensicReport {
    // A db without the threads table (older / not migrated) is vacuously ok.
    let has_threads = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'threads'",
            [],
            |_| Ok(()),
        )
        .is_ok();
    if !has_threads {
        return vacuous_forensic_report(state_db_present);
    }

    let mut violations = Vec::new();
    let threads_inspected: i64 = conn
        .query_row("SELECT COUNT(*) FROM threads", [], |row| row.get(0))
        .unwrap_or(0);
    let max_observed_subagent_depth: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(subagent_depth), 0) FROM threads",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // (a) Every subagent thread's depth must be exactly one deeper than its
    // parent. The parent of a depth-1 subagent is the (non-subagent) root, whose
    // subagent_depth is NULL — so a child of a NULL-depth parent must be depth 1,
    // and a child of a depth-`d` subagent must be depth `d + 1`. A subagent (a row
    // WITH a parent) must itself carry a depth.
    let depth_breaks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM threads c \
             JOIN threads p ON c.subagent_parent_thread_id = p.id \
             WHERE c.subagent_depth IS NULL \
                OR (p.subagent_depth IS NULL AND c.subagent_depth <> 1) \
                OR (p.subagent_depth IS NOT NULL AND c.subagent_depth <> p.subagent_depth + 1)",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if depth_breaks > 0 {
        violations.push(format!(
            "subagent_depth_not_monotone_parent_plus_one:{depth_breaks}"
        ));
    }

    // (b) A subagent (parent NOT NULL) must not be the parent of a thread that
    // itself has a child — subagents are leaf workers, so no grandchild may hang
    // off a subagent.
    let grandchild_under_subagent: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM threads x \
             JOIN threads y ON y.subagent_parent_thread_id = x.id \
             JOIN threads z ON z.subagent_parent_thread_id = y.id \
             WHERE x.subagent_parent_thread_id IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if grandchild_under_subagent > 0 {
        violations.push(format!(
            "subagent_has_grandchild_non_leaf_path:{grandchild_under_subagent}"
        ));
    }

    HarnessSubagentForensicReport {
        ok: violations.is_empty(),
        state_db_present,
        threads_inspected,
        max_observed_subagent_depth,
        violations,
        proof: FORENSIC_PROOF.to_string(),
    }
}

fn harness_subagent_spawn_contracts() -> &'static [HarnessSubagentSpawnContract] {
    &[HarnessSubagentSpawnContract {
        pattern: "free-subagents-removed",
        parent_entity_type: "HarnessThread",
        child_entity_type: "None",
        ranking_function: "not applicable",
        finite_bound: "zero model-controlled child threads",
        recursion_guard: "builder omission plus router rejection",
        worker_tool_surface: "no collaboration, fanout, or agent-job tools",
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harness_subagent_spawn_model_forbids_free_subagents() {
        let report = analyze_harness_subagent_spawn_model();
        assert!(
            report.ok,
            "harness subagent spawn model violations: {:?}",
            report.violations
        );
    }

    fn threads_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, subagent_parent_thread_id TEXT, subagent_depth INTEGER);",
        )
        .unwrap();
        conn
    }

    fn insert_thread(
        conn: &rusqlite::Connection,
        id: &str,
        parent: Option<&str>,
        depth: Option<i64>,
    ) {
        conn.execute(
            "INSERT INTO threads (id, subagent_parent_thread_id, subagent_depth) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, parent, depth],
        )
        .unwrap();
    }

    #[test]
    fn forensics_pass_on_a_valid_bounded_subagent_tree() {
        let conn = threads_db();
        // The root is a non-subagent thread: NULL depth, no parent. Depth-1
        // subagents hang off it — this is the shape the production runtime writes,
        // and it must NOT be flagged (the parent-NULL-depth -> child-depth-1 case).
        insert_thread(&conn, "root", None, None);
        insert_thread(&conn, "a", Some("root"), Some(1));
        insert_thread(&conn, "b", Some("root"), Some(1));
        let report = subagent_thread_forensics_from_conn(&conn, true);
        assert!(report.ok, "valid tree must pass: {:?}", report.violations);
        assert_eq!(report.threads_inspected, 3);
        assert_eq!(report.max_observed_subagent_depth, 1);
    }

    #[test]
    fn forensics_flag_a_depth_break() {
        let conn = threads_db();
        insert_thread(&conn, "root", None, None);
        insert_thread(&conn, "a", Some("root"), Some(1));
        // child of 'a' whose depth is 3, not 2 — a broken parent+1 chain.
        insert_thread(&conn, "b", Some("a"), Some(3));
        let report = subagent_thread_forensics_from_conn(&conn, true);
        assert!(!report.ok);
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.contains("subagent_depth_not_monotone")),
            "{:?}",
            report.violations
        );
    }

    #[test]
    fn forensics_flag_a_grandchild_under_a_subagent() {
        let conn = threads_db();
        // root -> x (subagent) -> y -> z : x is a subagent with a grandchild z.
        // Depths stay monotone so ONLY the leaf/path-shape invariant fires.
        insert_thread(&conn, "root", None, None);
        insert_thread(&conn, "x", Some("root"), Some(1));
        insert_thread(&conn, "y", Some("x"), Some(2));
        insert_thread(&conn, "z", Some("y"), Some(3));
        let report = subagent_thread_forensics_from_conn(&conn, true);
        assert!(!report.ok);
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.contains("subagent_has_grandchild")),
            "{:?}",
            report.violations
        );
    }

    #[test]
    fn forensics_are_vacuously_ok_without_a_threads_table() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let report = subagent_thread_forensics_from_conn(&conn, true);
        assert!(report.ok);
        assert_eq!(report.threads_inspected, 0);
        assert!(report.violations.is_empty());
    }
}
