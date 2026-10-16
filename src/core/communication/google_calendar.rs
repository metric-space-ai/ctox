// Origin: CTOX
// License: AGPL-3.0-only
//! Google OAuth and Calendar v3 transport. Secrets never enter replicated records.
use super::email_accounts::EmailAccountConfig;
use crate::{communication_store, secrets};
use anyhow::Context;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path, time::Duration};
use zeroize::Zeroizing;

pub(crate) const CLIENT_SCOPE: &str = "google-oauth";
pub(crate) const CLIENT_NAME: &str = "client";
const ATTEMPT_SCOPE: &str = "google-oauth-attempt";
const ACCOUNT_SCOPE: &str = "google-calendar-account";
pub(crate) const SCOPES: &[&str] = &[
    "openid",
    "email",
    "https://www.googleapis.com/auth/gmail.readonly",
    "https://www.googleapis.com/auth/gmail.send",
    "https://www.googleapis.com/auth/gmail.compose",
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/calendar.readonly",
    "https://www.googleapis.com/auth/calendar.events",
];
pub(crate) type ConnectResult<T> = Result<T, &'static str>;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientConfig {
    client_id: String,
    client_secret: String,
    #[serde(default)]
    redirect_uri: String,
}
#[derive(Serialize, Deserialize)]
struct AttemptSecret {
    state: String,
    verifier: String,
    client_revision: String,
}
#[derive(Serialize, Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    expires_at_ms: i64,
    client_revision: String,
}
struct Endpoints {
    authorize: String,
    token: String,
    userinfo: String,
    calendar: String,
}
impl Default for Endpoints {
    fn default() -> Self {
        Self {
            authorize: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            userinfo: "https://openidconnect.googleapis.com/v1/userinfo".into(),
            calendar: "https://www.googleapis.com/calendar/v3".into(),
        }
    }
}
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn nonce() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn client(root: &Path) -> ConnectResult<(ClientConfig, Zeroizing<String>)> {
    let raw = Zeroizing::new(
        secrets::read_secret_value(root, CLIENT_SCOPE, CLIENT_NAME)
            .map_err(|_| "oauth_client_missing")?,
    );
    let config: ClientConfig = serde_json::from_str(&raw).map_err(|_| "oauth_client_missing")?;
    if config.client_id.trim().is_empty() || config.client_secret.trim().is_empty() {
        return Err("oauth_client_missing");
    }
    let uri = url::Url::parse(&config.redirect_uri).map_err(|_| "redirect_unavailable")?;
    let loopback = matches!(
        uri.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    if !((uri.scheme() == "https" && uri.host_str().is_some())
        || (uri.scheme() == "http" && loopback))
        || !uri.username().is_empty()
        || uri.password().is_some()
        || uri.query().is_some()
        || uri.fragment().is_some()
    {
        return Err("redirect_unavailable");
    }
    Ok((config, raw))
}
fn db(root: &Path) -> ConnectResult<Connection> {
    let conn = communication_store::open_channel_db(&root.join("runtime/ctox.sqlite3"))
        .map_err(|_| "token_exchange_failed")?;
    conn.execute_batch("CREATE TABLE IF NOT EXISTS google_calendar_connect_attempts (
      attempt_id TEXT PRIMARY KEY, actor TEXT NOT NULL, state_hash TEXT NOT NULL,
      redirect_uri TEXT NOT NULL, created_ms INTEGER NOT NULL, expires_ms INTEGER NOT NULL,
      status TEXT NOT NULL, receipt_json TEXT);
      CREATE INDEX IF NOT EXISTS google_calendar_attempt_actor ON google_calendar_connect_attempts(actor, created_ms);")
        .map_err(|_| "token_exchange_failed")?;
    Ok(conn)
}
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .redirects(0)
        .try_proxy_from_env(false)
        .build()
}
fn response_json(response: Result<ureq::Response, ureq::Error>) -> ConnectResult<Value> {
    let response = response.map_err(|_| "token_exchange_failed")?;
    if !(200..300).contains(&response.status()) {
        return Err("token_exchange_failed");
    }
    let mut body = Vec::new();
    response
        .into_reader()
        .take(262_145)
        .read_to_end(&mut body)
        .map_err(|_| "token_exchange_failed")?;
    if body.len() > 262_144 {
        return Err("token_exchange_failed");
    }
    serde_json::from_slice(&body).map_err(|_| "token_exchange_failed")
}
fn scopes_valid(value: &str) -> bool {
    let granted: std::collections::BTreeSet<_> = value.split_whitespace().collect();
    SCOPES.iter().all(|scope| {
        granted.contains(scope)
            || (*scope == "email"
                && granted.contains("https://www.googleapis.com/auth/userinfo.email"))
    })
}

pub(crate) fn start(root: &Path, actor: &str, provider: &str) -> ConnectResult<Value> {
    start_at(root, actor, provider, &Endpoints::default())
}
fn start_at(
    root: &Path,
    actor: &str,
    provider: &str,
    endpoints: &Endpoints,
) -> ConnectResult<Value> {
    if provider != "google" {
        return Err("provider_unsupported");
    }
    let (config, raw) = client(root)?;
    let now = now_ms();
    let mut conn = db(root).map_err(|_| "rate_limited")?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| "rate_limited")?;
    let recent: i64 = tx.query_row("SELECT count(*) FROM google_calendar_connect_attempts WHERE actor=?1 AND created_ms>?2", params![actor,now-60_000], |r| r.get(0)).map_err(|_| "rate_limited")?;
    if recent >= 5 {
        return Err("rate_limited");
    }
    let id = uuid::Uuid::new_v4().to_string();
    let secret = AttemptSecret {
        state: nonce(),
        verifier: nonce(),
        client_revision: digest(&raw),
    };
    let mut url = url::Url::parse(&endpoints.authorize).map_err(|_| "redirect_unavailable")?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", config.client_id.as_str()),
        ("redirect_uri", config.redirect_uri.as_str()),
        ("response_type", "code"),
        ("scope", &SCOPES.join(" ")),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("state", secret.state.as_str()),
        (
            "code_challenge",
            &URL_SAFE_NO_PAD.encode(Sha256::digest(secret.verifier.as_bytes())),
        ),
        ("code_challenge_method", "S256"),
    ]);
    let secret_json =
        Zeroizing::new(serde_json::to_string(&secret).map_err(|_| "oauth_client_missing")?);
    secrets::write_secret_record(root, ATTEMPT_SCOPE, &id, &secret_json, None, json!({}))
        .map_err(|_| "oauth_client_missing")?;
    tx.execute(
        "INSERT INTO google_calendar_connect_attempts VALUES(?1,?2,?3,?4,?5,?6,'pending',NULL)",
        params![
            id,
            actor,
            digest(&secret.state),
            config.redirect_uri,
            now,
            now + 600_000
        ],
    )
    .map_err(|_| "rate_limited")?;
    tx.commit().map_err(|_| "rate_limited")?;
    Ok(
        json!({"attemptId":id,"authorizeUrl":url.as_str(),"redirectUri":config.redirect_uri,"scopes":SCOPES,"expiresAtMs":now+600_000}),
    )
}

pub(crate) fn complete(
    root: &Path,
    actor: &str,
    id: &str,
    callback: &str,
    authorize: &dyn Fn() -> ConnectResult<()>,
) -> ConnectResult<Value> {
    complete_at(root, actor, id, callback, authorize, &Endpoints::default())
}
fn complete_at(
    root: &Path,
    actor: &str,
    id: &str,
    callback: &str,
    authorize: &dyn Fn() -> ConnectResult<()>,
    endpoints: &Endpoints,
) -> ConnectResult<Value> {
    authorize()?;
    if uuid::Uuid::parse_str(id).is_err() || callback.len() > 8192 {
        return Err("state_mismatch");
    }
    let mut conn = db(root)?;
    let row: Option<(String,String,String,i64,String,Option<String>)> = conn.query_row(
        "SELECT actor,state_hash,redirect_uri,expires_ms,status,receipt_json FROM google_calendar_connect_attempts WHERE attempt_id=?1", [id],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional().map_err(|_| "attempt_expired")?;
    let Some((owner, state_hash, redirect, expiry, status, receipt)) = row else {
        return Err("attempt_expired");
    };
    if owner != actor {
        return Err("permission_denied");
    }
    let mut url = url::Url::parse(callback).map_err(|_| "state_mismatch")?;
    if url.fragment().is_some() {
        return Err("state_mismatch");
    }
    let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    let get = |key: &str| -> ConnectResult<Option<&str>> {
        let values: Vec<_> = pairs
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .collect();
        if values.len() > 1 {
            Err("state_mismatch")
        } else {
            Ok(values.first().copied())
        }
    };
    url.set_query(None);
    if url.as_str() != redirect {
        return Err("state_mismatch");
    }
    let state = get("state")?.ok_or("state_mismatch")?;
    // Bind the callback to exactly this actor's attempt before any token request.
    if digest(state) != state_hash {
        return Err("state_mismatch");
    }
    if expiry <= now_ms() {
        return Err("attempt_expired");
    }
    if status == "completed" {
        return serde_json::from_str(receipt.as_deref().ok_or("attempt_expired")?)
            .map_err(|_| "attempt_expired");
    }
    if status != "pending" {
        return Err("attempt_expired");
    }
    if get("error")?.is_some() {
        conn.execute("UPDATE google_calendar_connect_attempts SET status='failed' WHERE attempt_id=?1 AND status='pending'",[id]).map_err(|_| "consent_denied")?;
        let _ = secrets::delete_secret_record(root, ATTEMPT_SCOPE, id);
        return Err("consent_denied");
    }
    let code = get("code")?
        .filter(|s| !s.is_empty())
        .ok_or("token_exchange_failed")?;
    let raw = Zeroizing::new(
        secrets::read_secret_value(root, ATTEMPT_SCOPE, id).map_err(|_| "attempt_expired")?,
    );
    let secret: AttemptSecret = serde_json::from_str(&raw).map_err(|_| "attempt_expired")?;
    let (config, client_raw) = client(root).map_err(|_| "token_exchange_failed")?;
    if secret.client_revision != digest(&client_raw) || config.redirect_uri != redirect {
        return Err("token_exchange_failed");
    }
    let claimed = conn.execute("UPDATE google_calendar_connect_attempts SET status='exchanging' WHERE attempt_id=?1 AND status='pending' AND expires_ms>?2",params![id,now_ms()]).map_err(|_| "attempt_expired")?;
    if claimed != 1 {
        return Err("attempt_expired");
    }
    // No channel/permission transaction is held across provider I/O.
    let outcome = (|| {
        let response = response_json(agent().post(&endpoints.token).send_form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("code_verifier", secret.verifier.as_str()),
        ]))?;
        if !scopes_valid(response["scope"].as_str().unwrap_or("")) {
            return Err("scope_missing");
        }
        if response["token_type"]
            .as_str()
            .is_none_or(|s| !s.eq_ignore_ascii_case("bearer"))
        {
            return Err("token_exchange_failed");
        }
        let access = response["access_token"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 16_384)
            .ok_or("token_exchange_failed")?;
        let refresh = response["refresh_token"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 16_384)
            .ok_or("token_exchange_failed")?;
        let expires = response["expires_in"]
            .as_i64()
            .filter(|s| (1..=86_400).contains(s))
            .ok_or("token_exchange_failed")?;
        let info = response_json(
            agent()
                .get(&endpoints.userinfo)
                .set("Authorization", &format!("Bearer {access}"))
                .call(),
        )?;
        let address = info["email"]
            .as_str()
            .map(super::email_accounts::normalize_address)
            .filter(|s| s.len() <= 254 && s.contains('@') && !s.chars().any(char::is_whitespace))
            .ok_or("token_exchange_failed")?;
        if info["email_verified"].as_bool() != Some(true)
            || info["sub"].as_str().is_none_or(str::is_empty)
        {
            return Err("token_exchange_failed");
        }
        let label = info["name"]
            .as_str()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&address)
            .chars()
            .take(256)
            .collect::<String>();
        let tokens = Tokens {
            access_token: access.into(),
            refresh_token: refresh.into(),
            expires_at_ms: now_ms() + expires * 1000,
            client_revision: secret.client_revision,
        };
        authorize()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| "token_exchange_failed")?;
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM communication_accounts WHERE account_key=?1)",
                [format!("email:{address}")],
                |r| r.get(0),
            )
            .map_err(|_| "token_exchange_failed")?;
        if exists
            || super::email_accounts::load_accounts(root)
                .map_err(|_| "token_exchange_failed")?
                .iter()
                .any(|a| a.address == address)
        {
            return Err("account_exists");
        }
        let token_json =
            Zeroizing::new(serde_json::to_string(&tokens).map_err(|_| "token_exchange_failed")?);
        secrets::write_secret_record(root, ACCOUNT_SCOPE, &address, &token_json, None, json!({}))
            .map_err(|_| "token_exchange_failed")?;
        let profile = json!({"ownerUserId":actor,"shared_user_ids":[],"displayName":label,"source":"google-oauth","oauthAttemptId":id});
        tx.execute("INSERT INTO communication_accounts(account_key,channel,address,provider,profile_json,created_at,updated_at) VALUES(?1,'email',?2,'google',?3,?4,?4)",params![format!("email:{address}"),address,profile.to_string(),chrono::DateTime::from_timestamp_millis(now_ms()).ok_or("token_exchange_failed")?.to_rfc3339()]).map_err(|_| "token_exchange_failed")?;
        let receipt = json!({"account":{"id":address,"label":label,"provider":"google","calendar_id":format!("account:{}",digest(&address)),"supported":true}});
        tx.execute("UPDATE google_calendar_connect_attempts SET status='completed',receipt_json=?2 WHERE attempt_id=?1",params![id,receipt.to_string()]).map_err(|_| "token_exchange_failed")?;
        tx.commit().map_err(|_| "token_exchange_failed")?;
        Ok(receipt)
    })();
    if outcome.is_err() {
        let _ = conn.execute("UPDATE google_calendar_connect_attempts SET status='failed' WHERE attempt_id=?1 AND status='exchanging'",[id]);
    }
    let _ = secrets::delete_secret_record(root, ATTEMPT_SCOPE, id);
    outcome
}

pub(crate) fn accounts(root: &Path) -> anyhow::Result<Vec<EmailAccountConfig>> {
    if !root.join("runtime/ctox.sqlite3").is_file() {
        return Ok(Vec::new());
    }
    let conn = Connection::open_with_flags(
        root.join("runtime/ctox.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let mut stmt = conn.prepare("SELECT address,profile_json FROM communication_accounts WHERE channel='email' AND provider='google' AND json_extract(profile_json,'$.source')='google-oauth'")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(address, raw)| {
            let profile: Value = serde_json::from_str(&raw)?;
            anyhow::ensure!(
                profile["source"] == "google-oauth",
                "google account has no native OAuth binding"
            );
            Ok(EmailAccountConfig {
                address,
                provider: "google".into(),
                display_name: profile["displayName"].as_str().unwrap_or("").into(),
                owner_user_id: profile["ownerUserId"]
                    .as_str()
                    .context("google account owner missing")?
                    .into(),
                shared_user_ids: Some(Vec::new()),
                ..Default::default()
            })
        })
        .collect()
}

pub(crate) fn read_calendar(
    root: &Path,
    account: &EmailAccountConfig,
    start_ms: i64,
    end_ms: i64,
    limit: usize,
) -> anyhow::Result<Value> {
    read_calendar_at(
        root,
        account,
        start_ms,
        end_ms,
        limit,
        &Endpoints::default(),
    )
    .map_err(|code| anyhow::anyhow!(code))
}
fn read_calendar_at(
    root: &Path,
    account: &EmailAccountConfig,
    start_ms: i64,
    end_ms: i64,
    limit: usize,
    endpoints: &Endpoints,
) -> ConnectResult<Value> {
    use ctox_toolhub::toolkits::{google_calendar::GoogleCalendar, ToolkitAdapter};
    if !(1..=100).contains(&limit)
        || end_ms
            .checked_sub(start_ms)
            .is_none_or(|s| s <= 0 || s > 400 * 86_400_000)
    {
        return Err("permission_denied");
    }
    let fresh = accounts(root).map_err(|_| "permission_denied")?;
    if !fresh
        .iter()
        .any(|a| a.address == account.address && a.owner_user_id == account.owner_user_id)
    {
        return Err("permission_denied");
    }
    let raw = Zeroizing::new(
        secrets::read_secret_value(root, ACCOUNT_SCOPE, &account.address)
            .map_err(|_| "token_exchange_failed")?,
    );
    let mut token: Tokens = serde_json::from_str(&raw).map_err(|_| "token_exchange_failed")?;
    if token.expires_at_ms <= now_ms() + 60_000 {
        let (config, client_raw) = client(root).map_err(|_| "token_exchange_failed")?;
        if token.client_revision != digest(&client_raw) {
            return Err("oauth_client_missing");
        }
        let response = response_json(agent().post(&endpoints.token).send_form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", token.refresh_token.as_str()),
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
        ]))?;
        if let Some(scope) = response["scope"].as_str() {
            if !scopes_valid(scope) {
                return Err("scope_missing");
            }
        }
        token.access_token = response["access_token"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("token_exchange_failed")?
            .into();
        let expires = response["expires_in"]
            .as_i64()
            .filter(|s| (1..=86_400).contains(s))
            .ok_or("token_exchange_failed")?;
        token.expires_at_ms = now_ms() + expires * 1000;
        if let Some(refresh) = response["refresh_token"].as_str().filter(|s| !s.is_empty()) {
            token.refresh_token = refresh.into();
        }
        let next =
            Zeroizing::new(serde_json::to_string(&token).map_err(|_| "token_exchange_failed")?);
        if !secrets::compare_and_write_secret_records(
            root,
            &[
                (ACCOUNT_SCOPE, &account.address, Some(raw.as_str())),
                (CLIENT_SCOPE, CLIENT_NAME, Some(client_raw.as_str())),
            ],
            &[secrets::SecretRecordWrite {
                scope: ACCOUNT_SCOPE,
                name: &account.address,
                value: &next,
                description: None,
                metadata: json!({}),
            }],
            &[],
        )
        .map_err(|_| "token_exchange_failed")?
        {
            return Err("token_exchange_failed");
        }
    }
    let adapter = GoogleCalendar::with_api_base(&endpoints.calendar);
    let start = chrono::DateTime::from_timestamp_millis(start_ms)
        .ok_or("token_exchange_failed")?
        .to_rfc3339();
    let end = chrono::DateTime::from_timestamp_millis(end_ms)
        .ok_or("token_exchange_failed")?
        .to_rfc3339();
    let request = adapter.build_request("GOOGLECALENDAR_LIST_EVENTS",&json!({"calendar_id":"primary","start_date":start,"end_date":end,"single_events":true,"max_results":limit})).map_err(|_| "token_exchange_failed")?;
    let mut request_url = url::Url::parse(&request.url).map_err(|_| "token_exchange_failed")?;
    request_url.query_pairs_mut().append_pair(
        "fields",
        "items(id,summary,start,end,location,status),nextPageToken",
    );
    let page = response_json(
        agent()
            .get(request_url.as_str())
            .set("Authorization", &format!("Bearer {}", token.access_token))
            .call(),
    )?;
    if !page["items"].is_array()
        || page
            .get("nextPageToken")
            .is_some_and(|v| !v.is_null() && !v.is_string())
    {
        return Err("token_exchange_failed");
    }
    let response = ctox_toolhub::http::HttpResponse {
        status: 200,
        body: serde_json::to_vec(&page).map_err(|_| "token_exchange_failed")?,
        session_id: None,
    };
    let parsed = adapter
        .parse_response("GOOGLECALENDAR_LIST_EVENTS", &response)
        .map_err(|_| "token_exchange_failed")?;
    let items = parsed["events"].as_array().ok_or("token_exchange_failed")?;
    if items.len() > limit {
        return Err("token_exchange_failed");
    }
    let events = items.iter().map(|e| {
        let time = |v:&Value| -> ConnectResult<i64> {
            if let Some(t)=v["dateTime"].as_str() { return chrono::DateTime::parse_from_rfc3339(t).map(|d|d.timestamp_millis()).map_err(|_| "token_exchange_failed"); }
            let date=chrono::NaiveDate::parse_from_str(v["date"].as_str().ok_or("token_exchange_failed")?,"%Y-%m-%d").map_err(|_| "token_exchange_failed")?;
            Ok(date.and_hms_opt(0,0,0).ok_or("token_exchange_failed")?.and_utc().timestamp_millis())
        };
        let start=time(&e["start"])?; let end=time(&e["end"])?;
        let id=e["id"].as_str().filter(|s|!s.is_empty()&&s.len()<=256).ok_or("token_exchange_failed")?;
        if end<=start { return Err("token_exchange_failed"); }
        Ok(json!({"external_id":id,"title":e["summary"].as_str().filter(|s| !s.trim().is_empty()).unwrap_or("(Untitled)").chars().take(256).collect::<String>(),"start_ms":start,"end_ms":end,"all_day":e["start"].get("date").is_some(),"timezone":"UTC","location":e["location"].as_str().unwrap_or("").chars().take(512).collect::<String>()}))
    }).collect::<ConnectResult<Vec<_>>>()?;
    Ok(
        json!({"events":events,"truncated":parsed["next_page_token"].as_str().is_some_and(|s|!s.is_empty())}),
    )
}

#[cfg(test)]
#[path = "google_calendar_tests.rs"]
mod tests;
