// ref: internal/auth/devin/devin_auth_test.go:14-220
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use crate::sdk::pluginapi::{HttpResponse, HttpStreamChunk, HttpStreamResponse, PluginFuture};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
use tokio::sync::{mpsc, Notify};

struct Client {
    replies: Mutex<VecDeque<(u16, Vec<u8>, Option<PluginExecutionError>)>>,
    requests: Mutex<Vec<HttpRequest>>,
}
impl HostHttpClient for Client {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async { panic!("unbounded full-body HTTP is forbidden") })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            let (status_code, payload, error) = self.replies.lock().unwrap().pop_front().unwrap();
            let (sender, chunks) = mpsc::channel(1);
            sender.try_send(HttpStreamChunk { payload, error }).unwrap();
            drop(sender);
            Ok(HttpStreamResponse {
                status_code,
                headers: Headers::new(),
                chunks,
            })
        })
    }
}
fn client(
    replies: Vec<(u16, Vec<u8>, Option<PluginExecutionError>)>,
) -> (Arc<Client>, DevinAuthService) {
    let client = Arc::new(Client {
        replies: Mutex::new(replies.into()),
        requests: Mutex::new(Vec::new()),
    });
    let service =
        DevinAuthService::new(client.clone()).with_api_base_url(" https://selected.invalid/// ");
    (client, service)
}

struct PendingClient {
    started: Notify,
    calls: AtomicUsize,
    dropped: Arc<AtomicUsize>,
}
struct DropWitness(Arc<AtomicUsize>);
impl Drop for DropWitness {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl HostHttpClient for PendingClient {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async { panic!("must stream") })
    }
    fn execute_stream<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let _witness = DropWitness(self.dropped.clone());
            self.started.notify_one();
            std::future::pending().await
        })
    }
}
fn pending_client() -> Arc<PendingClient> {
    Arc::new(PendingClient {
        started: Notify::new(),
        calls: AtomicUsize::new(0),
        dropped: Arc::new(AtomicUsize::new(0)),
    })
}
