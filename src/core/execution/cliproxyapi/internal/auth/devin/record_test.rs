// ref: internal/auth/devin/record_test.go:13-100
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use crate::sdk::pluginapi::{
    Headers, HostHttpClient, HttpRequest, HttpResponse, HttpStreamChunk, HttpStreamResponse,
    PluginFuture,
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;
fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_791_000_000, 0).unwrap()
}
fn status() -> DevinUserStatus {
    DevinUserStatus {
        user_name: "status-user".into(),
        user_id: "status-id".into(),
        org_id: "status-org".into(),
        email: "test@example.com".into(),
        plan: "Pro".into(),
        daily_quota_remaining_percent: Some(0),
        weekly_quota_remaining_percent: Some(50),
        daily_quota_reset_at: Some(1_791_086_400),
        ..DevinUserStatus::default()
    }
}
struct UnavailableClient {
    requests: Mutex<Vec<HttpRequest>>,
}
impl HostHttpClient for UnavailableClient {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async { panic!("must stream") })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            let (sender, chunks) = mpsc::channel(1);
            sender
                .try_send(HttpStreamChunk {
                    payload: b"unavailable".to_vec(),
                    error: None,
                })
                .unwrap();
            drop(sender);
            Ok(HttpStreamResponse {
                status_code: 403,
                headers: Headers::new(),
                chunks,
            })
        })
    }
}
