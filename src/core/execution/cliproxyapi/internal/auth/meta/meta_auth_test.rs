// ref: internal/auth/meta/meta_auth_test.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Origin: CTOX selected-transport, lifecycle, clock and snapshot regressions
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use crate::sdk::pluginapi::{
    Headers, HostHttpClient, HttpRequest, HttpResponse, HttpStreamChunk, HttpStreamResponse,
    PluginFuture,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::mpsc,
    time::{self, Instant},
};

struct FixedClock;
impl MetaClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }
}
struct Reply {
    status: u16,
    body: Vec<u8>,
    hold: bool,
    chunk_error: Option<String>,
}
impl Reply {
    fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            body: serde_json::to_vec(&body).unwrap(),
            hold: false,
            chunk_error: None,
        }
    }
    fn hold() -> Self {
        Self {
            status: 200,
            body: Vec::new(),
            hold: true,
            chunk_error: None,
        }
    }
}
#[derive(Default)]
struct Transport {
    replies: Mutex<VecDeque<Result<Reply, String>>>,
    requests: Mutex<Vec<(HttpRequest, Instant)>>,
    held: Mutex<Vec<mpsc::Sender<HttpStreamChunk>>>,
}
impl Transport {
    fn new(replies: Vec<Reply>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into_iter().map(Ok).collect()),
            ..Self::default()
        })
    }
    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}
impl HostHttpClient for Transport {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async { panic!("Meta credential HTTP must retain the bounded stream receiver") })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests
                .lock()
                .unwrap()
                .push((request, Instant::now()));
            let reply = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("one selected test response");
            let reply = match reply {
                Ok(reply) => reply,
                Err(message) => return Err(Arc::new(std::io::Error::other(message)) as _),
            };
            let (sender, chunks) = mpsc::channel(1);
            if !reply.body.is_empty() || reply.chunk_error.is_some() {
                sender
                    .try_send(HttpStreamChunk {
                        payload: reply.body,
                        error: reply
                            .chunk_error
                            .map(|message| Arc::new(std::io::Error::other(message)) as _),
                    })
                    .unwrap();
            }
            if reply.hold {
                self.held.lock().unwrap().push(sender);
            }
            Ok(HttpStreamResponse {
                status_code: reply.status,
                headers: Headers::new(),
                chunks,
            })
        })
    }
}
fn service(transport: Arc<Transport>) -> MetaAuth {
    MetaAuth::new(transport)
        .with_clock(Arc::new(FixedClock))
        .with_mint_endpoint("https://fixture.invalid/key")
}
fn device() -> DeviceCodeResponse {
    DeviceCodeResponse {
        device_code: "test-only-device-code".into(),
        user_code: "TEST-1234".into(),
        token_endpoint: "https://fixture.invalid/token".into(),
        interval: 1,
        expires_in: 30,
        ..DeviceCodeResponse::default()
    }
}
fn token_reply() -> Reply {
    Reply::json(
        200,
        json!({"access_token":"dca:test-only-device-token","token_type":"Bearer","expires_in":3600}),
    )
}
fn mint_reply() -> Reply {
    Reply::json(
        200,
        json!({"api_key":"LLM|test-only-minted-key","base_url":" https://regional.fixture.invalid/v1 ",
        "user_email":"fixture@example.invalid","user_full_name":"Test Fixture",
        "subs_tier_name":"Muse Pro","subs_tier_id":"tier-fixture","is_subs_active":true,"has_payment_method":true}),
    )
}
fn bundle(minted: bool) -> MetaAuthBundle {
    MetaAuthBundle {
        token_data: Some(TokenData {
            access_token: "dca:test-only-device-token".into(),
            token_type: "Bearer".into(),
            expires_in: 3600,
            expires_at: 1_700_003_600,
            ..TokenData::default()
        }),
        minted_key: minted.then(|| MintedKeyResponse {
            api_key: "LLM|test-only-minted-key".into(),
            base_url: " https://regional.fixture.invalid/v1 ".into(),
            user_email: "minted@example.invalid".into(),
            user_full_name: "Minted Fixture".into(),
            ..MintedKeyResponse::default()
        }),
        email: "fallback@example.invalid".into(),
        name: "Fallback Fixture".into(),
    }
}
