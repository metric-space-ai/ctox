use anyhow::Result;
use std::path::Path;

pub use ctox_web_stack::handle_browser_command;
pub use ctox_web_stack::prepare_browser_environment;
pub use ctox_web_stack::run_browser_automation;
pub use ctox_web_stack::BrowserAutomationRequest;
pub use ctox_web_stack::BrowserPrepareOptions;

/// Canonical CTOX web-stack facade.
///
/// The dedicated `ctox-web-stack` crate now owns the CLI and runtime contract
/// for search, read, browser-prepare, browser automation, and the typed scrape
/// bridge request shape. The root binary only injects the actual scrape
/// executor, because the durable scrape runtime/database still belongs to the
/// wider CTOX scrape subsystem.
pub fn handle_web_command(root: &Path, args: &[String]) -> Result<()> {
    ctox_web_stack::handle_web_command(
        root,
        args,
        &crate::capabilities::scrape::handle_scrape_command,
    )
}
