//! Persisted native host configuration. Credentials and live IPC state never live
//! in this record; the operator host supplies them from its secret/runtime store.
use crate::authority::{auth::SigningIdentity, NodeId};
use rusqlite::{Connection, OptionalExtension};
#[cfg(feature = "webrtc")]
use std::collections::BTreeMap;
use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
};

pub use crate::contracts::{
    SyncHostConfiguration as HostConfiguration, SyncHostMember as HostMember,
};
impl HostConfiguration {
    pub fn node_id(&self) -> NodeId {
        match &self.local {
            HostMember::Voter { node_id } => *node_id,
            HostMember::Worker { member } => member.node_id,
        }
    }
    pub fn identity(&self) -> io::Result<&str> {
        match &self.local {
            HostMember::Voter { node_id } => self
                .voters
                .get(node_id)
                .map(|peer| peer.identity.as_str())
                .ok_or_else(invalid),
            HostMember::Worker { member } => Ok(&member.identity),
        }
    }
    pub fn validate(&self) -> io::Result<()> {
        let safe_id = |id: u64| id > 0 && id <= 9_007_199_254_740_991;
        if self.version != 1
            || self.scope_id.is_empty()
            || self.scope_id.len() > 128
            || !self
                .scope_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
            || self.voters.len() != 3
            || !safe_id(self.node_id())
            || self.voters.keys().any(|id| !safe_id(*id))
        {
            return Err(invalid());
        }
        let identities: BTreeSet<_> = self
            .voters
            .values()
            .map(|peer| peer.identity.as_str())
            .collect();
        if identities.len() != 3
            || self
                .voters
                .values()
                .filter(|peer| peer.executor && peer.data_replica)
                .count()
                < 2
        {
            return Err(invalid());
        }
        for identity in identities.iter().copied().chain([self.identity()?]) {
            crate::authority::auth::public_key(identity)?;
        }
        if let HostMember::Worker { member } = &self.local {
            if member.revoked
                || self.voters.contains_key(&member.node_id)
                || identities.contains(member.identity.as_str())
            {
                return Err(invalid());
            }
        }
        self.timing.raft_config()?;
        Ok(())
    }
    pub fn validate_key(&self, key: &SigningIdentity) -> io::Result<()> {
        self.validate()?;
        if self.identity()? != key.public_identity() {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn room(&self) -> String {
        format!("ctox-execution:{}", self.scope_id)
    }
    pub fn directory(&self, root: &Path) -> io::Result<PathBuf> {
        self.validate()?;
        if !root.is_absolute() {
            return Err(invalid());
        }
        Ok(root.join("runtime").join("ctox-sync").join(&self.scope_id))
    }
    /// Immutable identity binding. Reconfiguration may update transport secrets,
    /// but moving existing authority data to another identity needs a migration.
    pub fn same_binding(&self, other: &Self) -> bool {
        self.scope_id == other.scope_id && self.local == other.local && self.voters == other.voters
    }
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "invalid native Sync host configuration or identity binding",
    )
}

#[cfg(feature = "webrtc")]
impl HostConfiguration {
    /// Build the existing native attachment; transport and admission remain host-owned.
    pub fn voter_options(
        &self,
        root: &Path,
        ipc_directory: &Path,
        key: &SigningIdentity,
    ) -> io::Result<crate::native_execution::ExecutionGroupOptions> {
        self.validate_key(key)?;
        let HostMember::Voter { node_id } = self.local else {
            return Err(invalid());
        };
        let directory = self.directory(root)?;
        if !ipc_directory.is_absolute() {
            return Err(invalid());
        }
        Ok(crate::native_execution::ExecutionGroupOptions {
            timing: self.timing,
            node_id,
            scope_id: self.scope_id.clone(),
            room: self.room(),
            peers: self.voters.clone(),
            routes: BTreeMap::new(),
            store_path: directory.join("authority.sqlite3"),
            ipc_directory: ipc_directory.to_path_buf(),
        })
    }

    pub fn worker_options(
        &self,
        ipc_directory: &Path,
        key: &SigningIdentity,
    ) -> io::Result<crate::native_execution::WorkerExecutionOptions> {
        self.validate_key(key)?;
        let HostMember::Worker { member } = &self.local else {
            return Err(invalid());
        };
        if !ipc_directory.is_absolute() {
            return Err(invalid());
        }
        Ok(crate::native_execution::WorkerExecutionOptions {
            member: member.clone(),
            scope_id: self.scope_id.clone(),
            room: self.room(),
            voters: self.voters.clone(),
            routes: BTreeMap::new(),
            ipc_directory: ipc_directory.to_path_buf(),
        })
    }
}

/// Uses the host's existing runtime SQLite database, without creating a second
/// runtime-settings writer or persisting credentials into the public record.
pub fn load(connection: &Connection) -> io::Result<Option<HostConfiguration>> {
    let exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ctox_sync_host')", [], |r| r.get(0)).map_err(io::Error::other)?;
    if !exists {
        return Ok(None);
    }
    let text: Option<String> = connection
        .query_row(
            "SELECT configuration FROM ctox_sync_host WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(io::Error::other)?;
    text.map(|text| {
        let config: HostConfiguration = serde_json::from_str(&text).map_err(io::Error::other)?;
        config.validate()?;
        Ok(config)
    })
    .transpose()
}
pub fn save(connection: &mut Connection, config: &HostConfiguration) -> io::Result<()> {
    config.validate()?;
    let tx = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(io::Error::other)?;
    if let Some(previous) = load(&tx)? {
        if !previous.same_binding(config) {
            return Err(invalid());
        }
    }
    tx.execute_batch("CREATE TABLE IF NOT EXISTS ctox_sync_host (singleton INTEGER PRIMARY KEY CHECK(singleton=1), configuration TEXT NOT NULL)").map_err(io::Error::other)?;
    tx.execute("INSERT INTO ctox_sync_host(singleton, configuration) VALUES (1,?1) ON CONFLICT(singleton) DO UPDATE SET configuration=excluded.configuration", [serde_json::to_string(config).map_err(io::Error::other)?]).map_err(io::Error::other)?;
    tx.commit().map_err(io::Error::other)
}
