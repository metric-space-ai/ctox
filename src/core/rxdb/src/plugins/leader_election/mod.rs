//! Port of `src/plugins/leader-election/index.ts`.
//!
//! **T2 single-process stub.** In a single-process CTOX runtime there is
//! always exactly one process per database, so this process is *always* the
//! leader. The upstream `broadcast-channel`-based election mechanism is
//! replaced by trivial constant-true predicates.
//!
//! The plugin's upstream prototype-mutation path
//! (`prototypes.RxDatabase.{leaderElector,isLeader,waitForLeadership}`) is
//! omitted per the global plugin T1 decision; consumers call these helpers
//! directly instead of through the `RxDatabase` prototype.

use crate::plugin::RxPlugin;

// ref: rxdb/src/plugins/leader-election/index.ts:75-80
/// Whether the current process is the database leader.
/// CTOX is always single-process, so this is always true.
pub fn is_leader(multi_instance: bool) -> bool {
    if !multi_instance {
        return true;
    }
    // CTOX does not currently support multi-instance setups.
    // Returning true is the conservative single-process answer; a future
    // multi-process port would consult an actual elector here.
    true
}

// ref: rxdb/src/plugins/leader-election/index.ts:82-90
/// Resolves once the current process is the leader. CTOX is always single-process
/// so this future is immediately ready.
pub async fn wait_for_leadership(_multi_instance: bool) -> bool {
    true
}

// ref: rxdb/src/plugins/leader-election/index.ts:95-100
/// Upstream `onClose(db)` calls `elector.die()` on each registered elector.
/// CTOX has no electors to dismiss; this is a no-op.
pub fn on_close() {}

// ref: rxdb/src/plugins/leader-election/index.ts:102-120
pub struct RxDBLeaderElectionPlugin;

impl RxPlugin for RxDBLeaderElectionPlugin {
    fn name(&self) -> &str {
        "leader-election"
    }
}
