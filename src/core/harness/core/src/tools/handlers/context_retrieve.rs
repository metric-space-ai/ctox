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

pub struct ContextRetrieveHandler;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextRetrieveArgs {
    #[serde(default = "default_conversation_id")]
    conversation_id: i64,
    mode: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    summary_id: Option<String>,
    #[serde(default = "default_limit")]
    limit: u64,
    #[serde(default = "default_depth")]
    depth: u64,
    #[serde(default)]
    messages: bool,
    #[serde(default = "default_token_cap")]
    token_cap: i64,
}

fn default_conversation_id() -> i64 {
    1
}

fn default_limit() -> u64 {
    10
}

fn default_depth() -> u64 {
    1
}

fn default_token_cap() -> i64 {
    8_000
}

#[async_trait]
impl ToolHandler for ContextRetrieveHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn is_mutating(&self, _invocation: &ToolInvocation) -> bool {
        false
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { payload, .. } = invocation;
        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "context_retrieve handler received unsupported payload".to_string(),
                ));
            }
        };

        let args: ContextRetrieveArgs = parse_arguments(&arguments)?;
        let mut command = Command::new(resolve_ctox_binary()?);
        command
            .arg("context-retrieve")
            .arg("--conversation-id")
            .arg(args.conversation_id.to_string())
            .arg("--mode")
            .arg(args.mode)
            .arg("--limit")
            .arg(args.limit.to_string())
            .arg("--depth")
            .arg(args.depth.to_string())
            .arg("--token-cap")
            .arg(args.token_cap.to_string());

        if args.messages {
            command.arg("--messages");
        }
        push_optional_flag(&mut command, "--kind", args.kind.as_deref());
        push_optional_flag(&mut command, "--query", args.query.as_deref());
        push_optional_flag(&mut command, "--summary-id", args.summary_id.as_deref());
        push_optional_flag(
            &mut command,
            "--db",
            std::env::var("CTOX_CONTEXT_DB").ok().as_deref(),
        );

        let output = command.output().await.map_err(|err| {
            FunctionCallError::RespondToModel(format!("failed to run ctox: {err}"))
        })?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            return Err(FunctionCallError::RespondToModel(format!(
                "ctox context-retrieve failed: {detail}"
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
    if let Some(value) = value
        && !value.trim().is_empty()
    {
        command.arg(flag).arg(value);
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
