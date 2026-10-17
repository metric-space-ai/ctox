//! Adapter-builder agent interface (`docs/rfcs/toolhub-adapter-builder.md`).
//!
//! DE: Werkzeuge, mit denen ein CTOX-Agent Dienste sucht, ausführt und fehlende
//! Dienste als Spec entwirft, prüft, live testet und selbst aktiviert. Ohne
//! Nutzerfreigabe; Voraussetzung ist ein bestandener, höchstens 24 h alter
//! Live-Test, der an den sha256 der Version gebunden ist.
//!
//! EN: Tools for a CTOX agent to search and run services and to draft,
//! validate, live-test and activate missing ones autonomously. No user
//! approval: activation needs a passing live-test proof younger than 24 h
//! bound to the sha256 of the version. The runtime risk policy stays:
//! destructive calls need a per-call approval gate.
//!
//! The host (the `ctox` binary) supplies the secret backend, the per-call
//! approval gates and builder-mission lookup, and the Business OS policy
//! check; this crate owns the flow, the role matrix and the records:
//! - `<specs>/<slug>/reviews/<version>.json`   live-test proof
//! - `<specs>/<slug>/health.json`              consecutive failures (auto-rollback)
//! - `<root>/runtime/toolhub/gates/<id>.json`   per-call gate record
//! - `<root>/runtime/toolhub/drafts/<slug>/<version>.json` drafts
//! - `<specs>/<slug>/history.jsonl`            audit (append-only)
pub mod catalog;
pub mod probe;
pub mod roles;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::hub::{system_clock, Clock, ToolHub, ToolOutcome};
use crate::http::HttpTransport;
use crate::spec::adapter::sha256_hex;
use crate::spec::validate::Risk;
use crate::spec::{SpecAdapter, SpecStore};
use crate::store::SecretBackend;
use crate::toolkits::ToolkitAdapter;
pub use roles::{AgentRole, Denied, Tool};

/// A live-test proof older than this cannot activate a version.
pub const REVIEW_MAX_AGE_SECS: u64 = 24 * 3600;

/// Business OS permissions the host checks with `policy::evaluate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    DataRead,
    DataWrite,
    ExternalApprove,
    IntegrationsManage,
}

pub trait PolicyGate {
    fn check(&self, permission: Permission) -> Result<()>;
}

/// Consecutive failed calls of the active version before it is rolled back
/// automatically (only if a previous version exists).
pub const AUTO_ROLLBACK_AFTER: u32 = 5;

const AUTO_ROLLBACK_ACTOR: &str = "toolhub:auto-rollback";

/// New per-call `approval-gate` work item (destructive actions only).
#[derive(Debug, Clone, Serialize)]
pub struct GateRequest {
    pub title: String,
    pub body: String,
    pub metadata: Value,
}

/// Host side: per-call approval gates (`approval-gate` internal work items)
/// and the builder-mission lookup.
pub trait ApprovalGates {
    fn open(&self, request: &GateRequest) -> Result<String>;
    /// `open`, `closed`, `failed`, ... ; `None` if the gate does not exist.
    fn state(&self, gate_id: &str) -> Result<Option<String>>;
    fn decide(&self, gate_id: &str, approved: bool) -> Result<()>;
    /// Service slug of a live (not closed/failed) builder mission, if
    /// `work_id` is one. Workers may activate only inside such a mission.
    fn builder_mission_service(&self, work_id: &str) -> Result<Option<String>> {
        let _ = work_id;
        Ok(None)
    }
    /// Close a builder mission after its adapter `slug` was activated with
    /// the live-tested `sha256` (`summary` is the closing note). The host
    /// re-checks the condition before it closes. Default: nothing to close.
    fn close_builder_mission(&self, work_id: &str, slug: &str, sha256: &str, summary: &str) -> Result<()> {
        let _ = (work_id, slug, sha256, summary);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approve,
    Reject,
}

impl Decision {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "approve" | "approved" => Self::Approve,
            "reject" | "rejected" => Self::Reject,
            o => bail!("unknown decision `{o}` (approve|reject)"),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub decision: Decision,
    pub by: String,
    pub at: u64,
    /// The user's own words as relayed by the supervisor.
    pub user_quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GateSubject {
    Call { toolkit: String, tool: String, args_sha256: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateRecord {
    pub gate_id: String,
    pub subject: GateSubject,
    pub submitted_by: String,
    pub submitted_at: u64,
    #[serde(default)]
    pub decision: Option<DecisionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Probe {
    pub tool: String,
    #[serde(default = "empty_object")]
    pub args: Value,
    /// Required for write probes: calls that undo the probe.
    #[serde(default)]
    pub cleanup: Vec<Probe>,
    /// Result-shape assertion; without it an empty result still fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<probe::Expect>,
}

fn empty_object() -> Value {
    json!({})
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub tool: String,
    pub risk: String,
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub slug: String,
    pub version: String,
    pub sha256: String,
    pub tested_at: u64,
    pub tested_by: String,
    pub passed: bool,
    pub results: Vec<ProbeResult>,
    pub untested: Vec<String>,
    /// Builder mission (work item id) the test ran in; a worker can activate
    /// only with a proof from that same mission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission: Option<String>,
}

pub fn risk_str(r: Risk) -> &'static str {
    match r {
        Risk::Read => "read",
        Risk::Write => "write",
        Risk::Destructive => "destructive",
    }
}

/// File stem for a host gate id. Host ids look like `self-work:internal:<hex>`;
/// `:` is not file-safe everywhere, so it becomes `__`.
fn gate_stem(id: &str) -> Result<String> {
    if id.is_empty() || id.len() > 120 || id.starts_with('.') || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b)) {
        bail!("invalid gate id `{id}`");
    }
    Ok(id.replace(':', "__"))
}

fn safe(v: &str) -> Result<&str> {
    if v.is_empty() || v.len() > 80 || !v.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)) || v.starts_with('.') {
        bail!("invalid name `{v}`");
    }
    Ok(v)
}

/// `api_base` of a spec text (for URL suffixes in redirect hints).
fn adapter_base(text: &str) -> String {
    serde_json::from_str::<Value>(text).ok().and_then(|d| d["api_base"].as_str().map(String::from)).unwrap_or_default()
}

fn short(e: impl std::fmt::Display) -> String {
    e.to_string().chars().take(240).collect()
}

fn write_json(path: &Path, v: &impl Serialize) -> Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(v)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&std::fs::read(path)?).with_context(|| format!("{} is corrupt", path.display()))?))
}

pub struct Builder<'a> {
    root: PathBuf,
    store: SpecStore,
    backend: Arc<dyn SecretBackend>,
    transport: Arc<dyn HttpTransport>,
    gates: &'a dyn ApprovalGates,
    policy: &'a dyn PolicyGate,
    role: AgentRole,
    actor: String,
    project: String,
    clock: Clock,
}

impl<'a> Builder<'a> {
    pub fn new(
        root: &Path,
        backend: Arc<dyn SecretBackend>,
        transport: Arc<dyn HttpTransport>,
        gates: &'a dyn ApprovalGates,
        policy: &'a dyn PolicyGate,
        role: AgentRole,
        actor: &str,
    ) -> Self {
        Self {
            root: root.to_path_buf(),
            store: SpecStore::for_instance_root(root),
            backend,
            transport,
            gates,
            policy,
            role,
            actor: actor.to_string(),
            project: "default".into(),
            clock: system_clock(),
        }
    }

    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    pub fn with_project(mut self, project: &str) -> Self {
        self.project = project.to_string();
        self
    }

    pub fn store(&self) -> &SpecStore {
        &self.store
    }

    fn now(&self) -> u64 {
        (self.clock)()
    }

    fn who(&self) -> String {
        format!("{}:{}", self.role.as_str(), self.actor)
    }

    fn gate_path(&self, id: &str) -> Result<PathBuf> {
        Ok(self.root.join("runtime").join("toolhub").join("gates").join(format!("{}.json", gate_stem(id)?)))
    }

    fn draft_path(&self, slug: &str, version: &str) -> Result<PathBuf> {
        Ok(self.root.join("runtime").join("toolhub").join("drafts").join(safe(slug)?).join(format!("{}.json", safe(version)?)))
    }

    fn review_path(&self, slug: &str, version: &str) -> Result<PathBuf> {
        Ok(self.store.slug_path(slug)?.join("reviews").join(format!("{}.json", safe(version)?)))
    }

    fn health_path(&self, slug: &str) -> Result<PathBuf> {
        Ok(self.store.slug_path(slug)?.join("health.json"))
    }

    /// Passing live-test proof for exactly this installed text.
    fn proof(&self, slug: &str, version: &str) -> Result<ReviewRecord> {
        let sha = sha256_hex(self.store.version_text(slug, version).context("run a passing live_test first")?.as_bytes());
        let review: ReviewRecord = read_json(&self.review_path(slug, version)?)?.ok_or_else(|| anyhow!("{slug} {version} has no live-test proof"))?;
        if !review.passed || review.sha256 != sha {
            bail!("the live-test proof does not match {slug} {version} (changed after the test)");
        }
        Ok(review)
    }

    fn require(&self, tool: Tool, risk: Option<Risk>) -> Result<()> {
        roles::allows(self.role, tool, risk).map_err(anyhow::Error::new)
    }

    fn audit(&self, slug: &str, mut event: Value) -> Result<()> {
        if let Some(o) = event.as_object_mut() {
            o.insert("actor".into(), json!(self.who()));
            o.insert("at".into(), json!(self.now()));
        }
        self.store.append_history(slug, event)
    }

    fn hub(&self) -> Result<ToolHub<Arc<dyn SecretBackend>>> {
        let mut hub = ToolHub::new(self.backend.clone(), self.transport.clone()).with_clock(self.clock.clone());
        hub.reload_specs(&self.store)?;
        Ok(hub)
    }

    fn gate_record(&self, id: &str) -> Result<GateRecord> {
        read_json(&self.gate_path(id)?)?.ok_or_else(|| anyhow!("unknown gate `{id}`"))
    }

    // ---------------------------------------------------------------- read tools

    /// Search actions of built-in and active spec adapters; `include_catalog`
    /// also lists the inactive specs shipped with the release.
    pub fn search(&self, query: &str, include_catalog: bool, limit: usize) -> Result<Value> {
        self.require(Tool::Search, None)?;
        let hub = self.hub()?;
        let terms: Vec<String> = query.split_whitespace().map(|t| t.to_lowercase()).collect();
        let matches = |hay: &str| terms.iter().all(|t| hay.contains(t.as_str()));
        let mut out = Vec::new();
        for tk in hub.catalog() {
            for t in &tk.tools {
                let hay = format!("{} {} {} {} {} {}", t.slug, tk.slug, tk.name, t.title.en, t.title.de, t.description.en).to_lowercase();
                if matches(&hay) {
                    out.push(json!({"tool": t.slug, "toolkit": tk.slug, "title": t.title, "risk": risk_str(self.tool_risk(&hub, &t.slug)?), "source": if hub.is_spec_toolkit(&tk.slug) {"spec"} else {"builtin"}}));
                }
            }
        }
        let mut catalog = Vec::new();
        if include_catalog {
            for (slug, text) in catalog::CATALOG {
                if self.store.active(slug)?.is_some() {
                    continue;
                }
                let doc: Value = serde_json::from_str(text)?;
                let hay = format!("{} {} {}", slug, doc["name"].as_str().unwrap_or(""), doc["description"]["en"].as_str().unwrap_or("")).to_lowercase();
                if matches(&hay) {
                    catalog.push(json!({"slug": slug, "version": doc["version"], "name": doc["name"], "description": doc["description"], "actions": doc["actions"].as_array().map(|a| a.len()).unwrap_or(0)}));
                }
            }
        }
        out.truncate(limit.max(1));
        Ok(json!({"tools": out, "catalog": catalog}))
    }

    fn tool_risk(&self, hub: &ToolHub<Arc<dyn SecretBackend>>, tool: &str) -> Result<Risk> {
        let (_, spec) = hub.tool(tool)?;
        if hub.is_spec_toolkit(&spec.toolkit) {
            if let Some(a) = self.active_adapter(&spec.toolkit)? {
                if let Some(r) = a.risk(&spec.slug) {
                    return Ok(r);
                }
            }
        }
        Ok(if !spec.mutating {
            Risk::Read
        } else if spec.slug.contains("_DELETE") || spec.slug.contains("_REMOVE") || spec.slug.contains("_ARCHIVE") {
            Risk::Destructive
        } else {
            Risk::Write
        })
    }

    fn active_adapter(&self, slug: &str) -> Result<Option<SpecAdapter>> {
        let Some(a) = self.store.active(slug)? else { return Ok(None) };
        Ok(Some(SpecAdapter::from_json(&self.store.version_text(slug, &a.version)?)?))
    }

    pub fn get_schema(&self, tool: &str) -> Result<Value> {
        self.require(Tool::GetSchema, None)?;
        let hub = self.hub()?;
        let (adapter, spec) = hub.tool(tool)?;
        Ok(json!({
            "tool": spec.slug, "toolkit": spec.toolkit, "title": spec.title, "description": spec.description,
            "risk": risk_str(self.tool_risk(&hub, &spec.slug)?), "input_schema": spec.input_schema,
            "auth_schemes": adapter.spec().auth_schemes.iter().map(|s| s.kind()).collect::<Vec<_>>(),
            "connection_fields": adapter.connection_fields(),
        }))
    }

    pub fn list(&self) -> Result<Value> {
        self.require(Tool::List, None)?;
        let mut out = Vec::new();
        for s in self.store.list()? {
            let mut versions = Vec::new();
            for v in &s.versions {
                let review: Option<ReviewRecord> = read_json(&self.review_path(&s.slug, v)?)?;
                versions.push(json!({"version": v, "tested": review.as_ref().map(|r| json!({"passed": r.passed, "at": r.tested_at, "sha256": r.sha256}))}));
            }
            let health: Option<Value> = read_json(&self.health_path(&s.slug)?)?;
            out.push(json!({"slug": s.slug, "active": s.active, "versions": versions, "health": health}));
        }
        Ok(json!({"adapters": out}))
    }

    // ---------------------------------------------------------------- execute

    /// Run one action. `approval_gate` is required for destructive actions; without
    /// it a per-call gate is opened and `approval_required` is returned.
    pub fn execute(&self, tool: &str, args: &Value, account: Option<&str>, approval_gate: Option<&str>) -> Result<Value> {
        let hub = self.hub()?;
        let (_, spec) = hub.tool(tool)?;
        let (tool, toolkit) = (spec.slug.clone(), spec.toolkit.clone());
        let risk = self.tool_risk(&hub, &tool)?;
        self.require(Tool::Execute, Some(risk))?;
        self.policy.check(match risk {
            Risk::Read => Permission::DataRead,
            Risk::Write => Permission::DataWrite,
            Risk::Destructive => Permission::ExternalApprove,
        })?;
        let args_sha = sha256_hex(serde_json::to_string(args)?.as_bytes());
        if risk == Risk::Destructive {
            match approval_gate {
                None => {
                    let req = GateRequest {
                        title: format!("Freigabe für {tool} / Approve {tool}"),
                        body: format!("DE: Die Aktion {tool} ({toolkit}) lässt sich nicht rückgängig machen. Bitte einmalig für genau diesen Aufruf freigeben oder ablehnen.\nEN: The action {tool} ({toolkit}) cannot be undone. Approve or reject this exact call once.\nargs sha256: {args_sha}"),
                        metadata: json!({"source": "toolhub", "kind": "call", "tool": tool, "toolkit": toolkit, "args_sha256": args_sha, "dedupe_key": format!("toolhub-call:{tool}:{args_sha}")}),
                    };
                    let id = self.gates.open(&req)?;
                    write_json(&self.gate_path(&id)?, &GateRecord { gate_id: id.clone(), subject: GateSubject::Call { toolkit: toolkit.clone(), tool: tool.clone(), args_sha256: args_sha.clone() }, submitted_by: self.who(), submitted_at: self.now(), decision: None })?;
                    self.audit(&toolkit, json!({"event": "approval_requested", "tool": tool, "gate_id": id, "args_sha256": args_sha}))?;
                    return Ok(json!({"outcome": "approval_required", "gate_id": id, "message": "Destructive call: the user approves this exact call; record it with `approve`, then call again with this gate."}));
                }
                Some(id) => {
                    let rec = self.gate_record(id)?;
                    let ok_subject = matches!(&rec.subject, GateSubject::Call { tool: t, args_sha256: a, .. } if *t == tool && *a == args_sha);
                    if !ok_subject {
                        bail!("gate {id} does not cover this exact call");
                    }
                    if !matches!(rec.decision.as_ref().map(|d| d.decision), Some(Decision::Approve)) || self.gates.state(id)?.as_deref() != Some("closed") {
                        bail!("gate {id} is not approved");
                    }
                    let used = self.root.join("runtime").join("toolhub").join("gates").join(format!("{}.used", gate_stem(id)?));
                    if used.exists() {
                        bail!("gate {id} was already used");
                    }
                    std::fs::write(&used, b"")?;
                }
            }
        }
        let outcome = hub.execute(&self.project, &tool, args, account);
        let failed = match &outcome {
            Err(_) => true,
            Ok(ToolOutcome::ProviderError { status, .. }) => *status >= 500 || matches!(*status, 400 | 410),
            _ => false,
        };
        let auto_rollback = if hub.is_spec_toolkit(&toolkit) { self.record_health(&toolkit, failed)? } else { None };
        let (kind, status, value) = match outcome {
            Ok(ToolOutcome::Success { data }) => ("success", None, json!({"outcome": "success", "data": data})),
            Ok(ToolOutcome::ProviderError { status, data }) => ("provider_error", Some(status), json!({"outcome": "provider_error", "status": status, "data": data})),
            Ok(ToolOutcome::CredentialRequired { toolkit: tk, fields, auth_schemes, .. }) => {
                let (code, message) = roles::access_missing(self.role, &tk);
                ("access_missing", None, json!({"outcome": code, "message": message, "toolkit": tk, "auth_schemes": auth_schemes, "connection_fields": fields}))
            }
            Err(e) => {
                self.audit(&toolkit, json!({"event": "execute", "tool": tool, "risk": risk_str(risk), "result": "error", "args_sha256": args_sha}))?;
                return Err(match auto_rollback {
                    Some(v) => e.context(format!("{toolkit} was rolled back to {v} after repeated failures")),
                    None => e,
                });
            }
        };
        self.audit(&toolkit, json!({"event": "execute", "tool": tool, "risk": risk_str(risk), "result": kind, "status": status, "args_sha256": args_sha}))?;
        let mut value = value;
        if let (Some(v), Some(o)) = (auto_rollback, value.as_object_mut()) {
            o.insert("auto_rollback".into(), json!(v));
        }
        Ok(value)
    }

    /// Count consecutive failures of the active version (drift: mapping errors,
    /// 5xx, 400/410). After [`AUTO_ROLLBACK_AFTER`] it rolls back to the
    /// previous version if one exists (never twice in a row); returns that version.
    fn record_health(&self, slug: &str, failed: bool) -> Result<Option<String>> {
        let Some(active) = self.store.active(slug)? else { return Ok(None) };
        let path = self.health_path(slug)?;
        let prev: Option<Value> = read_json(&path)?;
        let mut n = match &prev {
            Some(h) if h["version"] == json!(active.version) => h["consecutive_failures"].as_u64().unwrap_or(0) as u32,
            _ => 0,
        };
        n = if failed { n + 1 } else { 0 };
        if prev.is_some() || failed {
            write_json(&path, &json!({"version": active.version, "consecutive_failures": n, "at": self.now()}))?;
        }
        if n < AUTO_ROLLBACK_AFTER {
            return Ok(None);
        }
        // No ping-pong: a version that an auto-rollback restored is not rolled
        // "back" to the broken one; the supervisor decides from here.
        if active.actor == AUTO_ROLLBACK_ACTOR {
            return Ok(None);
        }
        let Some(previous) = active.previous.clone() else { return Ok(None) };
        let ptr = self.store.rollback(slug, AUTO_ROLLBACK_ACTOR)?;
        self.audit(slug, json!({"event": "auto_rollback", "from": active.version, "to": previous, "consecutive_failures": n}))?;
        let _ = ptr;
        Ok(Some(previous))
    }

    // ---------------------------------------------------------------- build flow

    /// Store a draft (not validated, never active). `text` is the spec JSON.
    pub fn draft_spec(&self, text: &str) -> Result<Value> {
        self.require(Tool::DraftSpec, None)?;
        let doc: Value = serde_json::from_str(text).map_err(|e| anyhow!("draft is not valid JSON: {e}"))?;
        let slug = safe(doc["slug"].as_str().ok_or_else(|| anyhow!("draft needs `slug`"))?)?.to_string();
        let version = safe(doc["version"].as_str().ok_or_else(|| anyhow!("draft needs `version`"))?)?.to_string();
        if let Ok(existing) = self.store.version_text(&slug, &version) {
            if existing != text {
                bail!("{slug} {version} is already installed with different content; bump the version");
            }
        }
        let path = self.draft_path(&slug, &version)?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, text)?;
        self.audit(&slug, json!({"event": "draft", "version": version, "sha256": sha256_hex(text.as_bytes())}))?;
        Ok(json!({"slug": slug, "version": version, "sha256": sha256_hex(text.as_bytes()), "draft": path}))
    }

    /// Draft a copy of a vetted catalog spec (it still needs validate and live_test).
    pub fn draft_from_catalog(&self, slug: &str) -> Result<Value> {
        let text = catalog::get(slug).ok_or_else(|| anyhow!("`{slug}` is not in the shipped catalog"))?;
        self.draft_spec(text)
    }

    /// Fetch an OpenAPI document (core egress rules: https, public addresses,
    /// no redirects). Supervisor and worker only.
    pub fn fetch_document(&self, url: &str) -> Result<String> {
        self.require(Tool::DraftSpec, None)?;
        if !matches!(self.role, AgentRole::Supervisor | AgentRole::Worker) {
            bail!("not_allowed: only supervisor and worker fetch documents; pass a file instead");
        }
        if !url.starts_with("https://") {
            bail!("--from-openapi URL must be https");
        }
        let req = crate::http::HttpRequest::new("GET", url).header("accept", "application/json, application/yaml, text/yaml, */*");
        let r = self.transport.send_restricted(req).or_else(|e| {
            if probe::is_transient(&format!("{e:#}")) {
                self.transport.send_restricted(crate::http::HttpRequest::new("GET", url))
            } else {
                Err(e)
            }
        })?;
        if (300..400).contains(&r.status) {
            let to = self.transport.redirect_of(url).ok().flatten().map(|(_, l)| l).unwrap_or_else(|| "?".into());
            bail!("{url} answers {} -> {to}; redirects are not followed, pass that URL", r.status);
        }
        if r.status != 200 {
            bail!("{url} answered HTTP {}", r.status);
        }
        Ok(String::from_utf8_lossy(&r.body).into_owned())
    }

    /// Draft a spec (plus read probes) from an OpenAPI 3 / Swagger 2 document,
    /// then validate it. The probes land next to the draft (`live_test`
    /// without probes uses them).
    pub fn draft_from_openapi(&self, document: &str, opts: &crate::spec::openapi::Options) -> Result<Value> {
        self.require(Tool::DraftSpec, None)?;
        let doc = crate::spec::openapi::parse_document(document)?;
        let d = crate::spec::openapi::draft_from_openapi(&doc, opts)?;
        let text = serde_json::to_string_pretty(&d.spec)?;
        let stored = self.draft_spec(&text)?;
        let (slug, version) = (stored["slug"].as_str().unwrap_or_default().to_string(), stored["version"].as_str().unwrap_or_default().to_string());
        let probes_file = self.save_drafted_probes(&slug, &version, &json!(d.probes))?;
        let validation = self.validate(&slug, &version)?;
        Ok(json!({
            "slug": slug, "version": version, "sha256": stored["sha256"], "draft": stored["draft"],
            "actions": d.spec["actions"].as_array().map_or(0, Vec::len),
            "validate": validation, "auth": d.auth, "probes": d.probes, "probes_file": probes_file,
            "probes_missing": d.probes_missing, "skipped": d.skipped, "notes": d.notes, "spec": d.spec,
        }))
    }

    fn draft_text(&self, slug: &str, version: &str) -> Result<String> {
        let p = self.draft_path(slug, version)?;
        std::fs::read_to_string(&p).or_else(|_| self.store.version_text(slug, version)).with_context(|| format!("no draft {slug} {version}"))
    }

    pub fn validate(&self, slug: &str, version: &str) -> Result<Value> {
        self.require(Tool::Validate, None)?;
        let text = self.draft_text(slug, version)?;
        Ok(match SpecAdapter::from_json(&text) {
            Ok(a) => {
                let doc: Value = serde_json::from_str(&text)?;
                if a.spec().slug != slug || a.version() != version {
                    json!({"ok": false, "errors": [format!("draft declares {}@{}", a.spec().slug, a.version())]})
                } else {
                    let actions: Vec<Value> = a.spec().tools.iter().map(|t| json!({"tool": t.slug, "risk": a.risk(&t.slug).map(risk_str).unwrap_or("write")})).collect();
                    let builtin = crate::toolkits::builtin().iter().any(|b| b.spec().slug == slug);
                    let mut out = json!({"ok": true, "sha256": a.sha256(), "hosts": doc["allowed_hosts"], "auth_schemes": a.spec().auth_schemes.iter().map(|s| s.kind()).collect::<Vec<_>>(), "actions": actions, "builtin_exists": builtin, "replaces_builtin": a.replaces_builtin()});
                    if let Some(r) = self.base_redirect(&doc) {
                        out["redirect"] = r;
                    }
                    out
                }
            }
            Err(e) => json!({"ok": false, "errors": [format!("{e:#}")]}),
        })
    }

    /// A 3xx on the spec's fixed `api_base` (redirects are never followed).
    fn base_redirect(&self, doc: &Value) -> Option<Value> {
        let base = doc.get("api_base").and_then(Value::as_str)?;
        if doc.get("api_base_template").is_some() {
            return None;
        }
        probe::redirect_hint(self.transport.as_ref(), base, "")
    }

    /// Probes drafted next to a draft (`draft-spec --from-openapi`).
    pub fn drafted_probes(&self, slug: &str, version: &str) -> Result<Vec<Probe>> {
        let p = self.draft_path(slug, version)?.with_extension("probes.json");
        let text = std::fs::read_to_string(&p).with_context(|| format!("no probes given and none drafted for {slug} {version}"))?;
        Ok(serde_json::from_str(&text)?)
    }

    pub(crate) fn save_drafted_probes(&self, slug: &str, version: &str, probes: &Value) -> Result<PathBuf> {
        let p = self.draft_path(slug, version)?.with_extension("probes.json");
        std::fs::write(&p, serde_json::to_vec_pretty(probes)?)?;
        Ok(p)
    }

    /// Run probes against the real API (core only). On success the version is
    /// installed (inactive) and the review proof is written next to it.
    pub fn live_test(&self, slug: &str, version: &str, probes: &[Probe], account: Option<&str>) -> Result<Value> {
        self.live_test_in_mission(slug, version, probes, account, None)
    }

    /// `live_test` inside a builder mission: the proof records the work item id.
    pub fn live_test_in_mission(&self, slug: &str, version: &str, probes: &[Probe], account: Option<&str>, mission: Option<&str>) -> Result<Value> {
        let text = self.draft_text(slug, version)?;
        let adapter = SpecAdapter::from_json(&text).context("validate first")?;
        if adapter.spec().slug != slug || adapter.version() != version {
            bail!("draft declares {}@{}", adapter.spec().slug, adapter.version());
        }
        if probes.is_empty() {
            bail!("live_test needs at least one probe");
        }
        let risk_of = |t: &str| adapter.risk(t).ok_or_else(|| anyhow!("{slug} has no action `{t}`"));
        let mut max = Risk::Read;
        for p in probes {
            match risk_of(&p.tool)? {
                Risk::Destructive => bail!("destructive probes never run automatically (only as cleanup of a write probe)"),
                Risk::Write => {
                    if p.cleanup.is_empty() {
                        bail!("write probe {} needs a cleanup", p.tool);
                    }
                    max = Risk::Write;
                }
                Risk::Read => {}
            }
            for c in &p.cleanup {
                risk_of(&c.tool)?;
            }
        }
        self.require(Tool::LiveTest, Some(max))?;
        self.policy.check(if max == Risk::Read { Permission::DataRead } else { Permission::DataWrite })?;
        let sha = adapter.sha256().to_string();
        let mut hub = ToolHub::new(self.backend.clone(), self.transport.clone()).with_clock(self.clock.clone());
        hub.register(Box::new(SpecAdapter::from_json(&text)?));
        let base = adapter_base(&text);
        let mut redirects: Vec<Value> = Vec::new();
        let mut run = |p: &Probe| -> ProbeResult {
            let risk = risk_str(adapter.risk(&p.tool).unwrap_or(Risk::Write)).to_string();
            let mut outcome = hub.execute(&self.project, &p.tool, &p.args, account);
            // One retry on a transient network/TLS error (never on HTTP answers).
            if let Err(e) = &outcome {
                if probe::is_transient(&format!("{e:#}")) {
                    outcome = hub.execute(&self.project, &p.tool, &p.args, account);
                }
            }
            let (result, status, detail) = match outcome {
                Ok(ToolOutcome::Success { data }) => match probe::check(&data, p.expect.as_ref()) {
                    Ok(()) => ("pass", None, None),
                    Err(why) => ("fail", None, Some(short(format!("result shape: {why}")))),
                },
                Ok(ToolOutcome::ProviderError { status, .. }) if (300..400).contains(&status) => {
                    let url = adapter.build_request(&p.tool, &p.args).map(|r| r.url).unwrap_or_default();
                    let suffix = url.strip_prefix(base.as_str()).unwrap_or("");
                    match probe::redirect_hint(self.transport.as_ref(), &url, suffix) {
                        Some(h) => {
                            let d = h["hint"].as_str().unwrap_or_default().to_string();
                            redirects.push(h);
                            ("fail", Some(status), Some(short(d)))
                        }
                        None => ("fail", Some(status), Some(format!("provider answered HTTP {status} (redirect; not followed)"))),
                    }
                }
                Ok(ToolOutcome::ProviderError { status, .. }) => ("fail", Some(status), Some(format!("provider returned HTTP {status}"))),
                Ok(ToolOutcome::CredentialRequired { toolkit, .. }) => ("access_missing", None, Some(roles::access_missing(self.role, &toolkit).1)),
                Err(e) => {
                    let e = format!("{e:#}");
                    let d = match probe::egress_hint(&e) {
                        Some(h) => format!("{} | {h}", short(&e)),
                        None => short(&e),
                    };
                    ("fail", None, Some(d))
                }
            };
            ProbeResult { tool: p.tool.clone(), risk, result: result.into(), status, detail }
        };
        let mut results = Vec::new();
        for p in probes {
            results.push(run(p));
            for c in &p.cleanup {
                let mut r = run(c);
                if r.result != "pass" {
                    r.result = "cleanup_failed".into();
                }
                results.push(r);
            }
        }
        let tested: std::collections::BTreeSet<&str> = results.iter().map(|r| r.tool.as_str()).collect();
        let untested: Vec<String> = adapter.spec().tools.iter().map(|t| t.slug.clone()).filter(|t| !tested.contains(t.as_str())).collect();
        let passed = results.iter().all(|r| r.result == "pass");
        let review = ReviewRecord { slug: slug.into(), version: version.into(), sha256: sha.clone(), tested_at: self.now(), tested_by: self.who(), passed, results, untested, mission: mission.map(String::from) };
        if passed {
            self.store.install(&text)?;
            write_json(&self.review_path(slug, version)?, &review)?;
        }
        self.audit(slug, json!({"event": "live_test", "version": version, "sha256": sha, "passed": passed, "probes": review.results.len()}))?;
        let access_missing = review.results.iter().find(|r| r.result == "access_missing").and_then(|r| r.detail.clone());
        let mut out = json!({"passed": passed, "installed": passed, "review": review, "access_missing": access_missing});
        if !passed {
            if let Some(r) = self.base_redirect(&serde_json::from_str(&text)?) {
                redirects.insert(0, r);
            }
        }
        if !redirects.is_empty() {
            out["redirects"] = json!(redirects);
        }
        Ok(out)
    }

    /// Record the user's decision on a per-call (destructive) gate; the host
    /// closes or confirms the gate.
    pub fn approve(&self, gate_id: &str, decision: Decision, user_quote: &str) -> Result<Value> {
        self.require(Tool::Approve, None)?;
        self.policy.check(Permission::IntegrationsManage)?;
        if user_quote.trim().is_empty() {
            bail!("approve needs the user's own words (--user-quote)");
        }
        let mut rec = self.gate_record(gate_id)?;
        if rec.decision.is_some() {
            bail!("gate {gate_id} is already decided");
        }
        match self.gates.state(gate_id)?.as_deref() {
            Some("open") | Some("queued") | Some("publishing") => {}
            // Host gates can be closed by the user's authorized reply first.
            Some("closed") if decision != Decision::Reject => {}
            other => bail!("gate {gate_id} is not open ({})", other.unwrap_or("missing")),
        }
        // The host may refuse (e.g. no authorized user reply yet): then
        // nothing is recorded and the call can be repeated.
        self.gates.decide(gate_id, decision != Decision::Reject)?;
        rec.decision = Some(DecisionRecord { decision, by: self.who(), at: self.now(), user_quote: user_quote.chars().take(500).collect() });
        write_json(&self.gate_path(gate_id)?, &rec)?;
        let GateSubject::Call { toolkit: slug, .. } = &rec.subject;
        self.audit(slug, json!({"event": "decision", "gate_id": gate_id, "decision": decision}))?;
        Ok(json!({"gate_id": gate_id, "decision": decision}))
    }

    /// Activate a version autonomously. Needs: the installed text validates and
    /// a passing live-test proof younger than 24 h for exactly that sha256.
    /// Supervisor: always. Worker: only inside a builder mission for this slug.
    pub fn activate(&self, slug: &str, version: &str, mission: Option<&str>) -> Result<Value> {
        self.require(Tool::Activate, None)?;
        if self.role == AgentRole::Worker {
            let id = mission.ok_or_else(|| anyhow!("delegate_to_supervisor: workers activate only inside a builder mission (--mission <work_id>)"))?;
            match self.gates.builder_mission_service(id)? {
                // request-adapter writes `nager-date`; toolkit slugs usually say `nager_date`.
                Some(service) if service.replace('_', "-") == slug.replace('_', "-") => {}
                Some(service) => bail!("builder mission {id} is for `{service}`, not `{slug}`"),
                None => bail!("{id} is not a live builder mission"),
            }
        }
        self.policy.check(Permission::IntegrationsManage)?;
        let text = self.store.version_text(slug, version).context("run a passing live_test first")?;
        let adapter = SpecAdapter::from_json(&text).context("the installed spec does not validate")?;
        if adapter.spec().slug != slug || adapter.version() != version {
            bail!("installed text declares {}@{}", adapter.spec().slug, adapter.version());
        }
        let review = self.proof(slug, version)?;
        if self.now().saturating_sub(review.tested_at) > REVIEW_MAX_AGE_SECS {
            bail!("the live-test proof is older than 24 h; run live_test again");
        }
        // A worker's proof must come from this very mission (work item id).
        if self.role == AgentRole::Worker && review.mission.as_deref() != mission {
            bail!(
                "the live-test proof belongs to mission {}, not {}; run live_test with --mission {}",
                review.mission.as_deref().unwrap_or("(none)"),
                mission.unwrap_or("(none)"),
                mission.unwrap_or("<work_id>")
            );
        }
        let ptr = self.store.activate(slug, version, &self.who())?;
        let _ = std::fs::remove_file(self.health_path(slug)?);
        self.audit(slug, json!({"event": "activate_proof", "version": version, "sha256": review.sha256, "tested_at": review.tested_at, "tested_by": review.tested_by, "mission": mission, "proof_mission": review.mission}))?;
        let reload = self.hub().map(|_| "ok").unwrap_or("failed");
        let mut out = json!({"active": ptr, "sha256": review.sha256, "reload": reload});
        if let Some(id) = mission {
            let mine = self.gates.builder_mission_service(id)?.is_some_and(|s| s.replace('_', "-") == slug.replace('_', "-"));
            if mine {
                let tools: Vec<&str> = review.results.iter().map(|r| r.tool.as_str()).collect();
                let summary = format!(
                    "Adapter {slug} {version} aktiv / active (sha256 {}); live getestet / live-tested: {}",
                    &review.sha256[..12.min(review.sha256.len())],
                    tools.join(", ")
                );
                let closed = self.gates.close_builder_mission(id, slug, &review.sha256, &summary);
                self.audit(slug, json!({"event": "mission_closed", "mission": id, "ok": closed.is_ok()}))?;
                out["mission"] = match closed {
                    Ok(()) => json!({"work_id": id, "closed": true}),
                    Err(e) => json!({"work_id": id, "closed": false, "error": short(format!("{e:#}"))}),
                };
            }
        }
        Ok(out)
    }

    pub fn deactivate(&self, slug: &str) -> Result<Value> {
        self.require(Tool::Deactivate, None)?;
        self.policy.check(Permission::IntegrationsManage)?;
        Ok(json!({"deactivated": self.store.deactivate(slug, &self.who())?}))
    }

    pub fn rollback(&self, slug: &str) -> Result<Value> {
        self.require(Tool::Rollback, None)?;
        self.policy.check(Permission::IntegrationsManage)?;
        let cur = self.store.active(slug)?.ok_or_else(|| anyhow!("{slug} has no active version"))?;
        let prev = cur.previous.clone().ok_or_else(|| anyhow!("{slug} has no previous version"))?;
        self.proof(slug, &prev).with_context(|| format!("{slug} {prev} has no matching live-test proof; deactivate instead"))?;
        Ok(json!({"active": self.store.rollback(slug, &self.who())?}))
    }
}

#[cfg(test)]
mod tests;
