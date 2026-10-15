//! Per-subscription projection at the final frame boundary, independent of transport.
use super::{Projection, ResolvedScope, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ProjectedChange {
    Put {
        collection: String,
        id: String,
        #[serde(with = "super::decimal")]
        seq: u64,
        doc: Value,
    },
    Unproject {
        collection: String,
        id: String,
        #[serde(with = "super::decimal")]
        seq: u64,
    },
}
pub fn project(
    projection: Option<&Projection>,
    collection: &str,
    id: &str,
    seq: u64,
    document: Option<&Value>,
) -> Result<ProjectedChange> {
    if let (Some(projection), Some(doc)) = (projection, document) {
        if projection.collection == collection && projection.row_filter.matches(doc) {
            return Ok(ProjectedChange::Put {
                collection: collection.into(),
                id: id.into(),
                seq,
                doc: projection.field_mask.apply(doc)?,
            });
        }
    }
    // Emit even when this subscriber may never have held the row. No history
    // cache is needed; the client ignores unproject for an absent document.
    Ok(ProjectedChange::Unproject {
        collection: collection.into(),
        id: id.into(),
        seq,
    })
}

/// Scope updates and final SCTP handoff share this fence. A rights transaction
/// commits inside update_committed; an already-running handoff is at most one
/// frame, and no later frame can start with old rights. Do not queue raw frames
/// outside this fence or await network I/O while holding it.
pub struct FrameAuthority(Mutex<Option<ResolvedScope>>);
impl FrameAuthority {
    pub fn new(scope: ResolvedScope) -> Self {
        Self(Mutex::new(Some(scope)))
    }
    pub fn update_committed(
        &self,
        commit: impl FnOnce(&ResolvedScope) -> Result<ResolvedScope>,
    ) -> Result<()> {
        let mut current = self.0.lock().map_err(|_| "rights authority poisoned")?;
        let previous = current
            .take()
            .ok_or("rights authority unavailable; resubscribe")?;
        // If publication fails or returns an invalid version, fail closed. A
        // possibly committed transaction can never leave old frame authority live.
        let next = commit(&previous)?;
        if next.rights_ver <= previous.rights_ver {
            return Err("rights version must increase".into());
        }
        *current = Some(next);
        Ok(())
    }
    pub fn with_frame<T>(
        &self,
        expected_version: u64,
        expected_hash: &str,
        send_one_frame: impl FnOnce(&ResolvedScope) -> Result<T>,
    ) -> Result<T> {
        let scope = self.0.lock().map_err(|_| "rights authority poisoned")?;
        let scope = scope
            .as_ref()
            .ok_or("rights authority unavailable; resubscribe")?;
        if scope.rights_ver != expected_version || scope.hash()? != expected_hash {
            return Err("revoked".into());
        }
        send_one_frame(&scope)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactExposure {
    pub hash: String,
    pub collection: String,
    pub row_id: String,
    pub field_path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeRevoke {
    pub rights_ver: u64,
    /// Full replacement predicates/masks permit row and field cleanup, including
    /// absent collections. Consumers apply this atomically with checkpoint reset.
    pub remaining: Vec<Projection>,
    pub artifacts: Vec<String>,
    pub outbox_reason: String,
}
impl ScopeRevoke {
    pub fn new(
        scope: &ResolvedScope,
        held_rows: &std::collections::BTreeMap<(String, String), Value>,
        artifacts: &[ArtifactExposure],
    ) -> Self {
        let mut revoked: Vec<_> = artifacts
            .iter()
            .filter(|artifact| {
                let Some(projection) = scope.projection(&artifact.collection) else {
                    return true;
                };
                let Some(doc) =
                    held_rows.get(&(artifact.collection.clone(), artifact.row_id.clone()))
                else {
                    return true;
                };
                !projection.row_filter.matches(doc)
                    || !projection.field_mask.allows(&artifact.field_path)
            })
            .map(|a| a.hash.clone())
            .collect();
        revoked.sort();
        revoked.dedup();
        Self {
            rights_ver: scope.rights_ver,
            remaining: scope.projections.clone(),
            artifacts: revoked,
            outbox_reason: "revoked".into(),
        }
    }
    pub fn clean_document(&self, collection: &str, doc: &Value) -> Result<Option<Value>> {
        match self.remaining.iter().find(|p| p.collection == collection) {
            Some(p) if p.row_filter.matches(doc) => Ok(Some(p.field_mask.apply(doc)?)),
            _ => Ok(None),
        }
    }
}

/// Core rejection, not a silent deletion of an optimistic write. The client
/// moves its original payload to archive_local when applying this receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokedWrite {
    pub op_id: String,
    pub status: String,
    pub reason: String,
}
impl RevokedWrite {
    pub fn new(op_id: impl Into<String>) -> Self {
        Self {
            op_id: op_id.into(),
            status: "rejected".into(),
            reason: "revoked".into(),
        }
    }
}
