use anyhow::Context;
use anyhow::Result;
use sha2::Digest;
use std::collections::BTreeMap;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use crate::channels;
use crate::context_health;
use crate::governance;
use crate::inference::engine;
use crate::inference::model_adapters::LocalCodexExecPolicy;
use crate::inference::runtime_env;
use crate::inference::runtime_kernel;
use crate::inference::runtime_state;
use crate::inference::supervisor;
use crate::inference::turn_contract;
use crate::inference::turn_engine;
use crate::lcm;
use crate::live_context;

pub const CHAT_CONVERSATION_ID: i64 = 1;
const DEFAULT_CONTINUITY_REFRESH_TIMEOUT_SECS: u64 = 45;
const DEFAULT_REMOTE_CHAT_TURN_TIMEOUT_SECS: u64 = 180;
const DEFAULT_LOCAL_CHAT_TURN_TIMEOUT_SECS: u64 = 900;
const CHAT_MODEL_REASONING_EFFORT_ENV_KEY: &str = "CTOX_CHAT_MODEL_REASONING_EFFORT";
const CHAT_SKILL_PRESET_ENV_KEY: &str = "CTOX_CHAT_SKILL_PRESET";
const CONTINUITY_REFRESH_FAULT_FILE_ENV_KEY: &str = "CTOX_CONTINUITY_REFRESH_FAULT_FILE";
const CONTINUITY_REFRESH_TIMEOUT_ENV_KEY: &str = "CTOX_CONTINUITY_REFRESH_TIMEOUT_SECS";
const TOOL_VERIFICATION_RETRY_INSTRUCTIONS: &str = "Your previous completion tried to answer without using tools. That is invalid for this task. Emit a tool call now. Start by inspecting the workspace or running the required command. Do not emit a final answer until tool output proves the required filesystem or build result.";
const CTOX_CODEX_EXEC_STANDARD_OVERLAY: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_standard_overlay.md");
const CTOX_CODEX_EXEC_SIMPLE_OVERLAY: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_overlay.md");
const CTOX_CODEX_EXEC_SIMPLE_TASK_ROUTER: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_task_router.md");
const CTOX_CODEX_EXEC_SIMPLE_SMALL_STEP_CORE: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_small_step_core.md");
const CTOX_CODEX_EXEC_SIMPLE_TERMINAL_OPS_CORE: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_terminal_ops_core.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_ANALYSIS_READ_ONLY: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_analysis_read_only.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_DOCS_TEXT_CHANGE: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_docs_text_change.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_GENERAL_SAFE_TASK: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_general_safe_task.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_READ_TRACE_FIRST: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_read_trace_first.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_BUG_FIX: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_bug_fix.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_FEATURE_ADD: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_feature_add.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_REFACTOR_SAFE: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_refactor_safe.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_TEST_WORK: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_test_work.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_CODE_REVIEW: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_code_review.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_BUG_FIX_WITH_TESTS: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_bug_fix_with_tests.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_DEPENDENCY_UPDATE: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_dependency_update.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_MIGRATION_WITH_DATA_CHANGE: &str = include_str!(
    "../../../assets/prompts/ctox_codex_exec_simple_phase_migration_with_data_change.md"
);
const CTOX_CODEX_EXEC_SIMPLE_PHASE_MIGRATION_CHANGE: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_migration_change.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_DATA_CHANGE_SAFE: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_data_change_safe.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_INFRA_CONFIG_CHANGE: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_infra_config_change.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_DEPLOY_RELEASE: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_deploy_release.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_OPS_DEBUG_TERMINAL: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_ops_debug_terminal.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_INCIDENT_HOTFIX: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_incident_hotfix.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_INCIDENT_HOTFIX_DEPLOY: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_incident_hotfix_deploy.md");
const CTOX_CODEX_EXEC_SIMPLE_PHASE_ROLLBACK_RECOVERY: &str =
    include_str!("../../../assets/prompts/ctox_codex_exec_simple_phase_rollback_recovery.md");

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexExecBinary {
    path: PathBuf,
    working_dir: PathBuf,
}

impl CodexExecBinary {
    fn resolve(root: &Path, workspace_root: Option<&Path>) -> Result<Self> {
        let binary = engine::discover_source_layout_paths(root).codex_exec_binary;
        if !binary.is_file() {
            anyhow::bail!(
                "required codex-exec binary is missing at {}. CTOX no longer falls back to source execution; build or install tools/agent-runtime first",
                binary.display()
            );
        }
        Ok(Self {
            path: binary,
            working_dir: workspace_root
                .filter(|path| path.is_dir())
                .unwrap_or(root)
                .to_path_buf(),
        })
    }

    fn into_command(self) -> Command {
        let mut command = Command::new(self.path);
        command.current_dir(self.working_dir);
        command
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalModelProviderSpec {
    socket_path: String,
}

impl LocalModelProviderSpec {
    fn resolve(
        model: &str,
        resolved_runtime: Option<&runtime_kernel::InferenceRuntimeKernel>,
    ) -> Option<Self> {
        if !engine::uses_ctox_proxy_model(model) {
            return None;
        }
        let socket_path = resolved_local_socket_path(resolved_runtime)?;
        Some(Self { socket_path })
    }

    fn provider_id(&self) -> &'static str {
        "cto_local"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ApiModelProviderSpec {
    provider_id: &'static str,
    name: &'static str,
    base_url: String,
    env_key: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexExecConfigSpec {
    model_instructions_file: PathBuf,
    project_doc_max_bytes: Option<usize>,
    model_context_window: Option<usize>,
    model_auto_compact_token_limit: Option<usize>,
    model_reasoning_effort: Option<String>,
    web_search_mode: &'static str,
    include_apply_patch_tool: bool,
    unified_exec_enabled: bool,
    ctox_web_enabled: bool,
    disable_skills: bool,
    local_model_provider: Option<LocalModelProviderSpec>,
    api_model_provider: Option<ApiModelProviderSpec>,
    extra_cli_overrides: Vec<String>,
}

impl CodexExecConfigSpec {
    fn into_cli_overrides(self) -> Vec<String> {
        let mut overrides = vec![
            "features.multi_agent=false".to_string(),
            "features.apps=false".to_string(),
            "features.plugins=false".to_string(),
            "features.tool_suggest=false".to_string(),
            "features.memories=false".to_string(),
            "features.child_agents_md=false".to_string(),
            format!(
                "features.apply_patch_freeform={}",
                self.include_apply_patch_tool
            ),
            format!("features.unified_exec={}", self.unified_exec_enabled),
            format!(
                "model_instructions_file=\"{}\"",
                escape_inline_toml_string(&self.model_instructions_file.display().to_string())
            ),
            format!("web_search=\"{}\"", self.web_search_mode),
        ];
        if let Some(project_doc_max_bytes) = self.project_doc_max_bytes {
            overrides.push(format!("project_doc_max_bytes={project_doc_max_bytes}"));
        }
        if self.disable_skills {
            overrides.extend([
                "skills.enabled=false".to_string(),
                "skills.bundled.enabled=false".to_string(),
            ]);
        }
        if let Some(reasoning_effort) = self.model_reasoning_effort {
            overrides.push(format!("model_reasoning_effort=\"{reasoning_effort}\""));
        }
        if self.ctox_web_enabled {
            overrides.push("tools.ctox_web=true".to_string());
        }
        if let Some(model_context_window) = self.model_context_window {
            overrides.push(format!("model_context_window={model_context_window}"));
        }
        if let Some(model_auto_compact_token_limit) = self.model_auto_compact_token_limit {
            overrides.push(format!(
                "model_auto_compact_token_limit={model_auto_compact_token_limit}"
            ));
        }
        if let Some(provider) = self.local_model_provider {
            let provider_id = provider.provider_id();
            overrides.push(format!("model_provider=\"{provider_id}\""));
            overrides.extend([
                format!("model_providers.{provider_id}.name=\"cto-local\""),
                format!("model_providers.{provider_id}.socket_transport_required=true"),
                format!(
                    "model_providers.{provider_id}.socket_path=\"{}\"",
                    escape_inline_toml_string(&provider.socket_path)
                ),
                format!("model_providers.{provider_id}.wire_api=\"responses\""),
                format!("model_providers.{provider_id}.requires_openai_auth=false"),
            ]);
        }
        if let Some(provider) = self.api_model_provider {
            overrides.push(format!("model_provider=\"{}\"", provider.provider_id));
            overrides.extend([
                format!(
                    "model_providers.{}.name=\"{}\"",
                    provider.provider_id, provider.name
                ),
                format!(
                    "model_providers.{}.base_url=\"{}\"",
                    provider.provider_id,
                    escape_inline_toml_string(&provider.base_url)
                ),
                format!(
                    "model_providers.{}.env_key=\"{}\"",
                    provider.provider_id, provider.env_key
                ),
                format!(
                    "model_providers.{}.wire_api=\"responses\"",
                    provider.provider_id
                ),
                format!(
                    "model_providers.{}.requires_openai_auth=false",
                    provider.provider_id
                ),
            ]);
        }
        overrides.extend(self.extra_cli_overrides);
        overrides
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexExecInvocation {
    model: String,
    workspace_root: Option<PathBuf>,
    prompt: String,
    config: CodexExecConfigSpec,
}

impl CodexExecInvocation {
    fn into_args(self) -> Vec<String> {
        let mut args = vec![
            "-m".to_string(),
            self.model,
            "--skip-git-repo-check".to_string(),
            "--dangerously-bypass-approvals-and-sandbox".to_string(),
            "--json".to_string(),
        ];
        if let Some(workspace_root) = self.workspace_root {
            args.splice(
                0..0,
                ["-C".to_string(), workspace_root.display().to_string()],
            );
        }
        for override_entry in self.config.into_cli_overrides() {
            args.extend(["-c".to_string(), override_entry]);
        }
        args.extend(["--".to_string(), self.prompt]);
        args
    }
}

pub fn run_chat_turn_with_events<F>(
    root: &Path,
    db_path: &Path,
    prompt: &str,
    workspace_root: Option<&Path>,
    conversation_id: i64,
    suggested_skill: Option<&str>,
    mut emit: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let runtime = runtime_kernel::InferenceRuntimeKernel::resolve(root)?;
    let operator_settings = runtime_env::effective_operator_env_map(root).unwrap_or_default();
    let default_turn_timeout_secs = if runtime.state.source.is_local() {
        DEFAULT_LOCAL_CHAT_TURN_TIMEOUT_SECS
    } else {
        DEFAULT_REMOTE_CHAT_TURN_TIMEOUT_SECS
    };
    let config = turn_engine::ChatTurnConfig {
        max_context_tokens: runtime.turn_context_tokens(),
        turn_timeout_secs: read_usize_setting(
            &operator_settings,
            "CTOX_CHAT_TURN_TIMEOUT_SECS",
            default_turn_timeout_secs as usize,
        ) as u64,
    };
    emit("lcm-open");
    let engine = lcm::LcmEngine::open(db_path, lcm::LcmConfig::default())?;
    let _ = engine.continuity_init_documents(conversation_id)?;
    emit("turn-plan");
    let plan = turn_engine::build_turn_plan(&engine, conversation_id, config.clone())?;
    emit(&format!(
        "turn-plan context={} timeout={}s stage={}",
        plan.max_context_tokens,
        plan.turn_timeout_secs,
        plan.stage.as_str()
    ));
    emit("compaction-check");
    let decision = plan.compaction.clone();
    emit(&format!(
        "compaction-window {} / {} ({})",
        decision.current_tokens, decision.threshold, decision.reason
    ));
    let mut compaction_result = None;
    if decision.should_compact {
        emit("compaction-run");
        let result = engine.compact(
            conversation_id,
            config.max_context_tokens,
            &lcm::HeuristicSummarizer,
            false,
        )?;
        emit(&format!(
            "compaction-result before={} after={} rounds={} created={}",
            result.tokens_before,
            result.tokens_after,
            result.rounds,
            result.created_summary_ids.len()
        ));
        compaction_result = Some(result);
        emit("compaction-complete");
    }
    let compaction_guard =
        turn_engine::assess_compaction_guard(&decision, compaction_result.as_ref());
    emit(&format!("compaction-guard {}", compaction_guard.summary));
    emit("persist-user-turn");
    lcm::run_add_message(db_path, conversation_id, "user", prompt)
        .context("failed to persist user message into LCM")?;
    emit("snapshot-context");
    let snapshot = engine.snapshot(conversation_id)?;
    let continuity = engine.continuity_show_all(conversation_id)?;
    let mission_state = engine.mission_state(conversation_id)?;
    let mission_assurance = engine.mission_assurance_snapshot(conversation_id)?;
    let forgotten_entries = engine.continuity_forgotten(conversation_id, None, None)?;
    let health = context_health::assess_with_forgotten(
        &snapshot,
        &continuity,
        &forgotten_entries,
        prompt,
        config.max_context_tokens,
    );
    let governance_snapshot =
        governance::prompt_snapshot(root, conversation_id).unwrap_or_default();
    emit(&format!(
        "context-health {} {}",
        health.status.as_str(),
        health.overall_score
    ));
    emit("render-prompt");
    let rendered_prompt = live_context::render_runtime_prompt(
        root,
        &snapshot,
        &continuity,
        &mission_state,
        &mission_assurance,
        &governance_snapshot,
        &health,
        suggested_skill,
    )?;
    emit(&format!(
        "context-selection rendered={} omitted={}",
        rendered_prompt.rendered_context_items, rendered_prompt.omitted_context_items
    ));
    emit("invoke-model");
    let reply = invoke_codex_exec_with_timeout(
        root,
        &operator_settings,
        &rendered_prompt.prompt,
        workspace_root,
        Some(Duration::from_secs(config.turn_timeout_secs)),
    )?;
    emit("persist-assistant-turn");
    lcm::run_add_message(db_path, conversation_id, "assistant", &reply)?;
    let engine = lcm::LcmEngine::open(db_path, lcm::LcmConfig::default())?;
    emit("continuity-refresh");
    let continuity_stats = refresh_continuity_documents(
        root,
        &operator_settings,
        &engine,
        workspace_root,
        conversation_id,
        &mut emit,
    )?;
    let outcome = turn_engine::ChatTurnOutcome {
        stage: turn_engine::TurnStage::Complete,
        health_status: health.status,
        health_score: health.overall_score,
        context_items_rendered: rendered_prompt.rendered_context_items,
        context_items_omitted: rendered_prompt.omitted_context_items,
        reply_chars: reply.chars().count(),
        compaction: compaction_result,
        continuity: continuity_stats,
        compaction_guard,
    };
    emit(&format!(
        "turn-outcome stage={} health={} score={} reply_chars={} continuity_updates={} continuity_skips={} omitted={}",
        outcome.stage.as_str(),
        outcome.health_status.as_str(),
        outcome.health_score,
        outcome.reply_chars,
        outcome.continuity.updated,
        outcome.continuity.skipped_prompt_build
            + outcome.continuity.skipped_invoke
            + outcome.continuity.skipped_apply,
        outcome.context_items_omitted
    ));
    emit("turn-complete");
    Ok(reply)
}

#[cfg(test)]
#[path = "turn_loop_boundary_tests.rs"]
mod boundary_tests;

fn refresh_continuity_documents(
    root: &Path,
    settings: &BTreeMap<String, String>,
    engine: &lcm::LcmEngine,
    workspace_root: Option<&Path>,
    conversation_id: i64,
    emit: &mut impl FnMut(&str),
) -> Result<turn_engine::ContinuityRefreshStats> {
    let mut stats = turn_engine::ContinuityRefreshStats::default();
    let refresh_timeout_secs = continuity_refresh_timeout_secs(settings);
    for kind in [
        lcm::ContinuityKind::Narrative,
        lcm::ContinuityKind::Anchors,
        lcm::ContinuityKind::Focus,
    ] {
        let kind_label = match kind {
            lcm::ContinuityKind::Narrative => "narrative",
            lcm::ContinuityKind::Anchors => "anchors",
            lcm::ContinuityKind::Focus => "focus",
        };
        stats.attempted += 1;
        emit(&format!("continuity-{kind_label}-build"));
        let payload = match engine.continuity_build_prompt(conversation_id, kind) {
            Ok(payload) => payload,
            Err(err) => {
                stats.skipped_prompt_build += 1;
                eprintln!("ctox continuity refresh skipped {kind_label} prompt build: {err}");
                continue;
            }
        };
        emit(&format!("continuity-{kind_label}-invoke"));
        let diff = match take_continuity_refresh_fault(root, settings, kind_label) {
            Ok(Some(diff)) => {
                emit(&format!("continuity-{kind_label}-fault-injected"));
                eprintln!(
                    "ctox continuity refresh injected {kind_label} fault preview: {}",
                    summarize_continuity_diff_for_log(&diff)
                );
                diff
            }
            Ok(None) => match invoke_codex_exec_with_timeout(
                root,
                settings,
                &payload.prompt,
                workspace_root,
                Some(Duration::from_secs(refresh_timeout_secs)),
            ) {
                Ok(diff) => diff,
                Err(err) => {
                    stats.skipped_invoke += 1;
                    eprintln!("ctox continuity refresh skipped {kind_label} invocation: {err}");
                    continue;
                }
            },
            Err(err) => {
                stats.skipped_invoke += 1;
                eprintln!("ctox continuity refresh skipped {kind_label} fault injection: {err}");
                continue;
            }
        };
        let repaired_diff = match repair_continuity_refresh_output(&diff) {
            ContinuityRefreshRepair::Apply {
                diff: repaired,
                repair_reason,
            } => {
                if let Some(reason) = repair_reason {
                    eprintln!(
                        "ctox continuity refresh repaired {kind_label} response before apply: {reason}"
                    );
                }
                repaired
            }
            ContinuityRefreshRepair::Noop { reason } => {
                eprintln!(
                    "ctox continuity refresh treated {kind_label} response as no-op: {reason}"
                );
                continue;
            }
        };
        eprintln!(
            "ctox continuity refresh {kind_label} diff len={} empty={} preview={}",
            repaired_diff.len(),
            repaired_diff.trim().is_empty(),
            summarize_continuity_diff_for_log(&repaired_diff)
        );
        if !repaired_diff.trim().is_empty() {
            emit(&format!("continuity-{kind_label}-apply"));
            if let Err(err) =
                engine.continuity_apply_diff(conversation_id, kind, repaired_diff.trim())
            {
                stats.skipped_apply += 1;
                eprintln!("ctox continuity refresh skipped invalid {kind_label} diff: {err}");
                eprintln!(
                    "ctox continuity refresh invalid {kind_label} diff preview: {}",
                    summarize_continuity_diff_for_log(&repaired_diff)
                );
                if repaired_diff.trim() != diff.trim() {
                    eprintln!(
                        "ctox continuity refresh invalid {kind_label} raw response preview: {}",
                        summarize_continuity_diff_for_log(&diff)
                    );
                }
            } else {
                stats.updated += 1;
            }
        }
        if kind == lcm::ContinuityKind::Anchors {
            emit("continuity-anchors-preserve-literals");
            match engine.continuity_preserve_recent_anchor_literals(conversation_id) {
                Ok(Some(_)) => stats.updated += 1,
                Ok(None) => {}
                Err(err) => {
                    stats.skipped_apply += 1;
                    eprintln!("ctox continuity refresh skipped anchor literal preservation: {err}");
                }
            }
        }
    }
    Ok(stats)
}

enum ContinuityRefreshRepair {
    Apply {
        diff: String,
        repair_reason: Option<&'static str>,
    },
    Noop {
        reason: &'static str,
    },
}

fn repair_continuity_refresh_output(raw: &str) -> ContinuityRefreshRepair {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ContinuityRefreshRepair::Noop {
            reason: "empty response",
        };
    }
    if let Some(summary) = turn_contract::summarize_event_stream(trimmed) {
        if let Some(text) = summary
            .final_text
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            return ContinuityRefreshRepair::Apply {
                diff: strip_markdown_code_fences(text),
                repair_reason: Some("extracted final agent message from event stream"),
            };
        }
        return ContinuityRefreshRepair::Noop {
            reason: "event stream contained no non-empty agent message",
        };
    }
    let unfenced = strip_markdown_code_fences(trimmed);
    if unfenced != trimmed {
        return ContinuityRefreshRepair::Apply {
            diff: unfenced,
            repair_reason: Some("removed markdown code fences"),
        };
    }
    ContinuityRefreshRepair::Apply {
        diff: trimmed.to_string(),
        repair_reason: None,
    }
}

fn strip_markdown_code_fences(text: &str) -> String {
    let trimmed = text.trim();
    let Some(stripped) = trimmed.strip_prefix("```") else {
        return trimmed.to_string();
    };
    let body = match stripped.find('\n') {
        Some(index) => &stripped[index + 1..],
        None => return trimmed.to_string(),
    };
    let body = body.strip_suffix("```").unwrap_or(body).trim();
    if body.is_empty() {
        trimmed.to_string()
    } else {
        body.to_string()
    }
}

fn summarize_continuity_diff_for_log(diff: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 480;

    let trimmed = diff.trim();
    let preview = if trimmed.chars().count() > MAX_PREVIEW_CHARS {
        let head = trimmed.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
        format!("{head}...")
    } else {
        trimmed.to_string()
    };
    let escaped = preview
        .replace('\\', "\\\\")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t");
    format!(
        "chars={} lines={} text=\"{}\"",
        trimmed.chars().count(),
        trimmed.lines().count(),
        escaped
    )
}

fn continuity_refresh_fault_file_path(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    let raw_path = settings
        .get(CONTINUITY_REFRESH_FAULT_FILE_ENV_KEY)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let path = PathBuf::from(raw_path);
    Some(if path.is_absolute() {
        path
    } else {
        root.join(path)
    })
}

fn take_continuity_refresh_fault(
    root: &Path,
    settings: &BTreeMap<String, String>,
    kind_label: &str,
) -> Result<Option<String>> {
    let Some(path) = continuity_refresh_fault_file_path(root, settings) else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read continuity fault script {}", path.display()))?;
    let mut payload: serde_json::Value = serde_json::from_str(&raw).with_context(|| {
        format!(
            "failed to parse continuity fault script JSON {}",
            path.display()
        )
    })?;
    let Some(entries) = payload
        .get_mut(kind_label)
        .and_then(|value| value.as_array_mut())
    else {
        return Ok(None);
    };
    if entries.is_empty() {
        return Ok(None);
    }

    let entry = entries.remove(0);
    let raw_diff = match entry {
        serde_json::Value::String(text) => text,
        serde_json::Value::Object(map) => map
            .get("raw")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .with_context(|| {
                format!(
                    "continuity fault entry for {kind_label} in {} is missing `raw` text",
                    path.display()
                )
            })?,
        other => {
            anyhow::bail!(
                "unsupported continuity fault entry for {kind_label} in {}: {other}",
                path.display()
            );
        }
    };

    std::fs::write(&path, serde_json::to_vec_pretty(&payload)?).with_context(|| {
        format!(
            "failed to persist updated continuity fault script {}",
            path.display()
        )
    })?;
    Ok(Some(raw_diff))
}

pub fn conversation_id_for_thread_key(thread_key: Option<&str>) -> i64 {
    let Some(thread_key) = thread_key.map(str::trim).filter(|value| !value.is_empty()) else {
        return CHAT_CONVERSATION_ID;
    };

    let digest = sha2::Sha256::digest(thread_key.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    let value = (u64::from_be_bytes(bytes) & 0x3fff_ffff_ffff_ffff) as i64;
    if value < 2 {
        2
    } else {
        value
    }
}

pub(crate) fn invoke_codex_exec_with_timeout(
    root: &Path,
    settings: &BTreeMap<String, String>,
    prompt: &str,
    workspace_root: Option<&Path>,
    timeout: Option<Duration>,
) -> Result<String> {
    invoke_codex_exec_with_timeout_and_instructions(
        root,
        settings,
        prompt,
        workspace_root,
        timeout,
        None,
        None,
    )
}

pub(crate) fn invoke_codex_exec_with_timeout_and_instructions(
    root: &Path,
    settings: &BTreeMap<String, String>,
    prompt: &str,
    workspace_root: Option<&Path>,
    timeout: Option<Duration>,
    base_instructions_override: Option<&str>,
    include_apply_patch_tool_override: Option<bool>,
) -> Result<String> {
    invoke_codex_exec_with_timeout_and_instructions_inner(
        root,
        settings,
        prompt,
        workspace_root,
        timeout,
        base_instructions_override.map(str::to_string),
        include_apply_patch_tool_override,
        false,
    )
}

fn invoke_codex_exec_with_timeout_and_instructions_inner(
    root: &Path,
    settings: &BTreeMap<String, String>,
    prompt: &str,
    workspace_root: Option<&Path>,
    timeout: Option<Duration>,
    base_instructions_override: Option<String>,
    include_apply_patch_tool_override: Option<bool>,
    retried_for_tool_verification: bool,
) -> Result<String> {
    let resolved_runtime = runtime_kernel::InferenceRuntimeKernel::resolve(root).ok();
    let model = resolved_runtime
        .as_ref()
        .and_then(|runtime| runtime.active_model().map(ToOwned::to_owned))
        .or_else(|| {
            runtime_state::load_or_resolve_runtime_state(root)
                .ok()
                .and_then(|state| state.active_or_selected_model().map(ToOwned::to_owned))
        })
        .or_else(|| runtime_env::effective_chat_model_from_map(settings))
        .unwrap_or_else(runtime_state::default_primary_model);
    let local_exec_policy = LocalCodexExecPolicy::resolve(&model);
    let skill_preset = selected_skill_preset(settings);
    let debug_invoke = settings
        .get("CTOX_DEBUG_INVOKE_MODEL")
        .map(|value| {
            let trimmed = value.trim();
            trimmed == "1"
                || trimmed.eq_ignore_ascii_case("true")
                || trimmed.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false);
    channels::sync_prompt_identity(root, settings)?;
    let base_instructions_text = if let Some(override_text) = base_instructions_override.as_deref()
    {
        override_text.to_string()
    } else {
        render_codex_exec_instructions(root, settings, prompt, skill_preset)?
    };
    let include_apply_patch_tool = include_apply_patch_tool_override.unwrap_or(true);
    let instructions_file = create_codex_model_instructions_file(root, &base_instructions_text)?;

    let runtime_source_is_local = resolved_runtime
        .as_ref()
        .map(|runtime| runtime.state.source.is_local())
        .unwrap_or_else(|| chat_source_is_local(settings));

    if debug_invoke {
        eprintln!(
            "ctox invoke-model begin model={} preset={} local_source={} workspace_root={}",
            model,
            skill_preset.label(),
            runtime_source_is_local,
            workspace_root
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<none>".to_string())
        );
    }
    if engine::uses_ctox_proxy_model(&model) && runtime_source_is_local {
        if debug_invoke {
            eprintln!("ctox invoke-model ensure-chat-backend model={model} force_restart=false");
        }
        supervisor::ensure_chat_backend_ready(root, false)
            .with_context(|| format!("failed to prepare local runtime backend for {model}"))?;
        if debug_invoke {
            eprintln!("ctox invoke-model ensure-chat-backend-ok model={model}");
        }
    }

    let local_provider_spec = if engine::uses_ctox_proxy_model(&model) && runtime_source_is_local {
        Some(
            LocalModelProviderSpec::resolve(&model, resolved_runtime.as_ref()).with_context(
                || {
                    format!(
                        "local runtime for {} is unresolved; refusing legacy proxy fallback",
                        model
                    )
                },
            )?,
        )
    } else {
        None
    };
    let api_provider_spec =
        resolve_api_model_provider_spec(&model, settings, resolved_runtime.as_ref());
    if let Some(provider) = local_provider_spec.as_ref() {
        if !local_provider_socket_ready(&provider.socket_path) {
            if debug_invoke {
                eprintln!(
                    "ctox invoke-model restart-chat-backend model={} socket={}",
                    model, provider.socket_path
                );
            }
            supervisor::ensure_chat_backend_ready(root, true)
                .with_context(|| format!("failed to restart local runtime backend for {model}"))?;
            if !local_provider_socket_ready(&provider.socket_path) {
                anyhow::bail!(
                    "local runtime socket for {} is unavailable after backend restart: {}",
                    model,
                    provider.socket_path
                );
            }
            if debug_invoke {
                eprintln!(
                    "ctox invoke-model restart-chat-backend-ok model={} socket={}",
                    model, provider.socket_path
                );
            }
        }
    }
    let use_openai_native_web_search =
        use_openai_native_web_search(&model, api_provider_spec.as_ref());
    let configured_reasoning_effort =
        read_reasoning_effort_setting(settings, CHAT_MODEL_REASONING_EFFORT_ENV_KEY)
            .or_else(|| preset_reasoning_effort_for_model(settings, &model))
            .or_else(|| {
                (skill_preset == runtime_state::ChatSkillPreset::Simple)
                    .then_some(local_exec_policy.as_ref())
                    .flatten()
                    .and_then(|policy| policy.reasoning_effort_override())
                    .map(str::to_string)
            });
    let web_search_mode = if use_openai_native_web_search {
        "live"
    } else {
        "disabled"
    };
    let mut config = CodexExecConfigSpec {
        model_instructions_file: instructions_file.path().to_path_buf(),
        project_doc_max_bytes: (skill_preset == runtime_state::ChatSkillPreset::Simple)
            .then_some(0),
        model_context_window: None,
        model_auto_compact_token_limit: None,
        model_reasoning_effort: configured_reasoning_effort,
        web_search_mode,
        include_apply_patch_tool,
        unified_exec_enabled: local_exec_policy
            .as_ref()
            .is_some_and(|policy| policy.unified_exec_enabled()),
        ctox_web_enabled: !use_openai_native_web_search,
        disable_skills: false,
        local_model_provider: local_provider_spec,
        api_model_provider: api_provider_spec.clone(),
        extra_cli_overrides: Vec::new(),
    };

    if skill_preset == runtime_state::ChatSkillPreset::Simple {
        let realized_context = resolved_runtime
            .as_ref()
            .map(|runtime| runtime.turn_context_tokens() as usize)
            .unwrap_or_else(|| {
                read_usize_setting(
                    settings,
                    "CTOX_CHAT_MODEL_REALIZED_CONTEXT",
                    read_usize_setting(settings, "CTOX_CHAT_MODEL_MAX_CONTEXT", 4096),
                )
            })
            .max(2048);
        if let Some(policy) = local_exec_policy.as_ref() {
            let compact_limit = policy.compact_limit(realized_context);
            config.model_context_window = Some(realized_context);
            config.model_auto_compact_token_limit = Some(compact_limit);
        }
    }

    let launch = CodexExecInvocation {
        model: model.clone(),
        workspace_root: workspace_root
            .filter(|path| path.is_dir())
            .map(Path::to_path_buf),
        prompt: prompt.to_string(),
        config,
    };
    let mut command = CodexExecBinary::resolve(root, workspace_root)?.into_command();
    command.args(launch.into_args());
    if engine::is_openai_api_chat_model(&model) && api_provider_spec.is_none() {
        ensure_codex_api_auth(settings)?;
    }
    for (key, value) in settings {
        command.env(key, value);
    }
    if engine::is_openai_api_chat_model(&model) && api_provider_spec.is_none() {
        if let Some(api_key) = settings
            .get("OPENAI_API_KEY")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            command.env("CODEX_API_KEY", api_key);
        }
    }
    if let Some(codex_home) = resolve_codex_home(settings) {
        std::fs::create_dir_all(&codex_home).with_context(|| {
            format!(
                "failed to create CODEX_HOME for CTOX chat runtime at {}",
                codex_home.display()
            )
        })?;
        command.env("CODEX_HOME", codex_home);
    }
    command.env("CTOX_ROOT", root);
    command.env("CTOX_CONTEXT_DB", root.join("runtime/ctox_lcm.db"));
    if debug_invoke {
        eprintln!("ctox invoke-model spawn-codex-exec model={model}");
    }
    let output = if let Some(timeout) = timeout {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = command
            .spawn()
            .map_err(|err| anyhow::anyhow!("failed to launch codex-exec for CTOX: {err:#}"))?;
        if debug_invoke {
            eprintln!(
                "ctox invoke-model codex-exec-spawned model={} pid={}",
                model,
                child.id()
            );
        }
        collect_child_output_with_timeout(child, timeout)?
    } else {
        command
            .output()
            .map_err(|err| anyhow::anyhow!("failed to launch codex-exec for CTOX: {err:#}"))?
    };
    if debug_invoke {
        eprintln!(
            "ctox invoke-model codex-exec-finished model={} status={} stdout_bytes={} stderr_bytes={}",
            model,
            output.status,
            output.stdout.len(),
            output.stderr.len()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let event_stream_summary = if !stdout.is_empty() {
        turn_contract::summarize_event_stream(&stdout)
    } else {
        None
    };
    let stdout_response = if !stdout.is_empty() {
        extract_codex_text_response(&stdout)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    } else {
        None
    };
    let missing_required_tool_activity =
        response_missing_required_tool_activity(prompt, event_stream_summary.as_ref());
    if let Some(response) = stdout_response.clone() {
        if missing_required_tool_activity {
            if !retried_for_tool_verification {
                let retry_instructions = if let Some(existing) = base_instructions_override
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    format!(
                        "{}\n\n# Tool-Use Correction\n{}",
                        existing, TOOL_VERIFICATION_RETRY_INSTRUCTIONS
                    )
                } else {
                    format!(
                        "{}\n\n# Tool-Use Correction\n{}",
                        render_codex_exec_instructions(root, settings, prompt, skill_preset)?,
                        TOOL_VERIFICATION_RETRY_INSTRUCTIONS
                    )
                };
                return invoke_codex_exec_with_timeout_and_instructions_inner(
                    root,
                    settings,
                    prompt,
                    workspace_root,
                    timeout,
                    Some(retry_instructions),
                    include_apply_patch_tool_override,
                    true,
                );
            }
            anyhow::bail!(
                "codex-exec returned a final answer without any tool activity for a task that required filesystem or build verification"
            );
        }
        if !output.status.success() {
            return Ok(response);
        }
    }
    if !output.status.success() {
        let stdout_error = if !stdout.is_empty() {
            extract_codex_error_response(&stdout)
        } else {
            None
        };
        let message = if let Some(summary) = stdout_error {
            summary
        } else if !stderr.is_empty() {
            stderr
        } else {
            stdout
        };
        anyhow::bail!("{message}");
    }
    let response = if !stdout.is_empty() {
        if let Some(text) = stdout_response {
            text
        } else if let Some(summary) = extract_codex_error_response(&stdout) {
            anyhow::bail!("{summary}");
        } else {
            stdout
        }
    } else {
        stderr
    };
    if response.is_empty() {
        anyhow::bail!("codex-exec returned empty output");
    }
    Ok(response)
}

fn response_missing_required_tool_activity(
    prompt: &str,
    event_stream_summary: Option<&turn_contract::CodexExecEventStreamSummary>,
) -> bool {
    prompt_requires_tool_verification(prompt)
        && !event_stream_summary
            .map(|summary| summary.saw_tool_activity)
            .unwrap_or(false)
}

fn resolved_local_socket_path(
    resolved_runtime: Option<&runtime_kernel::InferenceRuntimeKernel>,
) -> Option<String> {
    let runtime = resolved_runtime?;
    if !runtime.state.source.is_local() {
        return None;
    }
    runtime
        .primary_generation
        .as_ref()
        .and_then(|binding| binding.socket_path.clone())
}

fn responses_api_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

fn local_provider_socket_ready(socket_path: &str) -> bool {
    #[cfg(unix)]
    {
        let path = Path::new(socket_path);
        path.exists() && UnixStream::connect(path).is_ok()
    }
    #[cfg(not(unix))]
    {
        Path::new(socket_path).exists()
    }
}

fn resolve_api_model_provider_spec(
    model: &str,
    settings: &BTreeMap<String, String>,
    resolved_runtime: Option<&runtime_kernel::InferenceRuntimeKernel>,
) -> Option<ApiModelProviderSpec> {
    let runtime_provider = resolved_runtime.map(|runtime| {
        runtime_state::api_provider_for_upstream_base_url(&runtime.state.upstream_base_url)
            .to_string()
    });
    let provider = settings
        .get("CTOX_API_PROVIDER")
        .map(|value| runtime_state::normalize_api_provider(value).to_string())
        .or(runtime_provider)
        .filter(|provider| !provider.eq_ignore_ascii_case("local"))
        .or_else(|| {
            settings
                .get("CTOX_CHAT_SOURCE")
                .filter(|value| value.trim().eq_ignore_ascii_case("api"))
                .map(|_| engine::default_api_provider_for_model(model).to_string())
        })
        .filter(|provider| engine::api_provider_supports_model(provider, model))?;
    if !engine::api_provider_supports_model(&provider, model) {
        return None;
    }
    if !provider.eq_ignore_ascii_case("openrouter") {
        return None;
    }
    let base_url = resolved_runtime
        .map(|runtime| runtime.internal_responses_base_url())
        .or_else(|| {
            settings
                .get("CTOX_UPSTREAM_BASE_URL")
                .map(|value| responses_api_base_url(value))
        })
        .unwrap_or_else(|| {
            responses_api_base_url(runtime_state::default_api_upstream_base_url_for_provider(
                "openrouter",
            ))
        });
    Some(ApiModelProviderSpec {
        provider_id: "cto_openrouter",
        name: "cto-openrouter",
        base_url,
        env_key: "OPENROUTER_API_KEY",
    })
}

fn use_openai_native_web_search(
    model: &str,
    api_provider_spec: Option<&ApiModelProviderSpec>,
) -> bool {
    engine::is_openai_api_chat_model(model) && api_provider_spec.is_none()
}

fn escape_inline_toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SimpleExecPhase {
    AnalysisReadOnly,
    DocsTextChange,
    ReadTraceFirst,
    BugFix,
    FeatureAdd,
    RefactorSafe,
    TestWork,
    CodeReview,
    BugFixWithTests,
    DependencyUpdate,
    MigrationWithDataChange,
    MigrationChange,
    DataChangeSafe,
    InfraConfigChange,
    DeployRelease,
    OpsDebugTerminal,
    IncidentHotfix,
    IncidentHotfixDeploy,
    RollbackRecovery,
    GeneralSafeTask,
}

impl SimpleExecPhase {
    fn label(self) -> &'static str {
        match self {
            Self::AnalysisReadOnly => "analysis-read-only",
            Self::DocsTextChange => "docs-text-change",
            Self::ReadTraceFirst => "read-trace-first",
            Self::BugFix => "bug-fix",
            Self::FeatureAdd => "feature-add",
            Self::RefactorSafe => "refactor-safe",
            Self::TestWork => "test-work",
            Self::CodeReview => "code-review",
            Self::BugFixWithTests => "bug-fix-with-tests",
            Self::DependencyUpdate => "dependency-update",
            Self::MigrationWithDataChange => "migration-with-data-change",
            Self::MigrationChange => "migration-change",
            Self::DataChangeSafe => "data-change-safe",
            Self::InfraConfigChange => "infra-config-change",
            Self::DeployRelease => "deploy-release",
            Self::OpsDebugTerminal => "ops-debug-terminal",
            Self::IncidentHotfix => "incident-hotfix",
            Self::IncidentHotfixDeploy => "incident-hotfix-deploy",
            Self::RollbackRecovery => "rollback-recovery",
            Self::GeneralSafeTask => "general-safe-task",
        }
    }

    fn prompt_fragment(self) -> &'static str {
        match self {
            Self::AnalysisReadOnly => CTOX_CODEX_EXEC_SIMPLE_PHASE_ANALYSIS_READ_ONLY,
            Self::DocsTextChange => CTOX_CODEX_EXEC_SIMPLE_PHASE_DOCS_TEXT_CHANGE,
            Self::ReadTraceFirst => CTOX_CODEX_EXEC_SIMPLE_PHASE_READ_TRACE_FIRST,
            Self::BugFix => CTOX_CODEX_EXEC_SIMPLE_PHASE_BUG_FIX,
            Self::FeatureAdd => CTOX_CODEX_EXEC_SIMPLE_PHASE_FEATURE_ADD,
            Self::RefactorSafe => CTOX_CODEX_EXEC_SIMPLE_PHASE_REFACTOR_SAFE,
            Self::TestWork => CTOX_CODEX_EXEC_SIMPLE_PHASE_TEST_WORK,
            Self::CodeReview => CTOX_CODEX_EXEC_SIMPLE_PHASE_CODE_REVIEW,
            Self::BugFixWithTests => CTOX_CODEX_EXEC_SIMPLE_PHASE_BUG_FIX_WITH_TESTS,
            Self::DependencyUpdate => CTOX_CODEX_EXEC_SIMPLE_PHASE_DEPENDENCY_UPDATE,
            Self::MigrationWithDataChange => {
                CTOX_CODEX_EXEC_SIMPLE_PHASE_MIGRATION_WITH_DATA_CHANGE
            }
            Self::MigrationChange => CTOX_CODEX_EXEC_SIMPLE_PHASE_MIGRATION_CHANGE,
            Self::DataChangeSafe => CTOX_CODEX_EXEC_SIMPLE_PHASE_DATA_CHANGE_SAFE,
            Self::InfraConfigChange => CTOX_CODEX_EXEC_SIMPLE_PHASE_INFRA_CONFIG_CHANGE,
            Self::DeployRelease => CTOX_CODEX_EXEC_SIMPLE_PHASE_DEPLOY_RELEASE,
            Self::OpsDebugTerminal => CTOX_CODEX_EXEC_SIMPLE_PHASE_OPS_DEBUG_TERMINAL,
            Self::IncidentHotfix => CTOX_CODEX_EXEC_SIMPLE_PHASE_INCIDENT_HOTFIX,
            Self::IncidentHotfixDeploy => CTOX_CODEX_EXEC_SIMPLE_PHASE_INCIDENT_HOTFIX_DEPLOY,
            Self::RollbackRecovery => CTOX_CODEX_EXEC_SIMPLE_PHASE_ROLLBACK_RECOVERY,
            Self::GeneralSafeTask => CTOX_CODEX_EXEC_SIMPLE_PHASE_GENERAL_SAFE_TASK,
        }
    }

    fn needs_terminal_ops_core(self) -> bool {
        matches!(
            self,
            Self::MigrationWithDataChange
                | Self::DataChangeSafe
                | Self::InfraConfigChange
                | Self::DeployRelease
                | Self::OpsDebugTerminal
                | Self::IncidentHotfix
                | Self::IncidentHotfixDeploy
                | Self::RollbackRecovery
        )
    }
}

fn classify_simple_exec_phase(prompt: &str) -> SimpleExecPhase {
    let lower = prompt.to_ascii_lowercase();
    if contains_any(
        &lower,
        &[
            "summarize",
            "explain",
            "compare",
            "analysis only",
            "investigate why",
            "why is this happening",
        ],
    ) && !contains_any(
        &lower,
        &[
            "edit",
            "change",
            "fix",
            "implement",
            "add ",
            "deploy",
            "rollback",
        ],
    ) {
        return SimpleExecPhase::AnalysisReadOnly;
    }
    if contains_any(
        &lower,
        &[
            "documentation",
            "docs",
            "readme",
            "markdown",
            "prompt",
            "comment",
            "wording",
            "copy change",
            "text change",
        ],
    ) && !contains_any(
        &lower,
        &["migration", "schema", "deploy", "rollback", "incident"],
    ) {
        return SimpleExecPhase::DocsTextChange;
    }
    if contains_any(
        &lower,
        &[
            "incident",
            "hotfix",
            "restore service now",
            "urgent restore",
        ],
    ) && contains_any(&lower, &["deploy", "release", "rollout", "promote", "ship"])
    {
        return SimpleExecPhase::IncidentHotfixDeploy;
    }
    if contains_any(
        &lower,
        &[
            "migration",
            "schema change",
            "database schema",
            "add column",
            "drop column",
            "alter table",
        ],
    ) && contains_any(
        &lower,
        &[
            "backfill",
            "data repair",
            "update records",
            "target rows",
            "dry run",
            "sample rows",
            "before count",
            "after count",
        ],
    ) {
        return SimpleExecPhase::MigrationWithDataChange;
    }
    if contains_any(
        &lower,
        &[
            "bug",
            "fix",
            "broken",
            "regression",
            "failing",
            "failure",
            "wrong behavior",
        ],
    ) && contains_any(
        &lower,
        &[
            "test only",
            "tests only",
            "add test",
            "fix test",
            "update test",
            "failing test",
            "flaky test",
            "test file",
        ],
    ) {
        return SimpleExecPhase::BugFixWithTests;
    }
    if contains_any(
        &lower,
        &[
            "code review",
            "review only",
            "review this diff",
            "review this patch",
            "no material issues found",
            "finding:",
        ],
    ) {
        return SimpleExecPhase::CodeReview;
    }
    if contains_any(
        &lower,
        &[
            "rollback",
            "roll back",
            "revert to last known good",
            "last known good",
            "restore previous version",
        ],
    ) {
        return SimpleExecPhase::RollbackRecovery;
    }
    if contains_any(
        &lower,
        &[
            "incident",
            "hotfix",
            "restore service now",
            "service is broken now",
            "urgent restore",
        ],
    ) {
        return SimpleExecPhase::IncidentHotfix;
    }
    if contains_any(
        &lower,
        &[
            "deploy",
            "release",
            "ship",
            "promote",
            "rollout",
            "publish build",
        ],
    ) {
        return SimpleExecPhase::DeployRelease;
    }
    if contains_any(
        &lower,
        &[
            "terraform",
            "helm",
            "kubernetes",
            "dockerfile",
            "docker compose",
            "docker-compose",
            "nginx",
            "systemd",
            "ci config",
            "github actions",
            "workflow yml",
            "infra config",
            "deployment config",
        ],
    ) {
        return SimpleExecPhase::InfraConfigChange;
    }
    if contains_any(
        &lower,
        &[
            "backfill",
            "data repair",
            "update records",
            "target rows",
            "dry run",
            "idempotent script",
            "sample rows",
            "before count",
            "after count",
        ],
    ) {
        return SimpleExecPhase::DataChangeSafe;
    }
    if contains_any(
        &lower,
        &[
            "migration",
            "schema change",
            "database schema",
            "add column",
            "drop column",
            "alter table",
            "rollback path",
        ],
    ) {
        return SimpleExecPhase::MigrationChange;
    }
    if contains_any(
        &lower,
        &[
            "dependency update",
            "upgrade dependency",
            "bump version",
            "lockfile",
            "package update",
            "library version",
            "runtime version",
            "base image",
        ],
    ) {
        return SimpleExecPhase::DependencyUpdate;
    }
    if contains_any(
        &lower,
        &[
            "journalctl",
            "systemctl",
            "kubectl",
            "docker ",
            "docker-compose",
            "service ",
            "logs",
            "log output",
            "deploy",
            "release",
            "restart",
            "terminal",
            "shell command",
            "process",
            "health check",
        ],
    ) {
        return SimpleExecPhase::OpsDebugTerminal;
    }
    if contains_any(
        &lower,
        &[
            "refactor",
            "cleanup",
            "clean up",
            "extract ",
            "rename ",
            "move ",
            "reorganize",
            "restructure",
        ],
    ) && !contains_any(
        &lower,
        &["bug", "fix", "broken", "failing", "feature", "new behavior"],
    ) {
        return SimpleExecPhase::RefactorSafe;
    }
    if contains_any(
        &lower,
        &[
            "test only",
            "tests only",
            "add test",
            "fix test",
            "update test",
            "failing test",
            "flaky test",
            "test file",
        ],
    ) {
        return SimpleExecPhase::TestWork;
    }
    if contains_any(
        &lower,
        &[
            "bug",
            "fix",
            "broken",
            "regression",
            "error",
            "failing",
            "failure",
            "does not work",
            "wrong behavior",
        ],
    ) {
        return SimpleExecPhase::BugFix;
    }
    if contains_any(
        &lower,
        &[
            "add ",
            "implement",
            "support ",
            "new behavior",
            "new feature",
            "allow ",
            "create ",
        ],
    ) {
        return SimpleExecPhase::FeatureAdd;
    }
    if contains_any(
        &lower,
        &[
            "investigate this area",
            "figure out where",
            "where should this change go",
            "trace this path",
            "find the entry point",
        ],
    ) {
        return SimpleExecPhase::ReadTraceFirst;
    }
    SimpleExecPhase::GeneralSafeTask
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn render_codex_exec_instructions(
    root: &Path,
    settings: &BTreeMap<String, String>,
    prompt: &str,
    preset: runtime_state::ChatSkillPreset,
) -> Result<String> {
    let core = live_context::render_system_prompt(root, settings)?;
    match preset {
        runtime_state::ChatSkillPreset::Standard => {
            Ok(format!("{core}\n\n{CTOX_CODEX_EXEC_STANDARD_OVERLAY}"))
        }
        runtime_state::ChatSkillPreset::Simple => {
            let phase = classify_simple_exec_phase(prompt);
            let include_terminal_core = phase.needs_terminal_ops_core();
            let mut sections = vec![
                core,
                CTOX_CODEX_EXEC_SIMPLE_OVERLAY.to_string(),
                format!(
                    "Active meta-skill loadout:\n- `task-router`\n- `small-step-core`{}- `{}`",
                    if include_terminal_core {
                        "\n- `terminal-ops-core`\n"
                    } else {
                        "\n"
                    },
                    phase.label()
                ),
                CTOX_CODEX_EXEC_SIMPLE_TASK_ROUTER.to_string(),
                CTOX_CODEX_EXEC_SIMPLE_SMALL_STEP_CORE.to_string(),
            ];
            if include_terminal_core {
                sections.push(CTOX_CODEX_EXEC_SIMPLE_TERMINAL_OPS_CORE.to_string());
            }
            sections.push(phase.prompt_fragment().to_string());
            Ok(sections.join("\n\n"))
        }
    }
}

struct CodexModelInstructionsFile {
    path: PathBuf,
}

impl CodexModelInstructionsFile {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for CodexModelInstructionsFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn create_codex_model_instructions_file(
    root: &Path,
    contents: &str,
) -> Result<CodexModelInstructionsFile> {
    let dir = root.join("runtime/codex_exec");
    std::fs::create_dir_all(&dir).with_context(|| {
        format!(
            "failed to create CTOX codex-exec runtime directory at {}",
            dir.display()
        )
    })?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = dir.join(format!(
        "model_instructions_{}_{}.md",
        std::process::id(),
        unique
    ));
    std::fs::write(&path, contents).with_context(|| {
        format!(
            "failed to write CTOX codex-exec model instructions file at {}",
            path.display()
        )
    })?;
    Ok(CodexModelInstructionsFile { path })
}

fn collect_child_output_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Result<Output> {
    let mut stdout = child
        .stdout
        .take()
        .context("codex-exec missing stdout pipe")?;
    let mut stderr = child
        .stderr
        .take()
        .context("codex-exec missing stderr pipe")?;

    let stdout_handle = thread::spawn(move || -> Vec<u8> {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });
    let stderr_handle = thread::spawn(move || -> Vec<u8> {
        let mut buffer = Vec::new();
        let _ = stderr.read_to_end(&mut buffer);
        buffer
    });

    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|err| anyhow::anyhow!("failed to poll codex-exec for CTOX chat: {err:#}"))?
        {
            break status;
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            let _ = child.kill();
            break child.wait().map_err(|err| {
                anyhow::anyhow!("failed to wait for timed out codex-exec: {err:#}")
            })?;
        }
        thread::sleep(Duration::from_millis(100));
    };

    let stdout = stdout_handle
        .join()
        .map_err(|_| anyhow::anyhow!("failed to join stdout reader for codex-exec"))?;
    let stderr = stderr_handle
        .join()
        .map_err(|_| anyhow::anyhow!("failed to join stderr reader for codex-exec"))?;

    if timed_out {
        anyhow::bail!("codex-exec timed out after {}s", timeout.as_secs());
    }

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn extract_codex_text_response(stdout: &str) -> Option<String> {
    turn_contract::extract_final_text_from_event_stream(stdout)
}

fn extract_codex_error_response(stdout: &str) -> Option<String> {
    turn_contract::extract_final_error_from_event_stream(stdout)
}

fn prompt_requires_tool_verification(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    if lower.contains("you are updating the ctox continuity document")
        && lower.contains("<current_document>")
    {
        return false;
    }
    let workspace_bound = lower.contains("work only inside this workspace")
        || lower.contains("work only in this workspace")
        || lower.contains("workspace:")
        || lower.contains("workspace root")
        || lower.contains("workspace_root")
        || prompt.contains("/home/")
        || prompt.contains("/tmp/")
        || prompt.contains("/Users/");
    let has_action_verb = [
        "create ",
        "edit ",
        "modify ",
        "implement ",
        "build ",
        "compile ",
        "run ",
        "test ",
        "verify ",
        "fix ",
        "debug ",
        "refactor ",
        "rename ",
        "patch ",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    let has_strong_verification_marker = [
        "cmake",
        "cargo ",
        "pytest",
        "npm ",
        "pnpm ",
        "make ",
        "./build/",
        "do not answer before",
        "on successful run",
        "must print exactly",
        "must output exactly",
        "verify the binary",
        "create at least these files",
        ".cpp",
        ".cc",
        ".cxx",
        ".h",
        ".hpp",
        "cmakelists.txt",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    (workspace_bound && has_action_verb) || has_strong_verification_marker
}

pub fn summarize_runtime_error(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return "CTOX execution failed without a stable error payload.".to_string();
    }
    if let Some(summary) = summarize_known_infra_error(trimmed) {
        return summary;
    }
    if live_context::looks_like_codex_event_stream(trimmed) {
        if let Some(summary) = extract_codex_text_response(trimmed) {
            return live_context::clip_prompt_text(&summary, 700);
        }
        return "The turn failed after emitting raw Codex event-stream output instead of a stable final reply. Re-check the current runtime state and recover from the real blocker instead of replaying raw event data.".to_string();
    }
    live_context::clip_prompt_text(trimmed, 700)
}

pub fn synthesize_failure_reply(content: &str) -> String {
    let summary = summarize_runtime_error(content);
    format!("Status: `blocked`\n\nBlocker: {summary}")
}

pub fn hard_runtime_blocker_retry_cooldown_secs(content: &str) -> Option<u64> {
    let lower = content.to_ascii_lowercase();
    if lower.contains("quota exceeded")
        || lower.contains("billing details")
        || lower.contains("openai api quota is exhausted")
        || lower.contains("billing is unavailable for the selected model")
    {
        return Some(1_800);
    }
    if summarize_known_infra_error(content).is_some()
        || lower.contains("chat backend could not start on this host")
    {
        return Some(900);
    }
    None
}

fn summarize_known_infra_error(content: &str) -> Option<String> {
    let lower = content.to_ascii_lowercase();
    if lower.contains("quota exceeded") || lower.contains("billing details") {
        return Some(
            "CTOX chat could not continue because the configured OpenAI API quota is exhausted or billing is unavailable for the selected model.".to_string(),
        );
    }
    if lower.contains("feature `edition2024` is required")
        || (lower.contains("edition2024") && lower.contains("cargo"))
    {
        return Some(
            "CTOX chat backend could not start on this host because the integrated agent runtime requires a newer Rust/Cargo toolchain with Edition 2024 support.".to_string(),
        );
    }
    if lower.contains("error[e0583]")
        && lower.contains("file not found for module")
        && lower.contains("state/src/runtime.rs")
        && (lower.contains("`agent_jobs`") || lower.contains("`backfill`"))
    {
        return Some(
            "CTOX chat backend could not start on this host because the integrated agent runtime checkout is incomplete: `state/src/runtime/` is missing required module files such as `agent_jobs.rs` and `backfill.rs`.".to_string(),
        );
    }
    if lower.contains("failed to load manifest for workspace member")
        && lower.contains("cargo.toml")
    {
        return Some(
            "CTOX chat backend could not start on this host because the integrated agent-runtime workspace manifest is not buildable in its current remote environment.".to_string(),
        );
    }
    None
}

fn ensure_codex_api_auth(settings: &BTreeMap<String, String>) -> Result<()> {
    let Some(api_key) = settings
        .get("OPENAI_API_KEY")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let Some(codex_home) = resolve_codex_home(settings) else {
        return Ok(());
    };
    std::fs::create_dir_all(&codex_home)?;
    let auth_path = codex_home.join("auth.json");
    let payload = serde_json::json!({
        "OPENAI_API_KEY": api_key,
    });
    std::fs::write(&auth_path, serde_json::to_vec_pretty(&payload)?)?;
    Ok(())
}

fn resolve_codex_home(settings: &BTreeMap<String, String>) -> Option<std::path::PathBuf> {
    settings
        .get("CODEX_HOME")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("CODEX_HOME").map(std::path::PathBuf::from))
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".codex"))
        })
}

fn read_usize_setting(settings: &BTreeMap<String, String>, key: &str, default: usize) -> usize {
    settings
        .get(key)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn continuity_refresh_timeout_secs(settings: &BTreeMap<String, String>) -> u64 {
    read_usize_setting(
        settings,
        CONTINUITY_REFRESH_TIMEOUT_ENV_KEY,
        DEFAULT_CONTINUITY_REFRESH_TIMEOUT_SECS as usize,
    ) as u64
}

fn read_reasoning_effort_setting(settings: &BTreeMap<String, String>, key: &str) -> Option<String> {
    let normalized = settings
        .get(key)
        .map(|value| value.trim().to_ascii_lowercase())?;
    match normalized.as_str() {
        "minimal" | "low" => Some("low".to_string()),
        "medium" => Some("medium".to_string()),
        "high" => Some("high".to_string()),
        "none" => Some("none".to_string()),
        _ => None,
    }
}

fn selected_skill_preset(settings: &BTreeMap<String, String>) -> runtime_state::ChatSkillPreset {
    settings
        .get(CHAT_SKILL_PRESET_ENV_KEY)
        .map(String::as_str)
        .map(runtime_state::ChatSkillPreset::from_label)
        .unwrap_or_default()
}

fn preset_reasoning_effort_for_model(
    settings: &BTreeMap<String, String>,
    model: &str,
) -> Option<String> {
    let preset = settings
        .get("CTOX_CHAT_LOCAL_PRESET")
        .map(String::as_str)
        .map(crate::inference::runtime_plan::ChatPreset::from_label)?;
    let normalized = model.trim();
    let supports_preset_reasoning = normalized == "openai/gpt-oss-20b"
        || normalized.eq_ignore_ascii_case("gpt-5.4")
        || normalized.eq_ignore_ascii_case("gpt-5.4-mini")
        || normalized.eq_ignore_ascii_case("gpt-5.4-nano");
    if !supports_preset_reasoning {
        return None;
    }
    Some(
        match preset {
            crate::inference::runtime_plan::ChatPreset::Quality => "high",
            crate::inference::runtime_plan::ChatPreset::Performance => "low",
        }
        .to_string(),
    )
}

fn chat_source_is_local(settings: &BTreeMap<String, String>) -> bool {
    settings
        .get("CTOX_CHAT_SOURCE")
        .map(|value| value.trim().eq_ignore_ascii_case("local"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    use std::collections::BTreeMap;

    use crate::context_health;
    use crate::governance;
    use crate::inference::model_adapters::LocalCodexExecPolicy;
    use crate::inference::runtime_kernel;
    use crate::inference::runtime_state;
    use crate::inference::turn_contract;
    use crate::lcm;
    use crate::live_context;

    use super::chat_source_is_local;
    use super::classify_simple_exec_phase;
    use super::collect_child_output_with_timeout;
    use super::continuity_refresh_timeout_secs;
    use super::extract_codex_error_response;
    use super::hard_runtime_blocker_retry_cooldown_secs;
    use super::prompt_requires_tool_verification;
    use super::read_reasoning_effort_setting;
    use super::repair_continuity_refresh_output;
    use super::resolve_api_model_provider_spec;
    use super::response_missing_required_tool_activity;
    use super::selected_skill_preset;
    use super::strip_markdown_code_fences;
    use super::summarize_continuity_diff_for_log;
    use super::summarize_runtime_error;
    use super::synthesize_failure_reply;
    use super::take_continuity_refresh_fault;
    use super::use_openai_native_web_search;
    use super::ApiModelProviderSpec;
    use super::CodexExecBinary;
    use super::CodexExecConfigSpec;
    use super::ContinuityRefreshRepair;
    use super::LocalModelProviderSpec;
    use super::SimpleExecPhase;
    use super::CHAT_MODEL_REASONING_EFFORT_ENV_KEY;
    use super::CHAT_SKILL_PRESET_ENV_KEY;
    use super::CONTINUITY_REFRESH_FAULT_FILE_ENV_KEY;
    use super::CONTINUITY_REFRESH_TIMEOUT_ENV_KEY;

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ctox-turn-loop-{label}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn chat_source_local_detection_is_case_insensitive() {
        let mut settings = BTreeMap::new();
        settings.insert("CTOX_CHAT_SOURCE".to_string(), "LoCaL".to_string());
        assert!(chat_source_is_local(&settings));
    }

    #[test]
    fn chat_source_local_detection_is_false_for_api_source() {
        let mut settings = BTreeMap::new();
        settings.insert("CTOX_CHAT_SOURCE".to_string(), "api".to_string());
        assert!(!chat_source_is_local(&settings));
    }

    #[test]
    fn continuity_refresh_timeout_setting_defaults_and_overrides() {
        let settings = BTreeMap::new();
        assert_eq!(continuity_refresh_timeout_secs(&settings), 45);

        let mut settings = BTreeMap::new();
        settings.insert(
            CONTINUITY_REFRESH_TIMEOUT_ENV_KEY.to_string(),
            "120".to_string(),
        );
        assert_eq!(continuity_refresh_timeout_secs(&settings), 120);
    }

    #[test]
    fn sanitize_context_message_reduces_raw_event_stream_to_agent_text() {
        let raw = concat!(
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"Ich prüfe Redis.\"}}\n",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"command_execution\",\"command\":\"echo hi\",\"aggregated_output\":\"hi\",\"exit_code\":0,\"status\":\"completed\"}}\n",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"Redis ist noch nicht bereit.\"}}\n"
        );
        let sanitized = live_context::sanitize_context_message(raw);
        assert!(sanitized.contains("Redis ist noch nicht bereit."));
        assert!(!sanitized.contains("\"type\":\"item.completed\""));
    }

    #[test]
    fn sanitize_context_message_clips_oversized_plain_text() {
        let huge = "a".repeat(10_000);
        let sanitized = live_context::sanitize_context_message(&huge);
        assert!(sanitized.len() < huge.len());
        assert!(sanitized.ends_with('…'));
    }

    #[test]
    fn internal_follow_up_prompt_is_not_rendered_as_owner_user_message() {
        let prompt = "Review the blocked owner-visible task without losing continuity.\n\nGoal:\nInstall Redis";
        assert!(live_context::is_internal_queue_prompt(prompt));
        assert_eq!(
            live_context::render_message_role_label("user", prompt),
            "internal_queue"
        );
        assert_eq!(
            live_context::render_message_role_label("user", "Install Redis now"),
            "user"
        );
    }

    #[test]
    fn inbound_email_wrapper_is_reduced_to_compact_summary() {
        let wrapped = concat!(
            "[E-Mail eingegangen]\n",
            "Sender: Max Mustermann <max@example.com>\n",
            "Betreff: Re: Helpdesk kaputt\n",
            "Thread: <abc@example.com>\n\n",
            "[Bisheriger Thread-Kontext]\n- ...\n\n",
            "[Letzte owner-relevante Kommunikation ueber alle Kanaele]\n- ...\n"
        );
        let summary = live_context::summarize_inbound_email_wrapper(wrapped)
            .expect("expected wrapper summary");
        assert!(summary.contains("Max Mustermann <max@example.com>"));
        assert!(summary.contains("Re: Helpdesk kaputt"));
        assert!(summary.contains("<abc@example.com>"));
        assert!(!summary.contains("[Bisheriger Thread-Kontext]"));
    }

    #[test]
    fn assistant_blocked_reply_is_marked_as_history() {
        assert!(live_context::is_historical_status_note(
            "blocked: redis install still missing sudo access"
        ));
        let rendered = live_context::render_context_message(
            "assistant",
            "blocked: redis install still missing sudo access",
        );
        assert!(rendered.starts_with("assistant_status_history:"));
        assert!(rendered.contains("Historical assistant status note only"));
    }

    #[test]
    fn codex_error_stream_extracts_turn_failure_message() {
        let raw = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"abc\"}\n",
            "{\"type\":\"turn.started\"}\n",
            "{\"type\":\"error\",\"message\":\"Quota exceeded. Check your plan and billing details.\"}\n",
            "{\"type\":\"turn.failed\",\"error\":{\"message\":\"Quota exceeded. Check your plan and billing details.\"}}\n"
        );
        assert_eq!(
            extract_codex_error_response(raw).as_deref(),
            Some("Quota exceeded. Check your plan and billing details.")
        );
    }

    #[test]
    fn runtime_error_summary_maps_quota_exceeded() {
        let summary =
            summarize_runtime_error("Quota exceeded. Check your plan and billing details.");
        assert!(summary.contains("configured OpenAI API quota is exhausted"));
    }

    #[test]
    fn render_chat_prompt_stays_compact_for_small_context() {
        let continuity = lcm::ContinuityShowAll {
            conversation_id: 1,
            narrative: lcm::ContinuityDocumentState {
                conversation_id: 1,
                kind: lcm::ContinuityKind::Narrative,
                head_commit_id: "n1".to_string(),
                content: "Current rollout is active.".to_string(),
                created_at: "2026-04-01T00:00:00Z".to_string(),
                updated_at: "2026-04-01T00:00:00Z".to_string(),
            },
            anchors: lcm::ContinuityDocumentState {
                conversation_id: 1,
                kind: lcm::ContinuityKind::Anchors,
                head_commit_id: "a1".to_string(),
                content: "Host: prod-1".to_string(),
                created_at: "2026-04-01T00:00:00Z".to_string(),
                updated_at: "2026-04-01T00:00:00Z".to_string(),
            },
            focus: lcm::ContinuityDocumentState {
                conversation_id: 1,
                kind: lcm::ContinuityKind::Focus,
                head_commit_id: "f1".to_string(),
                content: "# Focus\n\n## Contract\nmission: rollout\nmission_state: active\ncontinuation_mode: continuous\ntrigger_intensity: hot\nslice: verify-healthcheck\nslice_state: running\n\n## State\ngoal: verify the rollout\nblocker: none\nmissing_dependency: none\nnext_slice: run the healthcheck\ndone_gate: healthcheck passes\nretry_condition: rerun after a material change\nclosure_confidence: medium\n".to_string(),
                created_at: "2026-04-01T00:00:00Z".to_string(),
                updated_at: "2026-04-01T00:00:00Z".to_string(),
            },
        };
        let governance = governance::GovernancePromptSnapshot::default();
        let health = context_health::ContextHealthSnapshot {
            conversation_id: 1,
            overall_score: 96,
            status: context_health::ContextHealthStatus::Healthy,
            summary: "Context is healthy.".to_string(),
            repair_recommended: false,
            dimensions: vec![context_health::ContextHealthDimension {
                name: "mission_contract".to_string(),
                score: 96,
                summary: "Mission contract is explicit.".to_string(),
            }],
            warnings: Vec::new(),
        };
        let rendered_context = live_context::RenderedContextSelection {
            entries: vec![
                "user: please verify the rollout".to_string(),
                "assistant: rollout verification pending".to_string(),
            ],
            omitted_items: 0,
        };
        let runtime_blocks = live_context::PromptRuntimeBlocks {
            focus: live_context::continuity_block("Focus", &continuity.focus.content),
            anchors: live_context::continuity_block("Anchors", &continuity.anchors.content),
            narrative: live_context::continuity_block("Narrative", &continuity.narrative.content),
            verified_evidence: "Verified evidence:\nitems: []".to_string(),
            workflow_state:
                "Open CTOX work that counts right now:\nqueue_items: []\nplan_items: []\nschedule_items: []"
                    .to_string(),
        };
        let prompt = live_context::render_chat_prompt(
            &runtime_blocks,
            &governance,
            &health,
            "verify it now",
            &rendered_context,
            None,
        );
        assert!(prompt.contains("Focus:"));
        assert!(prompt.contains("What to do this turn:"));
        assert!(prompt.contains("A reply or file note does not count as open work."));
        assert!(prompt.contains("Main task: rollout"));
        assert!(prompt.contains("Conversation:"));
        assert!(prompt.contains("Latest user turn:\ncontent: verify it now"));
        assert!(!prompt.contains("Use current evidence."));
        assert!(!prompt.contains("Continuity:"));
        assert!(!prompt.contains("prefer continuity"));
        assert!(
            prompt.len() < 1850,
            "chat prompt too large: {}",
            prompt.len()
        );
    }

    #[test]
    fn benchmark_report_context_defaults_to_airbnb_mission() {
        let latest = "AIRBNB_BENCH_REPORT_CYCLE_2\n\nBenchmark progress report is due now.\n\nFirst update this file:\n/tmp/airbnb_clone_bench/workspace/ops/progress/progress-latest.md\n";
        let context = live_context::derive_mission_context(&[], latest);
        assert_eq!(context.mission_id.as_deref(), Some("airbnb_bench"));
        assert_eq!(
            context.workspace_root.as_deref(),
            Some("/tmp/airbnb_clone_bench/workspace")
        );
        assert_eq!(context.turn_class, "report");
        assert_eq!(context.read_scope, "wide");
    }

    #[test]
    fn service_continuation_context_starts_at_latest_user_turn() {
        let messages = vec![
            lcm::MessageRecord {
                message_id: 1,
                conversation_id: 1,
                seq: 1,
                role: "user".to_string(),
                content: "older user".to_string(),
                token_count: 4,
                created_at: "2026-04-06T00:00:00Z".to_string(),
            },
            lcm::MessageRecord {
                message_id: 2,
                conversation_id: 1,
                seq: 2,
                role: "assistant".to_string(),
                content: "older assistant".to_string(),
                token_count: 4,
                created_at: "2026-04-06T00:00:01Z".to_string(),
            },
            lcm::MessageRecord {
                message_id: 3,
                conversation_id: 1,
                seq: 3,
                role: "user".to_string(),
                content: "Mission continuity watchdog: the mission was idle for 45s.\n\nMission: Keep the active mission alive from the latest durable continuity.".to_string(),
                token_count: 32,
                created_at: "2026-04-06T00:00:02Z".to_string(),
            },
        ];
        let latest = messages.last().unwrap().content.clone();
        let context = live_context::derive_mission_context(&messages, &latest);
        assert_eq!(context.start_seq, Some(3));
        assert_eq!(context.turn_class, "continue");
        assert_eq!(context.read_scope, "narrow");
    }

    #[test]
    fn benchmark_anchor_block_includes_workspace_only() {
        let context = live_context::MissionContext {
            mission_id: Some("airbnb_bench".to_string()),
            workspace_root: Some("/tmp/airbnb_clone_bench/workspace".to_string()),
            report_headings: vec!["Mission".to_string(), "Completed".to_string()],
            ..Default::default()
        };
        let block = live_context::synthesize_anchor_block(&context);
        assert!(block.contains("- work only inside the workspace"));
    }

    #[test]
    fn strip_prompt_comments_removes_html_comment_blocks() {
        let rendered =
            live_context::strip_prompt_comments("alpha\n<!-- note\nkeep prompt tight\n-->\nbeta");
        assert_eq!(rendered, "alpha\n\nbeta");
    }

    #[test]
    fn quota_blocker_gets_long_retry_cooldown() {
        assert_eq!(
            hard_runtime_blocker_retry_cooldown_secs(
                "Quota exceeded. Check your plan and billing details."
            ),
            Some(1_800)
        );
        assert_eq!(
            hard_runtime_blocker_retry_cooldown_secs(
                "CTOX chat could not continue because the configured OpenAI API quota is exhausted or billing is unavailable for the selected model."
            ),
            Some(1_800)
        );
    }

    #[test]
    fn rendered_context_omits_older_items_when_history_is_large() {
        let messages = (1..=40)
            .map(|index| lcm::MessageRecord {
                message_id: index,
                conversation_id: 1,
                seq: index,
                role: if index % 2 == 0 {
                    "assistant".to_string()
                } else {
                    "user".to_string()
                },
                content: format!("message {index}: {}", "x".repeat(250)),
                token_count: 40,
                created_at: "2026-03-26T10:00:00Z".to_string(),
            })
            .collect::<Vec<_>>();
        let context_items = (1..=40)
            .map(|index| lcm::ContextItemSnapshot {
                ordinal: index,
                item_type: lcm::ContextItemType::Message,
                message_id: Some(index),
                summary_id: None,
                seq: index,
                depth: 0,
                token_count: 40,
            })
            .collect::<Vec<_>>();
        let snapshot = lcm::LcmSnapshot {
            conversation_id: 1,
            messages,
            summaries: Vec::new(),
            context_items,
            summary_edges: Vec::new(),
            summary_messages: Vec::new(),
        };

        let prompt_view = live_context::build_prompt_snapshot_view(&snapshot);
        let selected = live_context::select_rendered_context(
            &prompt_view.snapshot,
            prompt_view.latest_user_message_id,
            prompt_view.mission_start_seq,
        );
        assert!(!selected.entries.is_empty());
        assert!(selected.omitted_items > 0);
        assert!(selected.entries.len() <= live_context::MAX_RENDERED_MESSAGE_ITEMS);
        let joined = selected.entries.join("\n");
        assert!(!joined.contains("message 1:"));
        assert!(!joined.contains("message 40:"));
        assert!(joined.contains("message 38:"));
    }

    #[test]
    fn prompt_snapshot_view_excludes_latest_user_and_trailing_assistant_from_conversation() {
        let messages = vec![
            lcm::MessageRecord {
                message_id: 1,
                conversation_id: 1,
                seq: 1,
                role: "user".to_string(),
                content: "old user".to_string(),
                token_count: 5,
                created_at: "2026-03-26T10:00:00Z".to_string(),
            },
            lcm::MessageRecord {
                message_id: 2,
                conversation_id: 1,
                seq: 2,
                role: "assistant".to_string(),
                content: "old assistant".to_string(),
                token_count: 5,
                created_at: "2026-03-26T10:00:01Z".to_string(),
            },
            lcm::MessageRecord {
                message_id: 3,
                conversation_id: 1,
                seq: 3,
                role: "user".to_string(),
                content: "latest user".to_string(),
                token_count: 5,
                created_at: "2026-03-26T10:00:02Z".to_string(),
            },
            lcm::MessageRecord {
                message_id: 4,
                conversation_id: 1,
                seq: 4,
                role: "assistant".to_string(),
                content: "should not appear".to_string(),
                token_count: 5,
                created_at: "2026-03-26T10:00:03Z".to_string(),
            },
        ];
        let context_items = (1..=4)
            .map(|index| lcm::ContextItemSnapshot {
                ordinal: index,
                item_type: lcm::ContextItemType::Message,
                message_id: Some(index),
                summary_id: None,
                seq: index,
                depth: 0,
                token_count: 5,
            })
            .collect::<Vec<_>>();
        let snapshot = lcm::LcmSnapshot {
            conversation_id: 1,
            messages,
            summaries: Vec::new(),
            context_items,
            summary_edges: Vec::new(),
            summary_messages: Vec::new(),
        };

        let prompt_view = live_context::build_prompt_snapshot_view(&snapshot);
        let selected = live_context::select_rendered_context(
            &prompt_view.snapshot,
            prompt_view.latest_user_message_id,
            prompt_view.mission_start_seq,
        );
        let joined = selected.entries.join("\n");

        assert_eq!(prompt_view.latest_user_prompt, "latest user");
        assert!(!joined.contains("latest user"));
        assert!(!joined.contains("should not appear"));
        assert!(joined.contains("old user"));
        assert!(joined.contains("old assistant"));
    }

    #[test]
    fn service_continuation_prompt_omits_prior_conversation_history() {
        let messages = vec![
            lcm::MessageRecord {
                message_id: 1,
                conversation_id: 1,
                seq: 1,
                role: "user".to_string(),
                content: "older user".to_string(),
                token_count: 5,
                created_at: "2026-04-06T00:00:00Z".to_string(),
            },
            lcm::MessageRecord {
                message_id: 2,
                conversation_id: 1,
                seq: 2,
                role: "assistant".to_string(),
                content: "older assistant".to_string(),
                token_count: 5,
                created_at: "2026-04-06T00:00:01Z".to_string(),
            },
            lcm::MessageRecord {
                message_id: 3,
                conversation_id: 1,
                seq: 3,
                role: "user".to_string(),
                content: "Mission continuity watchdog: the mission was idle for 45s.\n\nMission: Keep the active mission alive from the latest durable continuity.".to_string(),
                token_count: 24,
                created_at: "2026-04-06T00:00:02Z".to_string(),
            },
        ];
        let summaries = vec![lcm::SummaryRecord {
            summary_id: "sum_1".to_string(),
            conversation_id: 1,
            depth: 0,
            kind: lcm::SummaryKind::Leaf,
            content: "LCM leaf summary at depth 0: older history".to_string(),
            token_count: 12,
            descendant_count: 2,
            descendant_token_count: 10,
            source_message_token_count: 10,
            created_at: "2026-04-06T00:00:03Z".to_string(),
        }];
        let context_items = vec![
            lcm::ContextItemSnapshot {
                ordinal: 1,
                item_type: lcm::ContextItemType::Summary,
                message_id: None,
                summary_id: Some("sum_1".to_string()),
                seq: 2,
                depth: 0,
                token_count: 12,
            },
            lcm::ContextItemSnapshot {
                ordinal: 2,
                item_type: lcm::ContextItemType::Message,
                message_id: Some(1),
                summary_id: None,
                seq: 1,
                depth: 0,
                token_count: 5,
            },
            lcm::ContextItemSnapshot {
                ordinal: 3,
                item_type: lcm::ContextItemType::Message,
                message_id: Some(2),
                summary_id: None,
                seq: 2,
                depth: 0,
                token_count: 5,
            },
            lcm::ContextItemSnapshot {
                ordinal: 4,
                item_type: lcm::ContextItemType::Message,
                message_id: Some(3),
                summary_id: None,
                seq: 3,
                depth: 0,
                token_count: 24,
            },
        ];
        let snapshot = lcm::LcmSnapshot {
            conversation_id: 1,
            messages,
            summaries,
            context_items,
            summary_edges: Vec::new(),
            summary_messages: Vec::new(),
        };

        let prompt_view = live_context::build_prompt_snapshot_view(&snapshot);
        let selected = live_context::select_rendered_context(
            &prompt_view.snapshot,
            prompt_view.latest_user_message_id,
            prompt_view.mission_start_seq,
        );

        assert!(
            selected.entries.is_empty(),
            "unexpected retained context entries: {}",
            selected.entries.join(" | ")
        );
    }

    #[test]
    fn runtime_error_summary_reduces_raw_event_stream() {
        let raw = concat!(
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"Ich prüfe noch den API-Pfad.\"}}\n",
            "{\"type\":\"item.started\",\"item\":{\"type\":\"command_execution\",\"command\":\"curl ...\"}}\n"
        );
        let summary = summarize_runtime_error(raw);
        assert!(summary.contains("Ich prüfe noch den API-Pfad."));
        assert!(!summary.contains("\"type\":\"item.completed\""));
    }

    #[test]
    fn runtime_error_summary_compacts_missing_module_build_failures() {
        let raw = concat!(
            "error[E0583]: file not found for module `agent_jobs`\n",
            "--> state/src/runtime.rs:52:1\n",
            "= help: to create the module `agent_jobs`, create file \"state/src/runtime/agent_jobs.rs\"\n"
        );
        let summary = summarize_runtime_error(raw);
        assert!(summary.contains("integrated agent runtime checkout is incomplete"));
        assert!(summary.contains("state/src/runtime/"));
        assert!(!summary.contains("error[E0583]"));
    }

    #[test]
    fn runtime_error_summary_compacts_old_cargo_failures() {
        let raw = concat!(
            "failed to parse manifest at `/home/metricspace/ctox/tools/agent-runtime/backend-client/Cargo.toml`\n\n",
            "Caused by:\n",
            "  feature `edition2024` is required\n"
        );
        let summary = summarize_runtime_error(raw);
        assert!(summary.contains("newer Rust/Cargo toolchain"));
        assert!(summary.contains("Edition 2024"));
        assert!(!summary.contains("failed to parse manifest"));
    }

    #[test]
    fn synthesized_failure_reply_has_operator_shape() {
        let reply = synthesize_failure_reply("codex-exec timed out after 180s");
        assert!(reply.starts_with("Status: `blocked`"));
        assert!(reply.contains("180s"));
    }

    #[test]
    fn local_exec_policy_keeps_model_specific_compaction_limit_in_adapter_layer() {
        let policy = LocalCodexExecPolicy::resolve("openai/gpt-oss-20b")
            .expect("gpt-oss local exec policy should resolve");
        assert_eq!(policy.compact_limit(131_072), 1_280);
    }

    #[test]
    fn local_model_provider_prefers_direct_runtime_endpoint() {
        let runtime = runtime_kernel::InferenceRuntimeKernel {
            state: runtime_state::InferenceRuntimeState {
                version: 4,
                source: runtime_state::InferenceSource::Local,
                local_runtime: runtime_state::LocalRuntimeKind::Candle,
                base_model: Some("openai/gpt-oss-20b".to_string()),
                requested_model: Some("openai/gpt-oss-20b".to_string()),
                active_model: Some("openai/gpt-oss-20b".to_string()),
                engine_model: Some("openai/gpt-oss-20b".to_string()),
                engine_port: Some(2234),
                realized_context_tokens: Some(131_072),
                proxy_host: "127.0.0.1".to_string(),
                proxy_port: 12434,
                upstream_base_url: "http://127.0.0.1:2234".to_string(),
                local_preset: Some("quality".to_string()),
                boost: runtime_state::BoostRuntimeState::default(),
                adapter_tuning: runtime_state::AdapterRuntimeTuning::default(),
                embedding: runtime_state::AuxiliaryRuntimeState::default(),
                transcription: runtime_state::AuxiliaryRuntimeState::default(),
                speech: runtime_state::AuxiliaryRuntimeState::default(),
            },
            ownership: Default::default(),
            proxy: runtime_kernel::ResolvedProxyRuntime {
                listen_host: "127.0.0.1".to_string(),
                listen_port: 12434,
                upstream_base_url: "http://127.0.0.1:2234".to_string(),
                active_model: Some("openai/gpt-oss-20b".to_string()),
                embedding_base_url: String::new(),
                embedding_model: None,
                transcription_base_url: String::new(),
                transcription_model: None,
                speech_base_url: String::new(),
                speech_model: None,
            },
            primary_generation: Some(runtime_kernel::ResolvedRuntimeBinding {
                workload: runtime_kernel::InferenceWorkloadRole::PrimaryGeneration,
                display_model: "openai/gpt-oss-20b".to_string(),
                request_model: "openai/gpt-oss-20b".to_string(),
                port: 2234,
                base_url: "http://127.0.0.1:2234".to_string(),
                socket_path: Some("/tmp/ctox-primary.sock".to_string()),
                health_path: "/health",
                launcher_kind: runtime_kernel::RuntimeLauncherKind::Engine,
                compute_target: None,
                visible_devices: None,
            }),
            embedding: None,
            transcription: None,
            speech: None,
        };

        let overrides = CodexExecConfigSpec {
            model_instructions_file: PathBuf::from("/tmp/instructions.md"),
            project_doc_max_bytes: None,
            model_context_window: None,
            model_auto_compact_token_limit: None,
            model_reasoning_effort: None,
            web_search_mode: "disabled",
            include_apply_patch_tool: false,
            unified_exec_enabled: false,
            ctox_web_enabled: false,
            disable_skills: false,
            local_model_provider: LocalModelProviderSpec::resolve(
                "openai/gpt-oss-20b",
                Some(&runtime),
            ),
            api_model_provider: None,
            extra_cli_overrides: Vec::new(),
        }
        .into_cli_overrides();

        assert!(overrides.contains(&"model_provider=\"cto_local\"".to_string()));
        assert!(overrides
            .contains(&"model_providers.cto_local.socket_transport_required=true".to_string()));
        assert!(overrides.contains(
            &"model_providers.cto_local.socket_path=\"/tmp/ctox-primary.sock\"".to_string()
        ));
        assert!(!overrides.iter().any(|entry| entry.contains("base_url=")));
    }

    #[test]
    fn local_model_provider_requires_resolved_runtime() {
        assert!(LocalModelProviderSpec::resolve("openai/gpt-oss-20b", None).is_none());
    }

    #[test]
    fn resolve_api_model_provider_spec_uses_openrouter_provider_for_remote_qwen() {
        let mut settings = BTreeMap::new();
        settings.insert("CTOX_API_PROVIDER".to_string(), "openrouter".to_string());
        settings.insert("CTOX_CHAT_SOURCE".to_string(), "api".to_string());
        settings.insert(
            "CTOX_UPSTREAM_BASE_URL".to_string(),
            "https://openrouter.ai/api".to_string(),
        );

        let provider = resolve_api_model_provider_spec("qwen/qwen3.5-9b", &settings, None)
            .expect("openrouter provider should resolve");
        assert_eq!(
            provider,
            ApiModelProviderSpec {
                provider_id: "cto_openrouter",
                name: "cto-openrouter",
                base_url: "https://openrouter.ai/api/v1".to_string(),
                env_key: "OPENROUTER_API_KEY",
            }
        );
    }

    #[test]
    fn openai_native_web_search_only_applies_to_direct_openai_models() {
        assert!(use_openai_native_web_search("gpt-5.4-mini", None));
        assert!(!use_openai_native_web_search("openai/gpt-oss-20b", None));
        assert!(!use_openai_native_web_search(
            "qwen/qwen3.5-9b",
            Some(&ApiModelProviderSpec {
                provider_id: "cto_openrouter",
                name: "cto-openrouter",
                base_url: "https://openrouter.ai/api/v1".to_string(),
                env_key: "OPENROUTER_API_KEY",
            })
        ));
    }

    #[test]
    fn codex_exec_config_emits_openrouter_provider_overrides() {
        let overrides = CodexExecConfigSpec {
            model_instructions_file: PathBuf::from("/tmp/instructions.md"),
            project_doc_max_bytes: None,
            model_context_window: None,
            model_auto_compact_token_limit: None,
            model_reasoning_effort: None,
            web_search_mode: "disabled",
            include_apply_patch_tool: false,
            unified_exec_enabled: false,
            ctox_web_enabled: false,
            disable_skills: false,
            local_model_provider: None,
            api_model_provider: Some(ApiModelProviderSpec {
                provider_id: "cto_openrouter",
                name: "cto-openrouter",
                base_url: "https://openrouter.ai/api/v1".to_string(),
                env_key: "OPENROUTER_API_KEY",
            }),
            extra_cli_overrides: Vec::new(),
        }
        .into_cli_overrides();

        assert!(overrides.contains(&"model_provider=\"cto_openrouter\"".to_string()));
        assert!(overrides.contains(
            &"model_providers.cto_openrouter.base_url=\"https://openrouter.ai/api/v1\"".to_string()
        ));
        assert!(overrides.contains(
            &"model_providers.cto_openrouter.env_key=\"OPENROUTER_API_KEY\"".to_string()
        ));
        assert!(overrides
            .contains(&"model_providers.cto_openrouter.wire_api=\"responses\"".to_string()));
    }

    #[test]
    fn missing_codex_exec_binary_is_a_hard_error() {
        let root = std::env::temp_dir().join(format!(
            "ctox-missing-codex-exec-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("tools/agent-runtime")).expect("create tools dir");

        let err = CodexExecBinary::resolve(&root, None).expect_err("expected hard failure");
        let rendered = err.to_string();
        assert!(rendered.contains("required codex-exec binary is missing"));
        assert!(rendered.contains("no longer falls back to source execution"));
    }

    #[test]
    fn live_local_responses_compact_path_uses_generic_base_instructions() {
        if std::env::var("CTOX_RUN_LIVE_GPT_OSS_SMOKE").ok().as_deref() != Some("1") {
            return;
        }

        let root = std::env::var_os("CTOX_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().expect("resolve current dir"));
        let mut settings = BTreeMap::new();
        settings.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        settings.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        settings.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        settings.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        settings.insert(
            "CTOX_CHAT_MODEL_REALIZED_CONTEXT".to_string(),
            "131072".to_string(),
        );
        settings.insert(
            "CTOX_CHAT_MODEL_MAX_CONTEXT".to_string(),
            "131072".to_string(),
        );
        settings.insert("CTOX_PROXY_HOST".to_string(), "127.0.0.1".to_string());
        settings.insert("CTOX_PROXY_PORT".to_string(), "12434".to_string());

        let codex_home =
            std::env::temp_dir().join(format!("ctox-live-gptoss-smoke-{}", std::process::id()));
        std::fs::create_dir_all(&codex_home).expect("create temporary CODEX_HOME");
        settings.insert(
            "CODEX_HOME".to_string(),
            codex_home.to_string_lossy().to_string(),
        );

        let request_dump_path = root.join("runtime/last_local_chat_request.json");
        let _ = std::fs::remove_file(&request_dump_path);

        let result = super::invoke_codex_exec_with_timeout_and_instructions(
            &root,
            &settings,
            "Reply with exactly CTOX_SMOKE_OK and nothing else.",
            None,
            Some(Duration::from_secs(180)),
            None,
            Some(false),
        );

        let raw_request =
            std::fs::read(&request_dump_path).expect("expected runtime request dump to exist");
        let payload: serde_json::Value =
            serde_json::from_slice(&raw_request).expect("parse runtime request dump");
        let system_prompt = payload["messages"][0]["content"]
            .as_str()
            .expect("expected rendered system prompt");

        assert!(system_prompt.contains("You are CTOX"));
        assert!(
            !system_prompt.contains("local responses-backed runtime"),
            "expected standard CTOX prompt instead of compact local override, got: {system_prompt}"
        );
        assert!(
            result.is_ok(),
            "expected integrated local GPT-OSS path to complete, got: {:?}",
            result
        );
    }

    #[test]
    fn simple_skill_preset_keeps_shared_ctox_core_and_adds_simple_exec_overlay() {
        let root = temp_root("ctox-live-simple-prompt");
        let mut settings = BTreeMap::new();
        settings.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        settings.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        settings.insert(
            "CTOX_CHAT_MODEL_REALIZED_CONTEXT".to_string(),
            "131072".to_string(),
        );
        settings.insert(
            CHAT_SKILL_PRESET_ENV_KEY.to_string(),
            runtime_state::ChatSkillPreset::Simple.label().to_string(),
        );
        settings.insert("CTOX_PROXY_HOST".to_string(), "127.0.0.1".to_string());
        settings.insert("CTOX_PROXY_PORT".to_string(), "12434".to_string());

        let codex_home =
            std::env::temp_dir().join(format!("ctox-live-simple-smoke-{}", std::process::id()));
        std::fs::create_dir_all(&codex_home).expect("create temporary CODEX_HOME");
        settings.insert(
            "CODEX_HOME".to_string(),
            codex_home.to_string_lossy().to_string(),
        );

        let request_dump_path = root.join("runtime/last_local_chat_request.json");
        let _ = std::fs::remove_file(&request_dump_path);

        let _ = super::invoke_codex_exec_with_timeout_and_instructions(
            &root,
            &settings,
            "Reply with exactly CTOX_SMOKE_OK and nothing else.",
            None,
            Some(Duration::from_secs(180)),
            None,
            Some(false),
        );

        let raw_request =
            std::fs::read(&request_dump_path).expect("expected runtime request dump to exist");
        let payload: serde_json::Value =
            serde_json::from_slice(&raw_request).expect("parse runtime request dump");
        let system_prompt = payload["messages"][0]["content"]
            .as_str()
            .expect("expected rendered system prompt");

        assert!(system_prompt.contains("You are CTOX"));
        assert!(system_prompt.contains("CTOX codex-exec mode: `Simple`"));
        assert!(system_prompt.contains("Current phase must fit exactly one work mode"));
    }

    #[test]
    fn standard_skill_preset_uses_standard_exec_overlay() {
        let root = temp_root("ctox-live-standard-prompt");
        let mut settings = BTreeMap::new();
        settings.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        settings.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "openai/gpt-oss-20b".to_string(),
        );
        settings.insert(
            "CTOX_CHAT_MODEL_REALIZED_CONTEXT".to_string(),
            "131072".to_string(),
        );
        settings.insert("CTOX_PROXY_HOST".to_string(), "127.0.0.1".to_string());
        settings.insert("CTOX_PROXY_PORT".to_string(), "12434".to_string());

        let codex_home =
            std::env::temp_dir().join(format!("ctox-live-standard-smoke-{}", std::process::id()));
        std::fs::create_dir_all(&codex_home).expect("create temporary CODEX_HOME");
        settings.insert(
            "CODEX_HOME".to_string(),
            codex_home.to_string_lossy().to_string(),
        );

        let request_dump_path = root.join("runtime/last_local_chat_request.json");
        let _ = std::fs::remove_file(&request_dump_path);

        let _ = super::invoke_codex_exec_with_timeout_and_instructions(
            &root,
            &settings,
            "Reply with exactly CTOX_SMOKE_OK and nothing else.",
            None,
            Some(Duration::from_secs(180)),
            None,
            Some(false),
        );

        let raw_request =
            std::fs::read(&request_dump_path).expect("expected runtime request dump to exist");
        let payload: serde_json::Value =
            serde_json::from_slice(&raw_request).expect("parse runtime request dump");
        let system_prompt = payload["messages"][0]["content"]
            .as_str()
            .expect("expected rendered system prompt");

        assert!(system_prompt.contains("You are CTOX"));
        assert!(system_prompt.contains("CTOX codex-exec mode: `Standard`"));
        assert!(!system_prompt.contains("Active meta-skill loadout:"));
    }

    #[test]
    fn simple_phase_classifier_prefers_ops_debug_terminal_for_service_debugging() {
        let prompt =
            "Check the service logs with journalctl, verify health, and restart only if needed.";
        assert_eq!(
            classify_simple_exec_phase(prompt),
            SimpleExecPhase::OpsDebugTerminal
        );
    }

    #[test]
    fn simple_phase_classifier_prefers_bug_fix_for_failures() {
        let prompt = "Fix the failing test and stop the regression in the parser.";
        assert_eq!(classify_simple_exec_phase(prompt), SimpleExecPhase::BugFix);
    }

    #[test]
    fn simple_phase_classifier_falls_back_to_read_trace_first_when_unclear() {
        let prompt = "Investigate this area and figure out where the change should go.";
        assert_eq!(
            classify_simple_exec_phase(prompt),
            SimpleExecPhase::ReadTraceFirst
        );
    }

    #[test]
    fn simple_phase_classifier_prefers_dependency_update_for_version_bumps() {
        let prompt =
            "Bump the base image version and update the lockfile for this dependency update.";
        assert_eq!(
            classify_simple_exec_phase(prompt),
            SimpleExecPhase::DependencyUpdate
        );
    }

    #[test]
    fn simple_phase_classifier_prefers_migration_change_for_schema_work() {
        let prompt = "Add a migration for this schema change and verify the rollback path.";
        assert_eq!(
            classify_simple_exec_phase(prompt),
            SimpleExecPhase::MigrationChange
        );
    }

    #[test]
    fn simple_phase_classifier_prefers_incident_hotfix_before_generic_bug_fix() {
        let prompt = "Service is broken now. Apply the smallest hotfix to restore traffic.";
        assert_eq!(
            classify_simple_exec_phase(prompt),
            SimpleExecPhase::IncidentHotfix
        );
    }

    #[test]
    fn simple_phase_classifier_prefers_rollback_recovery_for_rollbacks() {
        let prompt =
            "Rollback to the last known good version and verify health after the rollback.";
        assert_eq!(
            classify_simple_exec_phase(prompt),
            SimpleExecPhase::RollbackRecovery
        );
    }

    #[test]
    fn simple_phase_classifier_prefers_bug_fix_with_tests_for_mixed_fix_and_test_work() {
        let prompt = "Fix the regression and update the failing test for that exact behavior.";
        assert_eq!(
            classify_simple_exec_phase(prompt),
            SimpleExecPhase::BugFixWithTests
        );
    }

    #[test]
    fn simple_phase_classifier_prefers_migration_with_data_change_for_coupled_schema_and_backfill()
    {
        let prompt =
            "Add the migration, run the backfill dry run, and verify before and after counts.";
        assert_eq!(
            classify_simple_exec_phase(prompt),
            SimpleExecPhase::MigrationWithDataChange
        );
    }

    #[test]
    fn simple_phase_classifier_prefers_incident_hotfix_deploy_for_restore_with_deploy() {
        let prompt =
            "Service is broken now. Apply the smallest hotfix, deploy it, and verify health.";
        assert_eq!(
            classify_simple_exec_phase(prompt),
            SimpleExecPhase::IncidentHotfixDeploy
        );
    }

    #[test]
    fn simple_phase_classifier_prefers_docs_text_change_for_prompt_wording() {
        let prompt = "Update the prompt wording in this markdown doc and keep the text aligned.";
        assert_eq!(
            classify_simple_exec_phase(prompt),
            SimpleExecPhase::DocsTextChange
        );
    }

    #[test]
    fn simple_phase_classifier_prefers_analysis_read_only_for_explanations() {
        let prompt = "Explain why this behavior happens and compare the two approaches.";
        assert_eq!(
            classify_simple_exec_phase(prompt),
            SimpleExecPhase::AnalysisReadOnly
        );
    }

    #[test]
    fn simple_phase_classifier_falls_back_to_general_safe_task_for_non_specific_work() {
        let prompt = "Help me with this small task in the safest possible way.";
        assert_eq!(
            classify_simple_exec_phase(prompt),
            SimpleExecPhase::GeneralSafeTask
        );
    }

    #[test]
    fn selected_skill_preset_defaults_to_standard() {
        let settings = BTreeMap::new();
        assert_eq!(
            selected_skill_preset(&settings),
            runtime_state::ChatSkillPreset::Standard
        );
    }

    #[test]
    fn selected_skill_preset_accepts_simple_label() {
        let mut settings = BTreeMap::new();
        settings.insert(CHAT_SKILL_PRESET_ENV_KEY.to_string(), "Simple".to_string());
        assert_eq!(
            selected_skill_preset(&settings),
            runtime_state::ChatSkillPreset::Simple
        );
    }

    #[test]
    fn explicit_chat_reasoning_effort_setting_is_normalized() {
        let mut settings = BTreeMap::new();
        settings.insert(
            CHAT_MODEL_REASONING_EFFORT_ENV_KEY.to_string(),
            "minimal".to_string(),
        );
        assert_eq!(
            read_reasoning_effort_setting(&settings, CHAT_MODEL_REASONING_EFFORT_ENV_KEY),
            Some("low".to_string())
        );

        settings.insert(
            CHAT_MODEL_REASONING_EFFORT_ENV_KEY.to_string(),
            "MEDIUM".to_string(),
        );
        assert_eq!(
            read_reasoning_effort_setting(&settings, CHAT_MODEL_REASONING_EFFORT_ENV_KEY),
            Some("medium".to_string())
        );
    }

    #[test]
    fn explicit_chat_reasoning_effort_setting_ignores_invalid_values() {
        let mut settings = BTreeMap::new();
        settings.insert(
            CHAT_MODEL_REASONING_EFFORT_ENV_KEY.to_string(),
            "turbo".to_string(),
        );
        assert_eq!(
            read_reasoning_effort_setting(&settings, CHAT_MODEL_REASONING_EFFORT_ENV_KEY),
            None
        );
    }

    #[test]
    fn tool_verification_guard_detects_cpp_build_prompt() {
        let prompt = concat!(
            "Work only inside this workspace:\n",
            "/tmp/cpp-chat-app\n\n",
            "Create a bounded C++ verification project in this workspace.\n",
            "- Use CMake.\n",
            "- Create at least these files: CMakeLists.txt, src/main.cpp, include/MessageQueue.h\n",
            "- Build it with: cmake -S . -B build && cmake --build build -j\n",
            "- Verify the binary with: ./build/ctox_cpp_smoke\n",
            "- Do not answer before the files exist and the binary was executed successfully.\n",
        );
        assert!(prompt_requires_tool_verification(prompt));
    }

    #[test]
    fn tool_verification_guard_skips_marker_only_workspace_prompt() {
        let prompt = "Work only inside this workspace: /tmp/socket-smoke. Reply with exactly CTOX_SOCKET_SMOKE_OK and nothing else.";
        assert!(!prompt_requires_tool_verification(prompt));
    }

    #[test]
    fn tool_verification_guard_skips_continuity_refresh_prompt() {
        let prompt = concat!(
            "You are updating the CTOX continuity document for conversation 1.\n",
            "Reply with only a diff that uses the existing sections.\n",
            "<CURRENT_DOCUMENT>\n# CONTINUITY ANCHORS\n</CURRENT_DOCUMENT>\n",
            "<RECENT_MESSAGES>\n[user #1] Work only inside this workspace: /tmp/bench\n</RECENT_MESSAGES>\n",
        );
        assert!(!prompt_requires_tool_verification(prompt));
    }

    #[test]
    fn tool_verification_guard_flags_event_stream_without_tool_activity() {
        let prompt = concat!(
            "Work only inside this workspace:\n",
            "/tmp/bench\n\n",
            "Create ops/handoff/handoff-note.md and verify runtime queue state.\n",
        );
        let raw = concat!(
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"Mission\\n\\nDone.\"}}\n"
        );
        let summary = turn_contract::summarize_event_stream(raw).expect("event stream summary");
        assert!(response_missing_required_tool_activity(
            prompt,
            Some(&summary)
        ));
    }

    #[test]
    fn tool_verification_guard_accepts_event_stream_with_tool_activity() {
        let prompt = concat!(
            "Work only inside this workspace:\n",
            "/tmp/bench\n\n",
            "Create ops/handoff/handoff-note.md and verify runtime queue state.\n",
        );
        let raw = concat!(
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"command_execution\",\"command\":\"ctox queue add --title test --prompt test\",\"aggregated_output\":\"ok\",\"exit_code\":0,\"status\":\"completed\"}}\n",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"Mission\\n\\nDone.\"}}\n"
        );
        let summary = turn_contract::summarize_event_stream(raw).expect("event stream summary");
        assert!(!response_missing_required_tool_activity(
            prompt,
            Some(&summary)
        ));
    }

    #[test]
    fn child_output_is_drained_while_waiting_for_exit() {
        let mut command = std::process::Command::new("python3");
        command.args([
            "-c",
            "import sys; sys.stdout.write('x' * 200000); sys.stdout.flush()",
        ]);
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        let child = command.spawn().expect("failed to spawn python3");
        let output = collect_child_output_with_timeout(child, std::time::Duration::from_secs(10))
            .expect("failed to collect child output");
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 200000);
    }

    #[test]
    fn continuity_diff_log_summary_escapes_and_counts_lines() {
        let summary = summarize_continuity_diff_for_log(
            "  ## Status\n  + Mission: Keep gateway intake hardening as the main mission.\n  - none\n",
        );
        assert!(summary.contains("chars="));
        assert!(summary.contains("lines=3"));
        assert!(summary.contains("\\n"));
        assert!(summary.contains("Mission: Keep gateway intake hardening as the main mission."));
    }

    #[test]
    fn continuity_diff_log_summary_truncates_long_diff() {
        let long_diff = format!("+ {}\n", "x".repeat(800));
        let summary = summarize_continuity_diff_for_log(&long_diff);
        assert!(summary.contains("chars=802"));
        assert!(summary.ends_with("...\""));
    }

    #[test]
    fn continuity_refresh_repair_extracts_diff_from_event_stream() {
        let raw = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"abc\"}\n",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"## Status\\n+ Mission: Hold the same restart mission.\\n\"}}\n",
            "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":12,\"output_tokens\":6}}\n"
        );
        let repaired = repair_continuity_refresh_output(raw);
        match repaired {
            ContinuityRefreshRepair::Apply {
                diff,
                repair_reason,
            } => {
                assert_eq!(
                    repair_reason,
                    Some("extracted final agent message from event stream")
                );
                assert_eq!(diff, "## Status\n+ Mission: Hold the same restart mission.");
            }
            ContinuityRefreshRepair::Noop { reason } => {
                panic!("unexpected noop repair outcome: {reason}");
            }
        }
    }

    #[test]
    fn continuity_refresh_repair_treats_empty_event_stream_as_noop() {
        let raw = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"abc\"}\n",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"\"}}\n",
            "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":12,\"output_tokens\":6}}\n"
        );
        let repaired = repair_continuity_refresh_output(raw);
        match repaired {
            ContinuityRefreshRepair::Apply { diff, .. } => {
                panic!("unexpected repaired diff: {diff}");
            }
            ContinuityRefreshRepair::Noop { reason } => {
                assert_eq!(reason, "event stream contained no non-empty agent message");
            }
        }
    }

    #[test]
    fn continuity_refresh_repair_strips_markdown_fences() {
        let raw = "```diff\n## Status\n+ Mission: Keep the current task.\n```";
        assert_eq!(
            strip_markdown_code_fences(raw),
            "## Status\n+ Mission: Keep the current task."
        );
    }

    #[test]
    fn continuity_refresh_faults_are_consumed_in_order() {
        let root = std::env::temp_dir().join(format!(
            "ctox-continuity-faults-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("runtime")).expect("create runtime dir");
        let fault_path = root.join("runtime/continuity_faults.json");
        std::fs::write(
            &fault_path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "anchors": ["first anchors fault", {"raw": "second anchors fault"}],
                "focus": ["focus fault"],
            }))
            .expect("serialize fault script"),
        )
        .expect("write fault script");

        let mut settings = BTreeMap::new();
        settings.insert(
            CONTINUITY_REFRESH_FAULT_FILE_ENV_KEY.to_string(),
            "runtime/continuity_faults.json".to_string(),
        );

        assert_eq!(
            take_continuity_refresh_fault(&root, &settings, "anchors")
                .expect("read first anchors fault"),
            Some("first anchors fault".to_string())
        );
        assert_eq!(
            take_continuity_refresh_fault(&root, &settings, "anchors")
                .expect("read second anchors fault"),
            Some("second anchors fault".to_string())
        );
        assert_eq!(
            take_continuity_refresh_fault(&root, &settings, "anchors")
                .expect("anchors should now be exhausted"),
            None
        );
        assert_eq!(
            take_continuity_refresh_fault(&root, &settings, "focus").expect("read focus fault"),
            Some("focus fault".to_string())
        );

        let remaining: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&fault_path).expect("read updated fault file"))
                .expect("parse updated fault file");
        assert_eq!(remaining["anchors"], serde_json::json!([]));
        assert_eq!(remaining["focus"], serde_json::json!([]));
    }
}
