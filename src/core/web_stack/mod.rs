use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
mod credentials;

pub use ctox_web_stack::capture_browser_transport;
pub use ctox_web_stack::handle_browser_command;
pub use ctox_web_stack::prepare_browser_environment;
pub use ctox_web_stack::run_browser_automation;
pub use ctox_web_stack::spawn_persistent_browser;
pub use ctox_web_stack::BrowserAutomationRequest;
pub use ctox_web_stack::BrowserCaptureRequest;
pub use ctox_web_stack::BrowserPrepareOptions;
pub use ctox_web_stack::PersistentBrowserHandle;
pub use ctox_web_stack::PersistentBrowserSpawn;

/// Canonical CTOX web-stack facade.
///
/// The dedicated `ctox-web-stack` crate now owns the CLI and runtime contract
/// for search, read, browser-prepare, browser automation, and the typed scrape
/// bridge request shape. The root binary injects its scrape executor and a
/// request-borrowed, narrowly scoped encrypted credential resolver.
pub fn handle_web_command(root: &Path, args: &[String]) -> Result<()> {
    if args.first().map(String::as_str) == Some("browser-automation") {
        if let Some(session_id) = web_flag_value(args, "--session-id") {
            let script_file = web_flag_value(args, "--script-file").map(PathBuf::from);
            let source =
                ctox_web_stack::browser::read_browser_automation_source(script_file.as_deref())?;
            let payload = crate::business_os::run_browser_session_automation(
                root,
                crate::business_os::BrowserSessionAutomationRequest {
                    session_id: session_id.to_string(),
                    dir: web_flag_value(args, "--dir").map(PathBuf::from),
                    timeout_ms: web_flag_value(args, "--timeout-ms")
                        .map(|value| value.parse::<u64>())
                        .transpose()
                        .context("failed to parse --timeout-ms")?,
                    source,
                    profile_owner: None,
                },
            )?;
            println!("{}", serde_json::to_string_pretty(&payload)?);
            return Ok(());
        }
    }
    let resolver = credentials::NativeCredentialResolver::for_research(root);
    ctox_web_stack::surface::handle_web_command_with_resolver(
        root,
        args,
        &crate::capabilities::scrape::handle_scrape_command,
        Some(&resolver),
    )
}

pub fn run_ctox_person_research_tool(
    root: &Path,
    request: &ctox_web_stack::PersonResearchRequest,
) -> Result<serde_json::Value> {
    let resolver = credentials::NativeCredentialResolver::for_research(root);
    ctox_web_stack::person_research::run_ctox_person_research_tool_with_resolver(
        root,
        request,
        Some(&resolver),
    )
}

pub fn run_ctox_web_search_tool(
    root: &Path,
    request: &ctox_web_stack::CanonicalWebSearchRequest,
) -> Result<serde_json::Value> {
    let resolver = credentials::NativeCredentialResolver::for_research(root);
    ctox_web_stack::web_search::run_ctox_web_search_tool_with_resolver(
        root,
        request,
        Some(&resolver),
    )
}

pub fn execute_canonical_web_search(
    root: &Path,
    request: &ctox_web_stack::CanonicalWebSearchRequest,
) -> Result<Option<ctox_web_stack::CanonicalWebSearchExecution>> {
    let resolver = credentials::NativeCredentialResolver::for_research(root);
    ctox_web_stack::web_search::execute_canonical_web_search_with_resolver(
        root,
        request,
        Some(&resolver),
    )
}

pub fn augment_responses_request(
    root: &Path,
    payload: &mut serde_json::Value,
) -> Result<Option<ctox_web_stack::web_search::WebSearchAugmentation>> {
    let resolver = credentials::NativeCredentialResolver::for_research(root);
    ctox_web_stack::web_search::augment_responses_request_with_resolver(
        root,
        payload,
        Some(&resolver),
    )
}

fn web_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window.first().map(String::as_str) == Some(flag))
        .and_then(|window| window.get(1))
        .map(String::as_str)
}
