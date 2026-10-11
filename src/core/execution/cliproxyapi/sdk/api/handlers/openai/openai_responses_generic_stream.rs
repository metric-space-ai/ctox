// Origin: CTOX
// License: AGPL-3.0-only
//! A bounded translated Responses stream with holder-generation revalidation.
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::sdk::pluginapi::ExecutorStreamChunk;

pub struct OpenAiResponsesGenericStream {
    chunks: mpsc::Receiver<ExecutorStreamChunk>,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
    terminal: bool,
    bytes: usize,
}
impl OpenAiResponsesGenericStream {
    pub fn new(chunks: mpsc::Receiver<ExecutorStreamChunk>, current: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self { chunks, current, terminal: false, bytes: 0 }
    }
    fn failure(&mut self, retired: bool) -> Option<Vec<u8>> {
        self.terminal = true;
        self.chunks.close();
        let (code, message) = if retired { ("provider_configuration_changed", "Provider account configuration changed") } else { ("upstream_stream_error", "Provider stream failed") };
        Some(format!("event: response.failed\ndata: {{\"type\":\"response.failed\",\"response\":{{\"status\":\"failed\",\"error\":{{\"code\":\"{code}\",\"message\":\"{message}\"}}}}}}\n\n").into_bytes())
    }
    pub async fn next_chunk(&mut self) -> Option<Vec<u8>> {
        if self.terminal { return None; }
        if !(self.current)() { return self.failure(true); }
        let chunk = self.chunks.recv().await;
        if !(self.current)() { return self.failure(true); }
        let Some(chunk) = chunk else { self.terminal = true; return None; };
        let Some(bytes) = self.bytes.checked_add(chunk.payload.len()).filter(|n| *n <= 64*1024*1024) else { return self.failure(false); };
        if chunk.error.is_some() { return self.failure(false); }
        self.bytes = bytes;
        let mut payload = chunk.payload;
        if !payload.ends_with(b"\n\n") { if !payload.ends_with(b"\n") { payload.push(b'\n'); } payload.push(b'\n'); }
        if payload.starts_with(b"event: response.completed\n") || payload.starts_with(b"event: response.incomplete\n") || payload.starts_with(b"event: response.failed\n") { self.terminal = true; self.chunks.close(); }
        Some(payload)
    }
}
impl std::fmt::Debug for OpenAiResponsesGenericStream {
    fn fmt(&self, f:&mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.debug_struct("OpenAiResponsesGenericStream").field("terminal", &self.terminal).finish_non_exhaustive() }
}
