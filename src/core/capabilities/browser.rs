use anyhow::Result;
use std::path::Path;

pub use crate::web_stack::prepare_browser_environment;
pub use crate::web_stack::run_browser_automation;
pub use crate::web_stack::BrowserAutomationRequest;
pub use crate::web_stack::BrowserPrepareOptions;

pub fn handle_browser_command(root: &Path, args: &[String]) -> Result<()> {
    crate::web_stack::handle_browser_command(root, args)
}
