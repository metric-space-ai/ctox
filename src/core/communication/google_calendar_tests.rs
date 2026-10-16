use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

struct OAuthServer {
    endpoints: Endpoints,
    requests: Arc<Mutex<Vec<(String, String)>>>,
    scopes: Arc<Mutex<String>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl OAuthServer {
    fn new() -> anyhow::Result<Self> {
        let server =
            tiny_http::Server::http("127.0.0.1:0").map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let base = format!("http://{}", server.server_addr());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let scopes = Arc::new(Mutex::new(SCOPES.join(" ")));
        let stop = Arc::new(AtomicBool::new(false));
        let (r, s, done) = (requests.clone(), scopes.clone(), stop.clone());
        let worker = std::thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                let Some(mut req) = server.recv_timeout(Duration::from_millis(50)).unwrap() else {
                    continue;
                };
                let mut body = String::new();
                req.as_reader().read_to_string(&mut body).unwrap();
                let path = req.url().to_owned();
                let auth = req
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Authorization"))
                    .map(|h| h.value.as_str().to_owned());
                r.lock().unwrap().push((path.clone(), body.clone()));
                let result = if path == "/token" {
                    let form: std::collections::BTreeMap<_, _> =
                        url::form_urlencoded::parse(body.as_bytes())
                            .into_owned()
                            .collect();
                    assert_eq!(form["client_id"], "fake-client");
                    assert_eq!(form["client_secret"], "fake-client-secret");
                    if form["grant_type"] == "authorization_code" {
                        assert_eq!(form["code"], "fake-code");
                        assert_eq!(
                            form["redirect_uri"],
                            "https://calendar.example.test/callback"
                        );
                        assert_eq!(form["code_verifier"].len(), 64);
                    } else {
                        assert_eq!(form["refresh_token"], "fake-refresh");
                    }
                    json!({"access_token":"fake-access","refresh_token":"fake-refresh","expires_in":3600,"token_type":"Bearer","scope":s.lock().unwrap().clone()})
                } else if path == "/userinfo" {
                    assert_eq!(auth.as_deref(), Some("Bearer fake-access"));
                    json!({"sub":"fake-subject","email":"Calendar@Example.test","email_verified":true,"name":"My Calendar"})
                } else {
                    assert!(path.starts_with("/calendar/v3/calendars/primary/events?"));
                    assert!(path.contains("singleEvents=true"));
                    assert!(path.contains("maxResults=100"));
                    assert!(path.contains("timeMin="));
                    assert!(path.contains("timeMax="));
                    assert!(path.contains("fields="));
                    assert_eq!(auth.as_deref(), Some("Bearer fake-access"));
                    json!({"items":[{"id":"occurrence-1","summary":"Account event","start":{"dateTime":"2026-10-16T09:00:00+02:00"},"end":{"dateTime":"2026-10-16T10:00:00+02:00"},"status":"confirmed"},{"id":"all-day","summary":"","start":{"date":"2026-10-16"},"end":{"date":"2026-10-17"}}],"nextPageToken":"private-pagination-token"})
                };
                req.respond(
                    tiny_http::Response::from_string(result.to_string()).with_header(
                        tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                    ),
                )
                .unwrap();
            }
        });
        Ok(Self {
            endpoints: Endpoints {
                authorize: format!("{base}/authorize"),
                token: format!("{base}/token"),
                userinfo: format!("{base}/userinfo"),
                calendar: format!("{base}/calendar/v3"),
            },
            requests,
            scopes,
            stop,
            worker: Some(worker),
        })
    }
}
impl Drop for OAuthServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(w) = self.worker.take() {
            w.join().unwrap();
        }
    }
}
fn setup(root: &Path) -> anyhow::Result<()> {
    secrets::write_secret_record(root,CLIENT_SCOPE,CLIENT_NAME,&json!({"client_id":"fake-client","client_secret":"fake-client-secret","redirect_uri":"https://calendar.example.test/callback"}).to_string(),None,json!({}))?;
    Ok(())
}
fn callback(start: &Value, state: Option<&str>) -> anyhow::Result<String> {
    let url = url::Url::parse(start["authorizeUrl"].as_str().unwrap())?;
    let expected = url
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned();
    let mut callback = url::Url::parse(start["redirectUri"].as_str().unwrap())?;
    callback
        .query_pairs_mut()
        .append_pair("state", state.unwrap_or(&expected))
        .append_pair("code", "fake-code");
    Ok(callback.into())
}

#[test]
fn google_calendar_oauth_state_bound_one_time_and_real_adapter_read() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    setup(root.path())?;
    let server = OAuthServer::new()?;
    let mut legacy = db(root.path()).map_err(anyhow::Error::msg)?;
    crate::mission::channels::upsert_communication_account(
        &mut legacy,
        "email:legacy@example.test",
        "email",
        "legacy@example.test",
        "google",
        json!({"source":"mail-app-account","ownerUserId":"owner"}),
    )?;
    assert!(
        accounts(root.path())?.is_empty(),
        "legacy Gmail mailbox must not become an OAuth calendar"
    );
    let start =
        start_at(root.path(), "owner", "google", &server.endpoints).map_err(anyhow::Error::msg)?;
    let authorize = url::Url::parse(start["authorizeUrl"].as_str().unwrap())?;
    let pairs: std::collections::BTreeMap<_, _> = authorize.query_pairs().into_owned().collect();
    assert_eq!(pairs["access_type"], "offline");
    assert_eq!(pairs["prompt"], "consent");
    assert_eq!(pairs["scope"], SCOPES.join(" "));
    assert_eq!(pairs["code_challenge_method"], "S256");
    assert!(!pairs.contains_key("client_secret"));
    let id = start["attemptId"].as_str().unwrap();
    let wrong = callback(&start, Some("wrong-state"))?;
    assert_eq!(
        complete_at(
            root.path(),
            "owner",
            id,
            &wrong,
            &|| Ok(()),
            &server.endpoints
        ),
        Err("state_mismatch"),
        "state binding must reject callback before token exchange"
    );
    assert!(
        server.requests.lock().unwrap().is_empty(),
        "state mismatch must not contact provider"
    );
    let good = callback(&start, None)?;
    assert_eq!(
        complete_at(
            root.path(),
            "foreign",
            id,
            &good,
            &|| Ok(()),
            &server.endpoints
        ),
        Err("permission_denied")
    );
    let receipt = complete_at(
        root.path(),
        "owner",
        id,
        &good,
        &|| Ok(()),
        &server.endpoints,
    )
    .map_err(anyhow::Error::msg)?;
    assert_eq!(receipt["account"]["provider"], "google");
    assert_eq!(receipt["account"]["supported"], true);
    assert_eq!(receipt["account"]["id"], "calendar@example.test");
    assert_eq!(
        receipt["account"]["calendar_id"],
        format!("account:{}", digest("calendar@example.test"))
    );
    let before = server.requests.lock().unwrap().len();
    assert_eq!(
        complete_at(
            root.path(),
            "owner",
            id,
            &good,
            &|| Ok(()),
            &server.endpoints
        )
        .map_err(anyhow::Error::msg)?,
        receipt
    );
    assert_eq!(
        server.requests.lock().unwrap().len(),
        before,
        "replay must not exchange the code twice"
    );
    let accounts = accounts(root.path())?;
    assert_eq!(accounts[0].owner_user_id, "owner");
    let raw = secrets::read_secret_value(root.path(), ACCOUNT_SCOPE, "calendar@example.test")?;
    let mut token: Tokens = serde_json::from_str(&raw)?;
    token.expires_at_ms = 0;
    secrets::write_secret_record(
        root.path(),
        ACCOUNT_SCOPE,
        "calendar@example.test",
        &serde_json::to_string(&token)?,
        None,
        json!({}),
    )?;
    let page = read_calendar_at(
        root.path(),
        &accounts[0],
        1_792_108_800_000,
        1_792_281_600_000,
        100,
        &server.endpoints,
    )
    .map_err(anyhow::Error::msg)?;
    assert_eq!(page["truncated"], true);
    assert_eq!(page["events"][0]["external_id"], "occurrence-1");
    assert_eq!(
        page["events"][0]["end_ms"].as_i64().unwrap()
            - page["events"][0]["start_ms"].as_i64().unwrap(),
        3_600_000
    );
    assert_eq!(page["events"][1]["all_day"], true);
    assert_eq!(page["events"][1]["title"], "(Untitled)");
    assert!(!page.to_string().contains("private-pagination-token"));
    let conn = db(root.path()).map_err(anyhow::Error::msg)?;
    let profile: String = conn.query_row(
        "SELECT profile_json FROM communication_accounts WHERE provider='google' AND address='calendar@example.test'",
        [],
        |r| r.get(0),
    )?;
    for forbidden in [
        "fake-access",
        "fake-refresh",
        "fake-code",
        "fake-client-secret",
        pairs["state"].as_str(),
    ] {
        assert!(!profile.contains(forbidden));
        assert!(!receipt.to_string().contains(forbidden));
    }
    let next =
        start_at(root.path(), "owner", "google", &server.endpoints).map_err(anyhow::Error::msg)?;
    assert_eq!(
        complete_at(
            root.path(),
            "owner",
            next["attemptId"].as_str().unwrap(),
            &callback(&next, None)?,
            &|| Ok(()),
            &server.endpoints
        ),
        Err("account_exists")
    );
    Ok(())
}

#[test]
fn google_calendar_oauth_denies_expiry_scopes_consent_and_revocation() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    assert_eq!(
        start(root.path(), "owner", "graph"),
        Err("provider_unsupported")
    );
    assert_eq!(
        start(root.path(), "owner", "google"),
        Err("oauth_client_missing")
    );
    setup(root.path())?;
    let server = OAuthServer::new()?;
    let expired =
        start_at(root.path(), "owner", "google", &server.endpoints).map_err(anyhow::Error::msg)?;
    db(root.path()).map_err(anyhow::Error::msg)?.execute(
        "UPDATE google_calendar_connect_attempts SET expires_ms=0 WHERE attempt_id=?1",
        [expired["attemptId"].as_str().unwrap()],
    )?;
    assert_eq!(
        complete_at(
            root.path(),
            "owner",
            expired["attemptId"].as_str().unwrap(),
            &callback(&expired, None)?,
            &|| Ok(()),
            &server.endpoints
        ),
        Err("attempt_expired")
    );
    let denied =
        start_at(root.path(), "owner", "google", &server.endpoints).map_err(anyhow::Error::msg)?;
    let url = callback(&denied, None)?.replace("code=fake-code", "error=access_denied");
    assert_eq!(
        complete_at(
            root.path(),
            "owner",
            denied["attemptId"].as_str().unwrap(),
            &url,
            &|| Ok(()),
            &server.endpoints
        ),
        Err("consent_denied")
    );
    assert!(server.requests.lock().unwrap().is_empty());
    *server.scopes.lock().unwrap() = "openid email".into();
    let partial =
        start_at(root.path(), "owner", "google", &server.endpoints).map_err(anyhow::Error::msg)?;
    assert_eq!(
        complete_at(
            root.path(),
            "owner",
            partial["attemptId"].as_str().unwrap(),
            &callback(&partial, None)?,
            &|| Ok(()),
            &server.endpoints
        ),
        Err("scope_missing")
    );
    *server.scopes.lock().unwrap() = SCOPES.join(" ");
    let revoked =
        start_at(root.path(), "owner", "google", &server.endpoints).map_err(anyhow::Error::msg)?;
    let calls = std::cell::Cell::new(0);
    assert_eq!(
        complete_at(
            root.path(),
            "owner",
            revoked["attemptId"].as_str().unwrap(),
            &callback(&revoked, None)?,
            &|| {
                calls.set(calls.get() + 1);
                if calls.get() > 1 {
                    Err("permission_denied")
                } else {
                    Ok(())
                }
            },
            &server.endpoints
        ),
        Err("permission_denied")
    );
    assert!(
        accounts(root.path())?.is_empty(),
        "revoked owner must not publish an account"
    );
    Ok(())
}
