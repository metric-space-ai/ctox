//! In-session UI requests ("cards"): a harness-agnostic way for an agent to
//! ask the user for a secret, a provider connection, or a confirmation.
//!
//! `ctox ui request credential|connect|confirm --toolkit <x>` creates a typed
//! request on the instance and blocks until a client (Workjet card) resolves
//! it. Requests are plain JSON files under `runtime/toolhub/ui-requests/` and
//! never contain secret values: a submitted credential goes straight into the
//! instance secret store, the request only records that it was submitted.
//!
//! Exit codes for `ctox ui request`: 0 done, 4 declined, 5 expired, 2 usage.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub mod exit {
    pub const DONE: i32 = 0;
    pub const USAGE: i32 = 2;
    pub const DECLINED: i32 = 4;
    pub const EXPIRED: i32 = 5;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiKind {
    Credential,
    Connect,
    Confirm,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiStatus {
    Open,
    Done,
    Declined,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiField {
    pub name: String,
    /// Masked input; every credential field is secret unless stated otherwise.
    pub secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiRequest {
    pub id: String,
    pub kind: UiKind,
    pub status: UiStatus,
    pub toolkit: String,
    #[serde(default)]
    pub fields: Vec<UiField>,
    /// Optional plain-language text from the agent (confirm: what is asked).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Session/thread the request belongs to, if the harness knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    pub created_at: u64,
    pub expires_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<u64>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn dir(root: &Path) -> PathBuf {
    root.join("runtime").join("toolhub").join("ui-requests")
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn path(root: &Path, id: &str) -> Result<PathBuf> {
    if !valid_id(id) {
        bail!("invalid request id");
    }
    Ok(dir(root).join(format!("{id}.json")))
}

fn save(root: &Path, r: &UiRequest) -> Result<()> {
    std::fs::create_dir_all(dir(root))?;
    let p = path(root, &r.id)?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(r)?)?;
    std::fs::rename(tmp, p)?;
    Ok(())
}

/// Load a request, marking it expired once its deadline passed.
pub fn get(root: &Path, id: &str) -> Result<UiRequest> {
    let raw = std::fs::read(path(root, id)?).context("unknown request")?;
    let mut r: UiRequest = serde_json::from_slice(&raw)?;
    if r.status == UiStatus::Open && now() >= r.expires_at {
        r.status = UiStatus::Expired;
        r.resolved_at = Some(now());
        save(root, &r)?;
    }
    Ok(r)
}

pub fn create(
    root: &Path,
    kind: UiKind,
    toolkit: &str,
    fields: Vec<UiField>,
    message: Option<String>,
    session: Option<String>,
    ttl_secs: u64,
) -> Result<UiRequest> {
    if toolkit.is_empty()
        || toolkit.len() > 64
        || !toolkit
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("invalid toolkit");
    }
    // Reuse an open request for the same toolkit+kind+session instead of
    // stacking duplicate cards when an agent retries.
    if let Some(existing) = list(root)?.into_iter().find(|r| {
        r.status == UiStatus::Open && r.kind == kind && r.toolkit == toolkit && r.session == session
    }) {
        return Ok(existing);
    }
    let created_at = now();
    let id = format!("ui-{created_at:x}-{:08x}", rand_u32());
    let fields = if kind == UiKind::Credential && fields.is_empty() {
        vec![UiField {
            name: "token".into(),
            secret: true,
        }]
    } else {
        fields
    };
    let r = UiRequest {
        id,
        kind,
        status: UiStatus::Open,
        toolkit: toolkit.into(),
        fields,
        message,
        session,
        created_at,
        expires_at: created_at + ttl_secs.max(30),
        resolved_at: None,
    };
    save(root, &r)?;
    Ok(r)
}

fn rand_u32() -> u32 {
    let mut b = [0u8; 4];
    if getrandom(&mut b).is_err() {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        return n ^ std::process::id();
    }
    u32::from_le_bytes(b)
}

fn getrandom(buf: &mut [u8]) -> std::io::Result<()> {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")?.read_exact(buf)
}

/// All requests, newest first (expired ones get marked on the way).
pub fn list(root: &Path) -> Result<Vec<UiRequest>> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir(root)) else {
        return Ok(out);
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if let Some(id) = name.strip_suffix(".json") {
            if let Ok(r) = get(root, id) {
                out.push(r);
            }
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

/// Mark a request resolved. Secret values are never written here; the caller
/// stores them first (see [`resolve_credential`]).
pub fn finish(root: &Path, id: &str, status: UiStatus) -> Result<UiRequest> {
    let mut r = get(root, id)?;
    if r.status != UiStatus::Open {
        return Ok(r);
    }
    if status == UiStatus::Open {
        bail!("cannot reopen");
    }
    r.status = status;
    r.resolved_at = Some(now());
    save(root, &r)?;
    Ok(r)
}

/// Credential submit: one field is passed as-is, several as a JSON object.
/// `store` puts it into the instance secret store; only then is the request
/// marked done.
pub fn resolve_credential(
    root: &Path,
    id: &str,
    values: &Value,
    store: impl FnOnce(&str, &str) -> Result<()>,
) -> Result<UiRequest> {
    let r = get(root, id)?;
    if r.kind != UiKind::Credential {
        bail!("not a credential request");
    }
    if r.status != UiStatus::Open {
        return Ok(r);
    }
    let mut picked = serde_json::Map::new();
    for f in &r.fields {
        let v = values
            .get(&f.name)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if v.is_empty() {
            bail!("missing field `{}`", f.name);
        }
        picked.insert(f.name.clone(), Value::String(v.to_string()));
    }
    let credential = if picked.len() == 1 {
        picked
            .values()
            .next()
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    } else {
        Value::Object(picked).to_string()
    };
    store(&r.toolkit, &credential)?;
    finish(root, id, UiStatus::Done)
}

/// Block until the request leaves `open`. `poll` may resolve it from the
/// outside world (e.g. a connect request is done once the connection is live).
pub fn wait(root: &Path, id: &str, poll: impl Fn(&UiRequest) -> bool) -> Result<UiRequest> {
    loop {
        let r = get(root, id)?;
        if r.status != UiStatus::Open {
            return Ok(r);
        }
        if poll(&r) {
            return finish(root, id, UiStatus::Done);
        }
        std::thread::sleep(Duration::from_millis(750));
    }
}

pub fn exit_code(status: UiStatus) -> i32 {
    match status {
        UiStatus::Done | UiStatus::Open => exit::DONE,
        UiStatus::Declined => exit::DECLINED,
        UiStatus::Expired => exit::EXPIRED,
    }
}

pub fn parse_kind(s: &str) -> Option<UiKind> {
    match s {
        "credential" => Some(UiKind::Credential),
        "connect" => Some(UiKind::Connect),
        "confirm" => Some(UiKind::Confirm),
        _ => None,
    }
}

pub fn parse_fields(spec: Option<&str>) -> Vec<UiField> {
    spec.map(|s| {
        s.split(',')
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(|n| match n.strip_suffix(":plain") {
                Some(base) => UiField {
                    name: base.into(),
                    secret: false,
                },
                None => UiField {
                    name: n.into(),
                    secret: true,
                },
            })
            .collect()
    })
    .unwrap_or_default()
}

pub fn summary(r: &UiRequest) -> Value {
    json!({"id": r.id, "kind": r.kind, "status": r.status, "toolkit": r.toolkit})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        let p = std::env::temp_dir().join(format!("ctox-ui-{}-{}", std::process::id(), rand_u32()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn credential_value_goes_to_store_not_file() {
        let root = root();
        let r = create(
            &root,
            UiKind::Credential,
            "slack",
            parse_fields(Some("token")),
            None,
            Some("s1".into()),
            600,
        )
        .unwrap();
        assert_eq!(r.status, UiStatus::Open);
        let again = create(
            &root,
            UiKind::Credential,
            "slack",
            vec![],
            None,
            Some("s1".into()),
            600,
        )
        .unwrap();
        assert_eq!(again.id, r.id, "duplicate open request is reused");
        let mut stored = None;
        let done = resolve_credential(&root, &r.id, &json!({"token": " xoxb-123 "}), |tk, v| {
            stored = Some((tk.to_string(), v.to_string()));
            Ok(())
        })
        .unwrap();
        assert_eq!(done.status, UiStatus::Done);
        assert_eq!(stored, Some(("slack".into(), "xoxb-123".into())));
        let raw = std::fs::read_to_string(path(&root, &r.id).unwrap()).unwrap();
        assert!(
            !raw.contains("xoxb-123"),
            "secret must never be written to the request"
        );
        assert_eq!(exit_code(done.status), exit::DONE);
    }

    #[test]
    fn missing_field_rejected_and_request_stays_open() {
        let root = root();
        let r = create(
            &root,
            UiKind::Credential,
            "gmail",
            parse_fields(Some("client_id:plain,secret")),
            None,
            None,
            600,
        )
        .unwrap();
        assert!(!r.fields[0].secret && r.fields[1].secret);
        assert!(
            resolve_credential(&root, &r.id, &json!({"client_id": "x"}), |_, _| Ok(())).is_err()
        );
        assert_eq!(get(&root, &r.id).unwrap().status, UiStatus::Open);
    }

    #[test]
    fn decline_expire_and_wait() {
        let root = root();
        let c = create(
            &root,
            UiKind::Confirm,
            "slack",
            vec![],
            Some("Nachricht senden?".into()),
            None,
            600,
        )
        .unwrap();
        assert_eq!(
            finish(&root, &c.id, UiStatus::Declined).unwrap().status,
            UiStatus::Declined
        );
        assert_eq!(
            exit_code(wait(&root, &c.id, |_| false).unwrap().status),
            exit::DECLINED
        );
        let mut e = create(&root, UiKind::Connect, "vercel", vec![], None, None, 600).unwrap();
        e.expires_at = 0;
        save(&root, &e).unwrap();
        assert_eq!(exit_code(get(&root, &e.id).unwrap().status), exit::EXPIRED);
        let k = create(
            &root,
            UiKind::Connect,
            "cloudflare",
            vec![],
            None,
            None,
            600,
        )
        .unwrap();
        assert_eq!(wait(&root, &k.id, |_| true).unwrap().status, UiStatus::Done);
        assert!(path(&root, "../etc/passwd").is_err());
    }
}
