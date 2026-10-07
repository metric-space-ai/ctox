// Session-expiry reauthorization: protected-capture config resolution,
// login-landing detection, and the typed auth-assist handoff (source id,
// safe login URL, allowed domains, credential REFERENCES only).

use super::classify::{Classification, ScrapeRunStatus};
use super::execute::ProbeResult;
use super::{host_within_domains, url_host_lower, RegisteredTarget};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub(super) struct ProtectedCaptureConfig {
    pub(super) login_url: Option<String>,
    pub(super) allowed_domains: Vec<String>,
    pub(super) credential_ref: Option<String>,
}

pub(super) fn js_object_string_field(body: &str, key: &str) -> Option<String> {
    let marker = format!("{key}: \"");
    let start = body.find(&marker)? + marker.len();
    let end = body[start..].find('"')? + start;
    Some(body[start..end].to_string())
}

pub(super) fn js_object_string_list(body: &str, key: &str) -> Vec<String> {
    let marker = format!("{key}: [");
    let Some(start) = body.find(&marker).map(|i| i + marker.len()) else {
        return Vec::new();
    };
    let Some(end) = body[start..].find(']').map(|i| i + start) else {
        return Vec::new();
    };
    body[start..end]
        .split(',')
        .filter_map(|item| {
            let item = item.trim().trim_matches('"');
            (!item.is_empty()).then(|| item.to_string())
        })
        .collect()
}

/// Parse the `PROTECTED_SOURCE_CONFIG` entry for `source_id` out of the
/// registered adapter script body — the same static contract the unlock
/// acceptance report validates. Entry bodies contain arrays but no nested
/// braces, so a brace-delimited slice is sufficient.
pub(super) fn protected_config_from_script(
    script: &str,
    source_id: &str,
) -> Option<ProtectedCaptureConfig> {
    let marker = "const PROTECTED_SOURCE_CONFIG = Object.freeze({";
    let start = script.find(marker)? + marker.len();
    let rest = &script[start..];
    let end = rest.find("\n});").unwrap_or(rest.len());
    let section = &rest[..end];
    let entry_marker = format!("\"{source_id}\": {{");
    let entry_start = section.find(&entry_marker)? + entry_marker.len();
    let entry_end = section[entry_start..].find('}')? + entry_start;
    let body = &section[entry_start..entry_end];
    let config = ProtectedCaptureConfig {
        login_url: js_object_string_field(body, "login_url"),
        allowed_domains: js_object_string_list(body, "allowed_domains"),
        credential_ref: js_object_string_field(body, "credential_ref"),
    };
    (config.login_url.is_some() || !config.allowed_domains.is_empty()).then_some(config)
}

/// The stored secret a source is allowed to use, derived from its operator-
/// registered id rather than chosen by the adapter script. Without this a
/// rocketreach adapter could request the LinkedIn credential and have it typed
/// into rocketreach's login form.
///
/// Source ids are `brand.tld`, and the established secret names drop the tld
/// (`rocketreach.com` -> `ROCKETREACH_BROWSER_LOGIN`). Sources whose name is
/// not mechanically derivable — `dnbhoovers.com` stores `DNB_HOOVERS_...` —
/// carry an explicit `required_secret_name` in their compiled recipe, which
/// takes precedence over this fallback.
pub(super) fn derived_secret_name(provider: &str) -> String {
    let brand = provider.split('.').next().unwrap_or(provider);
    let name: String = brand
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .collect();
    format!("{name}_BROWSER_LOGIN")
}

/// A credential reference is only ever `ctox-secret://<scope>/<NAME>` — never
/// a raw value, never userinfo/query/fragment.
pub(super) fn valid_credential_reference(value: &str) -> bool {
    let prefix = format!("ctox-secret://{}/", crate::secrets::credential_scope());
    let Some(name) = value.strip_prefix(prefix.as_str()) else {
        return false;
    };
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
}

/// Resolve the protected-capture config for a target's `expected_provider`.
/// The compiled-in Rust `browser_recipe()` is authoritative when the source
/// module exists; otherwise the operator-registered target supplies the
/// boundary (start_url host) and the credential name. The adapter script may
/// only point at a login page, never widen the allow-list or choose the
/// secret. The result is validated: https login URL inside the trusted
/// allow-list, and a secret-store reference — never a value.
pub(super) fn resolve_protected_capture_config(
    target: &RegisteredTarget,
) -> Option<(String, ProtectedCaptureConfig)> {
    let provider = target
        .view
        .config
        .get("expected_provider")
        .and_then(Value::as_str)?
        .trim()
        .to_string();
    if provider.is_empty() {
        return None;
    }
    // Neither the boundary nor the credential may come from the adapter
    // script. Script bodies are hot-revisable — the repair pass writes them —
    // so a script that declared its own `allowed_domains` was certifying the
    // very boundary it had to stay inside: it could name any host, be
    // trivially "within" it, and pick which stored secret to send there.
    //
    // Both now come from sources the script cannot reach: the compiled-in
    // recipe, or else the operator-registered target (its start_url host, and
    // the secret name derived from the source id). The script may still point
    // at a specific login page, but that pointer is checked against the
    // trusted domains below.
    let recipe = ctox_web_stack::sources::find(&provider).and_then(|m| m.browser_recipe());
    let trusted_domains = match &recipe {
        Some(recipe) if !recipe.allowed_domains.is_empty() => recipe.allowed_domains.clone(),
        _ => url_host_lower(&target.view.start_url).into_iter().collect(),
    };
    if trusted_domains.is_empty() {
        return None;
    }
    let trusted_credential_ref = format!(
        "ctox-secret://{}/{}",
        crate::secrets::credential_scope(),
        recipe
            .as_ref()
            .and_then(|recipe| recipe.required_secret_name.map(str::to_string))
            .unwrap_or_else(|| derived_secret_name(&provider))
    );
    let script_login_url = fs::read_to_string(&target.script.script_path)
        .ok()
        .and_then(|body| protected_config_from_script(&body, &provider))
        .and_then(|script_config| script_config.login_url);
    let mut config = ProtectedCaptureConfig {
        login_url: recipe
            .as_ref()
            .map(|recipe| recipe.login_url.clone())
            .or(script_login_url),
        allowed_domains: trusted_domains,
        credential_ref: Some(trusted_credential_ref),
    };
    let login_url = config.login_url.clone()?;
    if !login_url.starts_with("https://") {
        return None;
    }
    if let Some(host) = url_host_lower(&login_url) {
        if !config.allowed_domains.is_empty()
            && !host_within_domains(&host, &config.allowed_domains)
        {
            return None;
        }
    }
    if let Some(reference) = &config.credential_ref {
        if !valid_credential_reference(reference) {
            config.credential_ref = None;
        }
    }
    Some((provider, config))
}

/// True when `final_url` is the source's own login page: same host as the
/// configured login URL with a matching (or root) login path, or a URL on an
/// allowed domain whose host/path is unambiguously a login endpoint
/// (`login.<domain>`, `/login`, `/sign/in`, `/anmeldung`, ...).
pub(super) fn url_is_login_landing(
    final_url: &str,
    login_url: &str,
    allowed_domains: &[String],
) -> bool {
    let Some(host) = url_host_lower(final_url) else {
        return false;
    };
    let path = url::Url::parse(final_url)
        .map(|parsed| parsed.path().to_ascii_lowercase())
        .unwrap_or_default();
    let login_host = url_host_lower(login_url);
    let login_path = url::Url::parse(login_url)
        .map(|parsed| parsed.path().trim_end_matches('/').to_ascii_lowercase())
        .unwrap_or_default();
    if login_host.as_deref() == Some(host.as_str())
        && (login_path.is_empty() || path.starts_with(login_path.as_str()))
    {
        return true;
    }
    host_within_domains(&host, allowed_domains)
        && (host.starts_with("login.")
            || path.contains("login")
            || path.contains("sign/in")
            || path.contains("sign-in")
            || path.contains("anmeld"))
}

/// Build the typed reauthorization action for a run when the evidence shows
/// an expired/invalid session on a credential-protected source: either the
/// adapter script classified `authorization_required` explicitly, or the
/// portal probe landed on the source's own login page after a derived empty
/// result. Explicit adapter diagnostics, failed commands and missing pages are
/// never reclassified as session expiry. Returns the action payload to persist
/// and to hand off.
pub(super) fn session_expiry_reauthorization(
    target: &RegisteredTarget,
    probe: &ProbeResult,
    payload: &Value,
    classification: &Classification,
) -> Option<Value> {
    // Expired subscriptions and disabled provider accounts need account repair,
    // not a fresh browser login. Keep the classifier's account verdict intact.
    if classification.status == ScrapeRunStatus::ProviderAccountInactive {
        return None;
    }
    let explicit = payload
        .get("failure_mode")
        .and_then(Value::as_str)
        .map(str::trim)
        == Some("authorization_required");
    let no_reported_failure = match payload.get("failure_mode") {
        None | Some(Value::Null) => true,
        Some(Value::String(mode)) => mode.trim().is_empty(),
        _ => false,
    };
    let login_landing_upgrade = no_reported_failure
        && classification.status == ScrapeRunStatus::PortalDrift
        && classification.reason == "empty_record_set_on_reachable_portal"
        && probe.reachable
        && !probe.human_verification;
    if !explicit && !login_landing_upgrade {
        return None;
    }
    let (provider, config) = resolve_protected_capture_config(target)?;
    let login_url = config.login_url.clone()?;
    if !explicit && !url_is_login_landing(&probe.final_url, &login_url, &config.allowed_domains) {
        return None;
    }
    Some(json!({
        "kind": "auth-assist-request",
        "source_id": provider,
        "login_url": login_url,
        "allowed_domains": config.allowed_domains,
        "credential_ref": config.credential_ref,
        "reason": "session_expired_or_invalid",
        "secret_value_in_payload": false,
    }))
}

/// Emit the typed `auth-assist-request` handoff through the existing
/// Business OS web-stack mechanism (the same enqueue the adapter scripts and
/// the `ctox business-os web-stack auth-assist-request` CLI use). Lossy by
/// design: a handoff failure is reported in the run error, never fatal.
/// How long a failed automatic sign-in blocks the next attempt for the same
/// source. A wrong or locked credential must not be retried by every research
/// agent that hits the login wall.
const AUTO_REAUTH_FAILURE_COOLDOWN_MS: i64 = 30 * 60 * 1000;
const AUTO_REAUTH_SUCCESS_COOLDOWN_MS: i64 = 15 * 60 * 1000;
const AUTO_REAUTH_TIMEOUT_MS: u64 = 240_000;

fn auto_reauth_state_path(root: &Path, source_id: &str) -> std::path::PathBuf {
    let safe: String = source_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    root.join("runtime/scraping/auto-reauth")
        .join(format!("{safe}.json"))
}

/// Whether an earlier failed attempt still blocks a new one.
pub(super) fn auto_reauth_in_cooldown(state: Option<&Value>, now_ms: i64) -> bool {
    let Some(state) = state else { return false };
    // A sign-in that reported success but left the run behind the login wall
    // must not repeat on every run: D&B saw eleven stored-credential sign-ins
    // in one hour (07.10.2026), which risks locking the account.
    let cooldown_ms = if state.get("ok").and_then(Value::as_bool) == Some(true) {
        AUTO_REAUTH_SUCCESS_COOLDOWN_MS
    } else {
        AUTO_REAUTH_FAILURE_COOLDOWN_MS
    };
    state
        .get("attempted_at_ms")
        .and_then(Value::as_i64)
        .is_some_and(|at| now_ms.saturating_sub(at) < cooldown_ms)
}

/// Try CTOX's own stored-credential sign-in before handing the login wall to
/// a human. Research agents saw `authorization_required` with a usable
/// `credential_ref` and skipped the source instead of signing in (D&B Hoovers:
/// 16 of 16 runs on 06.10.2026, zero sign-in attempts). One attempt at a time
/// per source across processes (file lock); a failure cools down for 30 min.
/// Returns the sign-in result when an attempt ran.
pub(super) fn attempt_automatic_reauthorization(
    root: &Path,
    run_id: &str,
    thread_key: Option<&str>,
    owner_user_id: Option<&str>,
    reauthorization: &Value,
) -> Option<Value> {
    let source_id = reauthorization.get("source_id")?.as_str()?.to_string();
    let login_url = reauthorization.get("login_url")?.as_str()?.to_string();
    let credential_ref = reauthorization
        .get("credential_ref")
        .and_then(Value::as_str)
        .filter(|value| value.starts_with("ctox-secret://"))?
        .to_string();
    let state_path = auto_reauth_state_path(root, &source_id);
    fs::create_dir_all(state_path.parent()?).ok()?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(state_path.with_extension("lock"))
        .ok()?;
    // Another process is signing in to this source right now: let it finish
    // and leave this run's result to the normal handoff path.
    if lock.try_lock().is_err() {
        return Some(
            json!({"ok": false, "skipped": "sign_in_in_progress", "source_id": source_id}),
        );
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default();
    let previous = fs::read(&state_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    if auto_reauth_in_cooldown(previous.as_ref(), now_ms) {
        return Some(json!({"ok": false, "skipped": "cooldown", "source_id": source_id}));
    }
    let task_id = thread_key.unwrap_or(run_id).to_string();
    let outcome = crate::service::business_os::auto_reauthorize_web_stack_source(
        root,
        &source_id,
        &credential_ref,
        &login_url,
        &task_id,
        owner_user_id,
        AUTO_REAUTH_TIMEOUT_MS,
    );
    let (ok, detail) = match &outcome {
        Ok(value) => (
            value.get("ok").and_then(Value::as_bool) == Some(true),
            value.get("status").cloned().unwrap_or(Value::Null),
        ),
        Err(err) => (
            false,
            json!(format!("{err:#}").chars().take(300).collect::<String>()),
        ),
    };
    let state = json!({
        "source_id": source_id,
        "ok": ok,
        "attempted_at_ms": now_ms,
        "run_id": run_id,
        "detail": detail,
        "secret_value_in_payload": false,
    });
    let _ = fs::write(
        &state_path,
        serde_json::to_vec_pretty(&state).unwrap_or_default(),
    );
    eprintln!("scrape execute: automatic sign-in for {source_id}: ok={ok}");
    Some(state)
}

pub(super) fn emit_reauthorization_handoff(
    root: &Path,
    run_id: &str,
    thread_key: Option<&str>,
    owner_user_id: Option<&str>,
    reauthorization: &Value,
) -> Option<Value> {
    let source_id = reauthorization.get("source_id")?.as_str()?.to_string();
    let login_url = reauthorization.get("login_url")?.as_str()?.to_string();
    let credential_ref = reauthorization
        .get("credential_ref")
        .and_then(Value::as_str)
        .map(str::to_string);
    let task_id = thread_key.unwrap_or(run_id).to_string();
    match crate::service::business_os::enqueue_web_stack_auth_assist_request(
        root,
        &source_id,
        Some(&login_url),
        credential_ref.as_deref(),
        Some("stored session expired or invalid; reauthorization required"),
        Some(reauthorization),
        &task_id,
        "scrape_executor",
        "ctox scrape execute",
        owner_user_id,
        !task_id.trim().is_empty(),
        true,
    ) {
        Ok(mut value) => {
            if let Some(object) = value.as_object_mut() {
                object.insert("kind".to_string(), json!("auth-assist-request"));
                object.insert("run_id".to_string(), json!(run_id));
            }
            Some(value)
        }
        Err(err) => {
            eprintln!("scrape execute: auth-assist handoff failed: {err:#}");
            None
        }
    }
}
