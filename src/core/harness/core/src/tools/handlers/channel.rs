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

pub struct ChannelHandler;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelSyncArgs {
    channel: String,
    #[serde(default)]
    db: Option<String>,
    #[serde(default)]
    limit: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelTakeArgs {
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    db: Option<String>,
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    lease_owner: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelAckArgs {
    message_keys: Vec<String>,
    #[serde(default)]
    db: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelSendArgs {
    channel: String,
    account_key: String,
    thread_key: String,
    body: String,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    to: Vec<String>,
    #[serde(default)]
    cc: Vec<String>,
    #[serde(default)]
    sender_display: Option<String>,
    #[serde(default)]
    sender_address: Option<String>,
    #[serde(default)]
    db: Option<String>,
}

#[async_trait]
impl ToolHandler for ChannelHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn is_mutating(&self, invocation: &ToolInvocation) -> bool {
        !matches!(invocation.tool_name.as_str(), "channel_take")
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            tool_name, payload, ..
        } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "channel handler received unsupported payload".to_string(),
                ));
            }
        };

        let mut command = Command::new(resolve_ctox_binary()?);
        command.arg("channel");
        match tool_name.as_str() {
            "channel_sync" => {
                let args: ChannelSyncArgs = parse_arguments(&arguments)?;
                command.arg("sync").arg("--channel").arg(args.channel);
                push_optional_flag(&mut command, "--db", args.db.as_deref());
                push_optional_number_flag(&mut command, "--limit", args.limit);
            }
            "channel_take" => {
                let args: ChannelTakeArgs = parse_arguments(&arguments)?;
                command.arg("take");
                push_optional_flag(&mut command, "--channel", args.channel.as_deref());
                push_optional_flag(&mut command, "--db", args.db.as_deref());
                push_optional_flag(
                    &mut command,
                    "--lease-owner",
                    args.lease_owner.as_deref().or(Some("codex")),
                );
                push_optional_number_flag(&mut command, "--limit", args.limit);
            }
            "channel_ack" => {
                let args: ChannelAckArgs = parse_arguments(&arguments)?;
                command.arg("ack");
                push_optional_flag(&mut command, "--db", args.db.as_deref());
                push_optional_flag(
                    &mut command,
                    "--status",
                    args.status.as_deref().or(Some("handled")),
                );
                if args.message_keys.is_empty() {
                    return Err(FunctionCallError::RespondToModel(
                        "channel_ack requires at least one message key".to_string(),
                    ));
                }
                for message_key in args.message_keys {
                    command.arg(message_key);
                }
            }
            "channel_send" => {
                let args: ChannelSendArgs = parse_arguments(&arguments)?;
                command
                    .arg("send")
                    .arg("--channel")
                    .arg(args.channel)
                    .arg("--account-key")
                    .arg(args.account_key)
                    .arg("--thread-key")
                    .arg(args.thread_key)
                    .arg("--body")
                    .arg(args.body);
                push_optional_flag(
                    &mut command,
                    "--subject",
                    args.subject.as_deref().or(Some("(no subject)")),
                );
                push_optional_flag(&mut command, "--db", args.db.as_deref());
                push_optional_flag(
                    &mut command,
                    "--sender-display",
                    args.sender_display.as_deref(),
                );
                push_optional_flag(
                    &mut command,
                    "--sender-address",
                    args.sender_address.as_deref(),
                );
                for recipient in args.to {
                    command.arg("--to").arg(recipient);
                }
                for cc in args.cc {
                    command.arg("--cc").arg(cc);
                }
            }
            other => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "unsupported channel tool: {other}"
                )));
            }
        }

        let output = command.output().await.map_err(|err| {
            FunctionCallError::RespondToModel(format!("failed to run ctox: {err}"))
        })?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            return Err(FunctionCallError::RespondToModel(format!(
                "ctox {} failed: {}",
                tool_name, detail
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

fn push_optional_flag(command: &mut Command, flag: &str, value: Option<&str>) {
    if let Some(value) = value {
        command.arg(flag).arg(value);
    }
}

fn push_optional_number_flag(command: &mut Command, flag: &str, value: Option<u64>) {
    if let Some(value) = value {
        command.arg(flag).arg(value.to_string());
    }
}

fn resolve_ctox_binary() -> Result<String, FunctionCallError> {
    if let Ok(path) = std::env::var("CTOX_CHANNEL_BIN")
        && !path.trim().is_empty()
    {
        return Ok(path);
    }

    if let Ok(root) = std::env::var("CTOX_ROOT") {
        let candidate = std::path::Path::new(&root).join("target/release/ctox");
        if candidate.exists() {
            return Ok(candidate.display().to_string());
        }
    }

    which::which("ctox")
        .map(|path| path.display().to_string())
        .map_err(|_| {
            FunctionCallError::RespondToModel(
                "ctox binary not found. Set CTOX_CHANNEL_BIN or install ctox.".to_string(),
            )
        })
}
