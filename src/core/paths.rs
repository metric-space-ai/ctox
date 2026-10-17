//! Centralized runtime database paths.
//!
//! Core mission and LCM state share [`core_db`] = `ctox.sqlite3` below the
//! effective state root. Mission-side tables (queue, tickets, governance,
//! channels, schedule, plans, approval nag, knowledge) and LCM-side
//! tables (messages, summaries, continuity, mission state, verification,
//! claims) share that file. [`mission_db`] and [`lcm_db`] are thin aliases
//! that exist for call-site clarity — both resolve to the same path.
//!
//! Business OS, sync, runtime configuration and secrets have separate files.
//! Every helper here respects `CTOX_STATE_ROOT`; without it, state is below
//! `root/runtime`. Helpers do not search or import data from another root.
//!
//! Historical `cto_agent.db` and `ctox_lcm.db` paths remain exposed only so
//! compatibility migration code can detect and import them when present.
//!
//! Tool-owned sqlite stores keep their own files and are exposed here so
//! `ctox source-status` and other callers can locate them without duplicating
//! string literals.

use std::path::{Path, PathBuf};

pub fn runtime_dir(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir_with_state_root(root, std::env::var_os("CTOX_STATE_ROOT").as_deref())
}

/// Explicit resolver for store construction and tests; never probes legacy locations.
pub fn runtime_dir_with_state_root(
    root: impl AsRef<Path>,
    state_root: Option<&std::ffi::OsStr>,
) -> PathBuf {
    state_root
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| root.as_ref().join("runtime"))
}

pub fn backup_dir(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir(root).join("backup")
}

/// The consolidated core state database.
pub fn core_db(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir(root).join("ctox.sqlite3")
}

pub fn sync_db(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir(root).join("business-os-rxdb.sqlite3")
}

pub fn business_os_db(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir(root).join("business-os.sqlite3")
}

pub fn runtime_config_db(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir(root).join("ctox-runtime.sqlite3")
}

pub fn secrets_db(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir(root).join("ctox-secrets.sqlite3")
}

pub fn office_db(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir(root).join("ctox-office.sqlite3")
}

pub fn maintenance_db(root: impl AsRef<Path>) -> PathBuf {
    runtime_dir(root).join("ctox-maintenance.sqlite3")
}

/// The caller supplies an already validated scope component (never a path).
pub fn authority_db(root: impl AsRef<Path>, scope: &str) -> Option<PathBuf> {
    if scope.is_empty()
        || !scope
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return None;
    }
    Some(
        runtime_dir(root)
            .join("ctox-sync")
            .join(scope)
            .join("authority.sqlite3"),
    )
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

#[cfg(test)]
mod path_tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn v4_state_root_is_explicit_and_does_not_fall_back_to_legacy_data() {
        let root = Path::new("install");
        assert_eq!(
            runtime_dir_with_state_root(root, None),
            root.join("runtime")
        );
        assert_eq!(
            runtime_dir_with_state_root(root, Some(OsStr::new(""))),
            root.join("runtime")
        );
        assert_eq!(
            runtime_dir_with_state_root(root, Some(OsStr::new("separate-state"))),
            PathBuf::from("separate-state")
        );
    }

    #[test]
    fn v4_authority_scope_cannot_escape_state_root() {
        for invalid in ["", "..", "a/b", "a\\b", "/root", "a.b"] {
            assert!(authority_db("install", invalid).is_none());
        }
        assert_eq!(
            authority_db("install", "scope-123_abc"),
            Some(runtime_dir("install").join("ctox-sync/scope-123_abc/authority.sqlite3"))
        );
    }
}
