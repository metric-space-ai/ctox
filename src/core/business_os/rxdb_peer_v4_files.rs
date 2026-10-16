//! E30 native file admission. Rows, exports and projected BlobRefs are checked
//! in one canonical reader snapshot before the existing CAS can be opened.
use super::*;
use ctox_store::{cas::Cas, read_pool::ReadPool};
use rusqlite::Transaction;
use rxdb::plugins::replication_webrtc::{
    index_mod::v4_session::{AuthorizedTransferBlob, TransferBlobAccess, TransferGet},
    webrtc_types::WebRTCPublicationGuard,
};
use std::sync::Arc;

#[derive(Clone, Debug)]
struct FileGrant {
    reference: String,
    size: u64,
    mime: String,
    rights_ver: u64,
    epoch: String,
}
fn failure(message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.into())
}
fn transfer_error(code: &str, reason: &str) -> rxdb::rx_error::RxError {
    rxdb::rx_error::new_rx_error(
        "V4_TRANSFER",
        Some(serde_json::json!({"code":code,"reason":reason})),
    )
}
fn identifier(name: &str) -> rusqlite::Result<String> {
    if name.is_empty() || name.contains('\0') {
        return Err(failure("invalid: collection table"));
    }
    Ok(format!("\"{}\"", name.replace('"', "\"\"")))
}
fn any_record(
    tx: &Transaction<'_>,
    name: &str,
    reference: &str,
    mut readable: impl FnMut(&Value) -> bool,
) -> rusqlite::Result<bool> {
    let Some(c) = ctox_store::schema::active_collection(tx, name)? else {
        return Ok(false);
    };
    let table = identifier(&c.table)?;
    // Stream matching candidates in the same snapshot. Arbitrary candidate
    // counts must not revoke an otherwise readable reference; memory stays one row.
    let mut query=tx.prepare(&format!("SELECT data FROM {table} d WHERE deleted=0 AND (id=?1 OR EXISTS(SELECT 1 FROM json_tree(d.data) j WHERE j.key IN ('ref','content_ref') AND j.value=?1)) ORDER BY id"))?;
    let mut rows = query.query([reference])?;
    while let Some(row) = rows.next()? {
        let raw: String = row.get(0)?;
        let doc = serde_json::from_str(&raw).map_err(|_| failure("invalid: file document"))?;
        if readable(&doc) {
            return Ok(true);
        }
    }
    Ok(false)
}
fn holds_ref(value: &Value, reference: &str) -> bool {
    match value {
        Value::Object(o) => {
            o.get("ref").is_some_and(|v| v == reference)
                || o.values().any(|v| holds_ref(v, reference))
        }
        Value::Array(a) => a.iter().any(|v| holds_ref(v, reference)),
        _ => false,
    }
}
fn admit(
    tx: &Transaction<'_>,
    rights: &sync_rights::RightsSnapshot,
    actor: &BusinessOsActor,
    app: &str,
    reference: &str,
) -> rusqlite::Result<FileGrant> {
    let actor_id = actor
        .id
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| failure("forbidden: actor"))?;
    let mut meta = None;
    any_record(tx, "files", reference, |doc| {
        if doc["ref"] == reference && doc["size"].as_u64().is_some() {
            meta = Some(doc.clone());
            true
        } else {
            false
        }
    })?;
    let meta = meta.ok_or_else(|| failure("forbidden: file reference unavailable"))?;
    let visible =
        |collection: &str, doc: &Value| read_sync_document(actor, rights, app, collection, doc);
    let own = meta["owner_app"] == app && meta["owner_user"] == actor_id;
    // E30(a) is an explicit ownership grant, independent of a foreign files export.
    let own_read = own;
    let mut allowed = own_read;
    if !allowed {
        allowed = any_record(tx, "desktop_files", reference, |doc| {
            doc["is_deleted"] != true
                && visible("desktop_files", doc).is_ok_and(|value| {
                    holds_ref(&value, reference) || value["content_ref"] == reference
                })
        })?;
    }
    if !allowed {
        let names=tx.prepare("SELECT name FROM _collections WHERE state='active' AND name NOT IN ('files','desktop_files') ORDER BY name")?.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for name in names {
            if rights
                .resolve_app_collection(app, actor.role.as_str(), actor_id, &name)
                .ok()
                .and_then(|s| s.projection(&name).cloned())
                .is_none()
            {
                continue;
            }
            // Only the actual projected row grants access. Hidden fields and
            // a caller-provided copy of another user's document grant nothing.
            allowed = any_record(tx, &name, reference, |doc| {
                visible(&name, doc).is_ok_and(|projected| holds_ref(&projected, reference))
            })?;
            if allowed {
                break;
            }
        }
    }
    if !allowed {
        return Err(failure(
            "forbidden: file is not owned or referenced by a readable document",
        ));
    }
    let epoch = tx.query_row("SELECT epoch FROM _meta WHERE singleton=1", [], |r| {
        r.get(0)
    })?;
    Ok(FileGrant {
        reference: reference.into(),
        size: meta["size"].as_u64().unwrap(),
        mime: meta["mime"]
            .as_str()
            .unwrap_or("application/octet-stream")
            .into(),
        rights_ver: rights.rights_ver,
        epoch,
    })
}
impl NativeStorePolicy {
    fn file_snapshot(
        &self,
        tx: &Transaction<'_>,
        participant: &str,
        reference: &str,
    ) -> rusqlite::Result<FileGrant> {
        let rights = self.snapshot(tx, participant)?;
        admit(tx, &rights, &self.actor, &self.app_id, reference)
    }
}
/// Constructed by P4 from its verified peer and existing owner readers. No
/// request field supplies an actor, role, app, permission or publication guard.
pub(crate) struct NativeFileAccess {
    readers: ReadPool,
    policy: NativeStorePolicy,
    participant: String,
    cas: Cas,
    session: Arc<dyn WebRTCPublicationGuard>,
}
impl NativeFileAccess {
    pub(crate) fn new(
        readers: ReadPool,
        policy: NativeStorePolicy,
        participant: String,
        cas: Cas,
        session: Arc<dyn WebRTCPublicationGuard>,
    ) -> Self {
        Self {
            readers,
            policy,
            participant,
            cas,
            session,
        }
    }
}
struct FilePublication {
    policy: NativeStorePolicy,
    grant: FileGrant,
    session: Arc<dyn WebRTCPublicationGuard>,
}
impl WebRTCPublicationGuard for FilePublication {
    fn with_current(
        &self,
        publish: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
    ) -> rxdb::rx_error::RxResult<()> {
        let mut result = None;
        self.policy
            .with_page_frame(self.grant.rights_ver, &self.grant.epoch, || {
                self.session
                    .with_current(&mut || {
                        result = Some(publish());
                        Ok(())
                    })
                    .map_err(|_| "revoked".to_string())
            })
            .map_err(|_| transfer_error("forbidden", "file_not_readable"))?;
        result.ok_or_else(|| transfer_error("forbidden", "publication_not_admitted"))?
    }
}
#[async_trait::async_trait]
impl TransferBlobAccess for NativeFileAccess {
    async fn resolve(
        &self,
        request: &TransferGet,
    ) -> rxdb::rx_error::RxResult<AuthorizedTransferBlob> {
        let policy = self.policy.clone();
        let participant = self.participant.clone();
        let reference = request.reference.clone();
        let grant = self
            .readers
            .read(move |tx| policy.file_snapshot(tx, &participant, &reference))
            .await
            .map_err(|_| transfer_error("forbidden", "file_not_readable"))?;
        let cas = self.cas.clone();
        let g = grant.clone();
        let blob = tokio::task::spawn_blocking(move || cas.verified(&g.reference, g.size))
            .await
            .map_err(|_| transfer_error("unavailable", "cas_worker"))?
            .map_err(|_| transfer_error("invalid", "cas_integrity"))?;
        Ok(AuthorizedTransferBlob {
            blob,
            mime: grant.mime.clone(),
            publication: Arc::new(FilePublication {
                policy: self.policy.clone(),
                grant,
                session: self.session.clone(),
            }),
        })
    }
}
#[cfg(test)]
#[path = "rxdb_peer_v4_files_tests.rs"]
mod tests;
