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
    /// The core never lets an agent close an `approval-gate` itself: only an
    /// authorized owner reply (`[ctox-approve:<id>]`, ledger proof) does. So
    /// an approval is accepted only once the gate is already closed that way.
    fn decide(&self, gate_id: &str, approved: bool) -> Result<()> {
        let state = self.state(gate_id)?.context("approval gate not found")?;
        if approved {
            if state != "closed" {
                bail!("waiting_for_user: gate {gate_id} is `{state}`. The user approves by answering `[ctox-approve:{gate_id}]`; record the decision after that.");
            }
            return Ok(());
        }
        if state == "open" {
            // Rejecting is always safe; if the core refuses the transition the
            // toolhub decision record still blocks activation.
            let _ = tickets::set_ticket_self_work_state_with_failure_reason(
                self.root, gate_id, "failed", Some("toolhub: user rejected the adapter"),
            );
        }
        Ok(())
    }

    fn builder_mission_service(&self, work_id: &str) -> Result<Option<String>> {
        Ok(tickets::load_ticket_self_work_item(self.root, work_id)?
            .filter(|item| item.kind == BUILDER_MISSION_KIND && !matches!(item.state.as_str(), "closed" | "failed" | "cancelled"))
            .and_then(|item| item.metadata["service"].as_str().map(str::to_string)))
    }

    /// Closes through the core's own terminal grant, which re-checks the
    /// active sha256 and the mission-bound live-test proof in the store.
    fn close_builder_mission(&self, work_id: &str, slug: &str, sha256: &str, summary: &str) -> Result<()> {
        tickets::close_adapter_build_mission_after_activation(self.root, work_id, slug, sha256, summary)?;
        let _ = tickets::append_ticket_self_work_note(self.root, work_id, summary, "toolhub", "internal");
        Ok(())
    }
}

/// Internal work-item kind of a builder mission (`request-adapter`).
pub(crate) const BUILDER_MISSION_KIND: &str = "adapter-build";

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

pub(crate) const USAGE: &str = "usage: ctox toolhub [--as supervisor|worker|oneshot|mcp] <search|get-schema|list|execute|draft-spec|validate|live-test|approve|activate|deactivate|rollback|request-adapter|rules|config> ...\n  draft-spec --from-openapi <https-url|file> [--slug s] [--version v] [--max-actions n] [--operations a,b]\n  live-test <slug> <version> [--probes-json|--probes-file] [--mission <work_id>]   (no probes: the drafted ones)\n  config show | config set fake-ip-egress on|off   (supervisor)";

/// Instance config key for the fake-IP egress transport option.
const FAKE_IP_KEY: &str = "CTOX_TOOLHUB_FAKE_IP_EGRESS";

/// `ctox toolhub config ...`: typed instance settings of the toolhub (stored in
/// the runtime config store, never env vars).
fn config_command(root: &Path, role: AgentRole, policy: &CtoxPolicy, args: &[String]) -> Result<Value> {
    use crate::execution::models::runtime_env;
    let show = || json!({"fake_ip_egress": runtime_env::config_flag(root, FAKE_IP_KEY)});
    match (positional(args, 1), positional(args, 2), positional(args, 3)) {
        (Some("show") | None, _, _) => Ok(show()),
        (Some("set"), Some("fake-ip-egress"), Some(v)) => {
            if role != AgentRole::Supervisor {
                bail!("not_allowed: only the supervisor (operator) changes toolhub settings");
            }
            policy.check(Permission::IntegrationsManage)?;
            match v {
                "on" | "true" | "1" => runtime_env::set_runtime_env_value(root, FAKE_IP_KEY, "1")?,
                "off" | "false" | "0" => runtime_env::clear_runtime_env_value(root, FAKE_IP_KEY)?,
                o => bail!("fake-ip-egress takes on|off, got `{o}`"),
            }
            Ok(json!({"fake_ip_egress": runtime_env::config_flag(root, FAKE_IP_KEY), "note": "only the 198.18.0.0/15 egress-proxy range is admitted; all other private ranges stay blocked"}))
        }
        _ => bail!("usage: ctox toolhub config show | config set fake-ip-egress on|off"),
    }
}

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
            fake_ip_egress: crate::execution::models::runtime_env::config_flag(root, FAKE_IP_KEY),
        }))
    });
    let gates = TicketGates { root };
    let policy = CtoxPolicy { actor: BusinessOsActor::new(Some(actor.clone()), &bos_role) };
    if cmd == "config" {
        return config_command(root, role, &policy, args);
    }
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
            } else if let Some(src) = flag(args, "--from-openapi") {
                let text = if src.starts_with("https://") || src.starts_with("http://") {
                    b.fetch_document(src)?
                } else {
                    std::fs::read_to_string(src).with_context(|| format!("read {src}"))?
                };
                let opts = ctox_toolhub::spec::openapi::Options {
                    slug: flag(args, "--slug").map(String::from),
                    version: flag(args, "--version").unwrap_or("1.0.0").to_string(),
                    max_actions: flag(args, "--max-actions").and_then(|n| n.parse().ok()).unwrap_or(40),
                    operations: flag(args, "--operations").map(|o| o.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()).unwrap_or_default(),
                    source: src.to_string(),
                };
                b.draft_from_openapi(&text, &opts)
            } else {
                let spec = json_arg(args, "--spec-json", "--spec-file")?.context("missing --spec-json/--spec-file/--from-catalog")?;
                b.draft_spec(&serde_json::to_string(&spec)?)
            }
        }
        "validate" => b.validate(p1()?, p2()?),
        "live-test" => {
            let probes: Vec<Probe> = match json_arg(args, "--probes-json", "--probes-file")? {
                Some(v) => serde_json::from_value(v)?,
                None => b.drafted_probes(p1()?, p2()?)?,
            };
            b.live_test_in_mission(p1()?, p2()?, &probes, flag(args, "--account"), flag(args, "--mission"))
        }
        "approve" => b.approve(
            req(flag(args, "--gate"), "--gate")?,
            Decision::parse(flag(args, "--decision").unwrap_or("approve"))?,
            req(flag(args, "--user-quote"), "--user-quote")?,
        ),
        "activate" => b.activate(p1()?, p2()?, flag(args, "--mission")),
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
    let base_key = format!("toolhub-adapter-build:{slug}");
    let items = tickets::list_ticket_self_work_items(root, Some("internal"), None, 500)?;
    // `<base>` for the first mission, `<base>:<n>` for later ones (a closed
    // mission is never reopened; the work id derives from the dedupe key).
    let ours: Vec<_> = items
        .iter()
        .filter(|i| i.kind == BUILDER_MISSION_KIND)
        .filter(|i| i.metadata["dedupe_key"].as_str().is_some_and(|k| k == base_key || k.strip_prefix(&format!("{base_key}:")).is_some_and(|n| n.parse::<u32>().is_ok())))
        .collect();
    if let Some(item) = ours.iter().find(|i| !matches!(i.state.as_str(), "closed" | "failed" | "cancelled")) {
        return Ok(json!({"work_id": item.work_id, "state": item.state, "existing": true}));
    }
    let dedupe_key = if ours.is_empty() { base_key } else { format!("{base_key}:{}", ours.len() + 1) };
    let item = tickets::put_ticket_self_work_item(
        root,
        tickets::TicketSelfWorkUpsertInput {
            source_system: "internal".into(),
            kind: BUILDER_MISSION_KIND.into(),
            title: format!("Adapter bauen: {slug} / Build adapter: {slug}"),
            body_text: format!(
                "Baue einen ToolHub-Adapter für `{slug}` mit dem Skill adapter-builder: Katalog prüfen (`ctox toolhub search --include-catalog`), sonst Spec entwerfen (mit OpenAPI: `draft-spec --from-openapi <url>`), validieren, mit `--mission <diese work_id>` live testen und selbst aktivieren (ohne Nutzerfreigabe); die Mission schließt sich danach selbst.\n\nBuild a ToolHub adapter for `{slug}` with the adapter-builder skill: check the catalog, else draft (with OpenAPI: `draft-spec --from-openapi <url>`), validate, live-test and activate it yourself with `--mission <this work_id>` (no user approval); the mission closes itself afterwards.\n\nGrund / Reason: {reason}"
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
