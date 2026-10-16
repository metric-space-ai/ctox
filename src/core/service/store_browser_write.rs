//! P6/P8 binding: no preflight permission snapshot, second writer, or early receipt.
use super::*;
use crate::business_os::policy::{self, BusinessOsActor};
use ctox_store::change_log::subscription::protocol::WriteOp;
use rusqlite::OptionalExtension;
use std::sync::Mutex;

pub(super) struct BrowserWrite {
    pub admission: Arc<BrowserAdmission>,
    pub peer: Arc<BrowserPeer>,
    // P8 invokes authorize for every drained ordered request, then validates its
    // final merged document synchronously on the same owning writer thread.
    request: Mutex<Option<Write>>,
}
impl BrowserWrite {
    pub fn new(admission: Arc<BrowserAdmission>, peer: Arc<BrowserPeer>) -> Self {
        Self {
            admission,
            peer,
            request: Mutex::new(None),
        }
    }
    fn actor(&self) -> BusinessOsActor {
        BusinessOsActor::new(Some(self.peer.user.clone()), &self.peer.role)
    }
    fn current(
        &self,
        tx: &rusqlite::Transaction<'_>,
        participant: &str,
    ) -> rusqlite::Result<sync_rights::RightsSnapshot> {
        if participant != self.peer.device || self.peer.expires <= Instant::now() {
            return Err(sql_invalid("expired or foreign browser writer"));
        }
        validate_user(tx, &self.peer.user, &self.peer.role, self.peer.local)
            .map_err(|_| sql_invalid("browser principal revoked"))?;
        let rights = sync_rights::read(tx)?;
        self.admission
            .authority()
            .router
            .with_snapshot(self.peer.rights_ver, &self.peer.epoch, |_, _| {
                if rights.rights_ver != self.peer.rights_ver {
                    return Err("revoked".into());
                }
                Ok(())
            })
            .map_err(sql_invalid)?;
        Ok(rights)
    }
    fn admit(
        &self,
        tx: &rusqlite::Transaction<'_>,
        participant: &str,
        request: &Write,
        after: Option<&Value>,
    ) -> rusqlite::Result<WritePermission> {
        let rights = self.current(tx, participant)?;
        let Some(collection) = ctox_store::schema::active_collection(tx, &request.collection)?
        else {
            return Ok(WritePermission::Reject("forbidden".into()));
        };
        if collection.gen != request.gen || !self.peer.scopes.contains_key(&request.collection) {
            return Ok(WritePermission::Reject("revoked".into()));
        }
        let table = &collection.table;
        if !table
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(sql_invalid("invalid registry table"));
        }
        let raw: Option<String> = tx
            .query_row(
                &format!("SELECT data FROM \"{table}\" WHERE id=?1 AND deleted=0"),
                [&request.id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let before: Option<Value> = raw
            .map(|v| serde_json::from_str(&v))
            .transpose()
            .map_err(|_| sql_invalid("invalid stored document"))?;
        let app = if request.collection == "business_commands" {
            after.and_then(|d| d["app_id"].as_str()).unwrap_or("")
        } else {
            &collection.definition.app_id
        };
        let actor = self.actor();
        if before.as_ref().is_some_and(|doc| {
            policy::read_sync_document(&actor, &rights, app, &request.collection, doc).is_err()
        }) {
            return Ok(WritePermission::Reject("forbidden".into()));
        }
        // The existing fence is entered INSIDE P8's WriteJob, with the actual
        // before and post-merge after. Rights publications use this same writer;
        // none can commit between this validation and the document/ops commit.
        let checked = self.admission.with_write_fence(
            &self.peer,
            &request.collection,
            collection.gen,
            &request.op_id,
            before.as_ref(),
            after,
            || {
                policy::write_sync_document(
                    &actor,
                    &rights,
                    app,
                    &request.collection,
                    before.as_ref(),
                    after,
                    None,
                )
            },
        );
        match checked {
            Ok(()) => Ok(WritePermission::Permit),
            Err(sync_rights::OutboxError::AuthorityUnavailable) => {
                Err(sql_invalid("current outbox authority unavailable"))
            }
            Err(_) => Ok(WritePermission::Reject("revoked".into())),
        }
    }
}
impl WriteAuthority for BrowserWrite {
    fn authorize(
        &self,
        tx: &rusqlite::Transaction<'_>,
        participant: &str,
        request: &Write,
    ) -> rusqlite::Result<WritePermission> {
        self.current(tx, participant)?;
        *self
            .request
            .lock()
            .map_err(|_| sql_invalid("write context poisoned"))? = Some(request.clone());
        if !self.peer.scopes.contains_key(&request.collection) {
            return Ok(WritePermission::Reject("forbidden".into()));
        }
        if request.op == WriteOp::Delete {
            self.admit(tx, participant, request, None)
        } else {
            Ok(WritePermission::Permit)
        }
    }
    fn validate_document(
        &self,
        tx: &rusqlite::Transaction<'_>,
        participant: &str,
        collection: &ctox_store::schema::Collection,
        document: &Value,
    ) -> rusqlite::Result<WritePermission> {
        let request = self
            .request
            .lock()
            .map_err(|_| sql_invalid("write context poisoned"))?
            .clone()
            .ok_or_else(|| sql_invalid("write context missing"))?;
        let schema: rxdb::types::RxJsonSchema =
            serde_json::from_value(collection.definition.schema_json.clone())
                .map_err(|_| sql_invalid("registered write schema unavailable"))?;
        let primary = rxdb::rx_schema_helper::get_primary_field_of_primary_key(&schema.primary_key);
        if rxdb::rx_schema::validate_write_document(&schema, &primary, document).is_err() {
            return Ok(WritePermission::Reject("invalid_input".into()));
        }
        if collection.definition.name != request.collection
            || document["id"].as_str() != Some(request.id.as_str())
        {
            return Ok(WritePermission::Reject("invalid_input".into()));
        }
        self.admit(tx, participant, &request, Some(document))
    }
    fn authorize_command_intent(
        &self,
        tx: &rusqlite::Transaction<'_>,
        participant: &str,
        command: &Value,
    ) -> rusqlite::Result<WritePermission> {
        self.current(tx, participant)?;
        if command["actor_id"].as_str() != Some(self.peer.user.as_str()) {
            return Ok(WritePermission::Reject("forbidden".into()));
        }
        // This admits the queued intent, not an external effect. P8 projects it
        // atomically; the command consumer starts only from the committed feed.
        // Core assignment is never inferred from browser input.
        let admitted = policy::dispatch_sync_command(
            &self.admission.authority().router,
            &self.actor(),
            command["app_id"].as_str().unwrap_or(""),
            command["type"].as_str().unwrap_or(""),
            false,
            |_| Ok(()),
        );
        Ok(if admitted.is_ok() {
            WritePermission::Permit
        } else {
            WritePermission::Reject("forbidden".into())
        })
    }
}
