use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

use crate::channels;
use crate::context_health;
use crate::governance;
use crate::inference::runtime_plan;
use crate::lcm;
use crate::plan;
use crate::schedule;
use crate::tickets;

pub(crate) const MAX_RENDERED_SUMMARY_ITEMS: usize = 8;
pub(crate) const MAX_RENDERED_MESSAGE_ITEMS: usize = 8;
pub(crate) const MAX_RENDERED_CONTEXT_CHARS: usize = 8_000;
const MAX_CONTINUITY_BLOCK_LINES: usize = 10;
const MAX_CONTINUITY_BLOCK_CHARS: usize = 900;
const CTOX_CHAT_SYSTEM_PROMPT: &str =
    include_str!("../../assets/prompts/ctox_chat_system_prompt.md");
const CTOX_DEFAULT_CTO_OPERATING_MODE: &str =
    include_str!("../../assets/prompts/ctox_cto_operating_mode.md");
const CTOX_CTO_OPERATING_MODE_KEY: &str = "CTOX_CTO_OPERATING_MODE_PROMPT";

#[derive(Debug, Clone, Default, Serialize)]
pub struct PromptContextBreakdown {
    pub system_prompt_chars: usize,
    pub focus_chars: usize,
    pub verified_evidence_chars: usize,
    pub workflow_state_chars: usize,
    pub anchors_chars: usize,
    pub narrative_chars: usize,
    pub governance_chars: usize,
    pub context_health_chars: usize,
    pub conversation_chars: usize,
    pub latest_user_turn_chars: usize,
    pub wrapper_chars: usize,
    pub rendered_context_items: usize,
    pub omitted_context_items: usize,
    pub total_ctox_prompt_chars: usize,
}

impl PromptContextBreakdown {
    pub fn continuity_chars(&self) -> usize {
        self.focus_chars + self.anchors_chars + self.narrative_chars
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LivePromptArtifact {
    pub conversation_id: i64,
    pub model: String,
    pub system_prompt: String,
    pub runtime_prompt: String,
    pub combined_review_prompt: String,
    pub breakdown: PromptContextBreakdown,
}

impl LivePromptArtifact {
    pub fn to_review_markdown(&self) -> String {
        format!(
            "# CTOX Live Prompt Review Artifact\n\nconversation_id: {}\nmodel: {}\n\n## Breakdown\n- system_prompt_chars: {}\n- latest_user_turn_chars: {}\n- verified_evidence_chars: {}\n- anchors_chars: {}\n- focus_chars: {}\n- workflow_state_chars: {}\n- narrative_chars: {}\n- governance_chars: {}\n- context_health_chars: {}\n- conversation_chars: {}\n- wrapper_chars: {}\n- rendered_context_items: {}\n- omitted_context_items: {}\n- total_ctox_prompt_chars: {}\n\n## System Prompt\n```md\n{}\n```\n\n## Runtime Prompt\n```md\n{}\n```\n\n## Combined Review View\n```md\n{}\n```\n",
            self.conversation_id,
            self.model,
            self.breakdown.system_prompt_chars,
            self.breakdown.latest_user_turn_chars,
            self.breakdown.verified_evidence_chars,
            self.breakdown.anchors_chars,
            self.breakdown.focus_chars,
            self.breakdown.workflow_state_chars,
            self.breakdown.narrative_chars,
            self.breakdown.governance_chars,
            self.breakdown.context_health_chars,
            self.breakdown.conversation_chars,
            self.breakdown.wrapper_chars,
            self.breakdown.rendered_context_items,
            self.breakdown.omitted_context_items,
            self.breakdown.total_ctox_prompt_chars,
            self.system_prompt,
            self.runtime_prompt,
            self.combined_review_prompt
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderedRuntimePrompt {
    pub prompt: String,
    pub latest_user_prompt: String,
    pub rendered_context_items: usize,
    pub omitted_context_items: usize,
}

pub fn render_system_prompt(root: &Path, settings: &BTreeMap<String, String>) -> Result<String> {
    render_system_prompt_for_skill_preset(
        root,
        settings,
        crate::inference::runtime_state::ChatSkillPreset::Standard,
    )
}

pub fn render_system_prompt_for_skill_preset(
    root: &Path,
    settings: &BTreeMap<String, String>,
    _preset: crate::inference::runtime_state::ChatSkillPreset,
) -> Result<String> {
    render_system_prompt_template(root, settings, CTOX_CHAT_SYSTEM_PROMPT)
}

fn render_system_prompt_template(
    root: &Path,
    settings: &BTreeMap<String, String>,
    template: &str,
) -> Result<String> {
    let owner = channels::load_prompt_identity(root, settings)?;
    let channels_block = owner.channels.join("\n");
    let preferred_channel = owner
        .preferred_channel
        .unwrap_or_else(|| "not set".to_string());
    let owner_email = owner
        .owner_email_address
        .unwrap_or_else(|| "not configured".to_string());
    let founder_emails = if owner.founder_email_addresses.is_empty() {
        "not configured".to_string()
    } else {
        owner.founder_email_addresses.join(", ")
    };
    let allowed_email_domain = owner
        .allowed_email_domain
        .unwrap_or_else(|| "not configured".to_string());
    let admin_email_policies = owner.admin_email_policies.join("\n");
    let cto_operating_mode = resolve_cto_operating_mode(settings);
    Ok(strip_prompt_comments(template)
        .replace("{{OWNER_NAME}}", &owner.owner_name)
        .replace("{{OWNER_CHANNELS}}", &channels_block)
        .replace("{{OWNER_EMAIL_ADDRESS}}", &owner_email)
        .replace("{{FOUNDER_EMAIL_ADDRESSES}}", &founder_emails)
        .replace("{{OWNER_EMAIL_DOMAIN}}", &allowed_email_domain)
        .replace("{{OWNER_EMAIL_ADMINS}}", &admin_email_policies)
        .replace("{{OWNER_PREFERRED_CHANNEL}}", &preferred_channel)
        .replace("{{CTO_OPERATING_MODE_BLOCK}}", &cto_operating_mode))
}

fn resolve_cto_operating_mode(settings: &BTreeMap<String, String>) -> String {
    settings
        .get(CTOX_CTO_OPERATING_MODE_KEY)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| CTOX_DEFAULT_CTO_OPERATING_MODE.trim().to_string())
}

pub fn render_runtime_prompt(
    root: &Path,
    snapshot: &lcm::LcmSnapshot,
    continuity: &lcm::ContinuityShowAll,
    mission_state: &lcm::MissionStateRecord,
    mission_assurance: &lcm::MissionAssuranceSnapshot,
    governance_snapshot: &governance::GovernancePromptSnapshot,
    health: &context_health::ContextHealthSnapshot,
    suggested_skill: Option<&str>,
) -> Result<RenderedRuntimePrompt> {
    let prompt_view = build_prompt_snapshot_view(snapshot);
    let latest_user_prompt = prompt_view.latest_user_prompt.clone();
    let runtime_blocks = derive_prompt_runtime_blocks(
        root,
        &prompt_view.snapshot,
        continuity,
        mission_state,
        mission_assurance,
        &latest_user_prompt,
    )?;
    let rendered_context = select_rendered_context(
        &prompt_view.snapshot,
        prompt_view.latest_user_message_id,
        prompt_view.mission_start_seq,
    );
    let prompt = render_chat_prompt(
        root,
        &runtime_blocks,
        governance_snapshot,
        health,
        &latest_user_prompt,
        &rendered_context,
        suggested_skill,
    );
    Ok(RenderedRuntimePrompt {
        prompt,
        latest_user_prompt,
        rendered_context_items: rendered_context.entries.len(),
        omitted_context_items: rendered_context.omitted_items,
    })
}

pub fn prompt_context_breakdown(
    root: &Path,
    system_prompt: &str,
    snapshot: &lcm::LcmSnapshot,
    continuity: &lcm::ContinuityShowAll,
    mission_state: &lcm::MissionStateRecord,
    mission_assurance: &lcm::MissionAssuranceSnapshot,
    governance_snapshot: &governance::GovernancePromptSnapshot,
    health: &context_health::ContextHealthSnapshot,
) -> Result<PromptContextBreakdown> {
    let prompt_view = build_prompt_snapshot_view(snapshot);
    let latest_user_prompt = prompt_view.latest_user_prompt.as_str();
    let runtime_blocks = derive_prompt_runtime_blocks(
        root,
        &prompt_view.snapshot,
        continuity,
        mission_state,
        mission_assurance,
        latest_user_prompt,
    )?;
    let rendered_context = select_rendered_context(
        &prompt_view.snapshot,
        prompt_view.latest_user_message_id,
        prompt_view.mission_start_seq,
    );
    let focus_block = runtime_blocks.focus.clone();
    let anchors_block = runtime_blocks.anchors.clone();
    let narrative_block = runtime_blocks.narrative.clone();
    let verified_evidence_block = runtime_blocks.verified_evidence.clone();
    let workflow_state_block = runtime_blocks.workflow_state.clone();
    let governance_block = governance::render_prompt_block(governance_snapshot);
    let health_block = context_health::render_prompt_block(health);
    let latest_user_turn = format!(
        "Latest user turn:\ncontent: {}",
        sanitize_context_message(latest_user_prompt)
    );
    let context_notice_chars = render_context_notice(rendered_context.omitted_items)
        .map(|line| line.len() + 1)
        .unwrap_or(0);
    let rendered_context_chars = rendered_context
        .entries
        .iter()
        .map(|entry| entry.len() + 2)
        .sum::<usize>()
        + rendered_context.entries.len().saturating_sub(1)
        + context_notice_chars;
    let rendered_prompt = render_chat_prompt(
        root,
        &runtime_blocks,
        governance_snapshot,
        health,
        latest_user_prompt,
        &rendered_context,
        None,
    );
    let known_dynamic_chars = narrative_block.len()
        + anchors_block.len()
        + focus_block.len()
        + verified_evidence_block.len()
        + workflow_state_block.len()
        + governance_block.len()
        + health_block.len()
        + rendered_context_chars
        + latest_user_turn.len();
    Ok(PromptContextBreakdown {
        system_prompt_chars: system_prompt.len(),
        focus_chars: focus_block.len(),
        verified_evidence_chars: verified_evidence_block.len(),
        workflow_state_chars: workflow_state_block.len(),
        anchors_chars: anchors_block.len(),
        narrative_chars: narrative_block.len(),
        governance_chars: governance_block.len(),
        context_health_chars: health_block.len(),
        conversation_chars: rendered_context_chars,
        latest_user_turn_chars: latest_user_turn.len(),
        wrapper_chars: rendered_prompt.len().saturating_sub(known_dynamic_chars),
        rendered_context_items: rendered_context.entries.len(),
        omitted_context_items: rendered_context.omitted_items,
        total_ctox_prompt_chars: system_prompt.len() + rendered_prompt.len(),
    })
}

pub fn prompt_context_breakdown_for_runtime(
    root: &Path,
    settings: &BTreeMap<String, String>,
    snapshot: &lcm::LcmSnapshot,
    continuity: &lcm::ContinuityShowAll,
    mission_state: &lcm::MissionStateRecord,
    mission_assurance: &lcm::MissionAssuranceSnapshot,
    governance_snapshot: &governance::GovernancePromptSnapshot,
    health: &context_health::ContextHealthSnapshot,
) -> Result<PromptContextBreakdown> {
    let system_prompt = render_system_prompt(root, settings)?;
    prompt_context_breakdown(
        root,
        &system_prompt,
        snapshot,
        continuity,
        mission_state,
        mission_assurance,
        governance_snapshot,
        health,
    )
}

pub fn build_live_prompt_artifact(
    root: &Path,
    system_prompt: &str,
    model: &str,
    snapshot: &lcm::LcmSnapshot,
    continuity: &lcm::ContinuityShowAll,
    mission_state: &lcm::MissionStateRecord,
    mission_assurance: &lcm::MissionAssuranceSnapshot,
    governance_snapshot: &governance::GovernancePromptSnapshot,
    health: &context_health::ContextHealthSnapshot,
) -> Result<LivePromptArtifact> {
    let runtime_prompt = render_runtime_prompt(
        root,
        snapshot,
        continuity,
        mission_state,
        mission_assurance,
        governance_snapshot,
        health,
        None,
    )?;
    let breakdown = prompt_context_breakdown(
        root,
        system_prompt,
        snapshot,
        continuity,
        mission_state,
        mission_assurance,
        governance_snapshot,
        health,
    )?;
    let combined_review_prompt = format!(
        "<SYSTEM_PROMPT>\n{}\n</SYSTEM_PROMPT>\n\n<RUNTIME_PROMPT>\n{}\n</RUNTIME_PROMPT>",
        system_prompt, runtime_prompt.prompt
    );
    Ok(LivePromptArtifact {
        conversation_id: snapshot.conversation_id,
        model: model.to_string(),
        system_prompt: system_prompt.to_string(),
        runtime_prompt: runtime_prompt.prompt,
        combined_review_prompt,
        breakdown,
    })
}

pub fn render_live_prompt_artifact(
    root: &Path,
    settings: &BTreeMap<String, String>,
    model: &str,
    db_path: &Path,
    conversation_id: i64,
) -> Result<LivePromptArtifact> {
    let engine = lcm::LcmEngine::open(db_path, lcm::LcmConfig::default())?;
    let _ = engine.continuity_init_documents(conversation_id)?;
    let snapshot = engine.snapshot(conversation_id)?;
    let continuity = engine.continuity_show_all(conversation_id)?;
    let mission_state = engine.mission_state(conversation_id)?;
    let mission_assurance = engine.mission_assurance_snapshot(conversation_id)?;
    let forgotten_entries = engine.continuity_forgotten(conversation_id, None, None)?;
    let prompt_view = build_prompt_snapshot_view(&snapshot);
    let latest_user_prompt = prompt_view.latest_user_prompt;
    let health = context_health::assess_with_forgotten(
        &snapshot,
        &continuity,
        &forgotten_entries,
        &latest_user_prompt,
        settings
            .get("CTOX_CHAT_MODEL_MAX_CONTEXT")
            .and_then(|value| runtime_plan::parse_chat_context_tokens(value))
            .unwrap_or_else(runtime_plan::default_chat_context_tokens) as i64,
    );
    let governance_snapshot =
        governance::prompt_snapshot(root, conversation_id).unwrap_or_default();
    let system_prompt = render_system_prompt(root, settings)?;
    build_live_prompt_artifact(
        root,
        &system_prompt,
        model,
        &snapshot,
        &continuity,
        &mission_state,
        &mission_assurance,
        &governance_snapshot,
        &health,
    )
}

pub(crate) fn render_chat_prompt(
    root: &Path,
    runtime_blocks: &PromptRuntimeBlocks,
    governance_snapshot: &governance::GovernancePromptSnapshot,
    health: &context_health::ContextHealthSnapshot,
    latest_user_prompt: &str,
    rendered_context: &RenderedContextSelection,
    suggested_skill: Option<&str>,
) -> String {
    let mut lines = vec![
        "Latest user turn:".to_string(),
        format!("content: {}", sanitize_context_message(latest_user_prompt)),
        String::new(),
        runtime_blocks.verified_evidence.clone(),
        String::new(),
        runtime_blocks.anchors.clone(),
        String::new(),
        runtime_blocks.focus.clone(),
        String::new(),
        render_execution_contract_block(
            &runtime_blocks.focus,
            &runtime_blocks.anchors,
            &runtime_blocks.workflow_state,
        ),
        String::new(),
        runtime_blocks.workflow_state.clone(),
        String::new(),
        runtime_blocks.narrative.clone(),
        String::new(),
        governance::render_prompt_block(governance_snapshot),
        String::new(),
        render_autonomy_policy_block(root),
        String::new(),
        context_health::render_prompt_block(health),
        String::new(),
        "Conversation:".to_string(),
    ];
    if let Some(skill_block) = render_skill_dispatch_block(suggested_skill) {
        lines.splice(3..3, [skill_block, String::new()]);
    }
    for entry in &rendered_context.entries {
        lines.push(format!("- {entry}"));
    }
    if rendered_context.entries.is_empty() {
        lines.push("- none".to_string());
    }
    if let Some(notice) = render_context_notice(rendered_context.omitted_items) {
        lines.push(notice);
    }
    lines.join("\n")
}

/// Runtime block injected into every chat turn to tell the model how
/// eagerly it should escalate decisions via approval-gate self-work
/// items. The text varies with `CTOX_AUTONOMY_LEVEL` (progressive /
/// balanced / defensive); the service propagates that variable from
/// the persisted runtime config at boot.
fn render_autonomy_policy_block(root: &Path) -> String {
    let level = crate::autonomy::AutonomyLevel::from_root(root);
    format!(
        "Autonomy policy:\nlevel: {}\npolicy: {}",
        level,
        level.runtime_policy_block()
    )
}

fn render_skill_dispatch_block(suggested_skill: Option<&str>) -> Option<String> {
    let skill = suggested_skill
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(format!(
        "Suggested skill dispatch:\n- preferred_skill: {skill}\n- expectation: use this skill first if it matches the work; if it does not fit, say why and continue within the normal CTOX loop"
    ))
}

pub(crate) fn continuity_block(label: &str, content: &str) -> String {
    let lines = match label {
        "Focus" => focus_block_lines(content),
        _ => compact_continuity_lines(content),
    };
    if lines.is_empty() {
        format!("{label}:\nitems: []")
    } else {
        let mut rendered = vec![format!("{label}:")];
        rendered.extend(lines);
        rendered.join("\n")
    }
}

fn render_context_notice(omitted_items: usize) -> Option<String> {
    (omitted_items > 0).then(|| format!("omitted_items: {omitted_items}"))
}

fn workflow_state_has_open_runtime_work(workflow_state_block: &str) -> bool {
    for names in [
        &["current_queue_item_id", "current queue item id"][..],
        &["current_plan_item_id", "current plan item id"][..],
        &["current_ticket_case_id", "current ticket case id"][..],
    ] {
        if let Some(value) = continuity_named_value(workflow_state_block, names) {
            if !value.eq_ignore_ascii_case("null") {
                return true;
            }
        }
    }
    false
}

fn render_execution_contract_block(
    focus_block: &str,
    anchors_block: &str,
    workflow_state_block: &str,
) -> String {
    let mut lines = vec!["What to do this turn:".to_string()];
    if let Some(goal) = continuity_named_value(focus_block, &["goal", "mission", "main task"]) {
        lines.push(format!("- Task: {goal}"));
    }
    if let Some(done_gate) =
        continuity_named_value(focus_block, &["done_gate", "done gate", "finish rule"])
    {
        lines.push(format!("- Finish only when: {done_gate}"));
    } else {
        lines.push(
            "- Finish only when: the required files are correct and the runtime state is correct"
                .to_string(),
        );
    }
    if let Some(next_slice) =
        continuity_named_value(focus_block, &["next_slice", "next slice", "next step"])
    {
        lines.push(format!("- If not finished, do this next: {next_slice}"));
    }
    if let Some(workspace_root) = continuity_named_value(anchors_block, &["workspace_root"]) {
        lines.push(format!(
            "- Only files under {workspace_root} count for this turn"
        ));
    }
    if workflow_state_has_open_runtime_work(workflow_state_block) {
        lines.push(
            "- Open CTOX work already exists below. Keep it accurate. Do not replace it with a sentence in the reply or a note file."
                .to_string(),
        );
    } else {
        lines.push(
            "- If work remains after this turn, create exactly one open CTOX plan or queue item before you finish. A reply or file note does not count as open work."
                .to_string(),
        );
    }
    lines.join("\n")
}

fn focus_block_lines(content: &str) -> Vec<String> {
    let fields = [
        ("Mission id", &["mission_id", "mission id"][..]),
        ("Turn type", &["turn_class", "turn class"][..]),
        ("Read scope", &["read_scope", "read scope"][..]),
        ("Main task", &["goal", "mission"][..]),
        ("Current step", &["slice_id", "slice"][..]),
        (
            "Current status",
            &["status", "slice_state", "slice state"][..],
        ),
        (
            "Verification gap",
            &["verification_gap", "verification gap"][..],
        ),
        ("Current blocker", &["blocker", "current blocker"][..]),
        ("Next step", &["next_slice", "next slice"][..]),
        ("Finish rule", &["done_gate", "done gate"][..]),
    ];
    let mut lines = Vec::new();
    for (label, names) in fields {
        if let Some(value) = continuity_named_value(content, names) {
            lines.push(format!("- {label}: {value}"));
        }
    }
    if lines.is_empty() {
        vec!["- Current status: unknown".to_string()]
    } else {
        lines
    }
}

fn compact_continuity_lines(content: &str) -> Vec<String> {
    // Collect all candidate lines first, then take the *last* N that fit
    // within the char/line budget so that the most recent entries (typically
    // appended at the end of continuity documents) are preserved.
    let mut candidates = Vec::new();
    for raw_line in content.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let normalized = trimmed.trim_start_matches("- ").trim();
        if normalized.is_empty() {
            continue;
        }
        let rendered = if let Some((key, value)) = normalized.split_once(':') {
            let value = collapse_spaces(value.trim());
            if value.is_empty() {
                continue;
            }
            format!("{}: {}", key.trim(), value)
        } else {
            let value = collapse_spaces(normalized);
            if value.eq_ignore_ascii_case("none") {
                continue;
            }
            format!("- {value}")
        };
        candidates.push(rendered);
    }

    let mut selected = Vec::new();
    let mut total_chars = 0usize;
    for line in candidates.iter().rev() {
        if selected.len() >= MAX_CONTINUITY_BLOCK_LINES {
            break;
        }
        if total_chars + line.len() > MAX_CONTINUITY_BLOCK_CHARS {
            break;
        }
        total_chars += line.len();
        selected.push(line.clone());
    }
    selected.reverse();
    selected
}

fn continuity_named_value(content: &str, names: &[&str]) -> Option<String> {
    for raw_line in content.lines() {
        let trimmed = raw_line.trim().trim_start_matches("- ").trim();
        if let Some((prefix, value)) = trimmed.split_once(':') {
            if names
                .iter()
                .any(|name| prefix.trim().eq_ignore_ascii_case(name))
            {
                let value = collapse_spaces(value.trim());
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }
    }
    None
}

fn collapse_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Clone)]
pub(crate) struct RenderedContextSelection {
    pub(crate) entries: Vec<String>,
    pub(crate) omitted_items: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct PromptRuntimeBlocks {
    pub(crate) focus: String,
    pub(crate) anchors: String,
    pub(crate) narrative: String,
    pub(crate) verified_evidence: String,
    pub(crate) workflow_state: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PromptSnapshotView {
    pub(crate) snapshot: lcm::LcmSnapshot,
    pub(crate) latest_user_prompt: String,
    pub(crate) latest_user_message_id: Option<i64>,
    pub(crate) mission_start_seq: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MissionContext {
    pub(crate) mission_id: Option<String>,
    pub(crate) start_seq: Option<i64>,
    pub(crate) workspace_root: Option<String>,
    pub(crate) main_objective: Option<String>,
    pub(crate) report_cycle: Option<String>,
    pub(crate) progress_artifact: Option<String>,
    pub(crate) report_headings: Vec<String>,
    pub(crate) non_negotiable_rules: Vec<String>,
    pub(crate) previous_blocker: Option<String>,
    pub(crate) turn_class: String,
    pub(crate) read_scope: String,
    pub(crate) verification_gap: Option<String>,
}

pub(crate) fn build_prompt_snapshot_view(snapshot: &lcm::LcmSnapshot) -> PromptSnapshotView {
    let latest_user_index = snapshot
        .messages
        .iter()
        .rposition(|message| message.role.trim().eq_ignore_ascii_case("user"));
    let latest_user_prompt = latest_user_index
        .and_then(|index| snapshot.messages.get(index))
        .map(|message| message.content.clone())
        .unwrap_or_default();
    let latest_user_message_id = latest_user_index
        .and_then(|index| snapshot.messages.get(index))
        .map(|message| message.message_id);

    let mut trimmed = snapshot.clone();
    if let Some(index) = latest_user_index {
        trimmed.messages.truncate(index + 1);
        let allowed_message_ids = trimmed
            .messages
            .iter()
            .map(|message| message.message_id)
            .collect::<BTreeSet<_>>();
        trimmed.context_items.retain(|item| match item.item_type {
            lcm::ContextItemType::Message => item
                .message_id
                .map(|message_id| allowed_message_ids.contains(&message_id))
                .unwrap_or(false),
            lcm::ContextItemType::Summary => true,
        });
    }
    let mission_context = derive_mission_context(&trimmed.messages, &latest_user_prompt);

    PromptSnapshotView {
        snapshot: trimmed,
        latest_user_prompt,
        latest_user_message_id,
        mission_start_seq: mission_context.start_seq,
    }
}

pub(crate) fn select_rendered_context(
    snapshot: &lcm::LcmSnapshot,
    latest_user_message_id: Option<i64>,
    mission_start_seq: Option<i64>,
) -> RenderedContextSelection {
    let mut summary_lines = Vec::new();
    let mut message_lines = Vec::new();
    for item in &snapshot.context_items {
        match item.item_type {
            lcm::ContextItemType::Message => {
                if let Some(message_id) = item.message_id {
                    if Some(message_id) == latest_user_message_id {
                        continue;
                    }
                    if let Some(message) = snapshot
                        .messages
                        .iter()
                        .find(|entry| entry.message_id == message_id)
                    {
                        if mission_start_seq
                            .map(|start_seq| message.seq < start_seq)
                            .unwrap_or(false)
                        {
                            continue;
                        }
                        message_lines.push(render_context_message(&message.role, &message.content));
                    }
                }
            }
            lcm::ContextItemType::Summary => {
                if mission_start_seq.is_some() {
                    continue;
                }
                if let Some(summary_id) = item.summary_id.as_deref() {
                    if let Some(summary) = snapshot
                        .summaries
                        .iter()
                        .find(|entry| entry.summary_id == summary_id)
                    {
                        summary_lines.push(format!("summary: {}", summary.content));
                    }
                }
            }
        }
    }

    let summary_start = summary_lines
        .len()
        .saturating_sub(MAX_RENDERED_SUMMARY_ITEMS);
    let selected_summaries = summary_lines[summary_start..].to_vec();
    let mut entries = selected_summaries.clone();
    let mut seen = BTreeSet::new();
    let mut selected_messages = Vec::new();
    let mut total_chars = entries.iter().map(|line| line.len()).sum::<usize>();
    let mut omitted_messages = 0usize;

    for line in message_lines.iter().rev() {
        if selected_messages.len() >= MAX_RENDERED_MESSAGE_ITEMS {
            omitted_messages += 1;
            continue;
        }
        if !seen.insert(line.clone()) {
            omitted_messages += 1;
            continue;
        }
        let projected = total_chars + line.len();
        if !selected_messages.is_empty() && projected > MAX_RENDERED_CONTEXT_CHARS {
            omitted_messages += 1;
            continue;
        }
        total_chars = projected;
        selected_messages.push(line.clone());
    }
    selected_messages.reverse();
    entries.extend(selected_messages);

    let omitted_summaries = summary_start;
    RenderedContextSelection {
        entries,
        omitted_items: omitted_summaries + omitted_messages,
    }
}

fn derive_prompt_runtime_blocks(
    root: &Path,
    snapshot: &lcm::LcmSnapshot,
    continuity: &lcm::ContinuityShowAll,
    mission_state: &lcm::MissionStateRecord,
    mission_assurance: &lcm::MissionAssuranceSnapshot,
    latest_user_prompt: &str,
) -> Result<PromptRuntimeBlocks> {
    let mission_context = derive_mission_context(&snapshot.messages, latest_user_prompt);
    let override_continuity = mission_context
        .workspace_root
        .as_ref()
        .map(|workspace| {
            !continuity.focus.content.contains(workspace)
                || continuity.anchors.content.contains("rust-blog-feed")
                || continuity.anchors.content.contains("planet-python-feed")
        })
        .unwrap_or(false);

    let focus = if override_continuity {
        synthesize_focus_block(&mission_context)
    } else {
        render_focus_block_from_sources(continuity, mission_state)
    };
    let anchors = if override_continuity {
        synthesize_anchor_block(&mission_context)
    } else {
        continuity_block("Anchors", &continuity.anchors.content)
    };
    let narrative = if override_continuity {
        synthesize_narrative_block(&mission_context)
    } else {
        continuity_block("Narrative", &continuity.narrative.content)
    };
    let verified_evidence = render_verified_evidence_block(mission_assurance, override_continuity);
    let workflow_state = render_workflow_state_block(root, &mission_context)?;

    Ok(PromptRuntimeBlocks {
        focus,
        anchors,
        narrative,
        verified_evidence,
        workflow_state,
    })
}

fn render_focus_block_from_sources(
    continuity: &lcm::ContinuityShowAll,
    mission_state: &lcm::MissionStateRecord,
) -> String {
    let mut lines = focus_block_lines(&continuity.focus.content);
    if lines.len() == 1 && lines.first().map(String::as_str) == Some("status: unknown") {
        lines = synthesize_focus_lines_from_mission_state(mission_state);
    }
    if lines.is_empty() {
        "Focus:\n- none".to_string()
    } else {
        let mut rendered = vec!["Focus:".to_string()];
        rendered.extend(lines);
        rendered.join("\n")
    }
}

fn synthesize_focus_lines_from_mission_state(
    mission_state: &lcm::MissionStateRecord,
) -> Vec<String> {
    let mut lines = Vec::new();
    if !mission_state.mission.trim().is_empty() {
        lines.push(format!(
            "- Main task: {}",
            collapse_spaces(mission_state.mission.trim())
        ));
    }
    if !mission_state.mission_status.trim().is_empty() {
        lines.push(format!(
            "- Current status: {}",
            collapse_spaces(mission_state.mission_status.trim())
        ));
    }
    if !mission_state.blocker.trim().is_empty() {
        lines.push(format!(
            "- Current blocker: {}",
            collapse_spaces(mission_state.blocker.trim())
        ));
    }
    if !mission_state.next_slice.trim().is_empty() {
        lines.push(format!(
            "- Next step: {}",
            collapse_spaces(mission_state.next_slice.trim())
        ));
    }
    if !mission_state.done_gate.trim().is_empty() {
        lines.push(format!(
            "- Finish rule: {}",
            collapse_spaces(mission_state.done_gate.trim())
        ));
    }
    if lines.is_empty() {
        vec!["- Current status: unknown".to_string()]
    } else {
        lines
    }
}

pub(crate) fn derive_mission_context(
    messages: &[lcm::MessageRecord],
    latest_user_prompt: &str,
) -> MissionContext {
    let mut context = MissionContext {
        turn_class: "execute".to_string(),
        read_scope: "narrow".to_string(),
        ..MissionContext::default()
    };
    let latest_user_seq = messages
        .iter()
        .rev()
        .find(|message| {
            message.role.eq_ignore_ascii_case("user") && message.content == latest_user_prompt
        })
        .map(|message| message.seq);
    let mission_bootstrap = messages.iter().rev().find(|message| {
        message.role.eq_ignore_ascii_case("user") && looks_like_mission_bootstrap(&message.content)
    });
    if let Some(message) = mission_bootstrap {
        context.start_seq = Some(message.seq);
        context.workspace_root =
            extract_block_value(&message.content, "Work only inside this workspace:");
        context.main_objective = extract_block_value(&message.content, "Main objective:");
        context.mission_id = derive_mission_id(
            context.workspace_root.as_deref(),
            context.main_objective.as_deref(),
        );
        context.non_negotiable_rules =
            extract_bullet_list(&message.content, "Non-negotiable rules:");
        context.report_headings = extract_report_headings(&message.content);
    } else if looks_like_progress_report_prompt(latest_user_prompt) {
        context.start_seq = latest_user_seq;
    }
    if looks_like_service_continuation_prompt(latest_user_prompt) {
        context.start_seq = latest_user_seq;
        context.turn_class = "continue".to_string();
        context.read_scope = "narrow".to_string();
    }
    if looks_like_progress_report_prompt(latest_user_prompt) {
        context.turn_class = "report".to_string();
        context.read_scope = "wide".to_string();
        context.report_cycle =
            first_non_empty_line(latest_user_prompt).map(|line| collapse_spaces(line.trim()));
        context.progress_artifact =
            extract_block_value(latest_user_prompt, "First update this file:");
        if context.workspace_root.is_none() {
            context.workspace_root = context.progress_artifact.as_deref().and_then(|path| {
                path.split_once("/ops/progress/")
                    .map(|(root, _)| root.to_string())
            });
        }
        if context.main_objective.is_none() {
            context.main_objective = Some(
                "Continue the active durable mission, re-verify the current workspace state, and return a structured progress report.".to_string(),
            );
        }
        if context.mission_id.is_none() {
            context.mission_id = derive_mission_id(
                context.workspace_root.as_deref(),
                context.main_objective.as_deref(),
            )
            .or_else(|| Some("active_mission".to_string()));
        }
        let headings = extract_reply_headings(latest_user_prompt);
        if !headings.is_empty() {
            context.report_headings = headings;
        }
    }
    context.previous_blocker = messages
        .iter()
        .rev()
        .filter(|message| latest_user_seq.map(|seq| message.seq < seq).unwrap_or(true))
        .find(|message| {
            message.role.eq_ignore_ascii_case("assistant") && message.content.contains("Blocker:")
        })
        .and_then(|message| extract_blocker_line(&message.content));
    context.verification_gap = context
        .previous_blocker
        .as_ref()
        .map(|blocker| format!("workspace state not re-verified after prior {}", blocker));
    context
}

fn derive_mission_id(_workspace_root: Option<&str>, objective: Option<&str>) -> Option<String> {
    objective
        .map(|goal| {
            goal.split(|ch: char| !ch.is_ascii_alphanumeric())
                .filter(|part| !part.is_empty())
                .take(3)
                .map(|part| part.to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join("_")
        })
        .filter(|id| !id.is_empty())
}

fn looks_like_mission_bootstrap(content: &str) -> bool {
    content.contains("Work only inside this workspace:")
        && content.contains("Main objective:")
        && (content.contains("durable mission") || content.contains("long-horizon"))
}

fn looks_like_progress_report_prompt(content: &str) -> bool {
    content.contains("AIRBNB_BENCH_REPORT_CYCLE_")
        || content.contains("MISSION_PROGRESS_REPORT_CYCLE_")
        || (content.contains("Benchmark progress report is due now.")
            && content.contains("First update this file:"))
        || (content.contains("Mission progress report is due now.")
            && content.contains("First update this file:"))
}

fn looks_like_service_continuation_prompt(content: &str) -> bool {
    let trimmed = content.trim();
    trimmed.starts_with("Mission continuity watchdog:")
        || trimmed.starts_with("Mission continuity watchdog detected")
        || trimmed.starts_with("Continue the interrupted CTOX slice")
        || trimmed.starts_with("Continue the interrupted task from the latest durable state")
}

fn extract_block_value(content: &str, header: &str) -> Option<String> {
    let mut lines = content.lines();
    while let Some(line) = lines.next() {
        if line.trim() == header.trim() {
            for candidate in lines.by_ref() {
                let trimmed = candidate.trim();
                if trimmed.is_empty() {
                    continue;
                }
                return Some(collapse_spaces(trimmed));
            }
        }
    }
    None
}

fn extract_bullet_list(content: &str, header: &str) -> Vec<String> {
    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim() == header.trim() {
            let mut out = Vec::new();
            while let Some(candidate) = lines.peek() {
                let trimmed = candidate.trim();
                if trimmed.is_empty() {
                    lines.next();
                    if !out.is_empty() {
                        break;
                    }
                    continue;
                }
                if let Some(value) = trimmed.strip_prefix("- ") {
                    out.push(collapse_spaces(value.trim()));
                    lines.next();
                    continue;
                }
                break;
            }
            return out;
        }
    }
    Vec::new()
}

fn extract_report_headings(content: &str) -> Vec<String> {
    extract_bullet_list(
        content,
        "When later asked for a mission progress report, use these exact headings:",
    )
}

fn extract_reply_headings(content: &str) -> Vec<String> {
    extract_bullet_list(content, "Then reply in chat using these exact headings:")
}

fn first_non_empty_line(content: &str) -> Option<&str> {
    content.lines().map(str::trim).find(|line| !line.is_empty())
}

fn extract_blocker_line(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        line.split_once(':').and_then(|(key, value)| {
            if key.trim().eq_ignore_ascii_case("Blocker") {
                let cleaned = value.trim().trim_matches('`').trim();
                (!cleaned.is_empty()).then(|| collapse_spaces(cleaned))
            } else {
                None
            }
        })
    })
}

fn synthesize_focus_block(context: &MissionContext) -> String {
    let mut lines = vec!["Focus:".to_string()];
    if let Some(mission_id) = &context.mission_id {
        lines.push(format!("- Mission id: {}", mission_id));
    }
    lines.push(format!("- Turn type: {}", context.turn_class));
    lines.push(format!("- Read scope: {}", context.read_scope));
    if let Some(objective) = &context.main_objective {
        lines.push(format!("- Main task: {}", objective));
    }
    if let Some(cycle) = &context.report_cycle {
        lines.push(format!("- Current step: {}", cycle.to_ascii_lowercase()));
        lines.push("- Current status: report_due".to_string());
    } else {
        lines.push("- Current step: initialize_mission_contract".to_string());
        lines.push("- Current status: ready".to_string());
    }
    if let Some(verification_gap) = &context.verification_gap {
        lines.push(format!("- Verification gap: {}", verification_gap));
    } else if let Some(blocker) = &context.previous_blocker {
        lines.push(format!("- Current blocker: {}", blocker));
    } else {
        lines.push("- Current blocker: none".to_string());
    }
    if let Some(progress_artifact) = &context.progress_artifact {
        lines.push(format!(
            "- Next step: inspect {}, update it, then reply",
            compact_workspace_path(progress_artifact, context.workspace_root.as_deref())
        ));
        let done_gate = if context.report_headings.is_empty() {
            format!(
                "the progress file {} is updated and the report is returned",
                compact_workspace_path(progress_artifact, context.workspace_root.as_deref())
            )
        } else {
            format!(
                "the progress file {} is updated and the report is returned using these headings: {}",
                compact_workspace_path(progress_artifact, context.workspace_root.as_deref()),
                context.report_headings.join("|")
            )
        };
        lines.push(format!("- Finish rule: {}", done_gate));
    } else {
        lines.push(
            "- Next step: inspect the workspace and write the first bounded delivery slice"
                .to_string(),
        );
        lines.push(
            "- Finish rule: the roadmap, architecture direction, and first implementation slice are written down durably"
                .to_string(),
        );
    }
    lines.join("\n")
}

pub(crate) fn synthesize_anchor_block(context: &MissionContext) -> String {
    let mut lines = vec!["Anchors:".to_string()];
    if let Some(workspace) = &context.workspace_root {
        lines.push(format!(
            "workspace_root: {}",
            compact_workspace_path(workspace, context.workspace_root.as_deref())
        ));
    }
    if let Some(progress_artifact) = &context.progress_artifact {
        lines.push(format!(
            "canonical_progress_artifact: {}",
            compact_workspace_path(progress_artifact, context.workspace_root.as_deref())
        ));
    }
    if !context.report_headings.is_empty() {
        lines.push(format!(
            "report_headings: {}",
            context.report_headings.join("|")
        ));
    }
    let mut constraint_codes = prioritized_anchor_rules(&context.non_negotiable_rules)
        .into_iter()
        .filter_map(|rule| anchor_code_for_rule(&rule))
        .collect::<Vec<_>>();
    if context.workspace_root.is_some()
        && !constraint_codes
            .iter()
            .any(|code| *code == "work only inside the workspace")
    {
        constraint_codes.insert(0, "work only inside the workspace");
    }
    constraint_codes.truncate(6);
    lines.push("constraints:".to_string());
    if constraint_codes.is_empty() {
        lines.push("[]".to_string());
    } else {
        for code in constraint_codes {
            lines.push(format!("- {}", code));
        }
    }
    lines.join("\n")
}

fn prioritized_anchor_rules(rules: &[String]) -> Vec<String> {
    let priority_needles = [
        "main mission",
        "sidequest",
        "durable next",
        "progress-latest",
        "progress report",
        "explicit docs",
    ];
    let mut scored = rules
        .iter()
        .map(|rule| {
            let normalized = rule.to_ascii_lowercase();
            let score = priority_needles
                .iter()
                .enumerate()
                .find_map(|(index, needle)| normalized.contains(needle).then_some(index))
                .unwrap_or(priority_needles.len());
            (score, rule.clone())
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    scored.dedup_by(|left, right| left.1 == right.1);
    scored.into_iter().map(|(_, rule)| rule).collect()
}

fn anchor_code_for_rule(rule: &str) -> Option<&'static str> {
    let normalized = rule.to_ascii_lowercase();
    if normalized.contains("work only inside") {
        Some("work only inside the workspace")
    } else if normalized.contains("ongoing product-and-operations mission")
        || normalized.contains("ongoing build-and-operate mission")
    {
        Some("treat this as an ongoing mission, not a one-shot task")
    } else if normalized.contains("do not abandon the main mission")
        || normalized.contains("main roadmap intact")
    {
        Some("keep the main mission primary")
    } else if normalized.contains("durable next slices") {
        Some("leave durable next slices")
    } else if normalized.contains("progress-latest.md") && normalized.contains("canonical") {
        Some("use the canonical progress file")
    } else if normalized.contains("progress report")
        && normalized.contains("update")
        && normalized.contains("first")
    {
        Some("update the progress file before the report")
    } else if normalized.contains("explicit docs")
        || normalized.contains("runbooks")
        || normalized.contains("backlog artifacts")
    {
        Some("prefer explicit docs and artifacts")
    } else {
        None
    }
}

fn synthesize_narrative_block(context: &MissionContext) -> String {
    let mut lines = vec!["Narrative:".to_string()];
    if let Some(workspace) = &context.workspace_root {
        lines.push(format!(
            "- durable mission opened in {}",
            compact_workspace_path(workspace, context.workspace_root.as_deref())
        ));
    }
    if let Some(blocker) = &context.previous_blocker {
        lines.push(format!(
            "- previous slice stopped with blocker: {}",
            blocker
        ));
    }
    if let Some(cycle) = &context.report_cycle {
        lines.push(format!("- current turn requests {}", cycle));
    }
    if lines.len() == 1 {
        lines.push("- none".to_string());
    }
    lines.join("\n")
}

fn render_verified_evidence_block(
    mission_assurance: &lcm::MissionAssuranceSnapshot,
    suppress_stale_assurance: bool,
) -> String {
    let mut lines = vec!["Verified evidence:".to_string()];
    if !suppress_stale_assurance {
        if let Some(run) = &mission_assurance.latest_run {
            lines.push(format!(
                "latest_run_source: {}",
                collapse_spaces(run.source_label.trim())
            ));
            if !run.goal.trim().is_empty() {
                lines.push(format!(
                    "latest_run_goal: {}",
                    collapse_spaces(run.goal.trim())
                ));
            }
            if !run.result_excerpt.trim().is_empty() {
                lines.push(format!(
                    "latest_run_result: {}",
                    clip_prompt_text(&collapse_spaces(run.result_excerpt.trim()), 240)
                ));
            }
        }
        if !mission_assurance.closure_blocking_claims.is_empty() {
            for claim in mission_assurance.closure_blocking_claims.iter().take(3) {
                lines.push(format!(
                    "closure_blocking_claim: {}",
                    clip_prompt_text(&collapse_spaces(claim.summary.trim()), 180)
                ));
            }
        }
    }
    if lines.len() == 1 {
        lines.push("items: []".to_string());
    }
    lines.join("\n")
}

fn render_workflow_state_block(root: &Path, context: &MissionContext) -> Result<String> {
    let mut lines = vec!["Open CTOX work that counts right now:".to_string()];
    lines.push(
        "Read this first: queue items count as open work only when their status is pending or leased. Plan items count until they are done. Blocked or failed queue rows are shown separately for context only."
            .to_string(),
    );
    let tasks = channels::list_queue_tasks(root, &[], 8).unwrap_or_default();
    let plans = plan::list_goals(root).unwrap_or_default();
    let schedules = schedule::list_tasks(root).unwrap_or_default();
    let ticket_cases = tickets::list_cases(root, None, 8).unwrap_or_default();
    let task_terms = mission_search_terms(context);

    let matched_schedules = schedules
        .into_iter()
        .filter(|task| task.enabled)
        .filter(|task| {
            workflow_matches_terms(&task.thread_key, &task.name, &task.prompt, &task_terms)
        })
        .take(3)
        .collect::<Vec<_>>();
    let schedule_thread_keys = matched_schedules
        .iter()
        .map(|task| task.thread_key.as_str())
        .collect::<Vec<_>>();
    let visible_queue_tasks = tasks
        .into_iter()
        .filter(|task| {
            matches!(
                task.route_status.as_str(),
                "pending" | "leased" | "blocked" | "failed"
            )
        })
        .filter(|task| {
            workflow_matches_terms(&task.thread_key, &task.title, &task.prompt, &task_terms)
                || schedule_thread_keys
                    .iter()
                    .any(|thread_key| task.thread_key == *thread_key)
        })
        .take(3)
        .collect::<Vec<_>>();
    let (counting_queue_tasks, context_only_queue_tasks): (Vec<_>, Vec<_>) = visible_queue_tasks
        .into_iter()
        .partition(|task| matches!(task.route_status.as_str(), "pending" | "leased"));
    let matched_plans = plans
        .into_iter()
        .filter(|goal| !matches!(goal.status.as_str(), "done" | "cancelled" | "archived"))
        .filter(|goal| {
            workflow_matches_terms(
                &goal.thread_key,
                &goal.title,
                &goal.source_prompt,
                &task_terms,
            ) || schedule_thread_keys
                .iter()
                .any(|thread_key| goal.thread_key == *thread_key)
        })
        .take(3)
        .collect::<Vec<_>>();
    let matched_ticket_cases = ticket_cases
        .into_iter()
        .filter(|case| !matches!(case.state.as_str(), "closed"))
        .filter(|case| {
            workflow_matches_terms(
                &case.ticket_key,
                &case.label,
                &case.support_mode,
                &task_terms,
            )
        })
        .take(3)
        .collect::<Vec<_>>();

    lines.push(format!(
        "Current queue item id: {}",
        counting_queue_tasks
            .first()
            .map(|task| task.message_key.as_str())
            .unwrap_or("null")
    ));
    lines.push(format!(
        "Current plan item id: {}",
        matched_plans
            .first()
            .map(|goal| goal.goal_id.as_str())
            .unwrap_or("null")
    ));
    lines.push(format!(
        "Current schedule item id: {}",
        matched_schedules
            .first()
            .map(|task| task.task_id.as_str())
            .unwrap_or("null")
    ));
    lines.push(format!(
        "Current ticket case id: {}",
        matched_ticket_cases
            .first()
            .map(|case| case.case_id.as_str())
            .unwrap_or("null")
    ));

    if counting_queue_tasks.is_empty() {
        lines.push("queue_items: []".to_string());
    } else {
        lines.push("queue_items:".to_string());
        for task in counting_queue_tasks {
            lines.push(format!("- id: {}", task.message_key));
            if let Some(mission_id) = &context.mission_id {
                lines.push(format!("  mission_id: {}", mission_id));
            }
            lines.push(format!("  kind: {}", context.turn_class));
            lines.push(format!("  thread_key: {}", task.thread_key));
            lines.push(format!("  status: {}", task.route_status));
            lines.push(format!("  priority: {}", task.priority));
            lines.push(format!("  title: {}", collapse_spaces(task.title.trim())));
            lines.push(format!(
                "  next step: {}",
                clip_prompt_text(&collapse_spaces(task.prompt.trim()), 120)
            ));
        }
    }
    if context_only_queue_tasks.is_empty() {
        lines.push("Visible queue rows that do not count as open work: []".to_string());
    } else {
        lines.push("Visible queue rows that do not count as open work:".to_string());
        for task in context_only_queue_tasks {
            lines.push(format!("- id: {}", task.message_key));
            lines.push(format!("  thread_key: {}", task.thread_key));
            lines.push(format!("  status: {}", task.route_status));
            lines.push(format!("  priority: {}", task.priority));
            lines.push(format!("  title: {}", collapse_spaces(task.title.trim())));
        }
    }
    if matched_plans.is_empty() {
        lines.push("plan_items: []".to_string());
    } else {
        lines.push("plan_items:".to_string());
        for goal in matched_plans {
            lines.push(format!("- id: {}", goal.goal_id));
            if let Some(mission_id) = &context.mission_id {
                lines.push(format!("  mission_id: {}", mission_id));
            }
            lines.push(format!("  thread_key: {}", goal.thread_key));
            lines.push(format!("  status: {}", goal.status));
            lines.push(format!("  title: {}", collapse_spaces(goal.title.trim())));
        }
    }
    if matched_ticket_cases.is_empty() {
        lines.push("ticket_cases: []".to_string());
    } else {
        lines.push("ticket_cases:".to_string());
        for case in matched_ticket_cases {
            lines.push(format!("- id: {}", case.case_id));
            lines.push(format!("  ticket_key: {}", case.ticket_key));
            lines.push(format!("  status: {}", case.state));
            lines.push(format!("  label: {}", case.label));
            lines.push(format!("  support_mode: {}", case.support_mode));
            lines.push(format!("  autonomy: {}", case.autonomy_level));
            lines.push(format!("  risk: {}", case.risk_level));
        }
    }
    if matched_schedules.is_empty() {
        lines.push("schedule_items: []".to_string());
    } else {
        lines.push("schedule_items:".to_string());
        for task in matched_schedules {
            lines.push(format!("- id: {}", task.task_id));
            if let Some(mission_id) = &context.mission_id {
                lines.push(format!("  mission_id: {}", mission_id));
            }
            lines.push(format!("  thread_key: {}", task.thread_key));
            lines.push("  status: enabled".to_string());
            lines.push(format!(
                "  name: {}",
                clip_prompt_text(&collapse_spaces(task.name.trim()), 80)
            ));
            lines.push(format!("  cron_expr: {}", task.cron_expr));
            if let Some(next_run_at) = task.next_run_at.as_deref() {
                lines.push(format!("  next_run_at: {}", next_run_at));
            }
        }
    }
    Ok(lines.join("\n"))
}

fn mission_search_terms(context: &MissionContext) -> Vec<String> {
    let mut terms = Vec::new();
    if let Some(mission_id) = &context.mission_id {
        terms.push(mission_id.to_ascii_lowercase());
        terms.extend(
            mission_id
                .split(|ch: char| !ch.is_ascii_alphanumeric())
                .filter(|part| part.len() >= 4)
                .map(|part| part.to_ascii_lowercase()),
        );
    }
    if let Some(workspace) = &context.workspace_root {
        if workspace.contains("airbnb_clone_bench") {
            terms.push("airbnb_clone_bench".to_string());
            terms.push("airbnb".to_string());
            terms.push("progress".to_string());
        }
    }
    if let Some(objective) = &context.main_objective {
        terms.extend(
            objective
                .split(|ch: char| !ch.is_ascii_alphanumeric())
                .filter(|part| part.len() >= 6)
                .map(|part| part.to_ascii_lowercase()),
        );
    }
    if let Some(cycle) = &context.report_cycle {
        let normalized = cycle.to_ascii_lowercase();
        terms.push(normalized.clone());
        terms.extend(
            normalized
                .split(|ch: char| !ch.is_ascii_alphanumeric())
                .filter(|part| part.len() >= 6)
                .map(|part| part.to_string()),
        );
    }
    terms.sort();
    terms.dedup();
    terms
}

fn workflow_matches_terms(thread_key: &str, title: &str, prompt: &str, terms: &[String]) -> bool {
    if terms.is_empty() {
        return false;
    }
    let haystack = format!("{thread_key}\n{title}\n{prompt}").to_ascii_lowercase();
    terms.iter().any(|term| haystack.contains(term))
}

fn compact_workspace_path(path: &str, workspace_root: Option<&str>) -> String {
    if let Some(root) = workspace_root {
        if path == root {
            return "<workspace>".to_string();
        }
        if let Some(stripped) = path.strip_prefix(root) {
            return format!("<workspace>{}", stripped);
        }
    }
    collapse_spaces(path.trim())
}

pub(crate) fn extract_codex_text_response(stdout: &str) -> Option<String> {
    let mut last_agent_message: Option<String> = None;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            return None;
        };
        let item = match value.get("item") {
            Some(item) => item,
            None => continue,
        };
        let agent_message = if item.get("type").and_then(Value::as_str) == Some("agent_message") {
            Some(item)
        } else {
            item.get("item").filter(|nested| {
                nested.get("type").and_then(Value::as_str) == Some("agent_message")
            })
        };
        if let Some(agent_message) = agent_message {
            if let Some(text) = agent_message.get("text").and_then(Value::as_str) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    last_agent_message = Some(trimmed.to_string());
                }
            }
        }
    }
    last_agent_message
}

pub(crate) fn sanitize_context_message(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if looks_like_mission_bootstrap(trimmed) {
        return summarize_mission_bootstrap_prompt(trimmed);
    }
    if looks_like_progress_report_prompt(trimmed) {
        return summarize_progress_report_request(trimmed);
    }
    if let Some(summary) = summarize_inbound_email_wrapper(trimmed) {
        return clip_prompt_text(&summary, 8_000);
    }
    let normalized = if looks_like_codex_event_stream(trimmed) {
        extract_codex_text_response(trimmed).unwrap_or_else(|| {
            "Previous turn stored raw Codex event-stream output instead of a final reply. Use only the stable user-visible outcome or re-check the current runtime state before continuing.".to_string()
        })
    } else {
        trimmed.to_string()
    };
    clip_prompt_text(&normalized, 8_000)
}

fn summarize_mission_bootstrap_prompt(content: &str) -> String {
    let workspace_raw = extract_block_value(content, "Work only inside this workspace:");
    let workspace = workspace_raw
        .as_ref()
        .map(|path| compact_workspace_path(path, Some(path)))
        .unwrap_or_else(|| "<workspace>".to_string());
    let objective = extract_block_value(content, "Main objective:")
        .unwrap_or_else(|| "start the active durable mission".to_string());
    let mut lines = vec![
        "durable_mission_bootstrap:".to_string(),
        format!("workspace_root: {workspace}"),
        format!(
            "mission_id: {}",
            derive_mission_id(workspace_raw.as_deref(), Some(&objective))
                .unwrap_or_else(|| "active_mission".to_string())
        ),
    ];
    let headings = extract_report_headings(content);
    if !headings.is_empty() {
        lines.push(format!("report_headings: {}", headings.join("|")));
    }
    lines.join("\n")
}

fn summarize_progress_report_request(content: &str) -> String {
    let cycle = first_non_empty_line(content)
        .map(|line| collapse_spaces(line))
        .unwrap_or_else(|| "MISSION_PROGRESS_REPORT".to_string());
    let progress_artifact = extract_block_value(content, "First update this file:")
        .map(|path| {
            let workspace_root = path
                .split_once("/ops/progress/")
                .map(|(root, _)| root.to_string());
            compact_workspace_path(&path, workspace_root.as_deref())
        })
        .unwrap_or_else(|| "<workspace>/ops/progress/progress-latest.md".to_string());
    let headings = extract_reply_headings(content);
    let mut lines = vec![
        "progress_report_request:".to_string(),
        format!("cycle: {cycle}"),
        format!("progress_artifact: {progress_artifact}"),
    ];
    if !headings.is_empty() {
        lines.push(format!("reply_headings: {}", headings.join("|")));
    }
    lines.join("\n")
}

pub(crate) fn render_context_message(role: &str, content: &str) -> String {
    let sanitized = sanitize_context_message(content);
    let label = render_message_role_label(role, content);
    if role == "assistant" && is_historical_status_note(&sanitized) {
        format!(
            "assistant_status_history: Historical assistant status note only; re-check the current runtime and host state before relying on it. {}",
            sanitized
        )
    } else {
        format!("{label}: {sanitized}")
    }
}

pub(crate) fn render_message_role_label<'a>(role: &'a str, content: &str) -> &'a str {
    if role == "user" && is_internal_queue_prompt(content) {
        "internal_queue"
    } else {
        role
    }
}

pub(crate) fn is_internal_queue_prompt(content: &str) -> bool {
    let trimmed = content.trim_start();
    [
        "Continue the broader goal using the latest completed turn as the starting point.",
        "Review the blocked owner-visible task without losing continuity.",
        "Recover or finish the owner-visible task without losing continuity.",
        "Use the queue-cleanup skill first.",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
}

pub(crate) fn is_historical_status_note(content: &str) -> bool {
    let trimmed = content.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("blocked:")
        || lower.starts_with("completed:")
        || lower.starts_with("failed:")
        || lower.starts_with("prepared:")
        || lower.starts_with("still blocked")
        || lower.starts_with("nextcloud_")
        || lower.starts_with("zammad_")
        || lower.starts_with("redis_")
}

pub(crate) fn looks_like_codex_event_stream(content: &str) -> bool {
    let lines = content.lines().take(8).collect::<Vec<_>>();
    if lines.len() < 2 {
        return false;
    }
    lines
        .iter()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('{') && trimmed.contains("\"type\"")
        })
        .count()
        >= 2
}

pub(crate) fn clip_prompt_text(content: &str, max_chars: usize) -> String {
    if content.chars().count() <= max_chars {
        return content.to_string();
    }
    let mut clipped = content
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    clipped.push('…');
    clipped
}

pub(crate) fn summarize_inbound_email_wrapper(content: &str) -> Option<String> {
    if !content.starts_with("[E-Mail eingegangen]") {
        return None;
    }
    let sender =
        extract_labeled_line(content, "Sender:").unwrap_or_else(|| "unknown sender".to_string());
    let subject =
        extract_labeled_line(content, "Betreff:").unwrap_or_else(|| "(ohne Betreff)".to_string());
    let thread =
        extract_labeled_line(content, "Thread:").unwrap_or_else(|| "unknown thread".to_string());
    Some(format!(
        "Inbound email wrapper from {sender} with subject {subject} on thread {thread}. The original wrapper also contained reply instructions and historical communication context; treat this as prior mail context only and rely on the newest concrete task evidence before repeating old conclusions."
    ))
}

fn extract_labeled_line(content: &str, prefix: &str) -> Option<String> {
    content
        .lines()
        .find_map(|line| {
            line.strip_prefix(prefix)
                .map(|value| value.trim().to_string())
        })
        .filter(|value| !value.is_empty())
}

pub(crate) fn strip_prompt_comments(prompt: &str) -> String {
    let mut rendered = String::with_capacity(prompt.len());
    let mut remaining = prompt;
    loop {
        if let Some(start) = remaining.find("<!--") {
            rendered.push_str(&remaining[..start]);
            let after_start = &remaining[start + 4..];
            if let Some(end) = after_start.find("-->") {
                remaining = &after_start[end + 3..];
            } else {
                break;
            }
        } else {
            rendered.push_str(remaining);
            break;
        }
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ctox-live-context-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn workflow_state_surfaces_open_ticket_cases() {
        let root = temp_root("ticket-cases");
        let remote = crate::mission::ticket_local_native::create_local_ticket(
            &root,
            "VPN outage",
            "Users cannot reach the VPN gateway.",
            Some("open"),
            Some("high"),
        )
        .expect("failed to create local ticket");
        crate::tickets::sync_ticket_system(&root, "local").expect("failed to sync ticket system");
        crate::tickets::handle_ticket_command(
            &root,
            &[
                "bundle-put".to_string(),
                "--label".to_string(),
                "support/vpn".to_string(),
                "--runbook-id".to_string(),
                "rb-vpn".to_string(),
                "--policy-id".to_string(),
                "pol-vpn".to_string(),
            ],
        )
        .expect("failed to create bundle");
        crate::tickets::handle_ticket_command(
            &root,
            &[
                "label-set".to_string(),
                "--ticket-key".to_string(),
                format!("local:{}", remote.ticket_id),
                "--label".to_string(),
                "support/vpn".to_string(),
            ],
        )
        .expect("failed to assign label");
        crate::tickets::handle_ticket_command(
            &root,
            &[
                "dry-run".to_string(),
                "--ticket-key".to_string(),
                format!("local:{}", remote.ticket_id),
            ],
        )
        .expect("failed to create dry run");

        let context = MissionContext {
            mission_id: None,
            start_seq: None,
            workspace_root: None,
            main_objective: Some("vpn support".to_string()),
            report_cycle: None,
            progress_artifact: None,
            report_headings: Vec::new(),
            non_negotiable_rules: Vec::new(),
            previous_blocker: None,
            turn_class: "support".to_string(),
            read_scope: "workspace".to_string(),
            verification_gap: None,
        };

        let workflow =
            render_workflow_state_block(&root, &context).expect("workflow render failed");
        assert!(workflow.contains("current_ticket_case_id:"));
        assert!(workflow.contains("ticket_cases:"));
        assert!(workflow.contains("support/vpn"));
        assert!(workflow.contains(&format!("local:{}", remote.ticket_id)));
    }

    #[test]
    fn render_chat_prompt_surfaces_suggested_skill_dispatch() {
        let runtime_blocks = PromptRuntimeBlocks {
            focus: "Focus:\n- keep the current mission stable".to_string(),
            anchors: "Anchors:\n- none".to_string(),
            narrative: "Narrative:\n- none".to_string(),
            verified_evidence: "Verified evidence:\nitems: []".to_string(),
            workflow_state: "Open CTOX work that counts right now:\nqueue_items: []".to_string(),
        };
        let prompt = render_chat_prompt(
            Path::new("/tmp/ctox"),
            &runtime_blocks,
            &governance::GovernancePromptSnapshot::default(),
            &context_health::ContextHealthSnapshot {
                conversation_id: 0,
                overall_score: 100,
                status: context_health::ContextHealthStatus::Healthy,
                summary: "healthy".to_string(),
                repair_recommended: false,
                dimensions: Vec::new(),
                warnings: Vec::new(),
            },
            "inspect the ticket system",
            &RenderedContextSelection {
                entries: vec!["user: inspect the ticket system".to_string()],
                omitted_items: 0,
            },
            Some("system-onboarding"),
        );

        assert!(prompt.contains("Suggested skill dispatch:"));
        assert!(prompt.contains("preferred_skill: system-onboarding"));
    }

    #[test]
    fn render_system_prompt_includes_default_cto_operating_mode() {
        let root = temp_root("default-cto-contract");
        let prompt = render_system_prompt(&root, &BTreeMap::new()).expect("rendered prompt");
        assert!(prompt.contains("## CTO Operating Mode"));
        assert!(prompt.contains("You own the mission outcome, not just the next task."));
    }

    #[test]
    fn render_system_prompt_uses_cto_operating_mode_override() {
        let root = temp_root("override-cto-contract");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_CTO_OPERATING_MODE_PROMPT".to_string(),
            "## CTO Operating Mode\n\nCustom CTO contract.\n".to_string(),
        );
        let prompt = render_system_prompt(&root, &settings).expect("rendered prompt");
        assert!(prompt.contains("Custom CTO contract."));
        assert!(!prompt.contains("You own the mission outcome, not just the next task."));
    }
}
