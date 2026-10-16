// ref: internal/registry/devin_models_test.go @ d7914afd
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

struct MemorySource {
    replies: HashMap<String, Result<Vec<u8>, String>>,
    calls: Mutex<Vec<(String, usize)>>,
}
impl ModelsSource for MemorySource {
    fn fetch<'a>(&'a self, source: &'a str, limit: usize) -> ModelsFetchFuture<'a> {
        self.calls.lock().unwrap().push((source.into(), limit));
        let result = self
            .replies
            .get(source)
            .cloned()
            .unwrap_or_else(|| Err("offline".into()));
        Box::pin(async move { result })
    }
}

struct BodyDrop(Arc<AtomicUsize>);
impl Drop for BodyDrop {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
struct PendingBodySource {
    started: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    dropped: Arc<AtomicUsize>,
}
impl ModelsSource for PendingBodySource {
    fn fetch<'a>(&'a self, source: &'a str, _: usize) -> ModelsFetchFuture<'a> {
        Box::pin(async move {
            if source != "body" {
                return Err("main unavailable".into());
            }
            let _body = BodyDrop(Arc::clone(&self.dropped));
            if let Some(started) = self.started.lock().unwrap().take() {
                let _ = started.send(());
            }
            std::future::pending::<Result<Vec<u8>, String>>().await
        })
    }
}
