// ref: internal/runtime/executor/helps/home_refresh_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::home_refresh::*;
use crate::sdk::cliproxy::auth::Auth;

struct FakeClient {
    heartbeat: bool,
    response: Mutex<Result<Vec<u8>, HomeRefreshClientError>>,
    calls: Mutex<Vec<(String, String)>>,
}

impl FakeClient {
    fn responding(value: Value) -> Self {
        Self {
            heartbeat: true,
            response: Mutex::new(Ok(serde_json::to_vec(&value).unwrap())),
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl HomeRefreshClient for FakeClient {
    fn heartbeat_ok(&self) -> bool {
        self.heartbeat
    }

    fn get_refresh_auth<'a>(
        &'a self,
        auth_index: &'a str,
        access_token_sha256: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, HomeRefreshClientError>> + Send + 'a>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push((auth_index.to_owned(), access_token_sha256.to_owned()));
            self.response.lock().unwrap().clone()
        })
    }
}

fn auth_with_index(index: &str) -> Auth {
    let mut auth = Auth::default();
    auth.id = "home-auth-1".to_owned();
    auth.index = index.to_owned();
    auth.provider = "antigravity".to_owned();
    auth.metadata = BTreeMap::from([
        (
            "refresh_token".to_owned(),
            Value::String("refresh-token".to_owned()),
        ),
        (
            "access_token".to_owned(),
            Value::String("old-access-token".to_owned()),
        ),
    ]);
    auth
}
