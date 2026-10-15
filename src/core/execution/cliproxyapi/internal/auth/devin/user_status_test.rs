// ref: internal/auth/devin/user_status_test.go:17-201
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;
use crate::sdk::pluginapi::{HttpResponse, HttpStreamChunk, HttpStreamResponse, PluginFuture};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
use tokio::sync::mpsc;

fn mock_status() -> Vec<u8> {
    let mut org = Vec::new();
    append_data(&mut org, 4, b"org-test-123");
    append_data(&mut org, 8, b"TestOrg");
    let mut info = Vec::new();
    append_data(&mut info, 2, b"Pro");
    append_data(&mut info, 33, &org);
    let mut plan = Vec::new();
    append_data(&mut plan, 1, &info);
    for (number, seconds) in [(2, 1789087364_u64), (3, 1791679364)] {
        let mut time = Vec::new();
        append_varint(&mut time, 8);
        append_varint(&mut time, seconds);
        append_data(&mut plan, number, &time);
    }
    for (number, value) in [(14, 100), (15, 50), (17, 1789200000), (18, 1789286400)] {
        append_varint(&mut plan, number << 3);
        append_varint(&mut plan, value);
    }
    let mut user = Vec::new();
    for (number, value) in [
        (3, "testuser"),
        (5, "team-123"),
        (7, "testuser@example.com"),
        (36, "user-id-456"),
    ] {
        append_data(&mut user, number, value.as_bytes());
    }
    append_data(&mut user, 13, &plan);
    let mut body = Vec::new();
    append_data(&mut body, 1, &user);
    body
}
fn unix_now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_791_000_000, 0).unwrap()
}

struct Transport {
    status: u16,
    chunks: Mutex<Option<mpsc::Receiver<HttpStreamChunk>>>,
    requests: Mutex<Vec<HttpRequest>>,
    full_calls: AtomicUsize,
}
impl HostHttpClient for Transport {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            self.full_calls.fetch_add(1, Ordering::SeqCst);
            panic!("unbounded execute must not be used")
        })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            Ok(HttpStreamResponse {
                status_code: self.status,
                headers: Headers::new(),
                chunks: self.chunks.lock().unwrap().take().unwrap(),
            })
        })
    }
}
fn transport(
    status: u16,
) -> (
    Arc<Transport>,
    mpsc::Sender<HttpStreamChunk>,
    DevinStatusService,
) {
    let (sender, receiver) = mpsc::channel(4);
    let transport = Arc::new(Transport {
        status,
        chunks: Mutex::new(Some(receiver)),
        requests: Mutex::new(Vec::new()),
        full_calls: AtomicUsize::new(0),
    });
    let service = DevinStatusService::new(transport.clone())
        .with_server_base_url(" https://test.invalid/// ");
    (transport, sender, service)
}
async fn send(sender: &mpsc::Sender<HttpStreamChunk>, bytes: Vec<u8>) {
    sender
        .send(HttpStreamChunk {
            payload: bytes,
            error: None,
        })
        .await
        .unwrap();
}
