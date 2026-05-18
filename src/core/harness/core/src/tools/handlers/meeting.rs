use async_trait::async_trait;
use serde::Deserialize;
use tokio::process::Command;

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;

pub struct MeetingHandler;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MeetingStatusArgs {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MeetingGetTranscriptArgs {
    session_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MeetingSendChatArgs {
    session_id: String,
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MeetingJoinArgs {
    url: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MeetingScheduleArgs {
    url: String,
    time: String,
    #[serde(default)]
    name: Option<String>,
}

fn resolve_ctox_binary() -> Result<String, FunctionCallError> {
    if let Ok(path) = std::env::var("CTOX_BINARY") {
        return Ok(path);
    }
    // Fall back to PATH lookup
    Ok("ctox".to_string())
}

#[async_trait]
impl ToolHandler for MeetingHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn is_mutating(&self, invocation: &ToolInvocation) -> bool {
        matches!(
            invocation.tool_name.as_str(),
            "meeting_send_chat" | "meeting_join" | "meeting_schedule"
        )
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            tool_name, payload, ..
        } = invocation;
        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "meeting handler received unsupported payload".to_string(),
                ));
            }
        };

        let mut command = Command::new(resolve_ctox_binary()?);
        match tool_name.as_str() {
            "meeting_status" => {
                let _args: MeetingStatusArgs = parse_arguments(&arguments)?;
                command.arg("meeting").arg("status");
            }
            "meeting_get_transcript" => {
                let args: MeetingGetTranscriptArgs = parse_arguments(&arguments)?;
                command
                    .arg("meeting")
                    .arg("transcript")
                    .arg(&args.session_id);
            }
            "meeting_send_chat" => {
                let args: MeetingSendChatArgs = parse_arguments(&arguments)?;
                // Meeting uses thread_key=session_id and a fixed system
                // account_key. The channels.rs send_message() arm for "meeting"
                // forwards thread_key as session_id to the Playwright process.
                command
                    .arg("channel")
                    .arg("send")
                    .arg("--channel")
                    .arg("meeting")
                    .arg("--account-key")
                    .arg("meeting:system")
                    .arg("--thread-key")
                    .arg(&args.session_id)
                    .arg("--body")
                    .arg(&args.text);
            }
            "meeting_join" => {
                let args: MeetingJoinArgs = parse_arguments(&arguments)?;
                command.arg("meeting").arg("join").arg(&args.url);
                if let Some(name) = args.name {
                    command.arg("--name").arg(name);
                }
            }
            "meeting_schedule" => {
                let args: MeetingScheduleArgs = parse_arguments(&arguments)?;
                command
                    .arg("meeting")
                    .arg("schedule")
                    .arg(&args.url)
                    .arg("--time")
                    .arg(&args.time);
                if let Some(name) = args.name {
                    command.arg("--name").arg(name);
                }
            }
            other => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "unsupported meeting tool: {other}"
                )));
            }
        }

        let output = command.output().await.map_err(|err| {
            FunctionCallError::RespondToModel(format!("failed to run ctox meeting: {err}"))
        })?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            return Err(FunctionCallError::RespondToModel(format!(
                "ctox {tool_name} failed: {detail}"
            )));
        }

        let stdout = String::from_utf8(output.stdout).map_err(|err| {
            FunctionCallError::RespondToModel(format!("ctox output was not utf-8: {err}"))
        })?;
        Ok(FunctionToolOutput::from_text(
            stdout.trim().to_string(),
            Some(true),
        ))
    }
}
