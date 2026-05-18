//! Centralized runtime database paths.
//!
//! All core state lives in a single consolidated sqlite file, [`core_db`] =
//! `runtime/ctox.sqlite3`. Mission-side tables (queue, tickets, governance,
//! secrets, channels, schedule, plans, approval nag, knowledge) and LCM-side
//! tables (messages, summaries, continuity, mission state, verification,
//! claims) share that file. [`mission_db`] and [`lcm_db`] are thin aliases
//! that exist for call-site clarity — both resolve to the same path.
//!
//! Historical `cto_agent.db` and `ctox_lcm.db` paths remain exposed only so
//! compatibility migration code can detect and import them when present.
//!
//! Tool-owned sqlite stores keep their own files and are exposed here so
//! `ctox source-status` and other callers can locate them without duplicating
//! string literals.

use std::path::{Path, PathBuf};

pub fn runtime_dir(root: impl AsRef<Path>) -> PathBuf {
    if let Some(state_root) = std::env::var_os("CTOX_STATE_ROOT")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return state_root;
    }
    root.as_ref().join("runtime")
}

pub fn backup_dir(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir(root).join("backup")
}

/// The consolidated core state database.
pub fn core_db(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir(root).join("ctox.sqlite3")
}

/// Alias of [`core_db`] used by mission-side call sites for readability.
pub fn mission_db(root: impl AsRef<Path>) -> PathBuf {
    core_db(root)
}

/// Alias of [`core_db`] used by LCM-side call sites for readability.
pub fn lcm_db(root: impl AsRef<Path>) -> PathBuf {
    core_db(root)
}

/// Legacy `runtime/cto_agent.db` — used only by the one-shot merge migration.
pub fn legacy_mission_db(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir(root).join("cto_agent.db")
}

/// Legacy `runtime/ctox_lcm.db` — used only by the one-shot merge migration.
pub fn legacy_lcm_db(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir(root).join("ctox_lcm.db")
}

/// Tool-owned store for the scrape capability. Stays separate from the core
/// consolidation by design (tools may keep their own sqlite files).
pub fn scrape_db(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir(root).join("ctox_scraping.db")
}

/// Tool-owned store for the local ticket adapter. Stays separate for the same
/// reason as [`scrape_db`].
pub fn ticket_local_db(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir(root).join("ticket_local.db")
}
