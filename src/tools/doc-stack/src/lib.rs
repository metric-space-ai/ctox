use anyhow::Result;
use std::path::Path;

mod formats;
mod parse;
mod store;
mod surface;

pub use surface::handle_doc_command;

pub trait EmbeddingExecutor: Send + Sync {
    fn default_model(&self, root: &Path) -> Result<String>;

    fn embed_texts(&self, root: &Path, model: &str, inputs: &[String]) -> Result<Vec<Vec<f64>>>;
}
