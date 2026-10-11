// Origin: CTOX
// License: AGPL-3.0-only
//! A bounded translated Responses stream with holder-generation revalidation.
use crate::sdk::pluginapi::ExecutorStreamChunk;
use std::sync::Arc;
use tokio::sync::mpsc;

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_EMPTY_CHUNKS: usize = 128;

pub struct OpenAiResponsesGenericStream {
    chunks: mpsc::Receiver<ExecutorStreamChunk>,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
    terminal: bool,
    bytes: usize,
}
impl OpenAiResponsesGenericStream {
    pub fn new(
        chunks: mpsc::Receiver<ExecutorStreamChunk>,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            chunks,
            current,
            terminal: false,
            bytes: 0,
        }
    }
    fn failure(&mut self, retired: bool) -> Option<Vec<u8>> {
        self.terminal = true;
        self.chunks.close();
        let (code, message) = if retired {
            (
                "provider_configuration_changed",
                "Provider account configuration changed",
            )
        } else {
            ("upstream_stream_error", "Provider stream failed")
        };
        let body = serde_json::json!({"type":"response.failed","response":{"status":"failed","error":{"code":code,"message":message}}});
        Some(format!("event: response.failed\ndata: {body}\n\n").into_bytes())
    }
    pub async fn next_chunk(&mut self) -> Option<Vec<u8>> {
        if self.terminal {
            return None;
        }
        for _ in 0..MAX_EMPTY_CHUNKS {
            if !(self.current)() {
                return self.failure(true);
            }
            let chunk = self.chunks.recv().await;
            if !(self.current)() {
                return self.failure(true);
            }
            // EOF is not a successful completion. The translator must deliver
            // the terminal Responses event before the upstream closes.
            let Some(chunk) = chunk else {
                return self.failure(false);
            };
            if chunk.error.is_some() {
                return self.failure(false);
            }
            let Some(bytes) = self
                .bytes
                .checked_add(chunk.payload.len())
                .filter(|n| *n <= MAX_BYTES)
            else {
                return self.failure(false);
            };
            self.bytes = bytes;
            if chunk.payload.is_empty() {
                continue;
            }
            let mut payload = chunk.payload;
            if !payload.ends_with(b"\n\n") && !payload.ends_with(b"\r\n\r\n") {
                if !payload.ends_with(b"\n") {
                    payload.push(b'\n');
                }
                payload.push(b'\n');
            }
            if terminal_event(&payload) {
                self.terminal = true;
                self.chunks.close();
            }
            return Some(payload);
        }
        self.failure(false)
    }
}
fn terminal_event(payload: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(payload) else {
        return false;
    };
    text.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .any(|data| {
            serde_json::from_str::<serde_json::Value>(data.trim())
                .ok()
                .and_then(|value| {
                    value
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
                .is_some_and(|kind| {
                    matches!(
                        kind.as_str(),
                        "response.completed" | "response.incomplete" | "response.failed"
                    )
                })
        })
}
impl std::fmt::Debug for OpenAiResponsesGenericStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiResponsesGenericStream")
            .field("terminal", &self.terminal)
            .finish_non_exhaustive()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    fn setup() -> (
        mpsc::Sender<ExecutorStreamChunk>,
        Arc<AtomicBool>,
        OpenAiResponsesGenericStream,
    ) {
        let (sender, chunks) = mpsc::channel(4);
        let current = Arc::new(AtomicBool::new(true));
        let check = current.clone();
        (
            sender,
            current,
            OpenAiResponsesGenericStream::new(
                chunks,
                Arc::new(move || check.load(Ordering::SeqCst)),
            ),
        )
    }
    fn chunk(payload: &[u8]) -> ExecutorStreamChunk {
        ExecutorStreamChunk {
            payload: payload.to_vec(),
            error: None,
        }
    }
    fn code(payload: &[u8]) -> String {
        let text = std::str::from_utf8(payload).unwrap();
        let data = text
            .lines()
            .find_map(|line| line.strip_prefix("data:"))
            .unwrap();
        serde_json::from_str::<serde_json::Value>(data).unwrap()["response"]["error"]["code"]
            .as_str()
            .unwrap()
            .to_owned()
    }
    #[tokio::test]
    async fn retirement_before_poll_closes_without_publishing_pending_output() {
        let (sender, current, mut stream) = setup();
        sender
            .send(chunk(b"data: secret-pending-output"))
            .await
            .unwrap();
        current.store(false, Ordering::SeqCst);
        assert_eq!(
            code(&stream.next_chunk().await.unwrap()),
            "provider_configuration_changed"
        );
        assert!(stream.next_chunk().await.is_none());
        assert!(sender.is_closed());
    }
    #[tokio::test]
    async fn retirement_during_receive_rejects_late_output() {
        let (sender, current, mut stream) = setup();
        let received = tokio::spawn(async move { stream.next_chunk().await.unwrap() });
        tokio::task::yield_now().await;
        current.store(false, Ordering::SeqCst);
        sender
            .send(chunk(b"data: late-private-output"))
            .await
            .unwrap();
        let payload = received.await.unwrap();
        assert_eq!(code(&payload), "provider_configuration_changed");
        assert!(!String::from_utf8_lossy(&payload).contains("late-private-output"));
    }
    #[tokio::test]
    async fn data_only_terminal_is_framed_and_stops_later_output() {
        let (sender, _, mut stream) = setup();
        sender
            .send(chunk(
                br#"data: {"type":"response.completed","response":{"status":"completed"}}"#,
            ))
            .await
            .unwrap();
        let frame = stream.next_chunk().await.unwrap();
        assert!(frame.ends_with(b"\n\n"));
        assert!(stream.next_chunk().await.is_none());
        assert!(sender.is_closed());
    }
    #[tokio::test]
    async fn missing_terminal_and_upstream_errors_are_fixed_public_failures() {
        let (sender, _, mut stream) = setup();
        drop(sender);
        assert_eq!(
            code(&stream.next_chunk().await.unwrap()),
            "upstream_stream_error"
        );
        assert!(stream.next_chunk().await.is_none());
        let (sender, _, mut stream) = setup();
        sender
            .send(ExecutorStreamChunk {
                payload: vec![],
                error: Some(anyhow::anyhow!("private upstream credential")),
            })
            .await
            .unwrap();
        let frame = stream.next_chunk().await.unwrap();
        assert_eq!(code(&frame), "upstream_stream_error");
        assert!(!String::from_utf8_lossy(&frame).contains("private upstream credential"));
    }
    #[tokio::test]
    async fn byte_bound_and_drop_cancel_the_producer() {
        let (sender, _, mut stream) = setup();
        stream.bytes = MAX_BYTES;
        sender.send(chunk(b"data: bounded")).await.unwrap();
        assert_eq!(
            code(&stream.next_chunk().await.unwrap()),
            "upstream_stream_error"
        );
        assert!(sender.is_closed());
        let (sender, _, stream) = setup();
        drop(stream);
        assert!(sender.is_closed());
    }
}
