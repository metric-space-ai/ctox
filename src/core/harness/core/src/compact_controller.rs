use std::collections::HashMap;

use crate::Prompt;
use crate::client::ModelClientSession;
use crate::client_common::ResponseEvent;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::compact::SUMMARIZATION_PROMPT;
use crate::compact::SUMMARY_PREFIX;
use crate::compact::collect_user_messages;
use crate::compact::content_items_to_text;
use crate::compact::is_summary_message;
use crate::error::CodexErr;
use crate::error::Result as CodexResult;
use crate::stream_events_utils::raw_assistant_output_text_from_item;
use crate::util::backoff;
use ctox_protocol::models::ContentItem;
use ctox_protocol::models::FunctionCallOutputPayload;
use ctox_protocol::models::ReasoningItemReasoningSummary;
use ctox_protocol::models::ResponseItem;
use ctox_protocol::openai_models::ModelPreset;
use ctox_protocol::user_input::UserInput;
use futures::StreamExt;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_json::json;

const CHAR_PER_TOKEN: usize = 4;
const DEFAULT_FINAL_TARGET_CHARS: usize = 3072 * CHAR_PER_TOKEN;
const DEFAULT_RESERVOIR_TARGET_CHARS: usize =
    ((DEFAULT_FINAL_TARGET_CHARS as f64) * 1.45).round() as usize;
const DEFAULT_ITERATIONS: usize = 2;
const DEFAULT_BLOCK_CHARS: usize = 1800;
const REVISED_TEXT_OVERFLOW_TOLERANCE: usize = 120;
const ROUTE_VALUES: &[&str] = &[
    "story",
    "anchor",
    "focus",
    "story_anchor",
    "story_focus",
    "anchor_focus",
    "all",
    "discard",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompactionTrigger {
    Auto,
    Manual,
    Interrupt,
}

impl CompactionTrigger {
    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
            Self::Interrupt => "interrupt",
        }
    }
}

pub(crate) struct CompactionControllerOutput {
    pub(crate) summary_text: String,
    pub(crate) target_model: Option<String>,
    pub(crate) trimmed_blocks: usize,
}

#[derive(Clone, Debug)]
struct CompactionBlock {
    id: String,
    title: String,
    current_text: String,
    action: String,
    bucket: String,
    dropped: bool,
    reason: String,
    history: Vec<BlockHistoryEntry>,
}

#[derive(Clone, Debug)]
struct BlockHistoryEntry;

#[derive(Clone, Debug, Serialize)]
struct PromptBlock<'a> {
    id: &'a str,
    title: &'a str,
    chars: usize,
    destination: &'a str,
    text: &'a str,
}

#[derive(Clone, Debug)]
struct AppliedDecision {
    changed: bool,
}

#[derive(Debug, Deserialize)]
struct DecisionStageResponse {
    #[serde(rename = "summary")]
    _summary: String,
    decisions: Vec<BlockDecision>,
}

#[derive(Debug, Deserialize)]
struct ProgressStageResponse {
    summary: String,
    #[serde(alias = "school_grade")]
    progress_score: u8,
    rationale: String,
}

#[derive(Debug, Deserialize)]
struct BlockDecision {
    id: String,
    action: String,
    #[serde(alias = "bucket")]
    destination: String,
    reason: String,
    revised_text: String,
}

#[derive(Debug, Deserialize)]
struct OutputStageResponse {
    #[serde(rename = "story_title")]
    _story_title: String,
    story_draft: String,
    #[serde(rename = "anchor_title")]
    _anchor_title: String,
    anchor_draft: String,
    #[serde(rename = "focus_title")]
    _focus_title: String,
    focus_draft: String,
}

#[derive(Debug, Deserialize)]
struct ReprioritizationStageResponse {
    summary: String,
    should_reprioritize: bool,
    interrupts_reviewed: bool,
    active_task: String,
    task_packet: Vec<String>,
    priority_reason: String,
    next_action: String,
    completed_tasks: Vec<String>,
    #[serde(alias = "spawned_tasks")]
    follow_up_tasks: Vec<TaskSpawn>,
    mutated_tasks: Vec<TaskMutation>,
    priority_order: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompactEnvelope {
    schema_version: &'static str,
    controller: &'static str,
    trigger: &'static str,
    task_hint: String,
    progress_review: ProgressReviewEnvelope,
    continuity_narrative: String,
    continuity_anchors: String,
    active_focus: String,
    reprioritization_review: ReprioritizationEnvelope,
    model_routing: ModelRoutingEnvelope,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReprioritizationEnvelope {
    summary: String,
    should_reprioritize: bool,
    interrupts_reviewed: bool,
    active_task: String,
    task_packet: Vec<String>,
    priority_reason: String,
    next_action: String,
    completed_tasks: Vec<String>,
    follow_up_tasks: Vec<TaskSpawn>,
    mutated_tasks: Vec<TaskMutation>,
    priority_order: Vec<String>,
    task_context_update: TaskContextUpdate,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskContextUpdate {
    controller: &'static str,
    mode: &'static str,
    next_action: String,
    active_task: String,
    task_packet: Vec<String>,
    priority_reason: String,
    completed_tasks: Vec<String>,
    follow_up_tasks: Vec<TaskSpawn>,
    mutated_tasks: Vec<TaskMutation>,
    priority_order: Vec<String>,
    interrupt_triggered: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressReviewEnvelope {
    progress_score: u8,
    label: &'static str,
    summary: String,
    rationale: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelRoutingEnvelope {
    tier: &'static str,
    current_model: String,
    candidate_models: Vec<String>,
    requested_model: String,
    switch_planned: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct TaskSpawn {
    title: String,
    detail: String,
    #[serde(alias = "priority_bucket", alias = "priorityBucket")]
    priority: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct TaskMutation {
    target: String,
    action: String,
    #[serde(alias = "revisedTitle")]
    revised_title: String,
    #[serde(alias = "revisedDetail")]
    revised_detail: String,
    reason: String,
}

#[derive(Debug)]
struct ModelRoutingDecision {
    tier: &'static str,
    current_model: String,
    candidate_models: Vec<String>,
    requested_model: String,
    switch_planned: bool,
}

pub(crate) async fn run_compaction_controller(
    sess: &Session,
    turn_context: &TurnContext,
    history_items: &[ResponseItem],
    input: &[UserInput],
    trigger: CompactionTrigger,
) -> CodexResult<CompactionControllerOutput> {
    let task_hint = derive_task_hint(input, history_items);
    let context_text = render_history_for_compaction(history_items);
    let recent_user_messages = collect_recent_user_messages(history_items, 4);
    let mut blocks = segment_context(&context_text, DEFAULT_BLOCK_CHARS);
    let mut trimmed_blocks = 0usize;
    let progress_review = if blocks.is_empty() {
        ProgressStageResponse {
            summary: "No usable context available.".to_string(),
            progress_score: 4,
            rationale: "Without usable history, progress can only be rated as adequate."
                .to_string(),
        }
    } else {
        loop {
            let prompt = build_progress_prompt(&task_hint, &turn_context.model_info.slug, &blocks)?;
            match run_structured_prompt::<ProgressStageResponse>(
                sess,
                turn_context,
                prompt,
                progress_schema(),
            )
            .await
            {
                Ok(result) => break sanitize_progress_review(result),
                Err(CodexErr::ContextWindowExceeded) if blocks.len() > 1 => {
                    blocks.remove(0);
                    trimmed_blocks += 1;
                }
                Err(err) => return Err(err),
            }
        }
    };
    let model_routing = select_model_routing(
        &turn_context.model_info.slug,
        progress_review.progress_score,
        available_models(sess),
    );

    if !blocks.is_empty() {
        loop {
            let prompt = build_screen_prompt(
                &task_hint,
                DEFAULT_RESERVOIR_TARGET_CHARS,
                DEFAULT_FINAL_TARGET_CHARS,
                &blocks,
            )?;
            match run_structured_prompt::<DecisionStageResponse>(
                sess,
                turn_context,
                prompt,
                screen_schema(),
            )
            .await
            {
                Ok(screen_result) => {
                    apply_decisions(&mut blocks, &screen_result.decisions, "Initial filter");
                    break;
                }
                Err(CodexErr::ContextWindowExceeded) if blocks.len() > 1 => {
                    blocks.remove(0);
                    trimmed_blocks += 1;
                }
                Err(err) => return Err(err),
            }
        }

        let mut previous_chars = retained_chars(&blocks);
        for iteration_index in 1..=DEFAULT_ITERATIONS {
            let retained_before = retained_blocks(&blocks);
            if retained_before.is_empty() {
                break;
            }

            if iteration_index > 1 && previous_chars <= DEFAULT_RESERVOIR_TARGET_CHARS {
                break;
            }

            let prompt = build_iteration_prompt(
                &task_hint,
                DEFAULT_RESERVOIR_TARGET_CHARS,
                iteration_index,
                previous_chars,
                &retained_before,
            )?;
            let iteration_result = run_structured_prompt::<DecisionStageResponse>(
                sess,
                turn_context,
                prompt,
                iteration_schema(),
            )
            .await?;
            let decisions = apply_decisions(
                &mut blocks,
                &iteration_result.decisions,
                &format!("Iteration {iteration_index}"),
            );
            let after_chars = retained_chars(&blocks);
            let real_change =
                after_chars != previous_chars || decisions.iter().any(|item| item.changed);
            previous_chars = after_chars;
            if !real_change {
                break;
            }
        }
    }

    let story_blocks = bucket_blocks(&blocks, "story");
    let anchor_blocks = bucket_blocks(&blocks, "anchor");
    let focus_blocks = bucket_blocks(&blocks, "focus");

    let output_result = if story_blocks.is_empty()
        && anchor_blocks.is_empty()
        && focus_blocks.is_empty()
    {
        OutputStageResponse {
            _story_title: "Continuity Narrative".to_string(),
            story_draft: String::new(),
            _anchor_title: "Continuity Anchors".to_string(),
            anchor_draft: String::new(),
            _focus_title: "Active Focus".to_string(),
            focus_draft: String::new(),
        }
    } else {
        let prompt = build_output_prompt(
            &task_hint,
            DEFAULT_FINAL_TARGET_CHARS,
            &story_blocks,
            &anchor_blocks,
            &focus_blocks,
        )?;
        run_structured_prompt::<OutputStageResponse>(sess, turn_context, prompt, output_schema())
            .await?
    };

    let reprioritization_prompt =
        build_reprioritization_prompt(&task_hint, trigger, &output_result, &recent_user_messages)?;
    let reprioritization = run_structured_prompt::<ReprioritizationStageResponse>(
        sess,
        turn_context,
        reprioritization_prompt,
        reprioritization_schema(),
    )
    .await?;

    let mode = if reprioritization.should_reprioritize
        || matches!(
            reprioritization.next_action.as_str(),
            "reprioritize" | "switch_task" | "park_current_task"
        ) {
        "reprioritize"
    } else {
        "execute_task"
    };

    let envelope = CompactEnvelope {
        schema_version: "compact_controller_v1",
        controller: "context_optimizer_simple_html_v1",
        trigger: trigger.as_str(),
        task_hint,
        progress_review: ProgressReviewEnvelope {
            progress_score: progress_review.progress_score,
            label: progress_score_label(progress_review.progress_score),
            summary: normalize_text(&progress_review.summary),
            rationale: normalize_text(&progress_review.rationale),
        },
        continuity_narrative: normalize_text(&output_result.story_draft),
        continuity_anchors: normalize_text(&output_result.anchor_draft),
        active_focus: normalize_text(&output_result.focus_draft),
        reprioritization_review: ReprioritizationEnvelope {
            summary: normalize_text(&reprioritization.summary),
            should_reprioritize: reprioritization.should_reprioritize,
            interrupts_reviewed: reprioritization.interrupts_reviewed,
            active_task: normalize_text(&reprioritization.active_task),
            task_packet: normalize_string_list(&reprioritization.task_packet),
            priority_reason: normalize_text(&reprioritization.priority_reason),
            next_action: normalize_next_action(&reprioritization.next_action),
            completed_tasks: normalize_string_list(&reprioritization.completed_tasks),
            follow_up_tasks: normalize_follow_up_tasks(&reprioritization.follow_up_tasks),
            mutated_tasks: normalize_mutated_tasks(&reprioritization.mutated_tasks),
            priority_order: normalize_string_list(&reprioritization.priority_order),
            task_context_update: TaskContextUpdate {
                controller: "compact_controller",
                mode,
                next_action: normalize_next_action(&reprioritization.next_action),
                active_task: normalize_text(&reprioritization.active_task),
                task_packet: normalize_string_list(&reprioritization.task_packet),
                priority_reason: normalize_text(&reprioritization.priority_reason),
                completed_tasks: normalize_string_list(&reprioritization.completed_tasks),
                follow_up_tasks: normalize_follow_up_tasks(&reprioritization.follow_up_tasks),
                mutated_tasks: normalize_mutated_tasks(&reprioritization.mutated_tasks),
                priority_order: normalize_string_list(&reprioritization.priority_order),
                interrupt_triggered: trigger == CompactionTrigger::Interrupt,
            },
        },
        model_routing: ModelRoutingEnvelope {
            tier: model_routing.tier,
            current_model: model_routing.current_model.clone(),
            candidate_models: model_routing.candidate_models.clone(),
            requested_model: model_routing.requested_model.clone(),
            switch_planned: model_routing.switch_planned,
        },
    };

    let serialized = serde_json::to_string_pretty(&envelope).map_err(|err| {
        CodexErr::InvalidRequest(format!("failed to serialize compact envelope: {err}"))
    })?;

    Ok(CompactionControllerOutput {
        summary_text: format!("{SUMMARY_PREFIX}\n{serialized}"),
        target_model: model_routing
            .switch_planned
            .then_some(model_routing.requested_model),
        trimmed_blocks,
    })
}

fn sanitize_progress_review(result: ProgressStageResponse) -> ProgressStageResponse {
    let progress_score = result.progress_score.clamp(1, 6);
    ProgressStageResponse {
        summary: normalize_text(&result.summary),
        progress_score,
        rationale: normalize_text(&result.rationale),
    }
}

fn derive_task_hint(input: &[UserInput], history_items: &[ResponseItem]) -> String {
    let provided = input
        .iter()
        .filter_map(|item| match item {
            UserInput::Text { text, .. } => Some(normalize_text(text)),
            _ => None,
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    if !provided.is_empty() && normalize_text(&provided) != normalize_text(SUMMARIZATION_PROMPT) {
        return provided;
    }

    collect_user_messages(history_items)
        .into_iter()
        .rev()
        .find(|message| !message.is_empty())
        .unwrap_or_else(|| "(empty)".to_string())
}

fn collect_recent_user_messages(history_items: &[ResponseItem], limit: usize) -> Vec<String> {
    let mut messages = collect_user_messages(history_items);
    if messages.len() > limit {
        messages = messages.split_off(messages.len().saturating_sub(limit));
    }
    messages
}

fn available_models(sess: &Session) -> Vec<String> {
    sess.services
        .models_manager
        .try_list_models()
        .unwrap_or_default()
        .into_iter()
        .map(|preset: ModelPreset| preset.model)
        .collect()
}

fn render_history_for_compaction(items: &[ResponseItem]) -> String {
    let mut sections = Vec::new();
    let mut counters: HashMap<&'static str, usize> = HashMap::new();

    for item in items {
        match item {
            ResponseItem::Message { role, content, .. } => {
                let Some(text) = content_items_to_text(content) else {
                    continue;
                };
                let text = normalize_text(&text);
                if text.is_empty() {
                    continue;
                }

                if role == "developer" {
                    continue;
                }

                if role == "user" && is_summary_message(&text) {
                    let body = text
                        .strip_prefix(format!("{SUMMARY_PREFIX}\n").as_str())
                        .unwrap_or(text.as_str())
                        .to_string();
                    push_section(
                        &mut sections,
                        &mut counters,
                        "Prior Compaction Summary",
                        &body,
                    );
                    continue;
                }

                let heading = match role.as_str() {
                    "user" => "User Message",
                    "assistant" => "Assistant Message",
                    _ => {
                        push_section(&mut sections, &mut counters, "Message", &text);
                        continue;
                    }
                };
                push_section(&mut sections, &mut counters, heading, &text);
            }
            ResponseItem::Reasoning { summary, .. } => {
                let text = summary
                    .iter()
                    .map(|entry| match entry {
                        ReasoningItemReasoningSummary::SummaryText { text } => text.as_str(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                push_section(&mut sections, &mut counters, "Reasoning Summary", &text);
            }
            ResponseItem::LocalShellCall {
                call_id,
                status,
                action,
                ..
            } => {
                let payload = json!({
                    "callId": call_id,
                    "status": status,
                    "action": action,
                });
                push_json_section(&mut sections, &mut counters, "Local Shell Call", &payload);
            }
            ResponseItem::FunctionCall {
                name,
                namespace,
                arguments,
                call_id,
                ..
            } => {
                let payload = json!({
                    "callId": call_id,
                    "namespace": namespace,
                    "name": name,
                    "arguments": arguments,
                });
                push_json_section(&mut sections, &mut counters, "Function Call", &payload);
            }
            ResponseItem::FunctionCallOutput { call_id, output } => {
                push_tool_output_section(
                    &mut sections,
                    &mut counters,
                    "Function Output",
                    call_id,
                    output,
                );
            }
            ResponseItem::CustomToolCall {
                call_id,
                name,
                input,
                status,
                ..
            } => {
                let payload = json!({
                    "callId": call_id,
                    "name": name,
                    "status": status,
                    "input": input,
                });
                push_json_section(&mut sections, &mut counters, "Custom Tool Call", &payload);
            }
            ResponseItem::CustomToolCallOutput { call_id, output } => {
                push_tool_output_section(
                    &mut sections,
                    &mut counters,
                    "Custom Tool Output",
                    call_id,
                    output,
                );
            }
            ResponseItem::ToolSearchCall {
                call_id,
                status,
                execution,
                arguments,
                ..
            } => {
                let payload = json!({
                    "callId": call_id,
                    "status": status,
                    "execution": execution,
                    "arguments": arguments,
                });
                push_json_section(&mut sections, &mut counters, "Tool Search Call", &payload);
            }
            ResponseItem::ToolSearchOutput {
                call_id,
                status,
                execution,
                tools,
            } => {
                let payload = json!({
                    "callId": call_id,
                    "status": status,
                    "execution": execution,
                    "tools": tools,
                });
                push_json_section(&mut sections, &mut counters, "Tool Search Output", &payload);
            }
            ResponseItem::WebSearchCall { status, action, .. } => {
                let payload = json!({
                    "status": status,
                    "action": action,
                });
                push_json_section(&mut sections, &mut counters, "Web Search Call", &payload);
            }
            ResponseItem::ImageGenerationCall {
                id,
                status,
                revised_prompt,
                ..
            } => {
                let payload = json!({
                    "id": id,
                    "status": status,
                    "revisedPrompt": revised_prompt,
                });
                push_json_section(&mut sections, &mut counters, "Image Generation", &payload);
            }
            ResponseItem::GhostSnapshot { .. }
            | ResponseItem::Compaction { .. }
            | ResponseItem::Other => {}
        }
    }

    normalize_text(&sections.join("\n\n"))
}

fn push_section(
    sections: &mut Vec<String>,
    counters: &mut HashMap<&'static str, usize>,
    heading: &'static str,
    body: &str,
) {
    let text = normalize_text(body);
    if text.is_empty() {
        return;
    }
    let counter = counters.entry(heading).or_insert(0);
    *counter += 1;
    sections.push(format!("## {heading} {counter}\n{text}"));
}

fn push_json_section(
    sections: &mut Vec<String>,
    counters: &mut HashMap<&'static str, usize>,
    heading: &'static str,
    payload: &Value,
) {
    let text = serde_json::to_string_pretty(payload).unwrap_or_else(|_| payload.to_string());
    push_section(sections, counters, heading, &text);
}

fn push_tool_output_section(
    sections: &mut Vec<String>,
    counters: &mut HashMap<&'static str, usize>,
    heading: &'static str,
    call_id: &str,
    output: &FunctionCallOutputPayload,
) {
    if let Some(text) = output.text_content() {
        let payload = format!("call_id: {call_id}\n\n{}", normalize_text(text));
        push_section(sections, counters, heading, &payload);
        return;
    }

    let payload = json!({
        "callId": call_id,
        "contentItems": output.content_items(),
    });
    push_json_section(sections, counters, heading, &payload);
}

fn normalize_text(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string()
}

fn count_chars(value: &str) -> usize {
    normalize_text(value).chars().count()
}

fn normalize_string_list(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| normalize_text(value))
        .filter(|value| !value.is_empty())
        .collect()
}

fn normalize_follow_up_tasks(values: &[TaskSpawn]) -> Vec<TaskSpawn> {
    values
        .iter()
        .map(|task| TaskSpawn {
            title: normalize_text(&task.title),
            detail: normalize_text(&task.detail),
            priority: normalize_priority(&task.priority),
        })
        .filter(|task| !task.title.is_empty())
        .collect()
}

fn normalize_mutated_tasks(values: &[TaskMutation]) -> Vec<TaskMutation> {
    values
        .iter()
        .map(|task| TaskMutation {
            target: normalize_text(&task.target),
            action: normalize_task_action(&task.action),
            revised_title: normalize_text(&task.revised_title),
            revised_detail: normalize_text(&task.revised_detail),
            reason: normalize_text(&task.reason),
        })
        .filter(|task| !task.target.is_empty())
        .collect()
}

fn normalize_next_action(value: &str) -> String {
    match normalize_text(value).as_str() {
        "reprioritize" => "reprioritize".to_string(),
        "switch_task" => "switch_task".to_string(),
        "park_current_task" => "park_current_task".to_string(),
        _ => "continue_current_task".to_string(),
    }
}

fn normalize_priority(value: &str) -> String {
    match normalize_text(value).as_str() {
        "now" => "now".to_string(),
        "later" => "later".to_string(),
        _ => "next".to_string(),
    }
}

fn normalize_task_action(value: &str) -> String {
    match normalize_text(value).as_str() {
        "complete" => "complete".to_string(),
        "revise" => "revise".to_string(),
        "split" => "split".to_string(),
        "drop" => "drop".to_string(),
        "defer" => "defer".to_string(),
        _ => "revise".to_string(),
    }
}

fn progress_score_label(score: u8) -> &'static str {
    match score {
        1 => "excellent",
        2 => "good",
        3 => "satisfactory",
        4 => "adequate",
        5 => "poor",
        _ => "insufficient",
    }
}

fn select_model_routing(
    current_model: &str,
    progress_score: u8,
    available_models: Vec<String>,
) -> ModelRoutingDecision {
    let (tier, candidates): (&'static str, Vec<String>) = match progress_score {
        1 | 2 => (
            "simple",
            vec![
                "gpt-oss-120b".to_string(),
                "Qwen3.5-35B-A3B".to_string(),
                "gpt-4.5-nano".to_string(),
            ],
        ),
        3 | 4 => (
            "medium",
            vec![
                "gpt-oss-120b".to_string(),
                "Qwen3-235B-A22B".to_string(),
                "gpt-4.5-mini".to_string(),
            ],
        ),
        _ => (
            "red",
            vec![
                "gpt-4.5".to_string(),
                "gpt-5.4".to_string(),
                "gpt-5.4-pro".to_string(),
            ],
        ),
    };

    let requested_model = if candidates
        .iter()
        .any(|candidate| model_slug_matches(current_model, candidate))
    {
        current_model.to_string()
    } else if let Some(available) = candidates.iter().find_map(|candidate| {
        available_models
            .iter()
            .find(|available| model_slug_matches(available, candidate))
            .cloned()
    }) {
        available
    } else {
        candidates
            .first()
            .cloned()
            .unwrap_or_else(|| current_model.to_string())
    };

    ModelRoutingDecision {
        tier,
        current_model: current_model.to_string(),
        candidate_models: candidates,
        switch_planned: !model_slug_matches(current_model, &requested_model),
        requested_model,
    }
}

fn model_slug_matches(left: &str, right: &str) -> bool {
    let left_lower = left.to_ascii_lowercase();
    let right_lower = right.to_ascii_lowercase();
    left_lower == right_lower
        || left_lower.ends_with(&format!("/{right_lower}"))
        || left_lower.ends_with(&format!(">{right_lower}"))
        || left_lower.ends_with(&format!(":{right_lower}"))
}

fn parse_sections(raw_text: &str) -> Vec<(String, String)> {
    let text = normalize_text(raw_text);
    if text.is_empty() {
        return Vec::new();
    }

    let mut sections = Vec::new();
    let mut current_heading = String::new();
    let mut current_lines = Vec::new();

    let flush =
        |sections: &mut Vec<(String, String)>, heading: &mut String, lines: &mut Vec<String>| {
            let body = lines.join("\n").trim().to_string();
            if !heading.is_empty() || !body.is_empty() {
                sections.push((
                    if heading.is_empty() {
                        "Kontext".to_string()
                    } else {
                        heading.clone()
                    },
                    body,
                ));
            }
            lines.clear();
        };

    for line in text.lines() {
        if let Some(heading) = markdown_heading(line) {
            flush(&mut sections, &mut current_heading, &mut current_lines);
            current_heading = heading;
        } else {
            current_lines.push(line.to_string());
        }
    }

    flush(&mut sections, &mut current_heading, &mut current_lines);

    let mut non_empty = sections
        .into_iter()
        .filter(|(_, body)| !normalize_text(body).is_empty())
        .collect::<Vec<_>>();
    if non_empty.is_empty() {
        non_empty.push(("Kontext".to_string(), text));
    }
    non_empty
}

fn markdown_heading(line: &str) -> Option<String> {
    let trimmed = line.trim_start_matches(' ');
    if line.len().saturating_sub(trimmed.len()) > 3 {
        return None;
    }

    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }

    let rest = trimmed[hashes..].trim();
    if rest.is_empty() {
        return None;
    }
    Some(rest.to_string())
}

fn segment_context(raw_text: &str, max_block_chars: usize) -> Vec<CompactionBlock> {
    let sections = parse_sections(raw_text);
    let mut blocks = Vec::new();
    let mut global_index = 1usize;

    for (section_heading, section_body) in sections {
        let paragraphs = normalize_text(&section_body)
            .split("\n\n")
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let source_paragraphs = if paragraphs.is_empty() {
            vec![normalize_text(&section_body)]
        } else {
            paragraphs
        };

        let mut buffer = String::new();
        let mut local_index = 1usize;
        for paragraph in source_paragraphs {
            let candidate = if buffer.is_empty() {
                paragraph.clone()
            } else {
                format!("{buffer}\n\n{paragraph}")
            };
            if !buffer.is_empty() && count_chars(&candidate) > max_block_chars {
                push_block(
                    &mut blocks,
                    &mut global_index,
                    &section_heading,
                    local_index,
                    &buffer,
                );
                local_index += 1;
                buffer = paragraph;
            } else {
                buffer = candidate;
            }
        }

        if !buffer.is_empty() {
            push_block(
                &mut blocks,
                &mut global_index,
                &section_heading,
                local_index,
                &buffer,
            );
        }
    }

    blocks
}

fn push_block(
    blocks: &mut Vec<CompactionBlock>,
    global_index: &mut usize,
    section_heading: &str,
    local_index: usize,
    text: &str,
) {
    let text = normalize_text(text);
    if text.is_empty() {
        return;
    }

    let id = format!("B{:02}", *global_index);
    let title = if local_index > 1 {
        format!("{section_heading} / Part {local_index}")
    } else {
        section_heading.to_string()
    };
    blocks.push(CompactionBlock {
        id,
        title,
        current_text: text,
        action: "keep".to_string(),
        bucket: "all".to_string(),
        dropped: false,
        reason: "Not reviewed yet.".to_string(),
        history: Vec::new(),
    });
    *global_index += 1;
}

fn serialize_blocks_for_prompt(blocks: &[CompactionBlock]) -> Vec<PromptBlock<'_>> {
    blocks
        .iter()
        .map(|block| PromptBlock {
            id: &block.id,
            title: &block.title,
            chars: count_chars(&block.current_text),
            destination: &block.bucket,
            text: &block.current_text,
        })
        .collect()
}

fn build_screen_prompt(
    task: &str,
    reservoir_target_chars: usize,
    final_target_chars: usize,
    blocks: &[CompactionBlock],
) -> CodexResult<String> {
    let serialized =
        serde_json::to_string_pretty(&serialize_blocks_for_prompt(blocks)).map_err(|err| {
            CodexErr::InvalidRequest(format!("failed to serialize screen blocks: {err}"))
        })?;
    Ok([
        "You are distilling a long work context into three products.".to_string(),
        "Product 1: continuity narrative.".to_string(),
        "This is a short cause-and-effect story: situation, diagnosed root cause, important turning points, durable decisions, and why the rules still apply.".to_string(),
        "Product 2: continuity anchors.".to_string(),
        "These are hard facts that must not get washed out: IDs, scripts, hosts, artifacts, gates, invariants, prohibitions, and verification paths.".to_string(),
        "Product 3: active focus.".to_string(),
        "This is only the current task state: status, blocker, next step, and finish conditions.".to_string(),
        "Process each block exactly once.".to_string(),
        "action:".to_string(),
        "- keep = preserve unchanged".to_string(),
        "- compress = preserve the meaning, but shorten".to_string(),
        "- drop = safely discard".to_string(),
        "where to keep the block:".to_string(),
        "- story = narrative only".to_string(),
        "- anchor = anchors only".to_string(),
        "- focus = focus only".to_string(),
        "- story_anchor = narrative + anchors".to_string(),
        "- story_focus = narrative + focus".to_string(),
        "- anchor_focus = anchors + focus".to_string(),
        "- all = include in all three products".to_string(),
        "- discard = include in none of the three products".to_string(),
        "Rules:".to_string(),
        "1. Do not invent facts.".to_string(),
        "2. If losing something later would be expensive, it belongs in story or anchor more than in discard.".to_string(),
        "3. Chat noise, UI noise, status spam, trivial intermediate updates, and repeated wording may be dropped.".to_string(),
        "4. If a block carries the main thread but is too wide, use compress instead of drop.".to_string(),
        "5. Fill revised_text only when you shorten a block. Otherwise leave it empty.".to_string(),
        "6. If action is drop, route the block to discard.".to_string(),
        format!(
            "7. After this phase, the retained context should shrink to roughly {reservoir_target_chars} characters or less without losing cause-and-effect history or recovery facts."
        ),
        format!(
            "8. The final three products together should later fit into roughly {final_target_chars} characters."
        ),
        String::new(),
        "<NEXT_STEP>".to_string(),
        if task.is_empty() {
            "(empty)".to_string()
        } else {
            task.to_string()
        },
        "</NEXT_STEP>".to_string(),
        String::new(),
        "<BLOCKS>".to_string(),
        serialized,
        "</BLOCKS>".to_string(),
    ]
    .join("\n"))
}

fn build_progress_prompt(
    task: &str,
    current_model: &str,
    blocks: &[CompactionBlock],
) -> CodexResult<String> {
    let serialized =
        serde_json::to_string_pretty(&serialize_blocks_for_prompt(blocks)).map_err(|err| {
            CodexErr::InvalidRequest(format!("failed to serialize progress blocks: {err}"))
        })?;
    Ok([
        "Evaluate the agent's progress so far for this work context using a strict 1-6 progress score.".to_string(),
        "Score 1 = excellent, clear progress with substantial forward movement.".to_string(),
        "Score 6 = insufficient, no effective movement, looping, dead end, or chaotic state.".to_string(),
        "Score hard and conservatively.".to_string(),
        "Rules:".to_string(),
        "1. Do not invent facts.".to_string(),
        "2. Give good scores only for real movement: decisions, verified partial progress, resolved blockers, or a clean next step.".to_string(),
        "3. Loops, repetition, diffuse state, lack of effectiveness, or growing confusion make the score worse.".to_string(),
        "4. Keep summary short. Use rationale for the main reason behind the score.".to_string(),
        String::new(),
        "<CURRENT_MODEL>".to_string(),
        current_model.to_string(),
        "</CURRENT_MODEL>".to_string(),
        String::new(),
        "<NEXT_STEP>".to_string(),
        if task.is_empty() {
            "(empty)".to_string()
        } else {
            task.to_string()
        },
        "</NEXT_STEP>".to_string(),
        String::new(),
        "<BLOCKS>".to_string(),
        serialized,
        "</BLOCKS>".to_string(),
    ]
    .join("\n"))
}

fn build_iteration_prompt(
    task: &str,
    reservoir_target_chars: usize,
    iteration_index: usize,
    current_chars: usize,
    blocks: &[CompactionBlock],
) -> CodexResult<String> {
    let serialized =
        serde_json::to_string_pretty(&serialize_blocks_for_prompt(blocks)).map_err(|err| {
            CodexErr::InvalidRequest(format!("failed to serialize iteration blocks: {err}"))
        })?;
    Ok([
        "You are refining an already prefiltered retained-context set for three products: continuity narrative, continuity anchors, and active focus.".to_string(),
        format!("Current iteration: {iteration_index}."),
        format!(
            "The retained context currently contains about {current_chars} characters. The target is about {reservoir_target_chars} characters or less without causing expensive knowledge loss later."
        ),
        "Process each block exactly once.".to_string(),
        "Rules:".to_string(),
        "1. Do not invent facts.".to_string(),
        "2. Keep narrative, anchors, and focus separate. Not everything durable belongs in focus, and not every anchor belongs in the story.".to_string(),
        "3. You may move a block between story, anchor, focus, and combination routes if that improves separation.".to_string(),
        "4. For duplicates, only the stronger or more compact version should survive.".to_string(),
        "5. compress may only condense locally. No new claims.".to_string(),
        "6. If action is drop, route the block to discard.".to_string(),
        "7. Keep turning points and cause-effect structure in story, and hard IDs, artifacts, and gates in anchors.".to_string(),
        "8. If the target is already nearly reached, prioritize clarity and clean routing over blind extra shrinking.".to_string(),
        String::new(),
        "<NEXT_STEP>".to_string(),
        if task.is_empty() {
            "(empty)".to_string()
        } else {
            task.to_string()
        },
        "</NEXT_STEP>".to_string(),
        String::new(),
        "<ACTIVE_BLOCKS>".to_string(),
        serialized,
        "</ACTIVE_BLOCKS>".to_string(),
    ]
    .join("\n"))
}

fn build_output_prompt(
    task: &str,
    final_target_chars: usize,
    story_blocks: &[CompactionBlock],
    anchor_blocks: &[CompactionBlock],
    focus_blocks: &[CompactionBlock],
) -> CodexResult<String> {
    let story_serialized = serde_json::to_string_pretty(&serialize_blocks_for_prompt(story_blocks))
        .map_err(|err| {
            CodexErr::InvalidRequest(format!("failed to serialize story blocks: {err}"))
        })?;
    let anchor_serialized = serde_json::to_string_pretty(&serialize_blocks_for_prompt(
        anchor_blocks,
    ))
    .map_err(|err| CodexErr::InvalidRequest(format!("failed to serialize anchor blocks: {err}")))?;
    let focus_serialized = serde_json::to_string_pretty(&serialize_blocks_for_prompt(focus_blocks))
        .map_err(|err| {
            CodexErr::InvalidRequest(format!("failed to serialize focus blocks: {err}"))
        })?;

    Ok([
        "Produce three compact outputs from the filtered context.".to_string(),
        "Output A: continuity narrative.".to_string(),
        "Output B: continuity anchors.".to_string(),
        "Output C: active focus.".to_string(),
        "Rules:".to_string(),
        "1. Do not invent facts.".to_string(),
        "2. story should be a short, robust progress narrative so the main thread survives later recompaction.".to_string(),
        "3. anchor should contain dense hard anchors: IDs, scripts, hosts, artifacts, invariants, gates, and verification paths.".to_string(),
        "4. focus should contain only the immediately active task state: status, blocker, next step, and finish condition.".to_string(),
        "5. If something appears in multiple outputs, keep duplication small.".to_string(),
        format!(
            "6. All three outputs together should be about {final_target_chars} characters or less."
        ),
        "7. story may use clear prose. anchor may be denser and more list-like. focus should stay short and direct.".to_string(),
        "8. No Markdown code fences.".to_string(),
        String::new(),
        "<NEXT_STEP>".to_string(),
        if task.is_empty() {
            "(empty)".to_string()
        } else {
            task.to_string()
        },
        "</NEXT_STEP>".to_string(),
        String::new(),
        "<STORY_BLOCKS>".to_string(),
        story_serialized,
        "</STORY_BLOCKS>".to_string(),
        String::new(),
        "<ANCHOR_BLOCKS>".to_string(),
        anchor_serialized,
        "</ANCHOR_BLOCKS>".to_string(),
        String::new(),
        "<FOCUS_BLOCKS>".to_string(),
        focus_serialized,
        "</FOCUS_BLOCKS>".to_string(),
    ]
    .join("\n"))
}

fn build_reprioritization_prompt(
    task: &str,
    trigger: CompactionTrigger,
    output: &OutputStageResponse,
    recent_user_messages: &[String],
) -> CodexResult<String> {
    let recent_messages = serde_json::to_string_pretty(recent_user_messages).map_err(|err| {
        CodexErr::InvalidRequest(format!("failed to serialize recent user messages: {err}"))
    })?;
    Ok([
        "After context compaction, choose what the same agent run should continue now.".to_string(),
        "Do not build a competing agent loop. This step only preserves the current task state and priority.".to_string(),
        "If an interrupt triggered this compaction, treat that as a reprioritization signal.".to_string(),
        "An interrupt does not always include concrete extra details. Do not invent interrupt contents; use only the signal that priority may have changed plus the visible context.".to_string(),
        "Rules:".to_string(),
        "1. Do not invent facts.".to_string(),
        "2. active_task must name exactly the task or small task group that is currently in front.".to_string(),
        "3. task_packet must stay small and concrete. No broad roadmap.".to_string(),
        "4. completed_tasks lists tasks that are now done.".to_string(),
        "5. follow_up_tasks lists only distinct bounded follow-up work that should already exist or be created by the parent task later; do not create review-driven work cascades.".to_string(),
        "6. mutated_tasks adjust existing tasks. action must be one of complete, revise, split, drop, defer.".to_string(),
        "7. priority_order ranks the most important tasks or small task groups in order.".to_string(),
        "8. If prioritization must be rechecked because of an interrupt or a changed situation, set should_reprioritize to true.".to_string(),
        "9. next_action must be one of four labels: continue_current_task, reprioritize, switch_task, park_current_task.".to_string(),
        "10. interrupts_reviewed is true if you accounted for the interrupt signal in your assessment.".to_string(),
        "11. No Markdown code fences.".to_string(),
        String::new(),
        "<TRIGGER>".to_string(),
        trigger.as_str().to_string(),
        "</TRIGGER>".to_string(),
        String::new(),
        "<NEXT_STEP>".to_string(),
        if task.is_empty() {
            "(empty)".to_string()
        } else {
            task.to_string()
        },
        "</NEXT_STEP>".to_string(),
        String::new(),
        "<CONTINUITY_NARRATIVE>".to_string(),
        normalize_text(&output.story_draft),
        "</CONTINUITY_NARRATIVE>".to_string(),
        String::new(),
        "<CONTINUITY_ANCHORS>".to_string(),
        normalize_text(&output.anchor_draft),
        "</CONTINUITY_ANCHORS>".to_string(),
        String::new(),
        "<ACTIVE_FOCUS>".to_string(),
        normalize_text(&output.focus_draft),
        "</ACTIVE_FOCUS>".to_string(),
        String::new(),
        "<RECENT_USER_MESSAGES>".to_string(),
        recent_messages,
        "</RECENT_USER_MESSAGES>".to_string(),
    ]
    .join("\n"))
}

fn screen_schema() -> Value {
    decision_stage_schema()
}

fn iteration_schema() -> Value {
    decision_stage_schema()
}

fn progress_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "progress_score", "rationale"],
        "properties": {
            "summary": {"type": "string"},
            "progress_score": {"type": "integer", "minimum": 1, "maximum": 6},
            "rationale": {"type": "string"}
        }
    })
}

fn decision_stage_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "decisions"],
        "properties": {
            "summary": {"type": "string"},
            "decisions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["id", "action", "destination", "reason", "revised_text"],
                    "properties": {
                        "id": {"type": "string"},
                        "action": {"type": "string", "enum": ["keep", "compress", "drop"]},
                        "destination": {"type": "string", "enum": ROUTE_VALUES},
                        "reason": {"type": "string"},
                        "revised_text": {"type": "string"}
                    }
                }
            }
        }
    })
}

fn output_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "story_title",
            "story_draft",
            "anchor_title",
            "anchor_draft",
            "focus_title",
            "focus_draft"
        ],
        "properties": {
            "story_title": {"type": "string"},
            "story_draft": {"type": "string"},
            "anchor_title": {"type": "string"},
            "anchor_draft": {"type": "string"},
            "focus_title": {"type": "string"},
            "focus_draft": {"type": "string"}
        }
    })
}

fn reprioritization_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "summary",
            "should_reprioritize",
            "interrupts_reviewed",
            "active_task",
            "task_packet",
            "priority_reason",
            "next_action",
            "completed_tasks",
            "follow_up_tasks",
            "mutated_tasks",
            "priority_order"
        ],
        "properties": {
            "summary": {"type": "string"},
            "should_reprioritize": {"type": "boolean"},
            "interrupts_reviewed": {"type": "boolean"},
            "active_task": {"type": "string"},
            "task_packet": {
                "type": "array",
                "items": {"type": "string"}
            },
            "priority_reason": {"type": "string"},
            "next_action": {
                "type": "string",
                "enum": [
                    "continue_current_task",
                    "reprioritize",
                    "switch_task",
                    "park_current_task"
                ]
            },
            "completed_tasks": {
                "type": "array",
                "items": {"type": "string"}
            },
            "follow_up_tasks": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["title", "detail", "priority"],
                    "properties": {
                        "title": {"type": "string"},
                        "detail": {"type": "string"},
                        "priority": {
                            "type": "string",
                            "enum": ["now", "next", "later"]
                        }
                    }
                }
            },
            "mutated_tasks": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["target", "action", "revised_title", "revised_detail", "reason"],
                    "properties": {
                        "target": {"type": "string"},
                        "action": {
                            "type": "string",
                            "enum": ["complete", "revise", "split", "drop", "defer"]
                        },
                        "revised_title": {"type": "string"},
                        "revised_detail": {"type": "string"},
                        "reason": {"type": "string"}
                    }
                }
            },
            "priority_order": {
                "type": "array",
                "items": {"type": "string"}
            }
        }
    })
}

fn retained_blocks(blocks: &[CompactionBlock]) -> Vec<CompactionBlock> {
    blocks
        .iter()
        .filter(|block| !block.dropped)
        .cloned()
        .collect()
}

fn retained_chars(blocks: &[CompactionBlock]) -> usize {
    blocks
        .iter()
        .filter(|block| !block.dropped)
        .map(|block| count_chars(&block.current_text))
        .sum()
}

fn bucket_blocks(blocks: &[CompactionBlock], target: &str) -> Vec<CompactionBlock> {
    blocks
        .iter()
        .filter(|block| !block.dropped && route_includes(&block.bucket, target))
        .cloned()
        .collect()
}

fn route_includes(route: &str, target: &str) -> bool {
    route_targets(route).contains(&target)
}

fn route_targets(route: &str) -> &'static [&'static str] {
    match route {
        "story" => &["story"],
        "anchor" => &["anchor"],
        "focus" => &["focus"],
        "story_anchor" => &["story", "anchor"],
        "story_focus" => &["story", "focus"],
        "anchor_focus" => &["anchor", "focus"],
        "all" => &["story", "anchor", "focus"],
        "discard" => &[],
        _ => &["story", "anchor", "focus"],
    }
}

fn normalize_bucket(action: &str, bucket: &str, fallback: &str) -> String {
    if action == "drop" {
        return "discard".to_string();
    }
    if ROUTE_VALUES.contains(&bucket) && bucket != "discard" {
        return bucket.to_string();
    }
    if ROUTE_VALUES.contains(&fallback) && fallback != "discard" {
        return fallback.to_string();
    }
    "all".to_string()
}

fn coerce_revised_text(action: &str, revised_text: &str, fallback: &str) -> String {
    let clean_fallback = normalize_text(fallback);
    if action == "drop" {
        return String::new();
    }
    let clean = normalize_text(revised_text);
    if clean.is_empty() {
        return clean_fallback;
    }
    if count_chars(&clean) > count_chars(&clean_fallback) + REVISED_TEXT_OVERFLOW_TOLERANCE {
        return clean_fallback;
    }
    clean
}

fn apply_decisions(
    blocks: &mut [CompactionBlock],
    decisions: &[BlockDecision],
    stage_label: &str,
) -> Vec<AppliedDecision> {
    let decision_map = decisions
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();

    let mut changes = Vec::new();
    for block in blocks.iter_mut() {
        if block.dropped {
            continue;
        }

        let incoming = decision_map.get(block.id.as_str());
        let action = incoming
            .map(|item| item.action.as_str())
            .filter(|action| matches!(*action, "keep" | "compress" | "drop"))
            .unwrap_or("keep");
        let previous_text = block.current_text.clone();
        let previous_bucket = block.bucket.clone();
        let next_text = coerce_revised_text(
            action,
            incoming
                .map(|item| item.revised_text.as_str())
                .unwrap_or(""),
            &previous_text,
        );
        let bucket = normalize_bucket(
            action,
            incoming
                .map(|item| item.destination.as_str())
                .unwrap_or(block.bucket.as_str()),
            &block.bucket,
        );
        let reason = normalize_text(
            incoming
                .map(|item| item.reason.as_str())
                .unwrap_or("Keine Modellentscheidung vorhanden, daher konservativ behalten."),
        );

        block.action = action.to_string();
        block.reason = if reason.is_empty() {
            "Keine Begruendung.".to_string()
        } else {
            reason
        };
        block.bucket = bucket.clone();

        if action == "drop" {
            block.dropped = true;
        } else {
            block.current_text = next_text;
        }

        let before_chars = count_chars(&previous_text);
        let after_chars = if action == "drop" {
            0
        } else {
            count_chars(&block.current_text)
        };
        let changed = action != "keep"
            || normalize_text(&previous_text) != normalize_text(&block.current_text)
            || bucket != previous_bucket;

        let _ = (stage_label, before_chars, after_chars);
        block.history.push(BlockHistoryEntry);
        changes.push(AppliedDecision { changed });
    }
    changes
}

async fn run_structured_prompt<T: DeserializeOwned>(
    sess: &Session,
    turn_context: &TurnContext,
    prompt_text: String,
    output_schema: Value,
) -> CodexResult<T> {
    let prompt = Prompt {
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text: prompt_text }],
            end_turn: None,
            phase: None,
        }],
        tools: Vec::new(),
        parallel_tool_calls: false,
        base_instructions: sess.get_base_instructions().await,
        personality: turn_context.personality,
        output_schema: Some(output_schema),
    };

    let max_retries = turn_context.provider.stream_max_retries();
    let mut retries = 0usize;
    let mut client_session = sess.services.model_client.new_session();

    loop {
        let attempt =
            run_structured_prompt_once::<T>(sess, turn_context, &mut client_session, &prompt).await;
        match attempt {
            Ok(value) => return Ok(value),
            Err(CodexErr::Interrupted) => return Err(CodexErr::Interrupted),
            Err(CodexErr::ContextWindowExceeded) => return Err(CodexErr::ContextWindowExceeded),
            Err(err) => {
                if retries < max_retries as usize {
                    retries += 1;
                    let delay = backoff(retries as u64);
                    sess.notify_stream_error(
                        turn_context,
                        format!("Reconnecting... {retries}/{max_retries}"),
                        err,
                    )
                    .await;
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Err(err);
            }
        }
    }
}

async fn run_structured_prompt_once<T: DeserializeOwned>(
    sess: &Session,
    turn_context: &TurnContext,
    client_session: &mut ModelClientSession,
    prompt: &Prompt,
) -> CodexResult<T> {
    let turn_metadata_header = turn_context.turn_metadata_state.current_header_value();
    let mut stream = client_session
        .stream(
            prompt,
            &turn_context.model_info,
            &turn_context.session_telemetry,
            turn_context.reasoning_effort,
            turn_context.reasoning_summary,
            turn_context.config.service_tier,
            turn_metadata_header.as_deref(),
        )
        .await?;

    let mut result = String::new();
    while let Some(event) = stream.next().await.transpose()? {
        match event {
            ResponseEvent::OutputTextDelta(delta) => result.push_str(&delta),
            ResponseEvent::OutputItemDone(item) => {
                if result.is_empty() {
                    if let Some(message) = raw_assistant_output_text_from_item(&item) {
                        result.push_str(&message);
                    } else if let ResponseItem::Message { content, .. } = &item
                        && let Some(text) = content_items_to_text(content)
                    {
                        result.push_str(&text);
                    }
                }
            }
            ResponseEvent::ServerReasoningIncluded(included) => {
                sess.set_server_reasoning_included(included).await;
            }
            ResponseEvent::RateLimits(snapshot) => {
                sess.update_rate_limits(turn_context, snapshot).await;
            }
            ResponseEvent::Completed { token_usage, .. } => {
                sess.update_token_usage_info(turn_context, token_usage.as_ref())
                    .await;
                let payload = normalize_text(&result);
                if payload.is_empty() {
                    return Err(CodexErr::Stream(
                        "structured compaction stream completed without output".into(),
                        None,
                    ));
                }
                return serde_json::from_str::<T>(&payload).map_err(|err| {
                    CodexErr::InvalidRequest(format!(
                        "failed to parse structured compaction response: {err}; payload={payload}"
                    ))
                });
            }
            _ => {}
        }
    }

    Err(CodexErr::Stream(
        "stream closed before response.completed".into(),
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_task_hint_ignores_legacy_summary_prompt() {
        let input = vec![UserInput::Text {
            text: SUMMARIZATION_PROMPT.to_string(),
            text_elements: Vec::new(),
        }];
        let history = vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "Fix the interrupt reprioritization path.".to_string(),
            }],
            end_turn: None,
            phase: None,
        }];

        let hint = derive_task_hint(&input, &history);
        assert_eq!(hint, "Fix the interrupt reprioritization path.");
    }

    #[test]
    fn segment_context_splits_markdown_sections_into_blocks() {
        let text = "## One\nAAAA\n\nBBBB\n\n## Two\nCCCC";
        let blocks = segment_context(text, 5);
        let titles = blocks
            .iter()
            .map(|block| block.title.as_str())
            .collect::<Vec<_>>();
        assert_eq!(titles, vec!["One", "One / Part 2", "Two"]);
    }

    #[test]
    fn apply_decisions_marks_drop_as_discard() {
        let mut blocks = vec![CompactionBlock {
            id: "B01".to_string(),
            title: "Test".to_string(),
            current_text: "alpha".to_string(),
            action: "keep".to_string(),
            bucket: "all".to_string(),
            dropped: false,
            reason: String::new(),
            history: Vec::new(),
        }];
        let decisions = vec![BlockDecision {
            id: "B01".to_string(),
            action: "drop".to_string(),
            destination: "story".to_string(),
            reason: "noise".to_string(),
            revised_text: String::new(),
        }];

        apply_decisions(&mut blocks, &decisions, "test");

        assert!(blocks[0].dropped);
        assert_eq!(blocks[0].bucket, "discard");
    }

    #[test]
    fn select_model_routing_prefers_current_model_when_it_already_matches_tier() {
        let routing = select_model_routing("Qwen3.5-35B-A3B", 2, Vec::new());
        assert_eq!(routing.tier, "simple");
        assert_eq!(routing.requested_model, "Qwen3.5-35B-A3B");
        assert!(!routing.switch_planned);
    }

    #[test]
    fn model_slug_matches_handles_namespaced_suffixes() {
        assert!(model_slug_matches(
            "custom_openai>Qwen/Qwen3.5-35B-A3B",
            "Qwen3.5-35B-A3B"
        ));
        assert!(model_slug_matches("openai/gpt-oss-120b", "gpt-oss-120b"));
    }

    #[test]
    fn reprioritization_response_accepts_snake_case_spawned_and_mutated_tasks() {
        let payload = serde_json::json!({
            "summary": "Continue the public launch fix.",
            "should_reprioritize": true,
            "interrupts_reviewed": true,
            "active_task": "Public launch surface rework",
            "task_packet": ["Fix homepage copy"],
            "priority_reason": "Public surface is still broken.",
            "next_action": "reprioritize",
            "completed_tasks": [],
            "follow_up_tasks": [{
                "title": "Public surface verification pass",
                "detail": "Re-check nav and homepage.",
                "priority": "now"
            }],
            "mutated_tasks": [{
                "target": "homepage",
                "action": "revise",
                "revised_title": "Homepage launch copy",
                "revised_detail": "Rewrite the hero and proof sections.",
                "reason": "Current copy is internal."
            }],
            "priority_order": ["Public launch surface rework"]
        });

        let parsed: ReprioritizationStageResponse =
            serde_json::from_value(payload).expect("snake_case payload should parse");

        assert_eq!(parsed.follow_up_tasks.len(), 1);
        assert_eq!(parsed.follow_up_tasks[0].priority, "now");
        assert_eq!(parsed.mutated_tasks.len(), 1);
        assert_eq!(
            parsed.mutated_tasks[0].revised_title,
            "Homepage launch copy"
        );
    }

    #[test]
    fn task_structs_still_accept_legacy_camel_case_fields() {
        let spawn: TaskSpawn = serde_json::from_value(serde_json::json!({
            "title": "Verify launch surface",
            "detail": "Check the top-level navigation.",
            "priorityBucket": "later"
        }))
        .expect("legacy camelCase task spawn should still parse");
        assert_eq!(spawn.priority, "later");

        let mutation: TaskMutation = serde_json::from_value(serde_json::json!({
            "target": "catalog",
            "action": "revise",
            "revisedTitle": "Catalog commercial copy",
            "revisedDetail": "Replace operator wording.",
            "reason": "Current phrasing is too internal."
        }))
        .expect("legacy camelCase task mutation should still parse");
        assert_eq!(mutation.revised_title, "Catalog commercial copy");
    }
}
