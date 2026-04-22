use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use crate::exec::ExecToolCallOutput;
use crate::exec::StreamOutput;
use crate::features::Feature;
use crate::function_tool::FunctionCallError;
use crate::protocol::ExecCommandSource;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::SharedTurnDiffTracker;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::events::ToolEmitter;
use crate::tools::events::ToolEventCtx;
use crate::tools::events::ToolEventFailure;
use crate::tools::events::ToolEventStage;
use crate::tools::handlers::parse_arguments;
use crate::tools::js_repl::JS_REPL_PRAGMA_PREFIX;
use crate::tools::js_repl::JsReplArgs;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use ctox_protocol::models::FunctionCallOutputContentItem;

pub struct JsReplHandler;
pub struct JsReplResetHandler;

fn join_outputs(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_string()
    } else if stderr.is_empty() {
        stdout.to_string()
    } else {
        format!("{stdout}\n{stderr}")
    }
}

fn build_js_repl_exec_output(
    output: &str,
    error: Option<&str>,
    duration: Duration,
) -> ExecToolCallOutput {
    let stdout = output.to_string();
    let stderr = error.unwrap_or("").to_string();
    let aggregated_output = join_outputs(&stdout, &stderr);
    ExecToolCallOutput {
        exit_code: if error.is_some() { 1 } else { 0 },
        stdout: StreamOutput::new(stdout),
        stderr: StreamOutput::new(stderr),
        aggregated_output: StreamOutput::new(aggregated_output),
        duration,
        timed_out: false,
    }
}

async fn emit_js_repl_exec_begin(
    session: &crate::codex::Session,
    turn: &crate::codex::TurnContext,
    call_id: &str,
    tool_event_name: &str,
) {
    let emitter = ToolEmitter::shell(
        vec![tool_event_name.to_string()],
        turn.cwd.clone(),
        ExecCommandSource::Agent,
        /*freeform*/ false,
    );
    let ctx = ToolEventCtx::new(session, turn, call_id, /*turn_diff_tracker*/ None);
    emitter.emit(ctx, ToolEventStage::Begin).await;
}

async fn emit_js_repl_exec_end(
    session: &crate::codex::Session,
    turn: &crate::codex::TurnContext,
    call_id: &str,
    output: &str,
    error: Option<&str>,
    duration: Duration,
    tool_event_name: &str,
) {
    let exec_output = build_js_repl_exec_output(output, error, duration);
    let emitter = ToolEmitter::shell(
        vec![tool_event_name.to_string()],
        turn.cwd.clone(),
        ExecCommandSource::Agent,
        /*freeform*/ false,
    );
    let ctx = ToolEventCtx::new(session, turn, call_id, /*turn_diff_tracker*/ None);
    let stage = if error.is_some() {
        ToolEventStage::Failure(ToolEventFailure::Output(exec_output))
    } else {
        ToolEventStage::Success(exec_output)
    };
    emitter.emit(ctx, stage).await;
}
#[async_trait]
impl ToolHandler for JsReplHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(
            payload,
            ToolPayload::Function { .. } | ToolPayload::Custom { .. }
        )
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { payload, .. } = &invocation;
        let args = match payload {
            ToolPayload::Function { arguments } => parse_arguments(arguments)?,
            ToolPayload::Custom { input } => parse_js_repl_freeform_args(input)?,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "js_repl expects custom or function payload".to_string(),
                ));
            }
        };
        let ToolInvocation {
            session,
            turn,
            tracker,
            call_id,
            ..
        } = invocation;
        execute_js_repl_tool(session, turn, tracker, call_id, args, "js_repl").await
    }
}

#[async_trait]
impl ToolHandler for JsReplResetHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        if !invocation.session.features().enabled(Feature::JsRepl) {
            return Err(FunctionCallError::RespondToModel(
                "js_repl is disabled by feature flag".to_string(),
            ));
        }
        let manager = invocation.turn.js_repl.manager().await?;
        manager.reset().await?;
        Ok(FunctionToolOutput::from_text(
            "js_repl kernel reset".to_string(),
            Some(true),
        ))
    }
}

pub(crate) async fn execute_js_repl_tool(
    session: Arc<crate::codex::Session>,
    turn: Arc<crate::codex::TurnContext>,
    tracker: SharedTurnDiffTracker,
    call_id: String,
    args: JsReplArgs,
    tool_event_name: &str,
) -> Result<FunctionToolOutput, FunctionCallError> {
    if !session.features().enabled(Feature::JsRepl) {
        return Err(FunctionCallError::RespondToModel(format!(
            "{tool_event_name} is disabled by feature flag"
        )));
    }

    let manager = turn.js_repl.manager().await?;
    let started_at = Instant::now();
    emit_js_repl_exec_begin(session.as_ref(), turn.as_ref(), &call_id, tool_event_name).await;
    let result = manager
        .execute(Arc::clone(&session), Arc::clone(&turn), tracker, args)
        .await;
    let result = match result {
        Ok(result) => result,
        Err(err) => {
            let message = err.to_string();
            emit_js_repl_exec_end(
                session.as_ref(),
                turn.as_ref(),
                &call_id,
                "",
                Some(&message),
                started_at.elapsed(),
                tool_event_name,
            )
            .await;
            return Err(err);
        }
    };

    let content = result.output;
    let mut items = Vec::with_capacity(result.content_items.len() + 1);
    if !content.is_empty() {
        items.push(FunctionCallOutputContentItem::InputText {
            text: content.clone(),
        });
    }
    items.extend(result.content_items);

    emit_js_repl_exec_end(
        session.as_ref(),
        turn.as_ref(),
        &call_id,
        &content,
        /*error*/ None,
        started_at.elapsed(),
        tool_event_name,
    )
    .await;

    if items.is_empty() {
        Ok(FunctionToolOutput::from_text(content, Some(true)))
    } else {
        Ok(FunctionToolOutput::from_content(items, Some(true)))
    }
}

pub(crate) fn parse_js_repl_freeform_args(input: &str) -> Result<JsReplArgs, FunctionCallError> {
    parse_freeform_args_with_pragma(input, JS_REPL_PRAGMA_PREFIX, "js_repl")
}

pub(crate) fn parse_freeform_args_with_pragma(
    input: &str,
    pragma_prefix: &str,
    tool_name: &str,
) -> Result<JsReplArgs, FunctionCallError> {
    if input.trim().is_empty() {
        let message = if tool_name == "js_repl" {
            "js_repl expects raw JavaScript tool input (non-empty). Provide JS source text, optionally with first-line `// codex-js-repl: ...`."
                .to_string()
        } else {
            format!(
                "{tool_name} expects raw JavaScript tool input (non-empty). Provide JS source text, optionally with a first-line pragma."
            )
        };
        return Err(FunctionCallError::RespondToModel(message));
    }

    let mut args = JsReplArgs {
        code: input.to_string(),
        timeout_ms: None,
    };

    let mut lines = input.splitn(2, '\n');
    let first_line = lines.next().unwrap_or_default();
    let rest = lines.next().unwrap_or_default();
    let trimmed = first_line.trim_start();
    let Some(pragma) = trimmed.strip_prefix(pragma_prefix) else {
        reject_json_or_quoted_source(&args.code, tool_name)?;
        return Ok(args);
    };

    let mut timeout_ms: Option<u64> = None;
    let directive = pragma.trim();
    if !directive.is_empty() {
        for token in directive.split_whitespace() {
            let (key, value) = token.split_once('=').ok_or_else(|| {
                FunctionCallError::RespondToModel(format!(
                    "{tool_name} pragma expects space-separated key=value pairs (supported keys: timeout_ms); got `{token}`"
                ))
            })?;
            match key {
                "timeout_ms" => {
                    if timeout_ms.is_some() {
                        return Err(FunctionCallError::RespondToModel(format!(
                            "{tool_name} pragma specifies timeout_ms more than once"
                        )));
                    }
                    let parsed = value.parse::<u64>().map_err(|_| {
                        FunctionCallError::RespondToModel(format!(
                            "{tool_name} pragma timeout_ms must be an integer; got `{value}`"
                        ))
                    })?;
                    timeout_ms = Some(parsed);
                }
                _ => {
                    return Err(FunctionCallError::RespondToModel(format!(
                        "{tool_name} pragma only supports timeout_ms; got `{key}`"
                    )));
                }
            }
        }
    }

    if rest.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(format!(
            "{tool_name} pragma must be followed by JavaScript source on subsequent lines"
        )));
    }

    reject_json_or_quoted_source(rest, tool_name)?;
    args.code = rest.to_string();
    args.timeout_ms = timeout_ms;
    Ok(args)
}

fn reject_json_or_quoted_source(code: &str, tool_name: &str) -> Result<(), FunctionCallError> {
    let trimmed = code.trim();
    if trimmed.starts_with("```") {
        let message = if tool_name == "js_repl" {
            "js_repl expects raw JavaScript source, not markdown code fences. Resend plain JS only (optional first line `// codex-js-repl: ...`)."
                .to_string()
        } else {
            format!(
                "{tool_name} expects raw JavaScript source, not markdown code fences. Resend plain JS only."
            )
        };
        return Err(FunctionCallError::RespondToModel(message));
    }
    let Ok(value) = serde_json::from_str::<JsonValue>(trimmed) else {
        return Ok(());
    };
    match value {
        JsonValue::Object(_) | JsonValue::String(_) => Err(FunctionCallError::RespondToModel(
            if tool_name == "js_repl" {
                "js_repl is a freeform tool and expects raw JavaScript source. Resend plain JS only (optional first line `// codex-js-repl: ...`); do not send JSON (`{\"code\":...}`), quoted code, or markdown fences."
                    .to_string()
            } else {
                format!(
                    "{tool_name} is a freeform JavaScript tool. Resend plain JS only; do not send JSON (`{{\"code\":...}}`), quoted code, or markdown fences."
                )
            },
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "js_repl_tests.rs"]
mod tests;
