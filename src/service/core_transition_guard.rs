// Origin: CTOX
// License: Apache-2.0

use crate::service::core_state_machine as csm;
use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreTransitionProof {
    pub proof_id: String,
    pub accepted: bool,
    pub report: csm::CoreTransitionReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreSpawnRequest {
    pub parent_entity_type: String,
    pub parent_entity_id: String,
    pub child_entity_type: String,
    pub child_entity_id: String,
    pub spawn_kind: String,
    pub spawn_reason: String,
    pub actor: String,
    pub checkpoint_key: Option<String>,
    pub budget_key: Option<String>,
    pub max_attempts: Option<i64>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreSpawnProof {
    pub edge_id: String,
    pub accepted: bool,
    pub violation_codes: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum CoreSpawnEffect {
    DurableSelfWork,
    QueueExecution,
    PlanEmission,
    ScheduleEmission,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum CoreInterventionEffect {
    BlockChild,
    ConsolidateIntoParent,
    RequeueParent,
    MarkTerminal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoreSpawnerContract {
    pub pattern: &'static str,
    pub parent_entity_types: &'static [&'static str],
    pub child_entity_type: &'static str,
    pub effect: CoreSpawnEffect,
    pub requires_budget: bool,
    pub max_budget: i64,
    pub intervention_skill: &'static str,
    pub intervention_effects: &'static [CoreInterventionEffect],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoreSpawnModelReport {
    pub ok: bool,
    pub spawner_contracts: Vec<CoreSpawnerContract>,
    pub violations: Vec<String>,
    pub proof: String,
}

pub fn ensure_core_transition_guard_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS ctox_core_transition_proofs (
            proof_id TEXT PRIMARY KEY,
            entity_type TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            lane TEXT NOT NULL,
            from_state TEXT NOT NULL,
            to_state TEXT NOT NULL,
            core_event TEXT NOT NULL,
            actor TEXT NOT NULL,
            accepted INTEGER NOT NULL,
            violation_codes_json TEXT NOT NULL DEFAULT '[]',
            request_json TEXT NOT NULL,
            report_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ctox_core_transition_proofs_entity
          ON ctox_core_transition_proofs(entity_type, entity_id, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_ctox_core_transition_proofs_accepted
          ON ctox_core_transition_proofs(accepted, updated_at DESC);

        CREATE TABLE IF NOT EXISTS ctox_core_spawn_edges (
            edge_id TEXT PRIMARY KEY,
            parent_entity_type TEXT NOT NULL,
            parent_entity_id TEXT NOT NULL,
            child_entity_type TEXT NOT NULL,
            child_entity_id TEXT NOT NULL,
            spawn_kind TEXT NOT NULL,
            spawn_reason TEXT NOT NULL,
            actor TEXT NOT NULL,
            checkpoint_key TEXT,
            budget_key TEXT,
            max_attempts INTEGER,
            accepted INTEGER NOT NULL,
            violation_codes_json TEXT NOT NULL DEFAULT '[]',
            request_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            terminal_reaped_at TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_ctox_core_spawn_edges_parent
          ON ctox_core_spawn_edges(parent_entity_type, parent_entity_id, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_ctox_core_spawn_edges_child
          ON ctox_core_spawn_edges(child_entity_type, child_entity_id, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_ctox_core_spawn_edges_budget
          ON ctox_core_spawn_edges(budget_key, spawn_kind, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_ctox_core_spawn_edges_accepted
          ON ctox_core_spawn_edges(accepted, updated_at DESC);
        "#,
    )?;
    Ok(())
}

pub fn evaluate_core_transition(
    conn: &Connection,
    request: &csm::CoreTransitionRequest,
) -> Result<CoreTransitionProof> {
    ensure_core_transition_guard_schema(conn)?;

    // Defensive noop guard: a state-preserving update is by definition not
    // a transition. Returning early here closes a class of false-positive
    // `invalid_transition` rejections that hit production before this fix
    // (e.g. the mission-watchdog refreshing metadata of a closed
    // ticket_self_work_items row 193 times — none of which were attempts to
    // *change* the state).  We do not write a proof for the noop because no
    // transition occurred; the caller sees `accepted = true` and proceeds.
    if request.from_state == request.to_state {
        let report = csm::CoreTransitionReport {
            accepted: true,
            violations: Vec::new(),
        };
        let proof_id = noop_proof_id(request)?;
        return Ok(CoreTransitionProof {
            proof_id,
            accepted: true,
            report,
        });
    }

    let mut report = csm::validate_transition(request);
    validate_outcome_artifact_state(conn, request, &mut report)?;
    let request_json = serde_json::to_string(request)?;
    let report_json = serde_json::to_string(&report)?;
    let violation_codes = report
        .violations
        .iter()
        .map(|violation| violation.code.clone())
        .collect::<Vec<_>>();
    let violation_codes_json = serde_json::to_string(&violation_codes)?;
    let proof_id = deterministic_proof_id(&request_json);
    let now = Utc::now().to_rfc3339();

    conn.execute(
        r#"
        INSERT INTO ctox_core_transition_proofs (
            proof_id, entity_type, entity_id, lane, from_state, to_state,
            core_event, actor, accepted, violation_codes_json,
            request_json, report_json, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)
        ON CONFLICT(proof_id) DO UPDATE SET
            accepted = excluded.accepted,
            violation_codes_json = excluded.violation_codes_json,
            request_json = excluded.request_json,
            report_json = excluded.report_json,
            updated_at = excluded.updated_at
        "#,
        params![
            &proof_id,
            format!("{:?}", request.entity_type),
            &request.entity_id,
            format!("{:?}", request.lane),
            format!("{:?}", request.from_state),
            format!("{:?}", request.to_state),
            format!("{:?}", request.event),
            &request.actor,
            if report.accepted { 1 } else { 0 },
            violation_codes_json,
            request_json,
            report_json,
            now,
        ],
    )?;

    Ok(CoreTransitionProof {
        proof_id,
        accepted: report.accepted,
        report,
    })
}

fn validate_outcome_artifact_state(
    conn: &Connection,
    request: &csm::CoreTransitionRequest,
    report: &mut csm::CoreTransitionReport,
) -> Result<()> {
    if !report.accepted || request.evidence.expected_artifact_refs.is_empty() {
        return Ok(());
    }

    for expected in &request.evidence.expected_artifact_refs {
        if !request
            .evidence
            .delivered_artifact_refs
            .iter()
            .any(|delivered| artifact_ref_satisfies(expected, delivered))
        {
            report.violations.push(csm::CoreTransitionViolation {
                code: "WP-Outcome-Missing".to_string(),
                message: format!(
                    "terminal work transition requires delivered {:?} artifact `{}` in `{}`",
                    expected.kind, expected.primary_key, expected.expected_terminal_state
                ),
            });
            continue;
        }
    }

    for delivered in &request.evidence.delivered_artifact_refs {
        if let Some(expected) = request
            .evidence
            .expected_artifact_refs
            .iter()
            .find(|expected| artifact_ref_satisfies(expected, delivered))
        {
            if let Some(thread_key) = expected.primary_key.strip_prefix("thread:") {
                if delivered.kind == csm::ArtifactKind::OutboundEmail
                    && delivered.primary_key != expected.primary_key
                    && !outbound_email_belongs_to_thread(conn, &delivered.primary_key, thread_key)?
                {
                    report.violations.push(csm::CoreTransitionViolation {
                        code: "WP-Outcome-Wrong-State".to_string(),
                        message: format!(
                            "delivered outbound email artifact `{}` does not belong to required thread `{}`",
                            delivered.primary_key, thread_key
                        ),
                    });
                    continue;
                }
            }
            if let Some(actual_state) = load_artifact_terminal_state(conn, delivered)? {
                if actual_state != expected.expected_terminal_state {
                    report.violations.push(csm::CoreTransitionViolation {
                        code: "WP-Outcome-Wrong-State".to_string(),
                        message: format!(
                            "delivered {:?} artifact `{}` is in `{}` but `{}` was required",
                            delivered.kind,
                            delivered.primary_key,
                            actual_state,
                            expected.expected_terminal_state
                        ),
                    });
                }
            } else {
                report.violations.push(csm::CoreTransitionViolation {
                    code: "WP-Outcome-Missing".to_string(),
                    message: format!(
                        "delivered {:?} artifact `{}` is not present in durable runtime state",
                        delivered.kind, delivered.primary_key
                    ),
                });
            }
        }
    }

    report.accepted = report.violations.is_empty();
    Ok(())
}

fn outbound_email_belongs_to_thread(
    conn: &Connection,
    message_key: &str,
    thread_key: &str,
) -> Result<bool> {
    let count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM communication_messages
        WHERE message_key = ?1
          AND direction = 'outbound'
          AND channel = 'email'
          AND thread_key = ?2
        "#,
        params![message_key, thread_key],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn artifact_ref_satisfies(expected: &csm::ArtifactRef, delivered: &csm::ArtifactRef) -> bool {
    expected.kind == delivered.kind
        && expected.expected_terminal_state == delivered.expected_terminal_state
        && (expected.primary_key == delivered.primary_key
            || expected.primary_key == "*"
            || expected.primary_key.starts_with("thread:"))
}

fn load_artifact_terminal_state(
    conn: &Connection,
    artifact: &csm::ArtifactRef,
) -> Result<Option<String>> {
    match artifact.kind {
        csm::ArtifactKind::OutboundEmail => {
            if let Some(thread_key) = artifact.primary_key.strip_prefix("thread:") {
                conn.query_row(
                    r#"
                    SELECT status
                    FROM communication_messages
                    WHERE direction = 'outbound'
                      AND channel = 'email'
                      AND thread_key = ?1
                    ORDER BY observed_at DESC, rowid DESC
                    LIMIT 1
                    "#,
                    params![thread_key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(anyhow::Error::from)
            } else {
                conn.query_row(
                    "SELECT status FROM communication_messages WHERE message_key = ?1 LIMIT 1",
                    params![artifact.primary_key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(anyhow::Error::from)
            }
        }
        csm::ArtifactKind::WorkspaceFile => {
            let path = Path::new(&artifact.primary_key);
            if path.is_file() {
                Ok(Some("present".to_string()))
            } else {
                Ok(None)
            }
        }
        csm::ArtifactKind::TicketClosure => conn
            .query_row(
                "SELECT state FROM ticket_self_work_items WHERE work_id = ?1 LIMIT 1",
                params![artifact.primary_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(anyhow::Error::from),
        csm::ArtifactKind::KnowledgeEntry => conn
            .query_row(
                "SELECT status FROM ticket_knowledge_entries WHERE entry_id = ?1 LIMIT 1",
                params![artifact.primary_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(anyhow::Error::from),
        csm::ArtifactKind::VerificationRun => conn
            .query_row(
                "SELECT status FROM slice_verification_runs WHERE run_id = ?1 LIMIT 1",
                params![artifact.primary_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(anyhow::Error::from),
    }
}

pub fn enforce_core_transition(
    conn: &Connection,
    request: &csm::CoreTransitionRequest,
) -> Result<CoreTransitionProof> {
    let proof = evaluate_core_transition(conn, request)?;
    if proof.accepted {
        return Ok(proof);
    }

    anyhow::bail!("{}", agent_recovery_message(&proof.report));
}

pub fn evaluate_core_spawn(
    conn: &Connection,
    request: &CoreSpawnRequest,
) -> Result<CoreSpawnProof> {
    ensure_core_transition_guard_schema(conn)?;

    let request_json = serde_json::to_string(request)?;
    let edge_id = deterministic_spawn_edge_id(&request_json);
    let violation_codes = validate_core_spawn(conn, request, &edge_id)?;
    let accepted = violation_codes.is_empty();
    let message = core_spawn_message(&violation_codes);
    let violation_codes_json = serde_json::to_string(&violation_codes)?;
    let now = Utc::now().to_rfc3339();

    conn.execute(
        r#"
        INSERT INTO ctox_core_spawn_edges (
            edge_id, parent_entity_type, parent_entity_id, child_entity_type,
            child_entity_id, spawn_kind, spawn_reason, actor, checkpoint_key,
            budget_key, max_attempts, accepted, violation_codes_json,
            request_json, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)
        ON CONFLICT(edge_id) DO UPDATE SET
            accepted = excluded.accepted,
            violation_codes_json = excluded.violation_codes_json,
            request_json = excluded.request_json,
            updated_at = excluded.updated_at
        "#,
        params![
            &edge_id,
            &request.parent_entity_type,
            &request.parent_entity_id,
            &request.child_entity_type,
            &request.child_entity_id,
            &request.spawn_kind,
            &request.spawn_reason,
            &request.actor,
            request.checkpoint_key.as_deref(),
            request.budget_key.as_deref(),
            request.max_attempts,
            if accepted { 1 } else { 0 },
            violation_codes_json,
            request_json,
            now,
        ],
    )?;

    Ok(CoreSpawnProof {
        edge_id,
        accepted,
        violation_codes,
        message,
    })
}

pub fn enforce_core_spawn(conn: &Connection, request: &CoreSpawnRequest) -> Result<CoreSpawnProof> {
    let proof = evaluate_core_spawn(conn, request)?;
    if proof.accepted {
        return Ok(proof);
    }
    anyhow::bail!("{}", proof.message);
}

pub fn analyze_core_spawn_model() -> CoreSpawnModelReport {
    let contracts = core_spawner_contracts().to_vec();
    let mut violations = Vec::new();
    if contracts.is_empty() {
        violations.push("no_core_spawner_contracts_registered".to_string());
    }

    for contract in &contracts {
        if contract.pattern.trim().is_empty() {
            violations.push("spawner_contract_requires_pattern".to_string());
        }
        if contract.child_entity_type.trim().is_empty() {
            violations.push(format!(
                "spawner_contract_requires_child_type:{}",
                contract.pattern
            ));
        }
        if contract.parent_entity_types.is_empty() {
            violations.push(format!(
                "spawner_contract_requires_parent_types:{}",
                contract.pattern
            ));
        }
        if contract.requires_budget && !(1..=64).contains(&contract.max_budget) {
            violations.push(format!(
                "spawner_contract_budget_invalid:{}",
                contract.pattern
            ));
        }
        if contract.intervention_skill != "queue-cleanup"
            && contract.intervention_skill != "harness-self-audit"
        {
            violations.push(format!(
                "spawner_contract_intervention_skill_not_approved:{}",
                contract.pattern
            ));
        }
        let Some(skill_contract_text) =
            intervention_skill_contract_text(contract.intervention_skill)
        else {
            violations.push(format!(
                "spawner_contract_intervention_skill_missing:{}",
                contract.pattern
            ));
            continue;
        };
        if !has_non_spawning_intervention_contract(skill_contract_text) {
            violations.push(format!(
                "spawner_contract_intervention_skill_not_non_spawning:{}",
                contract.pattern
            ));
        }
        if contract
            .intervention_effects
            .iter()
            .any(intervention_effect_spawns_work)
        {
            violations.push(format!(
                "spawner_contract_intervention_may_spawn:{}",
                contract.pattern
            ));
        }
    }

    CoreSpawnModelReport {
        ok: violations.is_empty(),
        spawner_contracts: contracts,
        violations,
        proof: "Every registered internal spawn has stable parent/child entity types, a finite budget, and a non-spawning intervention skill. Runtime enforcement rejects unregistered spawns, unstable IDs, over-budget requests, exhausted budgets, and cycles without finite budget. Therefore every accepted internal spawn path either advances to a new child once, consumes a finite budget, or is rejected into a bounded intervention effect set."
            .to_string(),
    }
}

fn deterministic_proof_id(request_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"ctox-core-transition-proof-v1");
    hasher.update(request_json.as_bytes());
    format!("ctp-{:x}", hasher.finalize())
}

fn deterministic_spawn_edge_id(request_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"ctox-core-spawn-edge-v1");
    hasher.update(request_json.as_bytes());
    format!("cse-{:x}", hasher.finalize())
}

fn noop_proof_id(request: &csm::CoreTransitionRequest) -> Result<String> {
    let request_json = serde_json::to_string(request)?;
    let mut hasher = Sha256::new();
    hasher.update(b"ctox-core-transition-proof-noop-v1");
    hasher.update(request_json.as_bytes());
    Ok(format!("ctp-noop-{:x}", hasher.finalize()))
}

fn validate_core_spawn(
    conn: &Connection,
    request: &CoreSpawnRequest,
    edge_id: &str,
) -> Result<Vec<String>> {
    let mut violations = Vec::new();
    let contract = core_spawner_contract(&request.spawn_kind);
    if contract.is_none() {
        violations.push("unregistered_spawn_kind".to_string());
    }
    if request.parent_entity_type.trim().is_empty()
        || request.parent_entity_id.trim().is_empty()
        || request.child_entity_type.trim().is_empty()
        || request.child_entity_id.trim().is_empty()
        || request.spawn_kind.trim().is_empty()
        || request.actor.trim().is_empty()
    {
        violations.push("spawn_requires_stable_entity_ids".to_string());
    }

    let has_budget = request
        .budget_key
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
        && request.max_attempts.unwrap_or_default() > 0;
    if request.max_attempts.unwrap_or_default() > 64 {
        violations.push("spawn_budget_too_large".to_string());
    }
    if let Some(contract) = contract {
        if !contract
            .parent_entity_types
            .iter()
            .any(|allowed| *allowed == "*" || *allowed == request.parent_entity_type)
        {
            violations.push("spawn_parent_type_not_registered".to_string());
        }
        if contract.child_entity_type != "*"
            && contract.child_entity_type != request.child_entity_type
        {
            violations.push("spawn_child_type_not_registered".to_string());
        }
        if contract.requires_budget && !has_budget {
            violations.push("spawn_requires_finite_budget".to_string());
        }
        if request.max_attempts.unwrap_or_default() > contract.max_budget {
            violations.push("spawn_budget_exceeds_contract".to_string());
        }
    }
    if looks_like_review_spawn(&request.spawn_kind) && !has_budget {
        violations.push("review_spawn_requires_finite_budget".to_string());
    }

    let self_cycle = request.parent_entity_type == request.child_entity_type
        && request.parent_entity_id == request.child_entity_id;
    let graph_cycle = if self_cycle {
        true
    } else if violations
        .iter()
        .any(|code| code == "spawn_requires_stable_entity_ids")
    {
        false
    } else {
        path_exists(
            conn,
            &request.child_entity_type,
            &request.child_entity_id,
            &request.parent_entity_type,
            &request.parent_entity_id,
        )?
    };
    if graph_cycle && !has_budget {
        violations.push("spawn_cycle_requires_finite_budget".to_string());
    }

    if let (Some(budget_key), Some(max_attempts)) =
        (request.budget_key.as_deref(), request.max_attempts)
    {
        if !budget_key.trim().is_empty() && max_attempts > 0 {
            let existing = accepted_spawn_budget_count(conn, budget_key, edge_id)?;
            if existing >= max_attempts {
                violations.push("spawn_budget_exhausted".to_string());
            }
        }
    }

    Ok(violations)
}

fn core_spawner_contract(spawn_kind: &str) -> Option<&'static CoreSpawnerContract> {
    core_spawner_contracts()
        .iter()
        .find(|contract| spawn_kind_matches(contract.pattern, spawn_kind))
}

fn core_spawner_contracts() -> &'static [CoreSpawnerContract] {
    const BLOCK_OR_CONSOLIDATE: &[CoreInterventionEffect] = &[
        CoreInterventionEffect::BlockChild,
        CoreInterventionEffect::ConsolidateIntoParent,
        CoreInterventionEffect::RequeueParent,
        CoreInterventionEffect::MarkTerminal,
    ];
    &[
        CoreSpawnerContract {
            pattern: "self-work:*",
            parent_entity_types: &["ControlPlane", "Message", "QueueTask", "Thread", "WorkItem"],
            child_entity_type: "WorkItem",
            effect: CoreSpawnEffect::DurableSelfWork,
            requires_budget: true,
            max_budget: 64,
            intervention_skill: "queue-cleanup",
            intervention_effects: BLOCK_OR_CONSOLIDATE,
        },
        CoreSpawnerContract {
            pattern: "self-work-queue-task",
            parent_entity_types: &["WorkItem"],
            child_entity_type: "QueueTask",
            effect: CoreSpawnEffect::QueueExecution,
            requires_budget: true,
            max_budget: 64,
            intervention_skill: "queue-cleanup",
            intervention_effects: BLOCK_OR_CONSOLIDATE,
        },
        CoreSpawnerContract {
            pattern: "queue-task",
            parent_entity_types: &["ControlPlane", "Message", "Thread", "WorkItem"],
            child_entity_type: "QueueTask",
            effect: CoreSpawnEffect::QueueExecution,
            requires_budget: true,
            max_budget: 64,
            intervention_skill: "queue-cleanup",
            intervention_effects: BLOCK_OR_CONSOLIDATE,
        },
        CoreSpawnerContract {
            pattern: "plan-step-message",
            parent_entity_types: &["PlanStep"],
            child_entity_type: "Message",
            effect: CoreSpawnEffect::PlanEmission,
            requires_budget: true,
            max_budget: 8,
            intervention_skill: "harness-self-audit",
            intervention_effects: BLOCK_OR_CONSOLIDATE,
        },
        CoreSpawnerContract {
            pattern: "schedule-run-message",
            parent_entity_types: &["ScheduleTask"],
            child_entity_type: "Message",
            effect: CoreSpawnEffect::ScheduleEmission,
            requires_budget: true,
            max_budget: 64,
            intervention_skill: "queue-cleanup",
            intervention_effects: BLOCK_OR_CONSOLIDATE,
        },
    ]
}

fn spawn_kind_matches(pattern: &str, spawn_kind: &str) -> bool {
    pattern
        .strip_suffix('*')
        .map(|prefix| spawn_kind.starts_with(prefix))
        .unwrap_or_else(|| pattern == spawn_kind)
}

fn intervention_effect_spawns_work(_effect: &CoreInterventionEffect) -> bool {
    false
}

fn intervention_skill_contract_text(skill: &str) -> Option<&'static str> {
    match skill {
        "queue-cleanup" => Some(include_str!(
            "../../skills/system/mission_orchestration/queue-cleanup/SKILL.md"
        )),
        "harness-self-audit" => Some(include_str!(
            "../../skills/system/skill_meta/harness-self-audit/SKILL.md"
        )),
        _ => None,
    }
}

fn has_non_spawning_intervention_contract(text: &str) -> bool {
    text.contains("## Core Spawn Intervention Contract")
        && text.contains("must not create new durable work")
        && text.contains("Do not run commands that create new queue tasks")
        && text.contains("ctox ticket self-work-put")
        && text.contains("ctox schedule ensure")
        && text.contains("ctox plan ingest")
}

fn looks_like_review_spawn(spawn_kind: &str) -> bool {
    let lowered = spawn_kind.to_ascii_lowercase();
    lowered.contains("review")
}

fn accepted_spawn_budget_count(conn: &Connection, budget_key: &str, edge_id: &str) -> Result<i64> {
    let count = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ctox_core_spawn_edges
        WHERE accepted = 1
          AND budget_key = ?1
          AND edge_id <> ?2
        "#,
        params![budget_key, edge_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(count.max(0))
}

fn path_exists(
    conn: &Connection,
    from_entity_type: &str,
    from_entity_id: &str,
    to_entity_type: &str,
    to_entity_id: &str,
) -> Result<bool> {
    let count = conn.query_row(
        r#"
        WITH RECURSIVE reachable(entity_type, entity_id) AS (
            SELECT child_entity_type, child_entity_id
            FROM ctox_core_spawn_edges
            WHERE accepted = 1
              AND parent_entity_type = ?1
              AND parent_entity_id = ?2
            UNION
            SELECT e.child_entity_type, e.child_entity_id
            FROM ctox_core_spawn_edges e
            JOIN reachable r
              ON e.parent_entity_type = r.entity_type
             AND e.parent_entity_id = r.entity_id
            WHERE e.accepted = 1
        )
        SELECT COUNT(*)
        FROM reachable
        WHERE entity_type = ?3
          AND entity_id = ?4
        LIMIT 1
        "#,
        params![
            from_entity_type,
            from_entity_id,
            to_entity_type,
            to_entity_id
        ],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(count > 0)
}

fn core_spawn_message(violation_codes: &[String]) -> String {
    if violation_codes.is_empty() {
        return "Spawn accepted by the core process graph.".to_string();
    }
    let mut actions = Vec::new();
    for code in violation_codes {
        let action = match code.as_str() {
            "spawn_requires_stable_entity_ids" => {
                "Jeder Task-Spawn braucht stabile Parent- und Child-Entity-IDs, damit Process Mining und Cleanup die Kante nachvollziehen koennen."
            }
            "unregistered_spawn_kind" => {
                "Diese Spawn-Art ist nicht im Kernel registriert. Fuege zuerst einen Core-Spawner-Contract mit Parent, Child, Budget und Intervention hinzu."
            }
            "spawn_parent_type_not_registered" | "spawn_child_type_not_registered" => {
                "Diese Parent-Child-Kante passt nicht zum registrierten Spawner-Contract. Nutze den kanonischen Parent oder konsolidiere in bestehende Arbeit."
            }
            "spawn_requires_finite_budget" => {
                "Jeder interne Spawn braucht ein endliches Budget, damit der Harness keine unbegrenzte interne Sequenz bilden kann."
            }
            "spawn_budget_exceeds_contract" => {
                "Das angeforderte Budget ueberschreitet den registrierten Spawner-Contract. Nutze die enge Kernel-Schranke oder konsolidiere."
            }
            "review_spawn_requires_finite_budget" => {
                "Review darf nur als endlicher Checkpoint wirken. Verknuepfe die Nacharbeit mit dem bestehenden Haupt-Work-Item oder gib ein kleines Spawn-Budget an."
            }
            "spawn_cycle_requires_finite_budget" => {
                "Diese Spawn-Kante wuerde einen Prozesszyklus erzeugen. Erlaube den Zyklus nur mit explizitem Budget und Counter."
            }
            "spawn_budget_too_large" => {
                "Das angeforderte Spawn-Budget ist zu gross. Waehle eine enge obere Schranke, damit Liveness beweisbar bleibt."
            }
            "spawn_budget_exhausted" => {
                "Das finite Spawn-Budget fuer diese Parent-Child-Klasse ist erschoepft. Konsolidiere die bestehende Arbeit statt weitere Self-Work-Kaskaden zu erzeugen."
            }
            _ => {
                "Die geplante Spawn-Kante passt nicht zum abgesicherten Prozessmodell. Verknuepfe sie mit einer existierenden Parent-Entity oder einem endlichen Budget."
            }
        };
        if !actions.iter().any(|existing| *existing == action) {
            actions.push(action);
        }
    }
    format!(
        "Dieser Task-Spawn wurde vom Core-Prozessgraphen gestoppt. {}",
        actions.join(" ")
    )
}

fn agent_recovery_message(report: &csm::CoreTransitionReport) -> String {
    let mut actions = Vec::new();
    for violation in &report.violations {
        let action = match violation.code.as_str() {
            "invalid_transition" => {
                "Bleib im erlaubten Arbeitsablauf: springe nicht direkt zum Zielzustand. Fuehre die fehlenden Zwischenschritte aus, dokumentiere jeden Schritt dauerhaft und versuche danach erneut fortzufahren."
            }
            "owner_visible_completion_requires_review" => {
                "Schliesse owner- oder founder-sichtbare Arbeit noch nicht ab. Fuehre zuerst ein echtes Review durch, arbeite kritische Review-Punkte inhaltlich nach und speichere die Review-Freigabe dauerhaft."
            }
            "closure_requires_verification" => {
                "Schliesse die Aufgabe noch nicht. Verifiziere das Ergebnis zuerst mit belastbarer Evidenz, speichere diese Evidenz und schliesse erst danach."
            }
            "founder_send_requires_review_audit" => {
                "Sende diese Founder-Kommunikation noch nicht. Baue zuerst den vollstaendigen Kontext auf, lasse den finalen Entwurf durch das Review laufen und speichere die Review-Freigabe dauerhaft."
            }
            "founder_send_body_hash_mismatch" => {
                "Der zu sendende Text entspricht nicht dem freigegebenen Review-Text. Stoppe den Versand, erstelle den finalen Entwurf erneut und lasse genau diese finale Fassung freigeben."
            }
            "founder_send_recipient_hash_mismatch" => {
                "Die Empfaenger oder CC-Liste entsprechen nicht der freigegebenen Fassung. Stoppe den Versand, pruefe To/CC gegen den Mail-Thread-Kontext und lasse die finale Empfaengerliste erneut freigeben."
            }
            "WP-Outcome-Missing" => {
                "Markiere die Aufgabe nicht als erledigt. Erzeuge zuerst das erwartete dauerhafte Ergebnis-Artefakt und verknuepfe dessen stabile Referenz mit dem Abschluss."
            }
            "WP-Outcome-Wrong-State" => {
                "Markiere die Aufgabe nicht als erledigt. Das Ergebnis-Artefakt existiert, ist aber noch nicht im geforderten Endzustand; repariere oder wiederhole die konkrete Ausfuehrung."
            }
            "commitment_requires_backing_schedule" => {
                "Lege kein Versprechen ohne Absicherung ab. Erstelle zuerst eine konkrete Termin- oder Queue-Absicherung, damit die Zusage rechtzeitig bearbeitet wird."
            }
            "commitment_delivery_requires_evidence" => {
                "Markiere die Zusage noch nicht als geliefert. Sammle zuerst belastbare Liefer-Evidenz und verknuepfe sie mit der Zusage."
            }
            "repair_requires_canonical_hot_path" => {
                "Fuehre die Reparatur ueber den kanonischen Repair-Pfad aus: Diagnose, Plan, Review, deterministische Massnahmen, Verifikation. Starte nicht mitten im Prozess."
            }
            "active_knowledge_requires_incident" => {
                "Lege Knowledge nicht direkt als aktiv ab. Halte zuerst den beobachteten Vorfall fest, formuliere die Lehre, pruefe sie und aktiviere sie erst nach Evidenz."
            }
            _ => {
                "Die geplante Aktion passt noch nicht zum gesicherten Harness-Zustand. Pruefe den naechsten erlaubten Arbeitsschritt, halte Evidenz fest und versuche erst danach erneut fortzufahren."
            }
        };
        if !actions.iter().any(|existing| *existing == action) {
            actions.push(action);
        }
    }

    if actions.is_empty() {
        actions.push("Die Aktion wurde vom Harness gestoppt. Pruefe den naechsten erlaubten Arbeitsschritt, halte Evidenz fest und versuche danach erneut fortzufahren.");
    }

    format!(
        "Diese Aktion wurde noch nicht ausgefuehrt, weil der abgesicherte Arbeitsablauf unvollstaendig ist. {}\nWenn du eine kompakte Diagnose brauchst, nutze `ctox process-mining guidance --limit 50` und arbeite die dort genannten naechsten Schritte ab.",
        actions.join(" ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::core_state_machine::{
        CoreEntityType, CoreEvent, CoreEvidenceRefs, CoreState, CoreTransitionRequest, RuntimeLane,
    };
    use std::collections::BTreeMap;

    fn founder_send_request(evidence: CoreEvidenceRefs) -> CoreTransitionRequest {
        let mut metadata = BTreeMap::new();
        metadata.insert("protected_party".to_string(), "founder".to_string());
        metadata.insert("channel".to_string(), "email".to_string());

        CoreTransitionRequest {
            entity_type: CoreEntityType::FounderCommunication,
            entity_id: "thread-founder".to_string(),
            lane: RuntimeLane::P0FounderCommunication,
            from_state: CoreState::Approved,
            to_state: CoreState::Sending,
            event: CoreEvent::Send,
            actor: "CTO1".to_string(),
            evidence,
            metadata,
        }
    }

    #[test]
    fn rejected_transition_is_persisted_as_proof() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let proof =
            evaluate_core_transition(&conn, &founder_send_request(CoreEvidenceRefs::default()))?;

        assert!(!proof.accepted);
        let accepted: i64 = conn.query_row(
            "SELECT accepted FROM ctox_core_transition_proofs WHERE proof_id = ?1",
            params![proof.proof_id],
            |row| row.get(0),
        )?;
        assert_eq!(accepted, 0);
        Ok(())
    }

    #[test]
    fn noop_transition_returns_accepted_without_writing_proof() -> Result<()> {
        // Bug B regression: a state-preserving update (from_state == to_state)
        // is by definition not a transition. The guard must short-circuit
        // before writing a proof, so that the mission-watchdog refreshing
        // metadata of a closed work item cannot generate hundreds of
        // false-positive `invalid_transition` rejections.
        let conn = Connection::open_in_memory()?;
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::WorkItem,
            entity_id: "wi-1".to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: CoreState::Planned,
            to_state: CoreState::Planned,
            event: CoreEvent::Plan,
            actor: "queue".to_string(),
            evidence: CoreEvidenceRefs::default(),
            metadata: BTreeMap::new(),
        };
        let proof = evaluate_core_transition(&conn, &request)?;
        assert!(proof.accepted);
        assert!(proof.report.violations.is_empty());
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ctox_core_transition_proofs",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 0, "noop transitions must not write proofs");
        Ok(())
    }

    #[test]
    fn accepted_transition_is_persisted_as_proof() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let evidence = CoreEvidenceRefs {
            review_audit_key: Some("review-1".to_string()),
            approved_body_sha256: Some("body".to_string()),
            outgoing_body_sha256: Some("body".to_string()),
            approved_recipient_set_sha256: Some("recipients".to_string()),
            outgoing_recipient_set_sha256: Some("recipients".to_string()),
            ..CoreEvidenceRefs::default()
        };
        let proof = evaluate_core_transition(&conn, &founder_send_request(evidence))?;

        assert!(proof.accepted);
        let accepted: i64 = conn.query_row(
            "SELECT accepted FROM ctox_core_transition_proofs WHERE proof_id = ?1",
            params![proof.proof_id],
            |row| row.get(0),
        )?;
        assert_eq!(accepted, 1);
        Ok(())
    }

    #[test]
    fn outcome_witness_rejects_delivered_email_in_wrong_state() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            r#"
            CREATE TABLE communication_messages (
                message_key TEXT PRIMARY KEY,
                direction TEXT,
                channel TEXT,
                thread_key TEXT,
                status TEXT,
                observed_at TEXT
            );
            INSERT INTO communication_messages (
                message_key, direction, channel, thread_key, status, observed_at
            ) VALUES (
                'email:cto@example.test::pending_send::abc',
                'outbound',
                'email',
                'founder-thread',
                'send_failed',
                '2026-05-04T18:00:00Z'
            );
            "#,
        )?;
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: "queue:send-mail".to_string(),
            lane: RuntimeLane::P0FounderCommunication,
            from_state: CoreState::Running,
            to_state: CoreState::Completed,
            event: CoreEvent::Complete,
            actor: "ctox-service".to_string(),
            evidence: CoreEvidenceRefs {
                expected_artifact_refs: vec![csm::ArtifactRef {
                    kind: csm::ArtifactKind::OutboundEmail,
                    primary_key: "thread:founder-thread".to_string(),
                    expected_terminal_state: "accepted".to_string(),
                }],
                delivered_artifact_refs: vec![csm::ArtifactRef {
                    kind: csm::ArtifactKind::OutboundEmail,
                    primary_key: "email:cto@example.test::pending_send::abc".to_string(),
                    expected_terminal_state: "accepted".to_string(),
                }],
                ..CoreEvidenceRefs::default()
            },
            metadata: BTreeMap::new(),
        };

        let proof = evaluate_core_transition(&conn, &request)?;

        assert!(!proof.accepted);
        assert!(proof
            .report
            .violations
            .iter()
            .any(|violation| violation.code == "WP-Outcome-Wrong-State"));
        Ok(())
    }

    #[test]
    fn outcome_witness_accepts_delivered_email_in_expected_state() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            r#"
            CREATE TABLE communication_messages (
                message_key TEXT PRIMARY KEY,
                direction TEXT,
                channel TEXT,
                thread_key TEXT,
                status TEXT,
                observed_at TEXT
            );
            INSERT INTO communication_messages (
                message_key, direction, channel, thread_key, status, observed_at
            ) VALUES (
                'email:cto@example.test::pending_send::abc',
                'outbound',
                'email',
                'founder-thread',
                'accepted',
                '2026-05-04T18:00:00Z'
            );
            "#,
        )?;
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: "queue:send-mail".to_string(),
            lane: RuntimeLane::P0FounderCommunication,
            from_state: CoreState::Running,
            to_state: CoreState::Completed,
            event: CoreEvent::Complete,
            actor: "ctox-service".to_string(),
            evidence: CoreEvidenceRefs {
                expected_artifact_refs: vec![csm::ArtifactRef {
                    kind: csm::ArtifactKind::OutboundEmail,
                    primary_key: "thread:founder-thread".to_string(),
                    expected_terminal_state: "accepted".to_string(),
                }],
                delivered_artifact_refs: vec![csm::ArtifactRef {
                    kind: csm::ArtifactKind::OutboundEmail,
                    primary_key: "email:cto@example.test::pending_send::abc".to_string(),
                    expected_terminal_state: "accepted".to_string(),
                }],
                ..CoreEvidenceRefs::default()
            },
            metadata: BTreeMap::new(),
        };

        let proof = evaluate_core_transition(&conn, &request)?;

        assert!(proof.accepted, "{:?}", proof.report.violations);
        Ok(())
    }

    #[test]
    fn outcome_witness_rejects_missing_workspace_file() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let path = std::env::temp_dir()
            .join("ctox-missing-workspace-artifact")
            .join("logbook.md");
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: "queue:tb2-bootstrap".to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: CoreState::Running,
            to_state: CoreState::Completed,
            event: CoreEvent::Complete,
            actor: "ctox-service".to_string(),
            evidence: CoreEvidenceRefs {
                expected_artifact_refs: vec![csm::ArtifactRef {
                    kind: csm::ArtifactKind::WorkspaceFile,
                    primary_key: path.display().to_string(),
                    expected_terminal_state: "present".to_string(),
                }],
                delivered_artifact_refs: Vec::new(),
                ..CoreEvidenceRefs::default()
            },
            metadata: BTreeMap::new(),
        };

        let proof = evaluate_core_transition(&conn, &request)?;

        assert!(!proof.accepted);
        assert!(proof
            .report
            .violations
            .iter()
            .any(|violation| violation.code == "WP-Outcome-Missing"));
        Ok(())
    }

    #[test]
    fn outcome_witness_accepts_present_workspace_file() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let dir = std::env::temp_dir().join(format!(
            "ctox-present-workspace-artifact-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("logbook.md");
        std::fs::write(&path, "# log\n")?;
        let artifact = csm::ArtifactRef {
            kind: csm::ArtifactKind::WorkspaceFile,
            primary_key: path.display().to_string(),
            expected_terminal_state: "present".to_string(),
        };
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: "queue:tb2-bootstrap".to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: CoreState::Running,
            to_state: CoreState::Completed,
            event: CoreEvent::Complete,
            actor: "ctox-service".to_string(),
            evidence: CoreEvidenceRefs {
                expected_artifact_refs: vec![artifact.clone()],
                delivered_artifact_refs: vec![artifact],
                ..CoreEvidenceRefs::default()
            },
            metadata: BTreeMap::new(),
        };

        let proof = evaluate_core_transition(&conn, &request)?;

        assert!(proof.accepted, "{:?}", proof.report.violations);
        Ok(())
    }

    #[test]
    fn rejected_transition_error_is_agent_readable_without_internal_ids() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let err =
            enforce_core_transition(&conn, &founder_send_request(CoreEvidenceRefs::default()))
                .expect_err("founder send without review must be rejected");
        let message = err.to_string();

        assert!(message.contains("Sende diese Founder-Kommunikation noch nicht"));
        assert!(message.contains("ctox process-mining guidance --limit 50"));
        assert!(!message.contains("ctp-"));
        assert!(!message.contains("founder_send_requires_review_audit"));
        assert!(!message.contains("core transition guard rejected"));
        Ok(())
    }

    fn spawn_request(parent: &str, child: &str, kind: &str) -> CoreSpawnRequest {
        CoreSpawnRequest {
            parent_entity_type: "WorkItem".to_string(),
            parent_entity_id: parent.to_string(),
            child_entity_type: "QueueTask".to_string(),
            child_entity_id: child.to_string(),
            spawn_kind: kind.to_string(),
            spawn_reason: "test".to_string(),
            actor: "ctox-test".to_string(),
            checkpoint_key: None,
            budget_key: Some(format!("test-budget:{parent}:{child}")),
            max_attempts: Some(4),
            metadata: BTreeMap::new(),
        }
    }

    fn self_work_spawn_request(parent: &str, child: &str) -> CoreSpawnRequest {
        let mut request = spawn_request(parent, child, "self-work:mission-follow-up");
        request.child_entity_type = "WorkItem".to_string();
        request
    }

    #[test]
    fn core_spawn_accepts_and_persists_acyclic_edge() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let proof = enforce_core_spawn(&conn, &spawn_request("parent", "child", "queue-task"))?;

        assert!(proof.accepted);
        let accepted: i64 = conn.query_row(
            "SELECT accepted FROM ctox_core_spawn_edges WHERE edge_id = ?1",
            params![proof.edge_id],
            |row| row.get(0),
        )?;
        assert_eq!(accepted, 1);
        Ok(())
    }

    #[test]
    fn core_spawn_rejects_unbudgeted_graph_cycle() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        enforce_core_spawn(&conn, &self_work_spawn_request("a", "b"))?;

        let mut cycle = self_work_spawn_request("b", "a");
        cycle.budget_key = None;
        cycle.max_attempts = None;
        let proof = evaluate_core_spawn(&conn, &cycle)?;

        assert!(!proof.accepted);
        assert!(proof
            .violation_codes
            .contains(&"spawn_cycle_requires_finite_budget".to_string()));
        Ok(())
    }

    #[test]
    fn core_spawn_allows_only_finite_budgeted_cycles() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        enforce_core_spawn(&conn, &self_work_spawn_request("a", "b"))?;
        let mut cycle = self_work_spawn_request("b", "a");
        cycle.budget_key = Some("cycle-budget".to_string());
        cycle.max_attempts = Some(1);
        let accepted = enforce_core_spawn(&conn, &cycle)?;
        assert!(accepted.accepted);

        let mut exhausted = self_work_spawn_request("b", "a2");
        exhausted.budget_key = Some("cycle-budget".to_string());
        exhausted.max_attempts = Some(1);
        let proof = evaluate_core_spawn(&conn, &exhausted)?;

        assert!(!proof.accepted);
        assert!(proof
            .violation_codes
            .contains(&"spawn_budget_exhausted".to_string()));
        Ok(())
    }

    #[test]
    fn core_spawn_review_children_require_finite_budget() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let mut request = spawn_request("main", "review-rework-1", "self-work:review-rework");
        request.child_entity_type = "WorkItem".to_string();
        request.budget_key = None;
        request.max_attempts = None;
        let proof = evaluate_core_spawn(&conn, &request)?;

        assert!(!proof.accepted);
        assert!(proof
            .violation_codes
            .contains(&"review_spawn_requires_finite_budget".to_string()));
        Ok(())
    }

    #[test]
    fn core_spawn_model_proves_registered_interventions_are_bounded() {
        let report = analyze_core_spawn_model();
        assert!(report.ok, "{report:#?}");
        assert!(report
            .proof
            .contains("Every registered internal spawn has stable parent/child"));
    }

    #[test]
    fn core_spawn_rejects_unregistered_spawn_kind() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let proof = evaluate_core_spawn(&conn, &spawn_request("a", "b", "unknown-spawn"))?;
        assert!(!proof.accepted);
        assert!(proof
            .violation_codes
            .contains(&"unregistered_spawn_kind".to_string()));
        Ok(())
    }
}
