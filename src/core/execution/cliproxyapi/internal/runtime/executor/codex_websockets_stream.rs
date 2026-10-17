// ref: internal/runtime/executor/codex_websockets_stream.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;

use super::codex_websockets_errors::{encode_codex_websocket_as_sse, CodexWebsocketError};
use super::codex_websockets_executor::CodexWebsocketExecutionResult;

/// Downstream stream created from one committed upstream execution. The
/// bounded channel prevents an unbounded producer queue.
pub struct CodexWebsocketStream {
    receiver: mpsc::Receiver<Result<Vec<u8>, CodexWebsocketError>>,
    committed: Arc<AtomicBool>,
}

impl CodexWebsocketStream {
    pub fn from_execution(result: CodexWebsocketExecutionResult, capacity: usize) -> Self {
        let (sender, receiver) = mpsc::channel(capacity.clamp(1, 64));
        let committed = Arc::new(AtomicBool::new(result.committed));
        tokio::spawn(async move {
            for event in result.events {
                if sender
                    .send(Ok(encode_codex_websocket_as_sse(&event)))
                    .await
                    .is_err()
                {
                    return;
                }
            }
            let _ = sender.send(Ok(b"data: [DONE]\n\n".to_vec())).await;
        });
        Self {
            receiver,
            committed,
        }
    }

    pub(crate) fn live(
        receiver: mpsc::Receiver<Result<Vec<u8>, CodexWebsocketError>>,
        committed: Arc<AtomicBool>,
    ) -> Self {
        Self {
            receiver,
            committed,
        }
    }

    pub async fn next_chunk(&mut self) -> Option<Result<Vec<u8>, CodexWebsocketError>> {
        self.receiver.recv().await
    }

    pub fn committed(&self) -> bool {
        self.committed.load(Ordering::Acquire)
    }
}

impl std::fmt::Debug for CodexWebsocketStream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CodexWebsocketStream")
            .field("committed", &self.committed())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
