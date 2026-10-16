//! Adapter-builder agent interface (`docs/rfcs/toolhub-adapter-builder.md`).
//!
//! DE: Werkzeuge, mit denen ein CTOX-Agent Dienste sucht, ausführt und fehlende
//! Dienste als Spec entwirft, prüft, live testet und zur Freigabe vorlegt.
//! Aktiviert wird nur mit einer Nutzerentscheidung, die an den sha256 der
//! geprüften Version gebunden ist.
//!
//! EN: Tools for a CTOX agent to search and run services and to draft,
//! validate, live-test and submit missing ones as specs. Activation needs a
//! user decision bound to the sha256 of the tested version.
//!
//! The host (the `ctox` binary) supplies the secret backend, the approval
//! gates (`approval-gate` work items) and the Business OS policy check; this
//! crate owns the flow, the role matrix and the per-instance records:
//! - `<specs>/<slug>/reviews/<version>.json`   live-test proof
//! - `<specs>/<slug>/approvals/<version>.json` pointer to the gate
//! - `<root>/runtime/toolhub/gates/<id>.json`   gate record with the decision
//! - `<root>/runtime/toolhub/drafts/<slug>/<version>.json` drafts
//! - `<specs>/<slug>/history.jsonl`            audit (append-only)
pub mod catalog;
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

/// A live-test proof older than this cannot be submitted.
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

/// New `approval-gate` work item for the supervisor.
#[derive(Debug, Clone, Serialize)]
pub struct GateRequest {
    pub title: String,
    pub body: String,
    pub metadata: Value,
}

/// Host side of the approval gates (`approval-gate` internal work items).
pub trait ApprovalGates {
    fn open(&self, request: &GateRequest) -> Result<String>;
    /// `open`, `closed`, `failed`, ... ; `None` if the gate does not exist.
    fn state(&self, gate_id: &str) -> Result<Option<String>>;
    fn decide(&self, gate_id: &str, approved: bool) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approve,
    ReadOnly,
    Reject,
}

impl Decision {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "approve" | "approved" => Self::Approve,
            "read_only" | "readonly" => Self::ReadOnly,
            "reject" | "rejected" => Self::Reject,
            o => bail!("unknown decision `{o}` (approve|read_only|reject)"),
        })
    }
    fn allows_activation(self) -> bool {
        matches!(self, Self::Approve | Self::ReadOnly)
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
    Adapter { slug: String, version: String, sha256: String },
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

    fn approval_path(&self, slug: &str, version: &str) -> Result<PathBuf> {
        Ok(self.store.slug_path(slug)?.join("approvals").join(format!("{}.json", safe(version)?)))
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

    /// Decision for `slug@version`, if its gate was decided.
    fn version_decision(&self, slug: &str, version: &str) -> Result<Option<(GateRecord, DecisionRecord)>> {
        let Some(ptr) = read_json::<Value>(&self.approval_path(slug, version)?)? else { return Ok(None) };
        let id = ptr["gate_id"].as_str().ok_or_else(|| anyhow!("approval pointer without gate id"))?;
        let rec = self.gate_record(id)?;
        Ok(rec.decision.clone().map(|d| (rec, d)))
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
                let gate = match read_json::<Value>(&self.approval_path(&s.slug, v)?)? {
                    Some(p) => {
                        let id = p["gate_id"].as_str().unwrap_or_default().to_string();
                        let rec = self.gate_record(&id).ok();
                        json!({"gate_id": id, "decision": rec.and_then(|r| r.decision).map(|d| d.decision)})
                    }
                    None => Value::Null,
                };
                versions.push(json!({"version": v, "tested": review.as_ref().map(|r| json!({"passed": r.passed, "at": r.tested_at})), "gate": gate}));
            }
            out.push(json!({"slug": s.slug, "active": s.active, "versions": versions}));
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
        if risk != Risk::Read && hub.is_spec_toolkit(&toolkit) {
            let active = self.store.active(&toolkit)?.ok_or_else(|| anyhow!("{toolkit} is not active"))?;
            match self.version_decision(&toolkit, &active.version)? {
                Some((_, d)) if d.decision == Decision::Approve => {}
                Some((_, d)) if d.decision == Decision::ReadOnly => bail!("read_only_approval: {toolkit} {} was approved for read actions only", active.version),
                _ => bail!("not_approved: {toolkit} {} has no recorded approval", active.version),
            }
        }
        let args_sha = sha256_hex(serde_json::to_string(args)?.as_bytes());
        if risk == Risk::Destructive {
            match approval_gate {
                None => {
                    let req = GateRequest {
                        title: format!("Freigabe für {tool} / Approve {tool}"),
                        body: format!("DE: Die Aktion {tool} ({toolkit}) lässt sich nicht rückgängig machen. Bitte einmalig für genau diesen Aufruf freigeben oder ablehnen.\nEN: The action {tool} ({toolkit}) cannot be undone. Approve or reject this exact call once.\nargs sha256: {args_sha}"),
                        metadata: json!({"source": "toolhub", "kind": "call", "tool": tool, "toolkit": toolkit, "args_sha256": args_sha, "approval_modality": "tui-only", "dedupe_key": format!("toolhub-call:{tool}:{args_sha}")}),
                    };
                    let id = self.gates.open(&req)?;
                    write_json(&self.gate_path(&id)?, &GateRecord { gate_id: id.clone(), subject: GateSubject::Call { toolkit: toolkit.clone(), tool: tool.clone(), args_sha256: args_sha.clone() }, submitted_by: self.who(), submitted_at: self.now(), decision: None })?;
                    self.audit(&toolkit, json!({"event": "approval_requested", "tool": tool, "gate_id": id, "args_sha256": args_sha}))?;
                    return Ok(json!({"outcome": "approval_required", "gate_id": id, "message": "Ask the user in the supervisor chat, record the answer with `approve`, then call again with this gate."}));
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
        let (kind, status, value) = match outcome {
            Ok(ToolOutcome::Success { data }) => ("success", None, json!({"outcome": "success", "data": data})),
            Ok(ToolOutcome::ProviderError { status, data }) => ("provider_error", Some(status), json!({"outcome": "provider_error", "status": status, "data": data})),
            Ok(ToolOutcome::CredentialRequired { toolkit: tk, fields, auth_schemes, .. }) => {
                let (code, message) = roles::access_missing(self.role, &tk);
                ("access_missing", None, json!({"outcome": code, "message": message, "toolkit": tk, "auth_schemes": auth_schemes, "connection_fields": fields}))
            }
            Err(e) => {
                self.audit(&toolkit, json!({"event": "execute", "tool": tool, "risk": risk_str(risk), "result": "error", "args_sha256": args_sha}))?;
                return Err(e);
            }
        };
        self.audit(&toolkit, json!({"event": "execute", "tool": tool, "risk": risk_str(risk), "result": kind, "status": status, "args_sha256": args_sha}))?;
        Ok(value)
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

    /// Draft a copy of a vetted catalog spec (it still goes through test and gate).
    pub fn draft_from_catalog(&self, slug: &str) -> Result<Value> {
        let text = catalog::get(slug).ok_or_else(|| anyhow!("`{slug}` is not in the shipped catalog"))?;
        self.draft_spec(text)
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
                    json!({"ok": true, "sha256": a.sha256(), "hosts": doc["allowed_hosts"], "auth_schemes": a.spec().auth_schemes.iter().map(|s| s.kind()).collect::<Vec<_>>(), "actions": actions, "builtin_exists": builtin, "replaces_builtin": a.replaces_builtin()})
                }
            }
            Err(e) => json!({"ok": false, "errors": [format!("{e:#}")]}),
        })
    }

    /// Run probes against the real API (core only). On success the version is
    /// installed (inactive) and the review proof is written next to it.
    pub fn live_test(&self, slug: &str, version: &str, probes: &[Probe], account: Option<&str>) -> Result<Value> {
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
        let run = |p: &Probe| -> ProbeResult {
            let risk = risk_str(adapter.risk(&p.tool).unwrap_or(Risk::Write)).to_string();
            let (result, status, detail) = match hub.execute(&self.project, &p.tool, &p.args, account) {
                Ok(ToolOutcome::Success { .. }) => ("pass", None, None),
                Ok(ToolOutcome::ProviderError { status, .. }) => ("fail", Some(status), Some(format!("provider returned HTTP {status}"))),
                Ok(ToolOutcome::CredentialRequired { toolkit, .. }) => ("access_missing", None, Some(roles::access_missing(self.role, &toolkit).1)),
                Err(e) => ("fail", None, Some(short(format!("{e:#}")))),
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
        let review = ReviewRecord { slug: slug.into(), version: version.into(), sha256: sha.clone(), tested_at: self.now(), tested_by: self.who(), passed, results, untested };
        if passed {
            self.store.install(&text)?;
            write_json(&self.review_path(slug, version)?, &review)?;
        }
        self.audit(slug, json!({"event": "live_test", "version": version, "sha256": sha, "passed": passed, "probes": review.results.len()}))?;
        let access_missing = review.results.iter().find(|r| r.result == "access_missing").and_then(|r| r.detail.clone());
        Ok(json!({"passed": passed, "installed": passed, "review": review, "access_missing": access_missing}))
    }

    /// Open the approval gate for a tested version.
    pub fn submit_for_approval(&self, slug: &str, version: &str) -> Result<Value> {
        self.require(Tool::Submit, None)?;
        let text = self.store.version_text(slug, version).context("run a passing live_test first")?;
        let sha = sha256_hex(text.as_bytes());
        let review: ReviewRecord = read_json(&self.review_path(slug, version)?)?.ok_or_else(|| anyhow!("{slug} {version} has no live-test proof"))?;
        if !review.passed || review.sha256 != sha {
            bail!("the live-test proof does not match {slug} {version}");
        }
        if self.now().saturating_sub(review.tested_at) > REVIEW_MAX_AGE_SECS {
            bail!("the live-test proof is older than 24 h; run live_test again");
        }
        if let Some(ptr) = read_json::<Value>(&self.approval_path(slug, version)?)? {
            let id = ptr["gate_id"].as_str().unwrap_or_default();
            let rec = self.gate_record(id)?;
            return match rec.decision {
                None => Ok(json!({"gate_id": id, "state": "pending", "existing": true})),
                Some(d) if d.decision == Decision::Reject => bail!("{slug} {version} was rejected; change the spec and bump the version"),
                Some(d) => Ok(json!({"gate_id": id, "state": "decided", "decision": d.decision})),
            };
        }
        let adapter = SpecAdapter::from_json(&text)?;
        let doc: Value = serde_json::from_str(&text)?;
        let mut by_risk: std::collections::BTreeMap<&str, Vec<String>> = Default::default();
        for t in &adapter.spec().tools {
            by_risk.entry(adapter.risk(&t.slug).map(risk_str).unwrap_or("write")).or_default().push(format!("{} ({})", t.title.de, t.title.en));
        }
        let hosts = doc["allowed_hosts"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_default();
        let passed = review.results.iter().filter(|r| r.result == "pass").count();
        let mut body = format!(
            "DE: Neuer Dienst {name} (Version {version}). Er spricht nur mit: {hosts}. Live getestet: {passed} Aufrufe bestanden, {untested} Aktionen ungetestet.\nEN: New service {name} (version {version}). It only talks to: {hosts}. Live test: {passed} calls passed, {untested} actions untested.\n",
            name = adapter.spec().name,
            untested = review.untested.len()
        );
        for (r, list) in &by_risk {
            let label = match *r { "read" => "Nur lesen / read", "write" => "Ändern / write", _ => "Löschen, nicht umkehrbar / destructive" };
            body.push_str(&format!("\n{label} ({}):\n- {}\n", list.len(), list.join("\n- ")));
        }
        body.push_str("\nAntworten / reply: approve | read_only | reject\n");
        let req = GateRequest {
            title: format!("Neuer Adapter: {} {version} / New adapter", adapter.spec().name),
            body,
            metadata: json!({"source": "toolhub", "kind": "adapter", "slug": slug, "version": version, "sha256": sha, "approval_modality": "tui-only", "dedupe_key": format!("toolhub-adapter:{slug}:{version}:{sha}")}),
        };
        let id = self.gates.open(&req)?;
        write_json(&self.gate_path(&id)?, &GateRecord { gate_id: id.clone(), subject: GateSubject::Adapter { slug: slug.into(), version: version.into(), sha256: sha.clone() }, submitted_by: self.who(), submitted_at: self.now(), decision: None })?;
        write_json(&self.approval_path(slug, version)?, &json!({"gate_id": id}))?;
        self.audit(slug, json!({"event": "submit", "version": version, "sha256": sha, "gate_id": id}))?;
        Ok(json!({"gate_id": id, "state": "pending", "title": req.title, "card_text": req.body}))
    }

    /// Record the user's decision (relayed by the supervisor) and close the gate.
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
        if decision == Decision::ReadOnly && matches!(rec.subject, GateSubject::Call { .. }) {
            bail!("read_only only applies to adapter gates");
        }
        match self.gates.state(gate_id)?.as_deref() {
            Some("open") | Some("queued") | Some("publishing") => {}
            other => bail!("gate {gate_id} is not open ({})", other.unwrap_or("missing")),
        }
        rec.decision = Some(DecisionRecord { decision, by: self.who(), at: self.now(), user_quote: user_quote.chars().take(500).collect() });
        write_json(&self.gate_path(gate_id)?, &rec)?;
        self.gates.decide(gate_id, decision != Decision::Reject)?;
        let slug = match &rec.subject {
            GateSubject::Adapter { slug, .. } => slug.clone(),
            GateSubject::Call { toolkit, .. } => toolkit.clone(),
        };
        self.audit(&slug, json!({"event": "decision", "gate_id": gate_id, "decision": decision}))?;
        Ok(json!({"gate_id": gate_id, "decision": decision}))
    }

    /// Activate an approved version: the gate must be closed with a recorded
    /// user decision whose sha256 matches the installed text.
    pub fn activate(&self, slug: &str, version: &str) -> Result<Value> {
        self.require(Tool::Activate, None)?;
        self.policy.check(Permission::IntegrationsManage)?;
        let sha = sha256_hex(self.store.version_text(slug, version)?.as_bytes());
        let (rec, d) = self.version_decision(slug, version)?.ok_or_else(|| anyhow!("{slug} {version} has no user decision; submit_for_approval and ask the user"))?;
        if !d.decision.allows_activation() {
            bail!("{slug} {version} was rejected");
        }
        match &rec.subject {
            GateSubject::Adapter { sha256, .. } if *sha256 == sha => {}
            _ => bail!("the approval does not match the installed {slug} {version} (changed after approval)"),
        }
        if self.gates.state(&rec.gate_id)?.as_deref() != Some("closed") {
            bail!("gate {} is not closed", rec.gate_id);
        }
        let ptr = self.store.activate(slug, version, &self.who())?;
        self.audit(slug, json!({"event": "activate_gate", "version": version, "gate_id": rec.gate_id, "mode": d.decision}))?;
        let reload = self.hub().map(|_| "ok").unwrap_or("failed");
        Ok(json!({"active": ptr, "mode": d.decision, "reload": reload}))
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
        match self.version_decision(slug, &prev)? {
            Some((_, d)) if d.decision.allows_activation() => {}
            _ => bail!("{slug} {prev} has no recorded approval; deactivate instead"),
        }
        Ok(json!({"active": self.store.rollback(slug, &self.who())?}))
    }
}

#[cfg(test)]
mod tests;
