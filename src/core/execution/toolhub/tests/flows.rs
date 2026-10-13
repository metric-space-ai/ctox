use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use ctox_toolhub::http::{HttpRequest, HttpResponse, HttpTransport};
use ctox_toolhub::oauth2::RedirectTarget;
use ctox_toolhub::*;
use serde_json::{json, Value};

/// Scripted provider: token endpoint + GitHub API.
#[derive(Default)]
struct Provider {
    log: Mutex<Vec<HttpRequest>>,
    valid_token: Mutex<String>,
    issued: AtomicU64,
}

impl Provider {
    fn mint(&self) -> Value {
        let n = self.issued.fetch_add(1, Ordering::SeqCst) + 1;
        let token = format!("at-{n}");
        *self.valid_token.lock().unwrap() = token.clone();
        json!({"access_token": token, "refresh_token": format!("rt-{n}"), "expires_in": 3600, "scope": "repo", "token_type": "bearer"})
    }
}

impl HttpTransport for Provider {
    fn send(&self, r: HttpRequest) -> anyhow::Result<HttpResponse> {
        self.log.lock().unwrap().push(r.clone());
        let ok = |v: Value| {
            Ok(HttpResponse {
                status: 200,
                body: serde_json::to_vec(&v).unwrap(),
            })
        };
        if r.url.ends_with("/login/oauth/access_token") {
            return ok(self.mint());
        }
        let auth = r.headers.get("authorization").cloned().unwrap_or_default();
        if auth != format!("Bearer {}", self.valid_token.lock().unwrap()) {
            return Ok(HttpResponse {
                status: 401,
                body: br#"{"message":"Bad credentials"}"#.to_vec(),
            });
        }
        if r.url.contains("/issues") {
            return ok(
                json!([{"number": 1, "title": "Bug", "state": "open", "html_url": "u", "user": {"login": "mw"}, "labels": []},
                             {"number": 2, "title": "PR", "pull_request": {}}]),
            );
        }
        ok(json!({"full_name": "metric-space-ai/ctox", "private": false}))
    }
}

fn hub(provider: Arc<Provider>, now: Arc<AtomicU64>) -> ToolHub<MemorySecretBackend> {
    let clock = now.clone();
    let hub = ToolHub::new(
        MemorySecretBackend::default(),
        provider,
        RedirectTarget::for_instance_name("welsch").unwrap(),
    )
    .with_clock(Arc::new(move || clock.load(Ordering::SeqCst)));
    hub.upsert_auth_config(
        &AuthConfig {
            id: "gh-oauth".into(),
            toolkit: "github".into(),
            scheme: AuthSchemeKind::OAuth2,
            source: ClientCredentialSource::Custom,
            client_id: Some("cid".into()),
            scopes: vec![],
            enabled: true,
        },
        Some("csecret"),
    )
    .unwrap();
    hub.upsert_auth_config(
        &AuthConfig {
            id: "gh-pat".into(),
            toolkit: "github".into(),
            scheme: AuthSchemeKind::Bearer,
            source: ClientCredentialSource::None,
            client_id: None,
            scopes: vec![],
            enabled: true,
        },
        None,
    )
    .unwrap();
    hub
}

#[test]
fn oauth_connect_execute_refresh_on_401_once() {
    let provider = Arc::new(Provider::default());
    let now = Arc::new(AtomicU64::new(1_000));
    let hub = hub(provider.clone(), now.clone());

    let req = hub.connect_oauth("gh-oauth", "michael").unwrap();
    assert!(req
        .redirect_url
        .starts_with("https://github.com/login/oauth/authorize?"));
    assert!(req.redirect_url.contains("code_challenge="));
    assert!(req
        .redirect_url
        .contains("welsch.ctox.dev%2Ftoolhub%2Foauth%2Fcallback"));
    assert_eq!(
        hub.status(&req.account_id).unwrap().unwrap().status,
        ConnectionStatus::Initializing
    );

    // Forged state is rejected, real state completes (once).
    assert!(hub.complete_oauth("code=x&state=forged").is_err());
    let account = hub
        .complete_oauth(&format!(
            "https://welsch.ctox.dev/toolhub/oauth/callback?code=c1&state={}",
            req.state
        ))
        .unwrap();
    assert_eq!(account.status, ConnectionStatus::Active);
    assert_eq!(account.expires_at, Some(4_600));
    assert_eq!(account.next_refresh_after, Some(4_300));
    assert!(
        hub.complete_oauth(&format!("code=c1&state={}", req.state))
            .is_err(),
        "state is single-use"
    );
    let token_body =
        String::from_utf8(provider.log.lock().unwrap()[0].body.clone().unwrap()).unwrap();
    assert!(token_body.contains("code_verifier=") && token_body.contains("client_secret=csecret"));

    let out = hub
        .execute(
            "michael",
            "GITHUB_LIST_ISSUES",
            &json!({"owner": "metric-space-ai", "repo": "ctox"}),
            None,
        )
        .unwrap();
    let ToolOutcome::Success { data } = out else {
        panic!("{out:?}")
    };
    assert_eq!(data.as_array().unwrap().len(), 1, "pull requests filtered");
    assert!(!data.to_string().contains("at-1"));

    // Provider revokes the token -> exactly one refresh + replay.
    *provider.valid_token.lock().unwrap() = "rotated-elsewhere".into();
    provider.issued.store(9, Ordering::SeqCst);
    let before = provider.log.lock().unwrap().len();
    let out = hub
        .execute(
            "michael",
            "GITHUB_GET_REPO",
            &json!({"owner": "metric-space-ai", "repo": "ctox"}),
            None,
        )
        .unwrap();
    assert!(matches!(out, ToolOutcome::Success { .. }), "{out:?}");
    assert_eq!(
        provider.log.lock().unwrap().len() - before,
        3,
        "401, refresh, replay"
    );

    // Other owners never see this account.
    assert!(matches!(
        hub.execute(
            "someone-else",
            "GITHUB_GET_REPO",
            &json!({"owner": "a", "repo": "b"}),
            None
        )
        .unwrap(),
        ToolOutcome::CredentialRequired { .. }
    ));

    // Scheduler refreshes when due.
    now.store(4_300, Ordering::SeqCst);
    let refreshed = hub.refresh_due().unwrap();
    assert_eq!(refreshed.len(), 1);
    assert_eq!(refreshed[0].status, ConnectionStatus::Active);
    assert!(refreshed[0].next_refresh_after.unwrap() > 4_300);

    assert!(hub.disconnect(&account.id).unwrap());
    assert!(hub.status(&account.id).unwrap().is_none());
}

#[test]
fn bearer_401_without_refresh_marks_expired_and_requires_credential() {
    let provider = Arc::new(Provider::default());
    let hub = hub(provider.clone(), Arc::new(AtomicU64::new(1)));
    *provider.valid_token.lock().unwrap() = "ghp_good".into();
    let acc = hub
        .connect_with_credential("gh-pat", "michael", "ghp_good")
        .unwrap();
    assert!(matches!(
        hub.execute(
            "michael",
            "GITHUB_GET_REPO",
            &json!({"owner": "a", "repo": "b"}),
            None
        )
        .unwrap(),
        ToolOutcome::Success { .. }
    ));
    *provider.valid_token.lock().unwrap() = "revoked".into();
    let out = hub
        .execute(
            "michael",
            "GITHUB_GET_REPO",
            &json!({"owner": "a", "repo": "b"}),
            None,
        )
        .unwrap();
    assert!(
        matches!(out, ToolOutcome::CredentialRequired { ref account_id, .. } if account_id.as_deref() == Some(acc.id.as_str()))
    );
    assert_eq!(
        hub.status(&acc.id).unwrap().unwrap().status,
        ConnectionStatus::Expired
    );
}

#[test]
fn denied_and_stale_callbacks_fail_the_account() {
    let now = Arc::new(AtomicU64::new(10));
    let hub = hub(Arc::new(Provider::default()), now.clone());
    let req = hub.connect_oauth("gh-oauth", "michael").unwrap();
    let acc = hub
        .complete_oauth(&format!("error=access_denied&state={}", req.state))
        .unwrap();
    assert_eq!(acc.status, ConnectionStatus::Failed);
    let req = hub.connect_oauth("gh-oauth", "michael").unwrap();
    now.store(10 + 901, Ordering::SeqCst);
    assert_eq!(
        hub.complete_oauth(&format!("code=c&state={}", req.state))
            .unwrap()
            .status,
        ConnectionStatus::Failed
    );
}

#[test]
fn mcp_tool_list_is_bilingual_and_schema_bearing() {
    let hub = hub(Arc::new(Provider::default()), Arc::new(AtomicU64::new(1)));
    let tools = hub.mcp_tools();
    assert_eq!(tools.len(), 3);
    let create = tools
        .iter()
        .find(|t| t["name"] == "GITHUB_CREATE_ISSUE")
        .unwrap();
    assert_eq!(create["annotations"]["destructiveHint"], true);
    assert_eq!(
        create["inputSchema"]["required"],
        json!(["owner", "repo", "title"])
    );
    assert!(create["description"].as_str().unwrap().contains(" / "));
}
