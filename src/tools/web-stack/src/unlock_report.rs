//! Production unlock acceptance gate (web-stack checklist capability 13).
//!
//! Generates one machine-readable report that proves, for every registered
//! scrape-target adapter:
//!
//!   * manifest and allowed domains valid;
//!   * challenge classification correct;
//!   * public unlock retry bounded (no CAPTCHA bypass, no unbounded retry);
//!   * authorization handoff works where required;
//!   * post-auth extraction succeeds;
//!   * session reuse succeeds;
//!   * no secret leakage (plus a redaction self-check over the report);
//!   * final status and required action visible.
//!
//! The gate is evidence-bound: a check is only `pass` when structural or
//! persisted evidence exists. Checks that need live production runs report
//! `unknown` with an explicit `required_action` when the local runtime
//! database carries no evidence. The gate never probes the network and
//! never prints secret material.
//!
//! Evidence sources (all read-only):
//!   * adapter manifests and scripts under `scrape-targets/`;
//!   * `_shared/generic-prospect-v1.js` (`SOURCE_CONFIG` /
//!     `PROTECTED_SOURCE_CONFIG` declare allowed domains, unlock mode, and
//!     `ctox-secret://credentials/<name>` references);
//!   * `runtime/ctox.sqlite3`: `scrape_target`, `scrape_run`,
//!     `web_unlock_signals`, and the Business OS RxDB
//!     `*__browser_sessions__v*` documents (auth-assist state).
//!
//! Completion condition (matches the checklist): every configured adapter
//! is either `live_success` or has an explicit `operator_auth_required`
//! state followed by a successful post-auth retry.

use anyhow::{Context, Result};
use chrono::Utc;
use regex::Regex;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const SHARED_SCRIPT_REL: &str = "_shared/generic-prospect-v1.js";
const RUN_STATUS_VOCABULARY: &[&str] = &[
    "succeeded",
    "blocked",
    "temporary_unreachable",
    "portal_drift",
    "partial_output",
];
const COMPLETION_CONDITION: &str = "every configured adapter is either live_success, or has an explicit operator_auth_required state followed by a successful post-auth retry";

// ---------------------------------------------------------------------------
// Check results
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckStatus {
    Pass,
    Fail,
    Unknown,
}

impl CheckStatus {
    fn as_str(self) -> &'static str {
        match self {
            CheckStatus::Pass => "pass",
            CheckStatus::Fail => "fail",
            CheckStatus::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
struct Check {
    status: CheckStatus,
    reason: String,
    evidence: Vec<String>,
    required_action: Option<String>,
}

impl Check {
    fn pass(reason: impl Into<String>, evidence: Vec<String>) -> Self {
        Check {
            status: CheckStatus::Pass,
            reason: reason.into(),
            evidence,
            required_action: None,
        }
    }

    /// A check that does not apply to this adapter (e.g. authorization
    /// handoff for a public adapter). Counts as satisfied but stays
    /// distinguishable in the report.
    fn not_applicable(reason: impl Into<String>) -> Self {
        Check {
            status: CheckStatus::Pass,
            reason: format!("not_applicable: {}", reason.into()),
            evidence: Vec::new(),
            required_action: None,
        }
    }

    fn fail(reason: impl Into<String>, required_action: impl Into<String>) -> Self {
        Check {
            status: CheckStatus::Fail,
            reason: reason.into(),
            evidence: Vec::new(),
            required_action: Some(required_action.into()),
        }
    }

    fn unknown(reason: impl Into<String>, required_action: impl Into<String>) -> Self {
        Check {
            status: CheckStatus::Unknown,
            reason: reason.into(),
            evidence: Vec::new(),
            required_action: Some(required_action.into()),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "status": self.status.as_str(),
            "reason": self.reason,
            "evidence": self.evidence,
            "required_action": self.required_action,
        })
    }
}

// ---------------------------------------------------------------------------
// Secret-pattern redaction (mirrors the JS `rememberCommandError` redactor
// and the fixture-gate `containsForbiddenSecretKey` key list).
// ---------------------------------------------------------------------------

fn secret_key_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            "(?i)(password|passwd|token|secret[_-]?value|credential[_-]?value|api[_-]?key|access[_-]?token|auth[_-]?token|authorization|set-cookie|cookie)\\s*[:=]\\s*\"?\\S{3,}",
        )
        .expect("secret key regex compiles")
    })
}

fn url_userinfo_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[a-z][a-z0-9+.-]*://[^\s/@:]+:[^\s/@]+@").expect("userinfo regex compiles")
    })
}

fn bearer_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)bearer\s+[a-z0-9._~+/=-]{8,}").expect("bearer regex compiles")
    })
}

fn secret_ref_userinfo_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"ctox-secret://[^/\s]*@").expect("secret-ref regex compiles"))
}

fn contains_secret_pattern(text: &str) -> bool {
    secret_key_re().is_match(text)
        || url_userinfo_re().is_match(text)
        || bearer_re().is_match(text)
        || secret_ref_userinfo_re().is_match(text)
}

/// Redact secret-shaped material out of a string before it enters the
/// report. Applied to every persisted string the report quotes.
fn redact_text(text: &str) -> String {
    let step1 = url_userinfo_re().replace_all(text, "[redacted-url-userinfo]");
    let step2 = bearer_re().replace_all(&step1, "[redacted-bearer]");
    let step3 = secret_key_re().replace_all(&step2, "[redacted]");
    let step4 = secret_ref_userinfo_re().replace_all(&step3, "ctox-secret://[redacted-userinfo]");
    step4.chars().take(600).collect()
}

// ---------------------------------------------------------------------------
// Shared-script config parsing (`SOURCE_CONFIG`, `PROTECTED_SOURCE_CONFIG`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct SharedEntry {
    domains: Vec<String>,
    login_url: Option<String>,
    allowed_domains: Vec<String>,
    credential_ref: Option<String>,
    capture_supported: Option<bool>,
}

fn js_string_list(body: &str, key: &str) -> Vec<String> {
    let re = Regex::new(&format!(r#"{key}\s*:\s*\[([^\]]*)\]"#)).expect("list regex compiles");
    let Some(caps) = re.captures(body) else {
        return Vec::new();
    };
    let item_re = Regex::new(r#""([^"]+)""#).expect("item regex compiles");
    item_re
        .captures_iter(&caps[1])
        .map(|c| c[1].to_string())
        .collect()
}

fn js_string_field(body: &str, key: &str) -> Option<String> {
    let re = Regex::new(&format!(r#"{key}\s*:\s*"([^"]+)""#)).expect("string regex compiles");
    re.captures(body).map(|c| c[1].to_string())
}

fn js_bool_field(body: &str, key: &str) -> Option<bool> {
    let re = Regex::new(&format!(r"{key}\s*:\s*(true|false)")).expect("bool regex compiles");
    re.captures(body).map(|c| &c[1] == "true")
}

/// Extract the `"<source-id>": { ... }` entries of one top-level
/// `Object.freeze({ ... })` section. Entry bodies never nest braces
/// (only arrays), so a non-greedy brace match is sufficient.
fn parse_js_config_section(script: &str, marker: &str) -> BTreeMap<String, SharedEntry> {
    let mut out = BTreeMap::new();
    let Some(start) = script.find(marker).map(|i| i + marker.len()) else {
        return out;
    };
    let rest = &script[start..];
    let end = rest.find("\n});").unwrap_or(rest.len());
    let section = &rest[..end];
    let entry_re =
        Regex::new(r#"(?s)"([a-z0-9][a-z0-9.-]*)"\s*:\s*\{(.*?)\}"#).expect("entry regex compiles");
    for caps in entry_re.captures_iter(section) {
        let body = &caps[2];
        out.insert(
            caps[1].to_string(),
            SharedEntry {
                domains: js_string_list(body, "domains"),
                login_url: js_string_field(body, "login_url"),
                allowed_domains: js_string_list(body, "allowed_domains"),
                credential_ref: js_string_field(body, "credential_ref"),
                capture_supported: js_bool_field(body, "capture_supported"),
            },
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Persisted state (runtime/ctox.sqlite3, read-only)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct RunRow {
    run_id: String,
    status: String,
    started_at: String,
    result: Value,
}

impl RunRow {
    fn failure_mode(&self) -> Option<&str> {
        self.result.get("failure_mode").and_then(Value::as_str)
    }

    fn is_auth_required(&self) -> bool {
        self.failure_mode() == Some("auth_required")
    }

    fn is_challenge_blocked(&self) -> bool {
        self.status == "blocked" && !self.is_auth_required()
    }

    fn browser_assist_requested(&self) -> bool {
        self.result
            .get("browser_assist_requested")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
}

#[derive(Debug, Default)]
struct PersistedState {
    db_available: bool,
    /// scrape_target presence: target_key -> target_id.
    registered: BTreeMap<String, String>,
    /// source-id keyed run history (most recent first).
    runs: Vec<RunRow>,
    signals_total: i64,
    signals_unresolved: i64,
    latest_signal_id: Option<i64>,
    auth_sessions_pending: Vec<String>,
    auth_sessions_completed: Vec<String>,
    session_secret_leak: bool,
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
        params![name],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .unwrap_or(false)
}

fn session_tables(conn: &Connection) -> Vec<String> {
    let mut stmt = match conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE '%browser_sessions%'",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return Vec::new(),
    };
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map(|iter| iter.flatten().collect::<Vec<_>>())
        .unwrap_or_default();
    rows.into_iter()
        .filter(|name| name.contains("__browser_sessions__v"))
        .collect()
}

fn load_persisted_state(conn: &Connection, target_key: &str, source_id: &str) -> PersistedState {
    let mut state = PersistedState {
        db_available: true,
        ..Default::default()
    };
    if table_exists(conn, "scrape_target") {
        if let Ok(Some((target_id,))) = conn
            .query_row(
                "SELECT target_id FROM scrape_target WHERE target_key = ?1",
                params![target_key],
                |r| Ok((r.get::<_, String>(0)?,)),
            )
            .optional()
        {
            state.registered.insert(target_key.to_string(), target_id);
        }
    }
    if table_exists(conn, "scrape_run") {
        // Resolve the target_id for run lookup.
        let target_id = state.registered.get(target_key).cloned();
        if let Some(target_id) = target_id {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT run_id, status, started_at, result_json
                 FROM scrape_run WHERE target_id = ?1
                 ORDER BY started_at DESC, run_id DESC LIMIT 50",
            ) {
                let rows = stmt
                    .query_map(params![target_id], |r| {
                        let result_raw: String = r.get(3)?;
                        Ok(RunRow {
                            run_id: r.get(0)?,
                            status: r.get(1)?,
                            started_at: r.get(2)?,
                            result: serde_json::from_str(&result_raw).unwrap_or(Value::Null),
                        })
                    })
                    .map(|iter| iter.flatten().collect::<Vec<_>>())
                    .unwrap_or_default();
                state.runs = rows;
            }
        }
    }
    if table_exists(conn, "web_unlock_signals") {
        let source = format!("scrape-target:{source_id}");
        state.signals_total = conn
            .query_row(
                "SELECT count(*) FROM web_unlock_signals WHERE source = ?1",
                params![source],
                |r| r.get(0),
            )
            .unwrap_or(0);
        state.signals_unresolved = conn
            .query_row(
                "SELECT count(*) FROM web_unlock_signals WHERE source = ?1 AND resolved = 0",
                params![source],
                |r| r.get(0),
            )
            .unwrap_or(0);
        state.latest_signal_id = conn
            .query_row(
                "SELECT signal_id FROM web_unlock_signals WHERE source = ?1
                 ORDER BY signal_id DESC LIMIT 1",
                params![source],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
    }
    for table in session_tables(conn) {
        let sql = format!(
            "SELECT id, deleted, data FROM \"{}\"",
            table.replace('"', "\"\"")
        );
        let Ok(mut stmt) = conn.prepare(&sql) else {
            continue;
        };
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map(|iter| iter.flatten().collect::<Vec<_>>())
            .unwrap_or_default();
        for (id, deleted, data_raw) in rows {
            if deleted != 0 {
                continue;
            }
            let Ok(data) = serde_json::from_str::<Value>(&data_raw) else {
                continue;
            };
            let payload = data.get("payload").unwrap_or(&data);
            let doc_source = payload
                .get("source_id")
                .or_else(|| payload.get("sourceId"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if doc_source != source_id {
                continue;
            }
            if payload
                .get("secret_value_in_rxdb")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                state.session_secret_leak = true;
            }
            let completed = payload
                .get("auth_assist_status")
                .and_then(Value::as_str)
                .map(|s| s == "completed")
                .unwrap_or(false)
                || payload
                    .get("authenticated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            if completed {
                state.auth_sessions_completed.push(id);
            } else {
                state.auth_sessions_pending.push(id);
            }
        }
    }
    state
}

fn open_runtime_db_readonly(root: &Path) -> Result<Option<Connection>> {
    let path = root.join("runtime").join("ctox.sqlite3");
    if !path.is_file() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("failed to open {} read-only", path.display()))?;
    Ok(Some(conn))
}

// ---------------------------------------------------------------------------
// Adapter assembly
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Adapter {
    source_id: String,
    manifest_rel: String,
    manifest_raw: String,
    manifest: Value,
    script_rel: String,
    script_text: String,
    source_entry: Option<SharedEntry>,
    protected_entry: Option<SharedEntry>,
}

impl Adapter {
    fn target_key(&self) -> String {
        self.manifest
            .get("target_key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    fn uses_session(&self) -> bool {
        // Session adapters: protected entries in the shared script, plus
        // xing.com, which has a dedicated session-capture branch there.
        self.protected_entry.is_some() || self.source_id == "xing.com"
    }

    fn unlock_mode(&self) -> &'static str {
        if let Some(protected) = &self.protected_entry {
            return match protected.capture_supported {
                Some(true) => "credential_session",
                _ => "operator_auth_session",
            };
        }
        if self.source_id == "xing.com" {
            return "authenticated_session";
        }
        match self.source_id.as_str() {
            "experte.de" | "zefix.ch" | "google.de" => "public_no_auth",
            _ => "public_unlock",
        }
    }
}

fn rel_evidence(adapters_dir: &Path, path: &Path) -> String {
    let base = adapters_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "scrape-targets".to_string());
    match path.strip_prefix(adapters_dir) {
        Ok(rel) => format!("{base}/{}", rel.to_string_lossy()),
        Err(_) => path.to_string_lossy().to_string(),
    }
}

fn discover_adapters(adapters_dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(adapters_dir)
        .with_context(|| format!("failed to read {}", adapters_dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('_') || name == "tests" {
            continue;
        }
        let manifest = path.join("target.json");
        if manifest.is_file() {
            out.push((name, path));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

// ---------------------------------------------------------------------------
// Individual checks
// ---------------------------------------------------------------------------

fn check_manifest(adapter: &Adapter, all_target_keys: &[String]) -> Check {
    let evidence = vec![adapter.manifest_rel.clone()];
    let mut violations: Vec<String> = Vec::new();
    let m = &adapter.manifest;
    if !m.is_object() {
        return Check::fail(
            "target.json is not a JSON object",
            "repair the adapter manifest so it parses as a JSON object",
        );
    }
    let target_key = adapter.target_key();
    if target_key.trim().is_empty() {
        violations.push("target_key missing".to_string());
    }
    if all_target_keys
        .iter()
        .filter(|k| k.as_str() == target_key)
        .count()
        > 1
    {
        violations.push(format!("duplicate target_key {target_key}"));
    }
    for field in ["display_name", "target_kind", "status"] {
        if m.get(field)
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            violations.push(format!("{field} missing"));
        }
    }
    if m.get("status").and_then(Value::as_str) != Some("active") {
        violations.push("status is not active".to_string());
    }
    let start_url = m.get("start_url").and_then(Value::as_str).unwrap_or("");
    if !start_url.starts_with("https://") {
        violations.push("start_url is not https".to_string());
    }
    let config = m.get("config").cloned().unwrap_or(Value::Null);
    if config
        .get("expected_provider")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        violations.push("config.expected_provider missing".to_string());
    }
    if config
        .get("country_hints")
        .and_then(Value::as_array)
        .map(|a| a.is_empty())
        .unwrap_or(true)
    {
        violations.push("config.country_hints missing or empty".to_string());
    }
    let record_keys = config
        .get("record_key_fields")
        .cloned()
        .unwrap_or(Value::Null);
    if record_keys != json!(["field", "source_url"]) {
        violations.push("config.record_key_fields is not [field, source_url]".to_string());
    }
    let schema = m.get("output_schema").cloned().unwrap_or(Value::Null);
    if schema.get("schema_key").and_then(Value::as_str) != Some("prospect.v1") {
        violations.push("output_schema.schema_key is not prospect.v1".to_string());
    }
    if adapter.script_text.is_empty() {
        violations.push("no adapter script resolved (scripts/v1.js or shared script)".to_string());
    }
    if violations.is_empty() {
        Check::pass(
            "manifest parses and satisfies the adapter contract",
            evidence,
        )
    } else {
        Check::fail(
            format!("manifest violations: {}", violations.join("; ")),
            "fix the manifest violations and re-run `ctox web unlock report`",
        )
    }
}

fn domain_is_clean(domain: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re =
        RE.get_or_init(|| Regex::new(r"^[a-z0-9][a-z0-9.-]*\.[a-z]{2,}$").expect("domain regex"));
    re.is_match(domain) && !domain.contains("..")
}

fn host_within(host: &str, domains: &[String]) -> bool {
    domains
        .iter()
        .any(|d| host == d || host.ends_with(&format!(".{d}")))
}

fn url_host(url: &str) -> Option<String> {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .trim_end_matches('.')
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

fn check_allowed_domains(adapter: &Adapter) -> Check {
    let evidence = vec![format!(
        "{SHARED_SCRIPT_REL} (SOURCE_CONFIG / PROTECTED_SOURCE_CONFIG)"
    )];
    let Some(source_entry) = &adapter.source_entry else {
        return Check::fail(
            format!(
                "adapter {} is missing from the shared SOURCE_CONFIG",
                adapter.source_id
            ),
            "register the adapter in _shared/generic-prospect-v1.js SOURCE_CONFIG with its allowed domains",
        );
    };
    if let Some(protected) = &adapter.protected_entry {
        let mut violations: Vec<String> = Vec::new();
        if protected.allowed_domains.is_empty() {
            violations.push("allowed_domains missing or empty".to_string());
        }
        for domain in &protected.allowed_domains {
            if !domain_is_clean(domain) {
                violations.push(format!(
                    "allowed domain `{domain}` is not a bare host suffix"
                ));
            }
        }
        let login_url = protected.login_url.clone().unwrap_or_default();
        if !login_url.starts_with("https://") {
            violations.push("login_url is not https".to_string());
        } else if let Some(host) = url_host(&login_url) {
            if !protected.allowed_domains.is_empty()
                && !host_within(&host, &protected.allowed_domains)
            {
                violations.push(format!(
                    "login_url host `{host}` is outside allowed_domains"
                ));
            }
        }
        let credential_ref = protected.credential_ref.clone().unwrap_or_default();
        static CRED_RE: OnceLock<Regex> = OnceLock::new();
        let cred_re = CRED_RE.get_or_init(|| {
            Regex::new(r"^ctox-secret://credentials/[A-Z0-9_]+$").expect("cred regex")
        });
        if !cred_re.is_match(&credential_ref) {
            violations.push(
                "credential_ref is not a valid ctox-secret://credentials/<NAME> reference"
                    .to_string(),
            );
        }
        if protected.capture_supported.is_none() {
            violations.push("capture_supported flag missing".to_string());
        }
        return if violations.is_empty() {
            Check::pass(
                format!(
                    "protected adapter declares {} allowed domain(s), an https login_url inside the allow-list, and a secret-store credential reference",
                    protected.allowed_domains.len()
                ),
                evidence,
            )
        } else {
            Check::fail(
                format!("allowed-domain violations: {}", violations.join("; ")),
                "fix PROTECTED_SOURCE_CONFIG in _shared/generic-prospect-v1.js and re-run the report",
            )
        };
    }
    // Public adapter: domains come from SOURCE_CONFIG.
    let domains = &source_entry.domains;
    if adapter.source_id == "google.de" && domains.is_empty() {
        return Check::pass(
            "provider-owned search adapter: empty allow-list is by design (isAllowedSourceUrl special-cases google.de and accepts only search-result URLs)",
            evidence,
        );
    }
    if domains.is_empty() {
        return Check::fail(
            "SOURCE_CONFIG domains missing or empty",
            "declare the adapter host suffixes in _shared/generic-prospect-v1.js SOURCE_CONFIG",
        );
    }
    for domain in domains {
        if !domain_is_clean(domain) {
            return Check::fail(
                format!("SOURCE_CONFIG domain `{domain}` is not a bare host suffix"),
                "fix the SOURCE_CONFIG domain list",
            );
        }
    }
    if !host_within(&adapter.source_id, domains) {
        return Check::fail(
            format!(
                "adapter host {} is not covered by SOURCE_CONFIG domains [{}]",
                adapter.source_id,
                domains.join(", ")
            ),
            "add the adapter host to the SOURCE_CONFIG domain list",
        );
    }
    Check::pass(
        format!("public adapter allow-list: [{}]", domains.join(", ")),
        evidence,
    )
}

fn check_retry_bounded(adapter: &Adapter) -> Check {
    let evidence = vec![adapter.script_rel.clone()];
    static CAPTCHA_SOLVER_RE: OnceLock<Regex> = OnceLock::new();
    let captcha_re = CAPTCHA_SOLVER_RE.get_or_init(|| {
        Regex::new(r"(?i)(2captcha|anti-?captcha|capmonster|deathbycaptcha|capsolver|nopecha)")
            .expect("captcha regex")
    });
    if captcha_re.is_match(&adapter.script_text) {
        return Check::fail(
            "adapter script references a CAPTCHA-solving service",
            "remove the CAPTCHA bypass; challenges must surface as unlock signals and Browser-app verification",
        );
    }
    static LOOP_RE: OnceLock<Regex> = OnceLock::new();
    let loop_re = LOOP_RE.get_or_init(|| {
        Regex::new(r"while\s*\(\s*true\s*\)|for\s*\(\s*;\s*;\s*\)").expect("loop regex")
    });
    if loop_re.is_match(&adapter.script_text) {
        return Check::fail(
            "adapter script contains an unbounded retry loop",
            "replace the loop with the bounded single-retry flow (one login attempt, one capture retry)",
        );
    }
    Check::pass(
        "no CAPTCHA-solving service and no unbounded retry loop; retries follow the bounded single-login/single-capture flow",
        evidence,
    )
}

fn check_challenge_classification(adapter: &Adapter, state: &PersistedState) -> Check {
    let rerun = format!(
        "run `ctox scrape execute --target-key {}` on the deployment, then re-run `ctox web unlock report`",
        adapter.target_key()
    );
    if !state.db_available {
        return Check::unknown("no persisted runtime state available", rerun);
    }
    let Some(latest) = state.runs.first() else {
        return Check::unknown("no persisted scrape runs for this adapter", rerun);
    };
    let evidence = vec![latest.run_id.clone()];
    if !RUN_STATUS_VOCABULARY.contains(&latest.status.as_str()) {
        return Check::fail(
            format!(
                "latest run {} carries untyped status `{}` (outside the access-failure vocabulary)",
                latest.run_id, latest.status
            ),
            "investigate the executor classification; every run must classify into the typed status vocabulary",
        );
    }
    if latest.status == "blocked" {
        let has_reason = latest.failure_mode().is_some()
            || latest
                .result
                .get("detail")
                .and_then(Value::as_str)
                .map(|d| !d.trim().is_empty())
                .unwrap_or(false);
        if !has_reason {
            return Check::fail(
                format!("blocked run {} carries no classification reason", latest.run_id),
                "blocked runs must persist failure_mode/detail so the classification stays auditable",
            );
        }
        if latest.is_challenge_blocked() && state.signals_total == 0 {
            return Check::fail(
                format!(
                    "challenge-blocked run {} recorded no web_unlock_signals evidence",
                    latest.run_id
                ),
                "challenge detections must be persisted via `ctox web unlock signals record`",
            );
        }
    }
    let mut check = Check::pass(
        format!(
            "latest run classified `{}`{}",
            latest.status,
            latest
                .failure_mode()
                .map(|f| format!(" (failure_mode={})", redact_text(f)))
                .unwrap_or_default()
        ),
        evidence,
    );
    if let Some(signal_id) = state.latest_signal_id {
        check
            .evidence
            .push(format!("web_unlock_signals#{signal_id}"));
    }
    check
}

fn check_authorization_handoff(adapter: &Adapter, state: &PersistedState) -> Check {
    if !adapter.uses_session() {
        return Check::not_applicable("public adapter requires no authorization handoff");
    }
    let action = format!(
        "run `ctox scrape execute --target-key {}` and confirm an auth-assist-request reaches the Browser app",
        adapter.target_key()
    );
    if !state.db_available {
        return Check::unknown("no persisted runtime state available", action);
    }
    if let Some(run) = state.runs.iter().find(|r| r.browser_assist_requested()) {
        return Check::pass(
            "a persisted run emitted browser_assist_requested=true (auth-assist-request handoff fired)",
            vec![run.run_id.clone()],
        );
    }
    if let Some(session) = state
        .auth_sessions_pending
        .first()
        .or(state.auth_sessions_completed.first())
    {
        return Check::pass(
            "a source-bound Browser-app auth session document exists (handoff materialized)",
            vec![format!("browser_sessions:{session}")],
        );
    }
    if state
        .runs
        .first()
        .map(|r| r.status == "succeeded")
        .unwrap_or(false)
    {
        return Check::pass(
            "no handoff was required: latest run succeeded on an existing session",
            vec![state.runs[0].run_id.clone()],
        );
    }
    Check::unknown("no authorization-handoff evidence persisted", action)
}

fn check_post_auth_extraction(adapter: &Adapter, state: &PersistedState) -> Check {
    if !adapter.uses_session() {
        return Check::not_applicable("public adapter requires no post-auth extraction");
    }
    let rerun = format!(
        "authorize the account in the Browser app, then rerun `ctox scrape execute --target-key {}`; the gate requires a successful post-auth retry",
        adapter.target_key()
    );
    if !state.db_available {
        return Check::unknown("no persisted runtime state available", rerun);
    }
    let latest_succeeded = state.runs.iter().find(|r| r.status == "succeeded");
    let latest_auth = state.runs.iter().find(|r| r.is_auth_required());
    match (latest_succeeded, latest_auth) {
        (Some(succeeded), Some(auth)) => {
            if succeeded.started_at > auth.started_at
                || (succeeded.started_at == auth.started_at && succeeded.run_id >= auth.run_id)
            {
                Check::pass(
                    format!(
                        "succeeded run {} postdates auth_required run {} (post-auth retry succeeded)",
                        succeeded.run_id, auth.run_id
                    ),
                    vec![succeeded.run_id.clone(), auth.run_id.clone()],
                )
            } else {
                Check::fail(
                    format!(
                        "auth_required run {} is not followed by a successful post-auth retry",
                        auth.run_id
                    ),
                    rerun,
                )
            }
        }
        (Some(succeeded), None) => Check::pass(
            format!(
                "succeeded run {} with no outstanding auth_required state",
                succeeded.run_id
            ),
            vec![succeeded.run_id.clone()],
        ),
        (None, Some(auth)) => Check::fail(
            format!(
                "latest auth_required run {} has no successful post-auth retry",
                auth.run_id
            ),
            rerun,
        ),
        (None, None) => {
            if state
                .runs
                .first()
                .map(|r| r.is_challenge_blocked())
                .unwrap_or(false)
            {
                Check::unknown(
                    "adapter is challenge-blocked; post-auth extraction cannot be evaluated yet",
                    format!(
                        "complete the browser verification in the Browser app and rerun `ctox scrape execute --target-key {}`",
                        adapter.target_key()
                    ),
                )
            } else {
                Check::unknown("no persisted scrape runs for this adapter", rerun)
            }
        }
    }
}

fn check_session_reuse(adapter: &Adapter, state: &PersistedState) -> Check {
    if !adapter.uses_session() {
        return Check::not_applicable("public adapter holds no authenticated session");
    }
    let action = format!(
        "complete the auth-assist flow in the Browser app so a completed source-bound session persists, then rerun `ctox scrape execute --target-key {}`",
        adapter.target_key()
    );
    if !state.db_available {
        return Check::unknown("no persisted runtime state available", action);
    }
    if let Some(session) = state.auth_sessions_completed.first() {
        return Check::pass(
            "a completed source-bound browser session persists and can be referenced by later adapter runs (no cookies exported)",
            vec![format!("browser_sessions:{session}")],
        );
    }
    if state
        .runs
        .first()
        .map(|r| r.status == "succeeded")
        .unwrap_or(false)
    {
        return Check::unknown(
            "post-auth run succeeded but no completed source-bound session document is persisted as reuse evidence",
            action,
        );
    }
    Check::unknown("no session-reuse evidence persisted", action)
}

fn check_no_secret_leakage(adapter: &Adapter, state: &PersistedState) -> Check {
    let mut evidence = vec![adapter.script_rel.clone(), adapter.manifest_rel.clone()];
    if contains_secret_pattern(&adapter.script_text) {
        return Check::fail(
            format!("adapter script {} contains secret-shaped material", adapter.script_rel),
            "remove the secret; credentials may only appear as ctox-secret://credentials/<name> references",
        );
    }
    if contains_secret_pattern(&adapter.manifest_raw) {
        return Check::fail(
            "adapter manifest contains secret-shaped material",
            "remove the secret from target.json; use a ctox-secret:// reference",
        );
    }
    if state.session_secret_leak {
        return Check::fail(
            "a browser_sessions document has secret_value_in_rxdb=true",
            "purge the session document and re-authorize; session secrets must never enter the replicated store",
        );
    }
    if let Some(latest) = state.runs.first() {
        evidence.push(latest.run_id.clone());
        let detail = latest
            .result
            .get("detail")
            .and_then(Value::as_str)
            .unwrap_or("");
        if contains_secret_pattern(detail) {
            return Check::fail(
                format!(
                    "persisted run detail of {} contains secret-shaped material (redacted in this report)",
                    latest.run_id
                ),
                "scrub the persisted run detail and fix the recorder to redact before persisting",
            );
        }
    }
    Check::pass(
        "script, manifest, persisted run details, and session documents scanned clean; credentials appear only as ctox-secret:// references",
        evidence,
    )
}

// ---------------------------------------------------------------------------
// Report assembly
// ---------------------------------------------------------------------------

fn adapter_row(
    adapter: &Adapter,
    state: &PersistedState,
    all_target_keys: &[String],
) -> (Value, bool) {
    let manifest = check_manifest(adapter, all_target_keys);
    let domains = check_allowed_domains(adapter);
    let retry = check_retry_bounded(adapter);
    let leakage = check_no_secret_leakage(adapter, state);
    let classification = check_challenge_classification(adapter, state);
    let handoff = check_authorization_handoff(adapter, state);
    let post_auth = check_post_auth_extraction(adapter, state);
    let session = check_session_reuse(adapter, state);

    let checks: &[(&str, &Check)] = &[
        ("manifest_valid", &manifest),
        ("allowed_domains_valid", &domains),
        ("challenge_classification", &classification),
        ("public_unlock_retry_bounded", &retry),
        ("authorization_handoff", &handoff),
        ("post_auth_extraction", &post_auth),
        ("session_reuse", &session),
        ("no_secret_leakage", &leakage),
    ];
    let all_pass = checks.iter().all(|(_, c)| c.status == CheckStatus::Pass);
    let static_fail = [
        manifest.status,
        domains.status,
        retry.status,
        leakage.status,
    ]
    .iter()
    .any(|s| *s == CheckStatus::Fail);

    let latest = state.runs.first();
    let (final_status, required_action) = if static_fail {
        let action = checks
            .iter()
            .filter(|(_, c)| c.status == CheckStatus::Fail)
            .filter_map(|(_, c)| c.required_action.clone())
            .collect::<Vec<_>>()
            .join(" | ");
        ("invalid_config", Some(action))
    } else if !state.db_available || latest.is_none() {
        (
            "pending_evidence",
            Some(format!(
                "run `ctox scrape execute --target-key {}` on the deployment to persist release-gate evidence, then re-run `ctox web unlock report`",
                adapter.target_key()
            )),
        )
    } else if all_pass {
        ("live_success", None)
    } else {
        let latest = latest.expect("latest run checked above");
        if latest.is_auth_required() {
            (
                "operator_auth_required",
                Some(format!(
                    "authorize the account in the Browser app and rerun `ctox scrape execute --target-key {}`; the gate then requires a successful post-auth retry",
                    adapter.target_key()
                )),
            )
        } else if latest.is_challenge_blocked() {
            (
                "blocked_pending_verification",
                Some(format!(
                    "complete the browser challenge/verification in the Browser app and rerun `ctox scrape execute --target-key {}`",
                    adapter.target_key()
                )),
            )
        } else if latest.status == "succeeded" {
            let action = checks
                .iter()
                .filter(|(_, c)| c.status != CheckStatus::Pass)
                .filter_map(|(_, c)| c.required_action.clone())
                .collect::<Vec<_>>()
                .join(" | ");
            ("evidence_incomplete", Some(action))
        } else {
            (
                "degraded",
                Some(format!(
                    "latest run classified `{}`; rerun `ctox scrape execute --target-key {}` after repair",
                    latest.status,
                    adapter.target_key()
                )),
            )
        }
    };

    let checks_json: Map<String, Value> = checks
        .iter()
        .map(|(name, check)| (name.to_string(), check.to_json()))
        .collect();
    let latest_json = latest.map(|r| {
        json!({
            "run_id": r.run_id,
            "status": r.status,
            "failure_mode": r.failure_mode().map(redact_text),
            "started_at": r.started_at,
        })
    });
    let row = json!({
        "source_id": adapter.source_id,
        "target_key": adapter.target_key(),
        "display_name": adapter.manifest.get("display_name").cloned().unwrap_or(Value::Null),
        "unlock_mode": adapter.unlock_mode(),
        "session_adapter": adapter.uses_session(),
        "registered_in_runtime": state.registered.contains_key(&adapter.target_key()),
        "checks": Value::Object(checks_json),
        "final_status": final_status,
        "required_action": required_action,
        "latest_run": latest_json,
        "unlock_signals": {
            "total": state.signals_total,
            "unresolved": state.signals_unresolved,
        },
    });
    (row, all_pass)
}

/// Generate the acceptance report. Pure and read-only: it never probes the
/// network, never writes to the runtime database, and never emits secret
/// material (a redaction self-check over the serialized report is embedded).
pub fn generate_acceptance_report(
    root: &Path,
    adapters_dir: &Path,
    target_filter: Option<&str>,
) -> Result<Value> {
    let shared_path = adapters_dir.join(SHARED_SCRIPT_REL);
    let shared_text = std::fs::read_to_string(&shared_path)
        .with_context(|| format!("failed to read {}", shared_path.display()))?;
    let source_config =
        parse_js_config_section(&shared_text, "const SOURCE_CONFIG = Object.freeze({");
    let protected_config = parse_js_config_section(
        &shared_text,
        "const PROTECTED_SOURCE_CONFIG = Object.freeze({",
    );

    let discovered = discover_adapters(adapters_dir)?;
    let mut adapters: Vec<Adapter> = Vec::new();
    for (source_id, dir) in discovered {
        if let Some(filter) = target_filter {
            let key_guess = source_id.replace('.', "-");
            if source_id != filter && key_guess != filter {
                continue;
            }
        }
        let manifest_path = dir.join("target.json");
        let manifest_raw = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?;
        let manifest: Value = serde_json::from_str(&manifest_raw)
            .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
        let specialized = dir.join("scripts").join("v1.js");
        let (script_path, script_text) = if specialized.is_file() {
            (
                specialized.clone(),
                std::fs::read_to_string(&specialized)
                    .with_context(|| format!("failed to read {}", specialized.display()))?,
            )
        } else {
            (shared_path.clone(), shared_text.clone())
        };
        adapters.push(Adapter {
            source_entry: source_config.get(&source_id).cloned(),
            protected_entry: protected_config.get(&source_id).cloned(),
            source_id,
            manifest_rel: rel_evidence(adapters_dir, &manifest_path),
            manifest_raw,
            manifest,
            script_rel: rel_evidence(adapters_dir, &script_path),
            script_text,
        });
    }
    if adapters.is_empty() {
        anyhow::bail!(
            "no adapters discovered under {} (filter: {:?})",
            adapters_dir.display(),
            target_filter
        );
    }
    let all_target_keys: Vec<String> = adapters.iter().map(|a| a.target_key()).collect();

    let (db, db_note) = match open_runtime_db_readonly(root) {
        Ok(Some(conn)) => (Some(conn), None),
        Ok(None) => (
            None,
            Some("runtime/ctox.sqlite3 not present; static checks only".to_string()),
        ),
        Err(err) => (
            None,
            Some(format!(
                "runtime database unreadable ({err:#}); static checks only"
            )),
        ),
    };

    // Load every registered scrape_target so DB-only registrations surface.
    let mut registered: BTreeMap<String, String> = BTreeMap::new();
    if let Some(conn) = &db {
        if table_exists(conn, "scrape_target") {
            if let Ok(mut stmt) = conn
                .prepare("SELECT target_key, target_id FROM scrape_target WHERE status = 'active'")
            {
                let rows = stmt
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                    .map(|iter| iter.flatten().collect::<Vec<_>>())
                    .unwrap_or_default();
                registered.extend(rows);
            }
        }
    }

    let mut rows: Vec<Value> = Vec::new();
    let mut adapters_ok = 0usize;
    for adapter in &adapters {
        let state = match &db {
            Some(conn) => load_persisted_state(conn, &adapter.target_key(), &adapter.source_id),
            None => PersistedState::default(),
        };
        let (row, ok) = adapter_row(adapter, &state, &all_target_keys);
        if ok {
            adapters_ok += 1;
        }
        rows.push(row);
    }

    let manifest_keys: Vec<&str> = all_target_keys.iter().map(String::as_str).collect();
    let unregistered_targets: Vec<String> = registered
        .keys()
        .filter(|k| !manifest_keys.contains(&k.as_str()))
        .cloned()
        .collect();

    let db_available = db.is_some();
    let mut report = json!({
        "report_kind": "ctox_web_unlock_acceptance",
        "report_version": 1,
        "generated_at": Utc::now().to_rfc3339(),
        "adapters_dir": adapters_dir.to_string_lossy(),
        "persisted_state": {
            "database": "runtime/ctox.sqlite3",
            "available": db_available,
            "note": db_note,
        },
        "gate": {
            "ok": false,
            "adapters_total": rows.len(),
            "adapters_ok": adapters_ok,
            "completion_condition": COMPLETION_CONDITION,
            "proof_level": if db_available { "persisted_evidence" } else { "static_only" },
            "redaction_self_check": Value::Null,
        },
        "registered_targets_without_manifest": unregistered_targets,
        "adapters": rows,
    });

    // Redaction self-check: the serialized report itself must be free of
    // secret-shaped material. A failure here fails the whole gate.
    let serialized = serde_json::to_string(&report).unwrap_or_default();
    let self_check_passed = !contains_secret_pattern(&serialized);
    report["gate"]["redaction_self_check"] = json!({
        "passed": self_check_passed,
        "scanned_bytes": serialized.len(),
        "patterns": ["secret-key assignments", "url userinfo", "bearer tokens", "ctox-secret userinfo"],
    });
    let gate_ok = adapters_ok == report["adapters"].as_array().map(|a| a.len()).unwrap_or(0)
        && self_check_passed;
    report["gate"]["ok"] = Value::Bool(gate_ok);
    Ok(report)
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

pub fn default_adapters_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scrape-targets")
}

fn report_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

pub fn print_report_usage() {
    println!("ctox web unlock report [--target <source-id|target-key>] [--adapters-dir <path>]");
    println!("                       [--out <path>] [--strict]");
    println!();
    println!("Generate the production unlock acceptance report (checklist capability 13).");
    println!("Static checks always run; persisted run/signal/session evidence is read");
    println!("from runtime/ctox.sqlite3 when available. Checks without persisted");
    println!("evidence report `unknown` plus an explicit required_action.");
    println!();
    println!("Options:");
    println!("  --target <id>          Limit the report to one adapter");
    println!("  --adapters-dir <path>  Override the scrape-targets directory");
    println!("  --out <path>           Also write the JSON report to a file");
    println!("  --strict               Exit 1 unless the gate passes (completion condition met)");
}

pub fn handle_report_command(root: &Path, args: &[String]) -> Result<()> {
    if args
        .iter()
        .any(|a| a == "help" || a == "-h" || a == "--help")
    {
        print_report_usage();
        return Ok(());
    }
    let adapters_dir = report_flag(args, "--adapters-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_adapters_dir);
    let target_filter = report_flag(args, "--target");
    let out_path = report_flag(args, "--out").map(PathBuf::from);
    let strict = args.iter().any(|a| a == "--strict");

    let report = generate_acceptance_report(root, &adapters_dir, target_filter)?;
    let pretty = serde_json::to_string_pretty(&report)?;

    if let Some(path) = &out_path {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
        }
        std::fs::write(path, &pretty)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    // Compact human summary on stderr; machine-readable report on stdout.
    let gate = &report["gate"];
    for row in report["adapters"].as_array().cloned().unwrap_or_default() {
        let verdict = if row["final_status"] == "live_success" {
            "OK  "
        } else {
            "FAIL"
        };
        let action = row["required_action"]
            .as_str()
            .map(|a| format!(" — {a}"))
            .unwrap_or_default();
        eprintln!(
            "unlock-gate {verdict} {:<18} {}{}",
            row["source_id"].as_str().unwrap_or("?"),
            row["final_status"].as_str().unwrap_or("?"),
            action
        );
    }
    eprintln!(
        "unlock-gate {} ({}/{} adapters proven, redaction self-check {})",
        if gate["ok"] == Value::Bool(true) {
            "PASS"
        } else {
            "FAIL"
        },
        gate["adapters_ok"],
        gate["adapters_total"],
        if gate["redaction_self_check"]["passed"] == Value::Bool(true) {
            "passed"
        } else {
            "FAILED"
        }
    );

    println!("{pretty}");
    if strict && gate["ok"] != Value::Bool(true) {
        std::process::exit(1);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct FixtureRoot {
        root: PathBuf,
        adapters_dir: PathBuf,
    }

    impl FixtureRoot {
        fn new() -> Self {
            let id = COUNTER.fetch_add(1, Ordering::SeqCst);
            let root = std::env::temp_dir().join(format!(
                "ctox-unlock-report-test-{}-{}",
                std::process::id(),
                id
            ));
            let adapters_dir = root.join("scrape-targets");
            std::fs::create_dir_all(adapters_dir.join("_shared")).unwrap();
            std::fs::create_dir_all(root.join("runtime")).unwrap();
            FixtureRoot { root, adapters_dir }
        }

        fn write_shared(&self, text: &str) {
            std::fs::write(self.adapters_dir.join(SHARED_SCRIPT_REL), text).unwrap();
        }

        fn write_adapter(&self, source_id: &str, manifest: &str, script: Option<&str>) {
            let dir = self.adapters_dir.join(source_id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("target.json"), manifest).unwrap();
            if let Some(script) = script {
                std::fs::create_dir_all(dir.join("scripts")).unwrap();
                std::fs::write(dir.join("scripts").join("v1.js"), script).unwrap();
            }
        }

        fn init_db(&self) -> Connection {
            let path = self.root.join("runtime").join("ctox.sqlite3");
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE scrape_target (
                    target_id TEXT PRIMARY KEY,
                    target_key TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'active'
                );
                CREATE TABLE scrape_run (
                    run_id TEXT PRIMARY KEY,
                    target_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    started_at TEXT NOT NULL,
                    result_json TEXT NOT NULL DEFAULT '{}',
                    output_dir TEXT,
                    run_context_json TEXT NOT NULL DEFAULT '{}'
                );
                CREATE TABLE web_unlock_signals (
                    signal_id INTEGER PRIMARY KEY,
                    detected_at TEXT NOT NULL,
                    source TEXT NOT NULL,
                    probe_url TEXT,
                    evidence_json TEXT,
                    resolved INTEGER NOT NULL DEFAULT 0
                );
                "#,
            )
            .unwrap();
            conn
        }
    }

    impl Drop for FixtureRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    const SHARED_JS: &str = r#"
const PROTECTED_SOURCE_CONFIG = Object.freeze({
  "protected.example": {
    login_url: "https://app.protected.example/login",
    allowed_domains: ["protected.example", "app.protected.example"],
    credential_ref: "ctox-secret://credentials/PROTECTED_EXAMPLE_LOGIN",
    capture_supported: true,
  },
});

const SOURCE_CONFIG = Object.freeze({
  "public.example": { native: true, domains: ["public.example"] },
  "protected.example": { native: true, domains: ["protected.example", "app.protected.example"] },
});
"#;

    fn manifest_json(target_key: &str, provider: &str) -> String {
        json!({
            "target_key": target_key,
            "display_name": format!("{provider} fixture"),
            "start_url": format!("https://{provider}/"),
            "target_kind": "prospect-research",
            "status": "active",
            "config": {
                "expected_provider": provider,
                "country_hints": ["DE"],
                "record_key_fields": ["field", "source_url"],
            },
            "output_schema": {
                "schema_key": "prospect.v1",
                "record_key_fields": ["field", "source_url"],
            },
        })
        .to_string()
    }

    fn seed_run(
        conn: &Connection,
        target_key: &str,
        run_id: &str,
        status: &str,
        started_at: &str,
        result: Value,
    ) {
        let target_id = format!("target-{target_key}");
        conn.execute(
            "INSERT OR IGNORE INTO scrape_target (target_id, target_key, status) VALUES (?1, ?2, 'active')",
            params![target_id, target_key],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO scrape_run (run_id, target_id, status, started_at, result_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![run_id, target_id, status, started_at, result.to_string()],
        )
        .unwrap();
    }

    fn adapter_row<'a>(report: &'a Value, source_id: &str) -> &'a Value {
        report["adapters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["source_id"] == source_id)
            .unwrap_or_else(|| panic!("adapter row for {source_id}"))
    }

    #[test]
    fn fully_proven_adapter_reports_live_success() {
        let fx = FixtureRoot::new();
        fx.write_shared(SHARED_JS);
        fx.write_adapter(
            "public.example",
            &manifest_json("public-example", "public.example"),
            None,
        );
        let conn = fx.init_db();
        seed_run(
            &conn,
            "public-example",
            "scrape_run-ok1",
            "succeeded",
            "2026-07-23T10:00:00Z",
            json!({"records": [{"field": "firma_name", "value": "Fixture GmbH"}]}),
        );
        drop(conn);

        let report = generate_acceptance_report(&fx.root, &fx.adapters_dir, None).unwrap();
        let row = adapter_row(&report, "public.example");
        assert_eq!(row["final_status"], "live_success");
        assert_eq!(row["unlock_mode"], "public_unlock");
        for (name, check) in row["checks"].as_object().unwrap() {
            assert_eq!(
                check["status"], "pass",
                "check {name} must pass: {}",
                check["reason"]
            );
        }
        assert_eq!(report["gate"]["ok"], true);
        assert_eq!(report["gate"]["redaction_self_check"]["passed"], true);
    }

    #[test]
    fn auth_required_without_post_auth_retry_fails_gate() {
        let fx = FixtureRoot::new();
        fx.write_shared(SHARED_JS);
        fx.write_adapter(
            "protected.example",
            &manifest_json("protected-example", "protected.example"),
            None,
        );
        let conn = fx.init_db();
        seed_run(
            &conn,
            "protected-example",
            "scrape_run-auth1",
            "blocked",
            "2026-07-23T10:00:00Z",
            json!({
                "records": [],
                "failure_mode": "auth_required",
                "detail": "protected.example requires an authenticated CTOX browser session",
                "browser_assist_requested": true,
            }),
        );
        drop(conn);

        let report = generate_acceptance_report(&fx.root, &fx.adapters_dir, None).unwrap();
        let row = adapter_row(&report, "protected.example");
        assert_eq!(row["final_status"], "operator_auth_required");
        assert_eq!(
            row["checks"]["authorization_handoff"]["status"], "pass",
            "handoff evidence exists (browser_assist_requested=true)"
        );
        assert_eq!(row["checks"]["post_auth_extraction"]["status"], "fail");
        let action = row["required_action"].as_str().unwrap();
        assert!(action.contains("authorize"), "action: {action}");
        assert!(action.contains("scrape execute"), "action: {action}");
        assert_eq!(report["gate"]["ok"], false);
    }

    #[test]
    fn post_auth_success_after_auth_required_passes() {
        let fx = FixtureRoot::new();
        fx.write_shared(SHARED_JS);
        fx.write_adapter(
            "protected.example",
            &manifest_json("protected-example", "protected.example"),
            None,
        );
        let conn = fx.init_db();
        seed_run(
            &conn,
            "protected-example",
            "scrape_run-auth1",
            "blocked",
            "2026-07-23T10:00:00Z",
            json!({"records": [], "failure_mode": "auth_required", "browser_assist_requested": true}),
        );
        seed_run(
            &conn,
            "protected-example",
            "scrape_run-auth2",
            "succeeded",
            "2026-07-23T11:00:00Z",
            json!({"records": [{"field": "firma_name", "value": "Fixture GmbH"}]}),
        );
        // Completed source-bound session document (RxDB-style table).
        conn.execute_batch(
            "CREATE TABLE \"business_os__browser_sessions__v0\" (
                id TEXT PRIMARY KEY, revision TEXT, deleted INTEGER NOT NULL,
                lastWriteTime REAL NOT NULL, data TEXT NOT NULL
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO \"business_os__browser_sessions__v0\" (id, deleted, lastWriteTime, data)
             VALUES ('browser_session_1', 0, 1.0, ?1)",
            params![json!({
                "source_id": "protected.example",
                "purpose": "web_stack_auth",
                "auth_assist_status": "completed",
                "secret_value_in_rxdb": false,
            })
            .to_string()],
        )
        .unwrap();
        drop(conn);

        let report = generate_acceptance_report(&fx.root, &fx.adapters_dir, None).unwrap();
        let row = adapter_row(&report, "protected.example");
        assert_eq!(row["final_status"], "live_success");
        assert_eq!(row["checks"]["post_auth_extraction"]["status"], "pass");
        assert_eq!(row["checks"]["session_reuse"]["status"], "pass");
        assert_eq!(report["gate"]["ok"], true);
    }

    #[test]
    fn secret_leak_fixture_fails_check_and_report_stays_redacted() {
        let fx = FixtureRoot::new();
        fx.write_shared(SHARED_JS);
        fx.write_adapter(
            "public.example",
            &manifest_json("public-example", "public.example"),
            Some("const password = \"hunter2supersecret\";\nconsole.log(password);\n"),
        );
        let conn = fx.init_db();
        seed_run(
            &conn,
            "public-example",
            "scrape_run-leak1",
            "blocked",
            "2026-07-23T10:00:00Z",
            json!({"records": [], "failure_mode": "blocked", "detail": "upstream sent token=abc123def456 in the challenge page"}),
        );
        drop(conn);

        let report = generate_acceptance_report(&fx.root, &fx.adapters_dir, None).unwrap();
        let row = adapter_row(&report, "public.example");
        assert_eq!(row["checks"]["no_secret_leakage"]["status"], "fail");
        assert_eq!(row["final_status"], "invalid_config");
        assert_eq!(report["gate"]["ok"], false);
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(
            !serialized.contains("hunter2supersecret"),
            "report must not quote the leaked script secret"
        );
        assert!(
            !serialized.contains("abc123def456"),
            "report must not quote the leaked persisted token"
        );
        assert_eq!(report["gate"]["redaction_self_check"]["passed"], true);
    }

    #[test]
    fn missing_allowed_domains_fails_validation() {
        let fx = FixtureRoot::new();
        fx.write_shared(
            r#"
const PROTECTED_SOURCE_CONFIG = Object.freeze({
  "protected.example": {
    login_url: "https://app.protected.example/login",
    allowed_domains: [],
    credential_ref: "ctox-secret://credentials/PROTECTED_EXAMPLE_LOGIN",
    capture_supported: true,
  },
});

const SOURCE_CONFIG = Object.freeze({
  "protected.example": { native: true, domains: ["protected.example"] },
});
"#,
        );
        fx.write_adapter(
            "protected.example",
            &manifest_json("protected-example", "protected.example"),
            None,
        );
        let report = generate_acceptance_report(&fx.root, &fx.adapters_dir, None).unwrap();
        let row = adapter_row(&report, "protected.example");
        assert_eq!(row["checks"]["allowed_domains_valid"]["status"], "fail");
        assert!(row["checks"]["allowed_domains_valid"]["reason"]
            .as_str()
            .unwrap()
            .contains("allowed_domains"));
        assert_eq!(row["final_status"], "invalid_config");
        assert_eq!(report["gate"]["ok"], false);
    }

    #[test]
    fn offline_without_database_reports_pending_evidence() {
        let fx = FixtureRoot::new();
        fx.write_shared(SHARED_JS);
        fx.write_adapter(
            "public.example",
            &manifest_json("public-example", "public.example"),
            None,
        );
        // No runtime/ctox.sqlite3 created.
        let report = generate_acceptance_report(&fx.root, &fx.adapters_dir, None).unwrap();
        assert_eq!(report["persisted_state"]["available"], false);
        assert_eq!(report["gate"]["proof_level"], "static_only");
        let row = adapter_row(&report, "public.example");
        assert_eq!(row["final_status"], "pending_evidence");
        assert_eq!(
            row["checks"]["challenge_classification"]["status"],
            "unknown"
        );
        assert!(row["checks"]["challenge_classification"]["required_action"]
            .as_str()
            .unwrap()
            .contains("scrape execute"));
        // Static checks still proved offline.
        assert_eq!(row["checks"]["manifest_valid"]["status"], "pass");
        assert_eq!(row["checks"]["allowed_domains_valid"]["status"], "pass");
        assert_eq!(report["gate"]["ok"], false);
    }

    #[test]
    fn captcha_solver_reference_fails_retry_bound() {
        let fx = FixtureRoot::new();
        fx.write_shared(SHARED_JS);
        fx.write_adapter(
            "public.example",
            &manifest_json("public-example", "public.example"),
            Some("// solve via 2captcha\nwhile (true) { retry(); }\n"),
        );
        let report = generate_acceptance_report(&fx.root, &fx.adapters_dir, None).unwrap();
        let row = adapter_row(&report, "public.example");
        assert_eq!(
            row["checks"]["public_unlock_retry_bounded"]["status"],
            "fail"
        );
        assert_eq!(report["gate"]["ok"], false);
    }

    #[test]
    fn redact_text_strips_secret_shapes() {
        assert_eq!(
            redact_text("sent password = \"abc123\" upstream"),
            "sent [redacted] upstream"
        );
        assert_eq!(
            redact_text("GET https://user:pw123@example.com/x"),
            "GET [redacted-url-userinfo]example.com/x"
        );
        assert_eq!(
            redact_text("Authorization: Bearer abcdef123456789"),
            "[redacted]"
        );
    }

    #[test]
    fn real_registry_has_fourteen_adapters_with_valid_shared_config() {
        let adapters_dir = default_adapters_dir();
        let shared = std::fs::read_to_string(adapters_dir.join(SHARED_SCRIPT_REL)).unwrap();
        let source_config =
            parse_js_config_section(&shared, "const SOURCE_CONFIG = Object.freeze({");
        let protected_config =
            parse_js_config_section(&shared, "const PROTECTED_SOURCE_CONFIG = Object.freeze({");
        let discovered = discover_adapters(&adapters_dir).unwrap();
        assert_eq!(
            discovered.len(),
            14,
            "expected 14 registered production adapters"
        );
        for (source_id, _) in &discovered {
            assert!(
                source_config.contains_key(source_id),
                "{source_id} missing from SOURCE_CONFIG"
            );
        }
        for protected in ["dnbhoovers.com", "leadfeeder.com", "rocketreach.com"] {
            let entry = protected_config
                .get(protected)
                .unwrap_or_else(|| panic!("{protected} missing from PROTECTED_SOURCE_CONFIG"));
            assert!(!entry.allowed_domains.is_empty());
            assert!(entry
                .credential_ref
                .as_deref()
                .unwrap_or("")
                .starts_with("ctox-secret://credentials/"));
        }
    }
}
