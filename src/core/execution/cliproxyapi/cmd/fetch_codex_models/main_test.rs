// ref: cmd/fetch_codex_models/main_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use chrono::TimeZone;
use ctox_cliproxyapi::sdk::cliproxy::auth::{AuthStatus, AuthStoreError};

use super::*;

#[test]
fn count_models_matches_upstream_loose_catalog_semantics() {
    assert_eq!(
        count_models(br#"{"models":[{"slug":"a"},{"slug":"b"}]}"#).unwrap(),
        2
    );
    assert_eq!(
        count_models(br#"{"models":[{"slug":"gpt-5.6-sol"}]}"#).unwrap(),
        1
    );
    assert_eq!(count_models(br#"{"models":[]}"#).unwrap(), 0);
    assert!(count_models(br#"{"models":"#).is_err());
    assert!(count_models(br#"{}"#).is_err());
}

#[derive(Default)]
struct MemoryStore {
    records: Mutex<Vec<Auth>>,
    saved: Mutex<Vec<String>>,
}

impl AuthStore for MemoryStore {
    fn list(&self) -> Result<Vec<Auth>, AuthStoreError> {
        Ok(self.records.lock().unwrap().clone())
    }

    fn save(&self, auth: &Auth) -> Result<String, AuthStoreError> {
        self.saved.lock().unwrap().push(auth.id.clone());
        Ok(auth.id.clone())
    }

    fn delete(&self, _id: &str) -> Result<(), AuthStoreError> {
        Ok(())
    }
}

struct Secrets;

impl SecretResolver for Secrets {
    fn resolve(&self, _auth: &Auth, name: &'static str) -> Result<Option<String>, CommandError> {
        Ok(match name {
            "access_token" => Some("access".to_owned()),
            "refresh_token" => Some("refresh".to_owned()),
            _ => None,
        })
    }
    fn store(&self, _auth: &Auth, _name: &'static str, _value: &str) -> Result<(), CommandError> {
        Ok(())
    }
}

struct NeverRefresh;

impl TokenRefresher for NeverRefresh {
    fn refresh(
        &self,
        _auth: &Auth,
        _refresh_token: &str,
        _cancelled: &dyn Cancellation,
    ) -> Result<RefreshedTokens, CommandError> {
        panic!("fresh token must not be refreshed")
    }
}

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap()
    }
}

#[derive(Default)]
struct Cancel(AtomicBool);

impl Cancellation for Cancel {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

struct Http;

impl HttpTransport for Http {
    fn execute(&self, request: HttpRequest) -> Result<HttpResponse, CommandError> {
        assert_eq!(request.headers["Authorization"], "Bearer access");
        Ok(HttpResponse {
            status: 200,
            body: br#"{"models":[{"slug":"a"}]}"#.to_vec(),
        })
    }
}

#[derive(Default)]
struct Files(Mutex<Vec<u8>>);

impl FileOutput for Files {
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), CommandError> {
        assert_eq!(path, Path::new("models.json"));
        *self.0.lock().unwrap() = bytes.to_vec();
        Ok(())
    }
}

#[derive(Default)]
struct Output(Mutex<Vec<String>>);

impl CommandOutput for Output {
    fn info(&self, message: &str) {
        self.0.lock().unwrap().push(message.to_owned());
    }
}

#[test]
fn command_core_uses_only_injected_authorities() {
    let mut auth = Auth::default();
    auth.id = "codex-1".to_owned();
    auth.provider = "codex".to_owned();
    auth.status = AuthStatus::Active;
    auth.metadata.insert(
        "expired".to_owned(),
        Value::String("2026-08-05T12:00:00Z".to_owned()),
    );
    let store = MemoryStore {
        records: Mutex::new(vec![auth]),
        ..MemoryStore::default()
    };
    let files = Files::default();
    let output = Output::default();
    let deps = Dependencies {
        auth_store: &store,
        secrets: &Secrets,
        refresher: &NeverRefresh,
        http: &Http,
        clock: &FixedClock,
        cancellation: &Cancel::default(),
        files: &files,
        output: &output,
        request_timeout: Duration::from_secs(30),
    };
    let options = Options {
        output: "models.json".to_owned(),
        ..Options::default()
    };
    assert_eq!(run(&options, &deps).unwrap(), 1);
    assert!(String::from_utf8(files.0.lock().unwrap().clone())
        .unwrap()
        .ends_with('\n'));
}

#[test]
fn cancellation_stops_before_store_or_network() {
    let cancel = Cancel(AtomicBool::new(true));
    let deps = Dependencies {
        auth_store: &MemoryStore::default(),
        secrets: &Secrets,
        refresher: &NeverRefresh,
        http: &Http,
        clock: &FixedClock,
        cancellation: &cancel,
        files: &Files::default(),
        output: &Output::default(),
        request_timeout: Duration::from_secs(30),
    };
    assert_eq!(
        run(&Options::default(), &deps),
        Err(CommandError::Cancelled)
    );
}
