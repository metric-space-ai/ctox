// ref: internal/managementasset/updater_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;
use crate::internal::httpfetch::{
    BodyChunkFuture, FetchFuture, FetchResponse, Headers, HttpDoer, ResponseBody,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tempfile::TempDir;

struct Body(VecDeque<Result<Vec<u8>, String>>);

impl ResponseBody for Body {
    fn next_chunk(&mut self) -> BodyChunkFuture<'_> {
        Box::pin(async move { self.0.pop_front().transpose() })
    }
}

#[derive(Default)]
struct MockClient {
    responses: StdMutex<MockResponses>,
    requests: StdMutex<Vec<(String, Headers)>>,
}

type MockResponses = BTreeMap<String, VecDeque<(u16, Vec<u8>)>>;

impl MockClient {
    fn respond(&self, url: &str, status: u16, body: impl Into<Vec<u8>>) {
        self.responses
            .lock()
            .expect("responses")
            .entry(url.to_owned())
            .or_default()
            .push_back((status, body.into()));
    }
}

impl HttpDoer for MockClient {
    fn get<'a>(&'a self, request_url: &'a str, headers: &'a Headers) -> FetchFuture<'a> {
        Box::pin(async move {
            self.requests
                .lock()
                .map_err(|_| "request lock poisoned".to_owned())?
                .push((request_url.to_owned(), headers.clone()));
            let (status, body) = self
                .responses
                .lock()
                .map_err(|_| "response lock poisoned".to_owned())?
                .get_mut(request_url)
                .and_then(VecDeque::pop_front)
                .ok_or_else(|| format!("unexpected request: {request_url}"))?;
            Ok(FetchResponse {
                status,
                body: Box::new(Body(VecDeque::from([Ok(body)]))),
            })
        })
    }
}

fn release_json(download_url: &str, digest: &str) -> Vec<u8> {
    serde_json::json!({
        "assets": [{
            "name": "management.html",
            "browser_download_url": download_url,
            "digest": digest
        }]
    })
    .to_string()
    .into_bytes()
}

fn test_sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
