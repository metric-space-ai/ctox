//! Text ↔ token-id tokenizer wrapper.
//!
//! Qwen3.5-27B uses the same HuggingFace-exported tokenizer that
//! ships with the model repository. The GGUF file embeds the
//! tokenizer too, but parsing GGUF metadata back into the
//! `tokenizers` crate's format is a detour we don't need — the
//! `tokenizer.json` is a small (~15 MB) file, and CTOX can resolve
//! it via the same HF snapshot layout as the weights.
//!
//! The server binary accepts a `--tokenizer <path>` flag that
//! points at the `tokenizer.json` file. Canonical location on a
//! machine that has downloaded the HF model:
//!
//! ```text
//! ~/.cache/huggingface/hub/models--Qwen--Qwen3.5-27B/snapshots/<rev>/tokenizer.json
//! ```

use anyhow::{anyhow, Context, Result};
use std::path::Path;
use tokenizers::Tokenizer as HfTokenizer;

/// Thin wrapper — narrows the `tokenizers::Tokenizer` API to the
/// two calls the server needs.
pub struct Tokenizer {
    inner: HfTokenizer,
}

impl Tokenizer {
    /// Load from a `tokenizer.json` path. Does not cache across
    /// invocations — the server calls this exactly once at startup.
    pub fn from_file(path: &Path) -> Result<Self> {
        let inner = HfTokenizer::from_file(path).map_err(|e| {
            anyhow!(
                "failed to load tokenizer.json from {}: {e}",
                path.display()
            )
        })?;
        Ok(Self { inner })
    }

    /// UTF-8 text → token IDs. Does NOT add special tokens — the
    /// caller composes the chat template explicitly (system +
    /// message boundaries) and hands us the fully-rendered string.
    pub fn encode(&self, text: &str) -> Result<Vec<i32>> {
        let encoding = self
            .inner
            .encode(text, false)
            .map_err(|e| anyhow!("tokenizer.encode failed: {e}"))?;
        Ok(encoding
            .get_ids()
            .iter()
            .map(|&id| id as i32)
            .collect::<Vec<_>>())
    }

    /// Token IDs → UTF-8 text. Skips special tokens in the output
    /// (stop tokens, padding, etc. don't belong in delta streams).
    pub fn decode(&self, ids: &[i32]) -> Result<String> {
        let u32_ids: Vec<u32> = ids.iter().map(|&id| id as u32).collect();
        self.inner
            .decode(&u32_ids, true)
            .map_err(|e| anyhow!("tokenizer.decode failed: {e}"))
    }

    /// End-of-sequence token id, if the tokenizer knows one. Used
    /// as an early-stop signal in the generation loop.
    pub fn eos_id(&self) -> Option<i32> {
        // Qwen3.5 uses `<|im_end|>` (id 151645 in Qwen3-series). Grab
        // it from the tokenizer's added-tokens map, falling back to
        // the classic `</s>`.
        self.token_id("<|im_end|>")
            .or_else(|| self.token_id("<|endoftext|>"))
            .or_else(|| self.token_id("</s>"))
    }

    fn token_id(&self, tok: &str) -> Option<i32> {
        self.inner.token_to_id(tok).map(|id| id as i32)
    }

    /// Find a cached tokenizer.json in the standard HuggingFace
    /// snapshot location. Returns the first match under
    /// `~/.cache/huggingface/hub/models--Qwen--Qwen3.5-27B*/snapshots/*/tokenizer.json`.
    pub fn resolve_default() -> Result<std::path::PathBuf> {
        let home = std::env::var_os("HOME")
            .context("HOME env var not set; cannot locate HF cache")?;
        let hub = Path::new(&home).join(".cache/huggingface/hub");
        for entry in std::fs::read_dir(&hub)
            .with_context(|| format!("read_dir {}", hub.display()))?
        {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with("models--Qwen--Qwen3.5-27B") {
                continue;
            }
            let snapshots = entry.path().join("snapshots");
            if !snapshots.is_dir() {
                continue;
            }
            for snap in std::fs::read_dir(&snapshots)? {
                let snap = snap?;
                let cand = snap.path().join("tokenizer.json");
                if cand.is_file() {
                    return Ok(cand);
                }
            }
        }
        Err(anyhow!(
            "no tokenizer.json found under {}/models--Qwen--Qwen3.5-27B*/snapshots/*/",
            hub.display()
        ))
    }
}
