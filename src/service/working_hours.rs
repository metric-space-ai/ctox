//! Working-hours window snapshot + admission gate.
//!
//! This module is a hygiene stub introduced to complete the merge of the
//! "Harden CTOX ticket queue orchestration" commit, which added call sites
//! referencing `crate::service::working_hours::*` to `service.rs` without
//! also adding the module file. The merged `ServiceStatus` struct carries
//! a `work_hours: WorkHoursSnapshot` field, the dispatcher consults
//! `accepts_work` / `hold_reason`, and snapshot constructors call
//! `snapshot`.
//!
//! Until the full working-hours policy lands as a follow-up change, this
//! stub preserves the pre-merge behaviour:
//!
//! * `WorkHoursSnapshot::default()` is empty and serialises as `{}` so
//!   wire-format readers stay forward-compatible with the eventual
//!   policy-aware payload.
//! * `accepts_work` always returns `true`, so the queue dispatcher keeps
//!   pulling work the same way it did before the merge.
//! * `hold_reason` always returns `None`, so the start-gate guards never
//!   block service start or job lease.
//!
//! No logic change relative to the pre-`73b37b3fd` revision; this purely
//! restores compilation of the merged tree.
//!
//! When the full policy lands it can replace these definitions in place
//! without touching any of the existing call sites.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Snapshot of the working-hours window for diagnostic surfaces
/// (TUI status panel, status JSON wire). Currently a placeholder; the
/// full policy is tracked separately and will populate this struct
/// without touching call sites.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkHoursSnapshot {}

/// Capture the current working-hours window state for the given service
/// root. Always returns an empty snapshot until the policy lands.
pub fn snapshot(_root: &Path) -> WorkHoursSnapshot {
    WorkHoursSnapshot::default()
}

/// Whether the work-hours dispatcher should currently lease new jobs.
/// Always `true` until the policy lands — preserves pre-merge behaviour.
pub fn accepts_work(_root: &Path) -> bool {
    true
}

/// If the working-hours policy is currently holding work, return a
/// human-readable reason; otherwise `None`. Always `None` until the
/// policy lands — preserves pre-merge behaviour.
pub fn hold_reason(_root: &Path) -> Option<String> {
    None
}
