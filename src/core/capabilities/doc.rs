use anyhow::Result;
use std::path::Path;

pub fn handle_doc_command(root: &Path, args: &[String]) -> Result<()> {
    crate::doc_stack::handle_doc_command(root, args)
}
