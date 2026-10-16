//! `ctox toolhub ...`: host glue for the adapter builder (RFC
//! `docs/rfcs/toolhub-adapter-builder.md`). Secrets, approval gates and
//! Business OS policy come from the core; the builder logic lives in the
//! `ctox-toolhub` crate. Outbound HTTPS happens only here, in core: worker
//! sandboxes reach this through the daemon relay (`sandboxed_cli_command_allowed`).

use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use ctox_toolhub::builder::{
    roles::{self, AgentRole},
    ApprovalGates, Builder, Decision, GateRequest, Permission, PolicyGate, Probe,
};
use ctox_toolhub::http::{TransportOptions, UreqTransport};
use ctox_toolhub::{SecretBackend, SecretEntry};
use serde_json::{json, Value};

use crate::business_os::policy::{self, BusinessOsActor, BusinessOsPermission, BusinessOsScope};
use crate::mission::tickets;

pub(crate) struct CtoxSecrets {
    root: std::path::PathBuf,
}

impl SecretBackend for CtoxSecrets {
    fn put(&self, scope: &str, name: &str, value: &str, metadata: Value) -> Result<()> {
        crate::secrets::write_secret_record(&self.root, scope, name, value, None, metadata).map(|_| ())
    }
    fn get(&self, scope: &str, name: &str) -> Result<Option<String>> {
        crate::secrets::read_secret_value_optional(&self.root, scope, name)
            .map_err(|e| anyhow::anyhow!("secret store unavailable ({e:?})"))
    }
    fn delete(&self, scope: &str, name: &str) -> Result<()> {
        crate::secrets::delete_secret_record(&self.root, scope, name)
    }
    fn list(&self) -> Result<Vec<SecretEntry>> {
        Ok(crate::secrets::list_secret_records(&self.root, None)?
            .into_iter()
            .map(|r| SecretEntry { scope: r.scope, name: r.secret_name, metadata: r.metadata })
            .collect())
    }
}

impl CtoxSecrets {
    pub(crate) fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf() }
    }
}

/// Approval gates are the existing `approval-gate` internal work items.
pub(crate) struct TicketGates<'a> {
    root: &'a Path,
}

impl ApprovalGates for TicketGates<'_> {
    fn open(&self, request: &GateRequest) -> Result<String> {
        let item = tickets::put_ticket_self_work_item(
            self.root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "internal".into(),
                kind: "approval-gate".into(),
                title: request.title.clone(),
                body_text: request.body.clone(),
                state: "open".into(),
                metadata: request.metadata.clone(),
            },
            false,
        )?;
        Ok(item.work_id)
    }
    fn state(&self, gate_id: &str) -> Result<Option<String>> {
        Ok(tickets::load_ticket_self_work_item(self.root, gate_id)?
            .filter(|item| item.kind == "approval-gate")
            .map(|item| item.state))
    }
    fn decide(&self, gate_id: &str, approved: bool) -> Result<()> {
        tickets::set_ticket_self_work_state(self.root, gate_id, if approved { "closed" } else { "failed" })
            .map(|_| ())
    }
}

/// `policy::evaluate` for the acting Business OS role.
pub(crate) struct CtoxPolicy {
    actor: BusinessOsActor,
}

impl CtoxPolicy {
    pub(crate) fn new(actor: &str, role: &str) -> Self {
        Self { actor: BusinessOsActor::new(Some(actor.to_string()), role) }
    }
}

impl PolicyGate for CtoxPolicy {
    fn check(&self, permission: Permission) -> Result<()> {
        let p = match permission {
            Permission::DataRead => BusinessOsPermission::DataRead,
            Permission::DataWrite => BusinessOsPermission::DataWrite,
            Permission::ExternalApprove => BusinessOsPermission::ExternalApprove,
            Permission::IntegrationsManage => BusinessOsPermission::IntegrationsManage,
        };
        let d = policy::evaluate(&self.actor, p, &BusinessOsScope::workspace());
        if d.allowed {
            Ok(())
        } else {
            bail!("policy_denied: {} ({})", d.display_reason, d.reason_code)
        }
    }
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn positional(args: &[String], index: usize) -> Option<&str> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i].starts_with("--") {
            i += if matches!(args[i].as_str(), "--include-catalog") { 1 } else { 2 };
            continue;
        }
        out.push(args[i].as_str());
        i += 1;
    }
    out.get(index).copied()
}

fn json_arg(args: &[String], inline: &str, file: &str) -> Result<Option<Value>> {
    if let Some(text) = flag(args, inline) {
        return Ok(Some(serde_json::from_str(text).with_context(|| format!("{inline} is not JSON"))?));
    }
    if let Some(path) = flag(args, file) {
        let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
        return Ok(Some(serde_json::from_str(&text).with_context(|| format!("{file} is not JSON"))?));
    }
    Ok(None)
}

fn req<'a>(v: Option<&'a str>, what: &str) -> Result<&'a str> {
    v.with_context(|| format!("missing {what}"))
}

/// Actor and Business OS role: from a verified command session, otherwise the
/// local operator (shell on the instance = owner).
fn resolve_actor(root: &Path, args: &[String]) -> Result<(String, String)> {
    if let Some(token) = flag(args, "--command-session") {
        let s = crate::business_os::mcp_channel::verify_internal_command_session_token(root, token)?;
        let actor = s.get("actor").and_then(Value::as_str).unwrap_or("session").to_string();
        let role = s.get("role").and_then(Value::as_str).unwrap_or("user").to_string();
        return Ok((actor, role));
    }
    Ok(("local-operator".into(), "chef".into()))
}

pub(crate) fn handle_toolhub_command(root: &Path, args: &[String]) -> Result<()> {
    let out = run(root, args, None)?;
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

pub(crate) const USAGE: &str = "usage: ctox toolhub [--as supervisor|worker|oneshot|mcp] <search|get-schema|list|execute|draft-spec|validate|live-test|submit|approve|activate|deactivate|rollback|request-adapter|rules> ...";

/// `transport` is injectable for tests; production uses ureq with the
/// SSRF-guarded options.
pub(crate) fn run(
    root: &Path,
    args: &[String],
    transport: Option<Arc<dyn ctox_toolhub::http::HttpTransport>>,
) -> Result<Value> {
    // Least privilege: without --as the caller is a one-shot worker.
    let role = AgentRole::parse(flag(args, "--as").unwrap_or("oneshot"))?;
    let Some(cmd) = positional(args, 0) else { bail!(USAGE) };
    if cmd == "rules" {
        return Ok(json!({"role": role.as_str(), "rules": roles::role_rules(role)}));
    }
    let (actor, bos_role) = resolve_actor(root, args)?;
    let transport = transport.unwrap_or_else(|| {
        Arc::new(UreqTransport::with_options(TransportOptions {
            fake_ip_egress: crate::execution::models::runtime_env::config_flag(root, "CTOX_TOOLHUB_FAKE_IP_EGRESS"),
        }))
    });
    let gates = TicketGates { root };
    let policy = CtoxPolicy { actor: BusinessOsActor::new(Some(actor.clone()), &bos_role) };
    let mut b = Builder::new(root, Arc::new(CtoxSecrets { root: root.to_path_buf() }), transport, &gates, &policy, role, &actor);
    if let Some(p) = flag(args, "--project") {
        b = b.with_project(p);
    }
    let p1 = || req(positional(args, 1), "argument");
    let p2 = || req(positional(args, 2), "version");
    match cmd {
        "search" => b.search(p1()?, has(args, "--include-catalog"), flag(args, "--limit").and_then(|l| l.parse().ok()).unwrap_or(20)),
        "get-schema" => b.get_schema(p1()?),
        "list" => b.list(),
        "execute" => {
            let a = json_arg(args, "--args-json", "--args-file")?.unwrap_or_else(|| json!({}));
            b.execute(p1()?, &a, flag(args, "--account"), flag(args, "--approval-gate"))
        }
        "draft-spec" => {
            if let Some(slug) = flag(args, "--from-catalog") {
                b.draft_from_catalog(slug)
            } else {
                let spec = json_arg(args, "--spec-json", "--spec-file")?.context("missing --spec-json/--spec-file/--from-catalog")?;
                b.draft_spec(&serde_json::to_string(&spec)?)
            }
        }
        "validate" => b.validate(p1()?, p2()?),
        "live-test" => {
            let probes: Vec<Probe> = serde_json::from_value(
                json_arg(args, "--probes-json", "--probes-file")?.context("missing --probes-json/--probes-file")?,
            )?;
            b.live_test(p1()?, p2()?, &probes, flag(args, "--account"))
        }
        "submit" => b.submit_for_approval(p1()?, p2()?),
        "approve" => b.approve(
            req(flag(args, "--gate"), "--gate")?,
            Decision::parse(flag(args, "--decision").unwrap_or("approve"))?,
            req(flag(args, "--user-quote"), "--user-quote")?,
        ),
        "activate" => b.activate(p1()?, p2()?),
        "deactivate" => b.deactivate(p1()?),
        "rollback" => b.rollback(p1()?),
        "request-adapter" => request_adapter(root, role, p1()?, flag(args, "--reason").unwrap_or("")),
        other => bail!("unknown toolhub command {other:?}. {USAGE}"),
    }
}

/// Builder mission: an internal work item that a worker picks up with the
/// `adapter-builder` skill. One open mission per service (dedupe key).
fn request_adapter(root: &Path, role: AgentRole, service: &str, reason: &str) -> Result<Value> {
    if !matches!(role, AgentRole::Supervisor | AgentRole::Worker) {
        bail!("not_allowed: {}", roles::access_missing(role, service).1);
    }
    let slug: String = service
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() || slug.len() > 64 {
        bail!("service name must be 1-64 letters, digits or dashes");
    }
    let dedupe_key = format!("toolhub-adapter-build:{slug}");
    let items = tickets::list_ticket_self_work_items(root, Some("internal"), None, 500)?;
    if let Some(item) = items.iter().find(|i| {
        i.metadata["dedupe_key"] == json!(dedupe_key) && !matches!(i.state.as_str(), "closed" | "failed" | "cancelled")
    }) {
        return Ok(json!({"work_id": item.work_id, "state": item.state, "existing": true}));
    }
    let item = tickets::put_ticket_self_work_item(
        root,
        tickets::TicketSelfWorkUpsertInput {
            source_system: "internal".into(),
            kind: "adapter-build".into(),
            title: format!("Adapter bauen: {slug} / Build adapter: {slug}"),
            body_text: format!(
                "Baue einen ToolHub-Adapter für `{slug}` mit dem Skill adapter-builder: Katalog prüfen (`ctox toolhub search --include-catalog`), sonst Spec entwerfen, validieren, live testen, zur Freigabe vorlegen. Aktivieren nur nach Freigabe durch den Nutzer.\n\nBuild a ToolHub adapter for `{slug}` with the adapter-builder skill: check the catalog, else draft, validate, live-test and submit the spec. Activate only after the user approves.\n\nGrund / Reason: {reason}"
            ),
            state: "open".into(),
            metadata: json!({"skill": "adapter-builder", "service": slug, "reason": reason, "dedupe_key": dedupe_key, "requested_by_role": role.as_str()}),
        },
        false,
    )?;
    Ok(json!({"work_id": item.work_id, "state": item.state, "existing": false, "skill": item.suggested_skill}))
}

#[cfg(test)]
#[path = "toolhub_cli_tests.rs"]
mod tests;
