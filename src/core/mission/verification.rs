use anyhow::Context;
use anyhow::Result;
use chrono::Duration;
use serde::Serialize;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use std::path::Path;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use crate::inference::turn_loop;
use crate::lcm;
use crate::review;

const OPERATIONAL_CLAIM_EXPIRY_HOURS: i64 = 6;
const DEFAULT_RUN_LIMIT: usize = 20;
const DEFAULT_CLAIM_LIMIT: usize = 50;

const CLOSURE_WORDS: &[&str] = &[
    "done",
    "completed",
    "finished",
    "verified",
    "works now",
    "fixed",
    "installed",
    "configured",
    "rolled out",
    "deploy",
    "smoke test",
    "tests pass",
    "validated",
];

const OPERATIONAL_WORDS: &[&str] = &[
    "deploy",
    "rollout",
    "install",
    "migration",
    "database",
    "schema",
    "service",
    "systemd",
    "restart",
    "http",
    "api",
    "endpoint",
    "config",
    "nginx",
    "docker",
    "compose",
    "secret",
    "credential",
];

const ARTIFACT_WORDS: &[&str] = &[
    "patch",
    "refactor",
    "updated",
    "changed",
    "edit",
    "helper",
    "skill",
    "contract",
    "src/",
    ".rs",
    ".ts",
    ".py",
    "cargo.toml",
    "package.json",
];

#[derive(Debug, Clone)]
pub struct SliceVerificationRequest {
    pub conversation_id: i64,
    pub goal: String,
    pub prompt: String,
    pub preview: String,
    pub source_label: String,
    pub owner_visible: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordedSliceAssurance {
    pub run: lcm::VerificationRunRecord,
    pub claims: Vec<lcm::MissionClaimRecord>,
}

impl RecordedSliceAssurance {
    pub fn closure_blocking_open_items(&self) -> Vec<String> {
        self.claims
            .iter()
            .filter(|claim| claim.blocks_closure && claim_is_open(claim))
            .map(|claim| claim.summary.clone())
            .collect()
    }
}

pub fn handle_verification_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    let db_path = root.join("runtime/ctox.sqlite3");
    let engine = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())?;
    match command {
        "init" => print_json(&json!({
            "ok": true,
            "db_path": db_path,
        })),
        "assurance" => {
            let conversation_id = parse_conversation_id(args)?;
            let assurance = engine.mission_assurance_snapshot(conversation_id)?;
            print_json(&json!({"ok": true, "assurance": assurance}))
        }
        "runs" => {
            let conversation_id = parse_conversation_id(args)?;
            let limit = parse_limit(args, DEFAULT_RUN_LIMIT);
            let runs = engine.list_verification_runs(conversation_id, limit)?;
            print_json(&json!({"ok": true, "count": runs.len(), "runs": runs}))
        }
        "claims" => {
            let conversation_id = parse_conversation_id(args)?;
            let include_verified = args.iter().any(|arg| arg == "--all");
            let limit = parse_limit(args, DEFAULT_CLAIM_LIMIT);
            let claims = engine.list_mission_claims(conversation_id, include_verified, limit)?;
            print_json(&json!({"ok": true, "count": claims.len(), "claims": claims}))
        }
        "claim-set" => {
            let claim = parse_manual_claim(args)?;
            engine.upsert_mission_claim(&claim)?;
            print_json(&json!({"ok": true, "claim": claim}))
        }
        _ => anyhow::bail!(
            "usage:\n  ctox verification init\n  ctox verification assurance [--conversation-id <id>]\n  ctox verification runs [--conversation-id <id>] [--limit <n>]\n  ctox verification claims [--conversation-id <id>] [--limit <n>] [--all]\n  ctox verification claim-set --conversation-id <id> --kind <kind> --status <verified|needs_recheck|reported|blocked> --subject <text> --summary <text> --evidence <text> [--blocks-closure] [--recheck-policy <always|on_change|never>] [--expires-at <epoch-ms>] [--last-run-id <id>] [--claim-key <id>]"
        ),
    }
}

pub fn record_slice_assurance(
    root: &Path,
    request: &SliceVerificationRequest,
    result_text: &str,
    blocker: Option<&str>,
    review_outcome: Option<&review::ReviewOutcome>,
) -> Result<RecordedSliceAssurance> {
    let db_path = root.join("runtime/ctox.sqlite3");
    let engine = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())?;
    let created_at = now_millis_string();
    let result_excerpt = clip_text(result_text, 280);
    let rendered_review_report = review_outcome
        .map(|outcome| outcome.canonical_report())
        .unwrap_or_default();
    let run_id = verification_run_id(
        request.conversation_id,
        &request.source_label,
        &request.goal,
        &request.preview,
        &result_excerpt,
        &created_at,
    );
    let claims = derive_claims(
        &run_id,
        &created_at,
        request,
        result_text,
        blocker,
        review_outcome,
    );
    let open_claim_count = claims.iter().filter(|claim| claim_is_open(claim)).count() as i64;
    let closure_blocking_claim_count = claims
        .iter()
        .filter(|claim| claim.blocks_closure && claim_is_open(claim))
        .count() as i64;
    let run = lcm::VerificationRunRecord {
        run_id,
        conversation_id: request.conversation_id,
        source_label: request.source_label.clone(),
        goal: request.goal.trim().to_string(),
        preview: request.preview.trim().to_string(),
        result_excerpt,
        blocker: blocker.map(|value| clip_text(value, 180)),
        review_required: review_outcome
            .map(|outcome| outcome.required)
            .unwrap_or(false),
        review_verdict: review_outcome
            .map(|outcome| outcome.verdict.as_gate_label().to_string())
            .unwrap_or_else(|| "not_run".to_string()),
        review_summary: review_outcome
            .map(|outcome| clip_text(&outcome.summary, 180))
            .unwrap_or_else(|| "No completion review was attached to this slice.".to_string()),
        review_score: review_outcome
            .map(|outcome| outcome.score as i64)
            .unwrap_or(0),
        review_reasons: review_outcome
            .map(|outcome| outcome.reasons.clone())
            .unwrap_or_default(),
        report_excerpt: review_outcome
            .map(|_| clip_text(&rendered_review_report, 280))
            .unwrap_or_default(),
        raw_report: rendered_review_report,
        mission_state: review_outcome
            .map(|outcome| outcome.mission_state.clone())
            .unwrap_or_else(|| "UNCLEAR".to_string()),
        failed_gates: review_outcome
            .map(|outcome| outcome.failed_gates.clone())
            .unwrap_or_default(),
        semantic_findings: review_outcome
            .map(|outcome| outcome.semantic_findings.clone())
            .unwrap_or_default(),
        open_items: review_outcome
            .map(|outcome| outcome.open_items.clone())
            .unwrap_or_default(),
        evidence: review_outcome
            .map(|outcome| outcome.evidence.clone())
            .unwrap_or_default(),
        handoff: review_outcome.and_then(|outcome| outcome.handoff.clone()),
        claim_count: claims.len() as i64,
        open_claim_count,
        closure_blocking_claim_count,
        created_at,
    };
    engine.persist_verification_run(&run, &claims)?;
    Ok(RecordedSliceAssurance { run, claims })
}

fn parse_conversation_id(args: &[String]) -> Result<i64> {
    find_flag_value(args, "--conversation-id")
        .map(|value| value.parse::<i64>())
        .transpose()
        .context("failed to parse --conversation-id")?
        .or(Some(turn_loop::CHAT_CONVERSATION_ID))
        .context("missing conversation id")
}

fn parse_limit(args: &[String], default: usize) -> usize {
    find_flag_value(args, "--limit")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn parse_manual_claim(args: &[String]) -> Result<lcm::MissionClaimRecord> {
    let conversation_id = parse_conversation_id(args)?;
    let claim_kind = required_flag_value(args, "--kind")
        .context("claim-set requires --kind")?
        .to_string();
    let claim_status = required_flag_value(args, "--status")
        .context("claim-set requires --status")?
        .to_string();
    let subject = required_flag_value(args, "--subject")
        .context("claim-set requires --subject")?
        .to_string();
    let summary = required_flag_value(args, "--summary")
        .context("claim-set requires --summary")?
        .to_string();
    let evidence_summary = required_flag_value(args, "--evidence")
        .context("claim-set requires --evidence")?
        .to_string();
    let now = now_millis_string();
    Ok(lcm::MissionClaimRecord {
        claim_key: find_flag_value(args, "--claim-key")
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| mission_claim_key(conversation_id, &claim_kind, &subject)),
        conversation_id,
        last_run_id: find_flag_value(args, "--last-run-id")
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("manual_{now}")),
        claim_kind,
        claim_status,
        blocks_closure: args.iter().any(|arg| arg == "--blocks-closure"),
        subject,
        summary,
        evidence_summary,
        recheck_policy: find_flag_value(args, "--recheck-policy")
            .unwrap_or("on_change")
            .to_string(),
        expires_at: find_flag_value(args, "--expires-at").map(ToOwned::to_owned),
        created_at: now.clone(),
        updated_at: now,
    })
}

fn print_json(value: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    find_flag_value(args, flag)
}

fn derive_claims(
    run_id: &str,
    created_at: &str,
    request: &SliceVerificationRequest,
    result_text: &str,
    blocker: Option<&str>,
    review_outcome: Option<&review::ReviewOutcome>,
) -> Vec<lcm::MissionClaimRecord> {
    let combined = format!(
        "{}\n{}\n{}\n{}",
        request.goal, request.prompt, request.preview, result_text
    );
    let lowered = combined.to_ascii_lowercase();
    let closure_claim =
        has_review_reason(review_outcome, "closure_claim") || contains_any(&lowered, CLOSURE_WORDS);
    let operational_claim = has_review_reason(review_outcome, "runtime_or_infra_change")
        || contains_any(&lowered, OPERATIONAL_WORDS);
    let artifact_claim = has_review_reason(review_outcome, "code_or_artifact_change")
        || contains_any(&lowered, ARTIFACT_WORDS);
    let subject = claim_subject(request);
    let mut claims = Vec::new();

    if operational_claim {
        claims.push(build_claim(
            run_id,
            created_at,
            request.conversation_id,
            "operational_state",
            &subject,
            derive_claim_status(review_outcome, blocker),
            review_outcome,
            result_text,
            blocker,
            review_outcome
                .map(|outcome| outcome.required)
                .unwrap_or(false)
                || closure_claim
                || request.owner_visible,
        ));
    }

    if artifact_claim {
        claims.push(build_claim(
            run_id,
            created_at,
            request.conversation_id,
            "artifact_state",
            &subject,
            derive_claim_status(review_outcome, blocker),
            review_outcome,
            result_text,
            blocker,
            closure_claim && request.owner_visible,
        ));
    }

    if closure_claim
        || review_outcome
            .map(|outcome| outcome.required)
            .unwrap_or(false)
    {
        claims.push(build_claim(
            run_id,
            created_at,
            request.conversation_id,
            "completion_gate",
            &subject,
            derive_claim_status(review_outcome, blocker),
            review_outcome,
            result_text,
            blocker,
            true,
        ));
    }

    if claims.is_empty() && blocker.is_some() {
        claims.push(build_claim(
            run_id,
            created_at,
            request.conversation_id,
            "completion_gate",
            &subject,
            "blocked",
            review_outcome,
            result_text,
            blocker,
            true,
        ));
    }

    claims
}

fn build_claim(
    run_id: &str,
    created_at: &str,
    conversation_id: i64,
    claim_kind: &str,
    subject: &str,
    claim_status: &'static str,
    review_outcome: Option<&review::ReviewOutcome>,
    result_text: &str,
    blocker: Option<&str>,
    blocks_closure: bool,
) -> lcm::MissionClaimRecord {
    let evidence_summary = claim_evidence_summary(review_outcome, result_text, blocker);
    lcm::MissionClaimRecord {
        claim_key: mission_claim_key(conversation_id, claim_kind, subject),
        conversation_id,
        last_run_id: run_id.to_string(),
        claim_kind: claim_kind.to_string(),
        claim_status: claim_status.to_string(),
        blocks_closure,
        subject: subject.to_string(),
        summary: claim_summary(claim_kind, claim_status, subject),
        evidence_summary,
        recheck_policy: recheck_policy_for(claim_kind).to_string(),
        expires_at: claim_expiry(claim_kind, claim_status),
        created_at: created_at.to_string(),
        updated_at: created_at.to_string(),
    }
}

fn derive_claim_status(
    review_outcome: Option<&review::ReviewOutcome>,
    blocker: Option<&str>,
) -> &'static str {
    if blocker.is_some() {
        return "blocked";
    }
    match review_outcome.map(|outcome| &outcome.verdict) {
        Some(review::ReviewVerdict::Pass) => "verified",
        Some(review::ReviewVerdict::Fail)
        | Some(review::ReviewVerdict::Partial)
        | Some(review::ReviewVerdict::Unavailable) => "needs_recheck",
        Some(review::ReviewVerdict::Skipped) | None => "reported",
    }
}

fn claim_summary(claim_kind: &str, claim_status: &str, subject: &str) -> String {
    match (claim_kind, claim_status) {
        ("operational_state", "verified") => {
            format!("Operational state for \"{subject}\" was verified against the live surface.")
        }
        ("operational_state", "blocked") => format!(
            "Operational state for \"{subject}\" remains blocked and must be revalidated before closure."
        ),
        ("operational_state", _) => format!(
            "Operational state for \"{subject}\" still needs live revalidation before CTOX can close this slice."
        ),
        ("artifact_state", "verified") => {
            format!("Artifact behavior for \"{subject}\" was verified strongly enough for closure.")
        }
        ("artifact_state", "blocked") => {
            format!("Artifact behavior for \"{subject}\" is still blocked and must stay open.")
        }
        ("artifact_state", _) => format!(
            "Artifact behavior for \"{subject}\" still needs evidence before final mission closure."
        ),
        ("completion_gate", "verified") => {
            format!("Completion gate for \"{subject}\" is currently supported by evidence.")
        }
        ("completion_gate", "blocked") => {
            format!("Completion gate for \"{subject}\" is blocked and must remain open.")
        }
        _ => format!(
            "Completion gate for \"{subject}\" must stay open until supporting claims are verified."
        ),
    }
}

fn claim_evidence_summary(
    review_outcome: Option<&review::ReviewOutcome>,
    result_text: &str,
    blocker: Option<&str>,
) -> String {
    if let Some(blocker) = blocker {
        return clip_text(&format!("Blocked: {blocker}"), 180);
    }
    if let Some(outcome) = review_outcome {
        let label = outcome.verdict.as_gate_label().to_ascii_uppercase();
        return clip_text(&format!("Review {label}: {}", outcome.summary.trim()), 180);
    }
    clip_text(&format!("Reported result: {}", result_text.trim()), 180)
}

fn recheck_policy_for(claim_kind: &str) -> &'static str {
    match claim_kind {
        "operational_state" => "revalidate_live_state_before_close",
        "artifact_state" => "verify_behavior_before_close",
        _ => "keep_open_until_supporting_claims_verified",
    }
}

fn claim_expiry(claim_kind: &str, claim_status: &str) -> Option<String> {
    if claim_kind != "operational_state" || claim_status != "verified" {
        return None;
    }
    let now = current_millis();
    let delta = Duration::hours(OPERATIONAL_CLAIM_EXPIRY_HOURS)
        .num_milliseconds()
        .max(0);
    Some(now.saturating_add(delta).to_string())
}

fn claim_is_open(claim: &lcm::MissionClaimRecord) -> bool {
    if claim.claim_status != "verified" {
        return true;
    }
    claim
        .expires_at
        .as_deref()
        .and_then(|value| value.parse::<i64>().ok())
        .map(|expiry| expiry <= current_millis())
        .unwrap_or(false)
}

fn claim_subject(request: &SliceVerificationRequest) -> String {
    let candidate = if request.preview.trim().is_empty() {
        request.goal.trim()
    } else {
        request.preview.trim()
    };
    clip_text(candidate, 80)
}

fn has_review_reason(review_outcome: Option<&review::ReviewOutcome>, reason: &str) -> bool {
    review_outcome
        .map(|outcome| outcome.reasons.iter().any(|item| item == reason))
        .unwrap_or(false)
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn verification_run_id(
    conversation_id: i64,
    source_label: &str,
    goal: &str,
    preview: &str,
    result_excerpt: &str,
    created_at: &str,
) -> String {
    let mut hash = Sha256::new();
    hash.update(conversation_id.to_string().as_bytes());
    hash.update(source_label.as_bytes());
    hash.update(goal.as_bytes());
    hash.update(preview.as_bytes());
    hash.update(result_excerpt.as_bytes());
    hash.update(created_at.as_bytes());
    let digest = hash.finalize();
    format!("vrun_{}", hex_prefix(&digest))
}

fn mission_claim_key(conversation_id: i64, claim_kind: &str, subject: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(conversation_id.to_string().as_bytes());
    hash.update(claim_kind.as_bytes());
    hash.update(normalize_key(subject).as_bytes());
    let digest = hash.finalize();
    format!("claim_{}", hex_prefix(&digest))
}

fn hex_prefix(digest: &[u8]) -> String {
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn normalize_key(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn clip_text(value: &str, max_chars: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut clipped = collapsed
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    clipped.push('…');
    clipped
}

fn now_millis_string() -> String {
    current_millis().to_string()
}

fn current_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("ctox-verification-{label}-{}", current_millis()));
        std::fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        root
    }

    #[test]
    fn records_operational_failure_as_open_closure_blocking_claim() -> Result<()> {
        let root = temp_root("operational-failure");
        let request = SliceVerificationRequest {
            conversation_id: 7,
            goal: "Repair the API rollout".to_string(),
            prompt: "Restart the service and verify the HTTP health endpoint.".to_string(),
            preview: "Repair rollout".to_string(),
            source_label: "queue".to_string(),
            owner_visible: false,
        };
        let review_outcome = review::ReviewOutcome {
            required: true,
            verdict: review::ReviewVerdict::Fail,
            mission_state: "UNHEALTHY".to_string(),
            summary: "HTTP health check still returns 502.".to_string(),
            report: "VERDICT: FAIL".to_string(),
            score: 4,
            reasons: vec![
                "closure_claim".to_string(),
                "runtime_or_infra_change".to_string(),
            ],
            failed_gates: vec!["health endpoint".to_string()],
            semantic_findings: vec!["Rollout is still unhealthy.".to_string()],
            categorized_findings: Vec::new(),
            open_items: vec!["Repair the failing health endpoint.".to_string()],
            evidence: vec!["curl /health => 502".to_string()],
            handoff: None,
            disposition: review::ReviewDisposition::Send,
            pipeline_resolution: None,
        };

        let recorded = record_slice_assurance(
            &root,
            &request,
            "Restarted the service and completed the rollout.",
            None,
            Some(&review_outcome),
        )?;

        assert!(recorded.run.open_claim_count >= 1);
        assert!(recorded.run.closure_blocking_claim_count >= 1);
        assert!(!recorded.closure_blocking_open_items().is_empty());

        let engine = lcm::LcmEngine::open(
            &root.join("runtime/ctox.sqlite3"),
            lcm::LcmConfig::default(),
        )?;
        let assurance = engine.mission_assurance_snapshot(7)?;
        assert_eq!(
            assurance.latest_run.as_ref().map(|run| run.run_id.clone()),
            Some(recorded.run.run_id)
        );
        assert!(!assurance.closure_blocking_claims.is_empty());
        Ok(())
    }

    #[test]
    fn artifact_report_without_closure_claim_does_not_block_closure() -> Result<()> {
        let root = temp_root("artifact-report");
        let request = SliceVerificationRequest {
            conversation_id: 11,
            goal: "Refine helper internals".to_string(),
            prompt: "Refactor the helper module and leave the mission open.".to_string(),
            preview: "Helper refactor".to_string(),
            source_label: "queue".to_string(),
            owner_visible: false,
        };

        let recorded = record_slice_assurance(
            &root,
            &request,
            "Refactored src/helper.rs and updated the contract comments.",
            None,
            Some(&review::ReviewOutcome::skipped(
                "No completion review gate triggered.",
            )),
        )?;

        assert!(recorded.run.claim_count >= 1);
        assert_eq!(recorded.run.closure_blocking_claim_count, 0);
        assert!(recorded.closure_blocking_open_items().is_empty());
        Ok(())
    }

    #[test]
    fn synthesizes_full_raw_report_when_review_outcome_report_is_blank() -> Result<()> {
        let root = temp_root("verification-synthesizes-raw-report");
        let request = SliceVerificationRequest {
            conversation_id: 15,
            goal: "Reset the buyer-facing product surface".to_string(),
            prompt: "Rework the owner-visible surface against the active mission.".to_string(),
            preview: "Homepage reset".to_string(),
            source_label: "queue".to_string(),
            owner_visible: true,
        };
        let review_outcome = review::ReviewOutcome {
            required: true,
            verdict: review::ReviewVerdict::Fail,
            mission_state: "UNHEALTHY".to_string(),
            summary: "The public surface still reads like internal process copy.".to_string(),
            report: String::new(),
            score: 5,
            reasons: vec!["owner_visible_claim".to_string()],
            failed_gates: vec!["Mission fit".to_string()],
            semantic_findings: vec![
                "Homepage still behaves like a brochure instead of a platform.".to_string(),
            ],
            categorized_findings: Vec::new(),
            open_items: vec!["Persist active strategy, then rework the buyer path.".to_string()],
            evidence: vec!["GET / => static shell".to_string()],
            handoff: None,
            disposition: review::ReviewDisposition::Send,
            pipeline_resolution: None,
        };

        let recorded = record_slice_assurance(
            &root,
            &request,
            "Implemented another owner-visible homepage slice.",
            None,
            Some(&review_outcome),
        )?;

        assert!(recorded.run.raw_report.contains("VERDICT: FAIL"));
        assert!(recorded.run.raw_report.contains("FINDINGS:"));
        assert!(recorded
            .run
            .raw_report
            .contains("Homepage still behaves like a brochure instead of a platform."));
        assert!(!recorded.run.report_excerpt.trim().is_empty());
        Ok(())
    }

    #[test]
    fn claim_set_persists_manual_claim_to_canonical_runtime_db() -> Result<()> {
        let root = temp_root("claim-set");
        let args = vec![
            "claim-set".to_string(),
            "--conversation-id".to_string(),
            "42".to_string(),
            "--kind".to_string(),
            "design_artifact".to_string(),
            "--status".to_string(),
            "verified".to_string(),
            "--subject".to_string(),
            "five_front_door_mockups_delivered".to_string(),
            "--summary".to_string(),
            "Five front-door mockups were delivered.".to_string(),
            "--evidence".to_string(),
            "Gallery plus five HTML mockup files exist.".to_string(),
        ];

        handle_verification_command(&root, &args)?;

        let engine = lcm::LcmEngine::open(
            &root.join("runtime/ctox.sqlite3"),
            lcm::LcmConfig::default(),
        )?;
        let claims = engine.list_mission_claims(42, true, 10)?;
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].claim_kind, "design_artifact");
        assert_eq!(claims[0].claim_status, "verified");
        assert_eq!(claims[0].subject, "five_front_door_mockups_delivered");
        Ok(())
    }
}
