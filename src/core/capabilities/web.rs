use anyhow::Result;
use std::path::Path;

pub fn handle_web_command(root: &Path, args: &[String]) -> Result<()> {
    crate::web_stack::handle_web_command(root, args)
}
