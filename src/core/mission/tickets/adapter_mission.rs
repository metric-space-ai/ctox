//! Closing a ToolHub builder mission (`adapter-build` work item) after its
//! adapter was activated. The terminal grant is minted only here, after the
//! condition is checked against the toolhub store itself: the adapter is
//! active with exactly `sha256`, and its passing live-test proof for that
//! version names this mission.
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use super::work_items::{load_ticket_self_work_item, set_ticket_self_work_state_internal_with_policy};
use super::{open_ticket_db, record_audit, AuditRequest, TicketSelfWorkItemView, TerminalPolicyGrant};

pub(crate) const ADAPTER_BUILD_KIND: &str = "adapter-build";

fn same_service(a: &str, b: &str) -> bool {
    a.replace('_', "-") == b.replace('_', "-")
}

pub(crate) fn close_adapter_build_mission_after_activation(
    root: &Path,
    work_id: &str,
    slug: &str,
    sha256: &str,
    note: &str,
) -> Result<TicketSelfWorkItemView> {
    let item = load_ticket_self_work_item(root, work_id)?.context("builder mission not found")?;
    if item.kind != ADAPTER_BUILD_KIND {
        bail!("{work_id} is not a builder mission");
    }
    let service = item.metadata["service"].as_str().unwrap_or_default();
    if !same_service(service, slug) {
        bail!("builder mission {work_id} is for `{service}`, not `{slug}`");
    }
    // Establish the condition from the toolhub store, not from the caller.
    let store = ctox_toolhub::spec::SpecStore::for_instance_root(root);
    let active = store.active(slug)?.context("the adapter is not active")?;
    if active.sha256 != sha256 {
        bail!("the active version of {slug} is not the tested one");
    }
    let review_path = store.slug_path(slug)?.join("reviews").join(format!("{}.json", active.version));
    let review: Value = serde_json::from_slice(&std::fs::read(&review_path).context("no live-test proof")?)?;
    if review["passed"] != json!(true) || review["sha256"] != json!(sha256) || review["mission"] != json!(work_id) {
        bail!("the live-test proof of {slug} {} does not belong to mission {work_id}", active.version);
    }
    let grant = TerminalPolicyGrant::adapter_activated_with_proof(slug, sha256);
    let proof = grant.proof();
    let mut conn = open_ticket_db(root)?;
    let closed = set_ticket_self_work_state_internal_with_policy(root, &mut conn, work_id, "closed", None, Some(&grant))?;
    record_audit(
        &conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", closed.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "self_work_state_set",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({"work_id": work_id, "kind": closed.kind, "state": closed.state, "terminal_policy_proof": proof, "note": note}),
        },
    )?;
    Ok(closed)
}
