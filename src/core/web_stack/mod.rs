use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

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
/// bridge request shape. The root binary only injects the actual scrape
/// executor, because the durable scrape runtime/database still belongs to the
/// wider CTOX scrape subsystem.
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
                },
            )?;
            println!("{}", serde_json::to_string_pretty(&payload)?);
            return Ok(());
        }
    }
    ctox_web_stack::handle_web_command(
        root,
        args,
        &crate::capabilities::scrape::handle_scrape_command,
    )
}

fn web_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window.first().map(String::as_str) == Some(flag))
        .and_then(|window| window.get(1))
        .map(String::as_str)
}
