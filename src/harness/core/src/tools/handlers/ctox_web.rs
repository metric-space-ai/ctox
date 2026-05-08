use async_trait::async_trait;
use serde::Deserialize;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;

pub struct CtoxWebHandler;
pub struct CtoxDocHandler;
pub struct CtoxBrowserAutomationHandler;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CtoxWebSearchArgs {
    query: String,
    #[serde(default)]
    domains: Vec<String>,
    #[serde(default)]
    search_context_size: Option<String>,
    #[serde(default)]
    cached: bool,
    #[serde(default)]
    include_sources: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CtoxWebReadArgs {
    url: String,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    find: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CtoxScholarlySearchArgs {
    query: String,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    content_types: Vec<String>,
    #[serde(default)]
    languages: Vec<String>,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    sort: Option<String>,
    #[serde(default)]
    max_results: Option<u64>,
    #[serde(default)]
    page: Option<u64>,
    #[serde(default)]
    with_oa_pdf: bool,
    #[serde(default)]
    only_doi: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CtoxDeepResearchArgs {
    query: String,
    #[serde(default)]
    focus: Option<String>,
    #[serde(default)]
    depth: Option<String>,
    #[serde(default)]
    max_sources: Option<u64>,
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    include_annas_archive: bool,
    #[serde(default)]
    no_papers: bool,
    #[serde(default)]
    no_workspace: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CtoxWebScrapeArgs {
    target_key: String,
    mode: String,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CtoxDocSearchArgs {
    query: String,
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CtoxDocReadArgs {
    path: String,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    find: Vec<String>,
}

#[async_trait]
impl ToolHandler for CtoxWebHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn is_mutating(&self, invocation: &ToolInvocation) -> bool {
        let _ = invocation;
        false
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            tool_name, payload, ..
        } = invocation;
        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "ctox web handler received unsupported payload".to_string(),
                ));
            }
        };

        let mut command = Command::new(resolve_ctox_binary()?);
        match tool_name.as_str() {
            "ctox_web_search" => {
                let args: CtoxWebSearchArgs = parse_arguments(&arguments)?;
                command
                    .arg("web")
                    .arg("search")
                    .arg("--query")
                    .arg(args.query);
                for domain in args.domains {
                    command.arg("--domain").arg(domain);
                }
                if let Some(search_context_size) = args.search_context_size {
                    command.arg("--context-size").arg(search_context_size);
                }
                if args.cached {
                    command.arg("--cached");
                }
                if args.include_sources {
                    command.arg("--include-sources");
                }
            }
            "ctox_web_read" => {
                let args: CtoxWebReadArgs = parse_arguments(&arguments)?;
                command.arg("web").arg("read").arg("--url").arg(args.url);
                if let Some(query) = args.query {
                    command.arg("--query").arg(query);
                }
                for pattern in args.find {
                    command.arg("--find").arg(pattern);
                }
            }
            "ctox_scholarly_search" => {
                let args: CtoxScholarlySearchArgs = parse_arguments(&arguments)?;
                command
                    .arg("web")
                    .arg("scholarly")
                    .arg("search")
                    .arg("--query")
                    .arg(args.query);
                if let Some(provider) = args.provider {
                    command.arg("--provider").arg(provider);
                }
                for content_type in args.content_types {
                    command.arg("--content-type").arg(content_type);
                }
                for language in args.languages {
                    command.arg("--language").arg(language);
                }
                for ext in args.extensions {
                    command.arg("--ext").arg(ext);
                }
                if let Some(sort) = args.sort {
                    command.arg("--sort").arg(sort);
                }
                if let Some(max_results) = args.max_results {
                    command.arg("--max-results").arg(max_results.to_string());
                }
                if let Some(page) = args.page {
                    command.arg("--page").arg(page.to_string());
                }
                if args.with_oa_pdf {
                    command.arg("--with-oa-pdf");
                }
                if args.only_doi {
                    command.arg("--only-doi");
                }
            }
            "ctox_deep_research" => {
                let args: CtoxDeepResearchArgs = parse_arguments(&arguments)?;
                command
                    .arg("web")
                    .arg("deep-research")
                    .arg("--query")
                    .arg(args.query);
                if let Some(focus) = args.focus {
                    command.arg("--focus").arg(focus);
                }
                if let Some(depth) = args.depth {
                    command.arg("--depth").arg(depth);
                }
                if let Some(max_sources) = args.max_sources {
                    command.arg("--max-sources").arg(max_sources.to_string());
                }
                if let Some(workspace) = args.workspace {
                    command.arg("--workspace").arg(workspace);
                }
                if args.include_annas_archive {
                    command.arg("--include-annas-archive");
                }
                if args.no_papers {
                    command.arg("--no-papers");
                }
                if args.no_workspace {
                    command.arg("--no-workspace");
                }
            }
            "ctox_web_scrape" => {
                let args: CtoxWebScrapeArgs = parse_arguments(&arguments)?;
                command
                    .arg("web")
                    .arg("scrape")
                    .arg("--target-key")
                    .arg(args.target_key)
                    .arg("--mode")
                    .arg(args.mode);
                if let Some(query) = args.query {
                    command.arg("--query").arg(query);
                }
                if let Some(limit) = args.limit {
                    command.arg("--limit").arg(limit.to_string());
                }
            }
            other => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "unsupported CTOX web tool: {other}"
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

#[async_trait]
impl ToolHandler for CtoxDocHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn is_mutating(&self, invocation: &ToolInvocation) -> bool {
        let _ = invocation;
        false
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            tool_name, payload, ..
        } = invocation;
        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "ctox doc handler received unsupported payload".to_string(),
                ));
            }
        };

        let mut command = Command::new(resolve_ctox_binary()?);
        match tool_name.as_str() {
            "ctox_doc_search" => {
                let args: CtoxDocSearchArgs = parse_arguments(&arguments)?;
                command
                    .arg("doc")
                    .arg("search")
                    .arg("--query")
                    .arg(args.query);
                if let Some(limit) = args.limit {
                    command.arg("--limit").arg(limit.to_string());
                }
                if let Some(mode) = args.mode {
                    command.arg("--mode").arg(mode);
                }
            }
            "ctox_doc_read" => {
                let args: CtoxDocReadArgs = parse_arguments(&arguments)?;
                command.arg("doc").arg("read").arg("--path").arg(args.path);
                if let Some(query) = args.query {
                    command.arg("--query").arg(query);
                }
                for pattern in args.find {
                    command.arg("--find").arg(pattern);
                }
            }
            other => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "unsupported CTOX doc tool: {other}"
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

#[async_trait]
impl ToolHandler for CtoxBrowserAutomationHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Custom { .. })
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { payload, .. } = invocation;

        let ToolPayload::Custom { input } = payload else {
            return Err(FunctionCallError::RespondToModel(
                "ctox_browser_automation expects raw JavaScript source".to_string(),
            ));
        };

        let mut command = Command::new(resolve_ctox_binary()?);
        command
            .arg("web")
            .arg("browser-automation")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|err| {
            FunctionCallError::RespondToModel(format!(
                "failed to launch ctox browser automation runtime: {err}"
            ))
        })?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input.as_bytes()).await.map_err(|err| {
                FunctionCallError::RespondToModel(format!(
                    "failed to send browser automation source to ctox: {err}"
                ))
            })?;
        }
        let output = child.wait_with_output().await.map_err(|err| {
            FunctionCallError::RespondToModel(format!(
                "failed to wait for ctox browser automation runtime: {err}"
            ))
        })?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            return Err(FunctionCallError::RespondToModel(format!(
                "ctox browser automation failed: {detail}"
            )));
        }

        let stdout = String::from_utf8(output.stdout).map_err(|err| {
            FunctionCallError::RespondToModel(format!(
                "ctox browser automation output was not utf-8: {err}"
            ))
        })?;
        Ok(FunctionToolOutput::from_text(
            stdout.trim().to_string(),
            Some(true),
        ))
    }
}

fn resolve_ctox_binary() -> Result<String, FunctionCallError> {
    for env_key in ["CTOX_DOC_BIN", "CTOX_WEB_BIN", "CTOX_CHANNEL_BIN"] {
        if let Ok(path) = std::env::var(env_key)
            && !path.trim().is_empty()
        {
            return Ok(path);
        }
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
                "ctox binary not found. Set CTOX_WEB_BIN or install ctox.".to_string(),
            )
        })
}
