//! Streaming state shell.
//!
//! This module is the place to port `vox_stream_t`: incremental mel buffering,
//! encoder chunking, decoder restarts, alt tokens and output queue.

use crate::consts::{DEFAULT_DELAY_TOKENS, LEFT_PAD_TOKENS, TOKEN_BOS, TOKEN_STREAMING_PAD};

#[derive(Debug, Clone)]
pub struct StreamConfig {
    pub delay_tokens: usize,
    pub processing_interval_seconds: f32,
    pub continuous: bool,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            delay_tokens: DEFAULT_DELAY_TOKENS,
            processing_interval_seconds: 2.0,
            continuous: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct OutputQueue {
    tokens: Vec<String>,
    read_pos: usize,
}

impl OutputQueue {
    pub fn push(&mut self, token: String) {
        self.tokens.push(token);
    }
    pub fn drain_into(&mut self, out: &mut Vec<String>, max: usize) -> usize {
        let mut n = 0usize;
        while n < max && self.read_pos < self.tokens.len() {
            out.push(self.tokens[self.read_pos].clone());
            self.read_pos += 1;
            n += 1;
        }
        if self.read_pos == self.tokens.len() {
            self.tokens.clear();
            self.read_pos = 0;
        }
        n
    }
}

pub fn default_prompt_ids(delay_tokens: usize) -> Vec<u32> {
    let mut ids = Vec::with_capacity(1 + LEFT_PAD_TOKENS + delay_tokens);
    ids.push(TOKEN_BOS);
    ids.extend(std::iter::repeat(TOKEN_STREAMING_PAD).take(LEFT_PAD_TOKENS + delay_tokens));
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prompt_len() {
        assert_eq!(default_prompt_ids(6).len(), 39);
    }
}
