use super::*;
use ctox_app_runtime::core_host::{StoreCodec, StoreSnapshot, StoreWrite};
use ctox_store::change_log::subscription::write_contract;
use rusqlite::Transaction;
use std::io::Read;

pub(crate) struct RuntimeCodec {
    pub signer: super::super::Signer,
    pub cas: Cas,
}
impl RuntimeCodec {
    pub(super) fn manifest(&self, db: &Connection, app: &str) -> Result<Value, StoreError> {
        let app_record =
            document(db, "business_apps", app)?.ok_or_else(|| invalid("not_found: app"))?;
        if app_record["state"] != "active" {
            return Err(invalid("revoked: app removed"));
        }
        self.version(
            db,
            app,
            app_record["active_ver"]
                .as_str()
                .ok_or_else(|| invalid("invalid: active version"))?,
        )
        .map(|v| v["manifest"].clone())
    }
    pub(super) fn check(
        &self,
        db: &Connection,
        binding: &Value,
    ) -> Result<BusinessOsActor, StoreError> {
        let command = &binding["command"];
        let a = actor(db, text(&command["actor"], "id").map_err(sql_error)?)?;
        let parent = text(&binding["fence"], "run_id")
            .map_err(sql_error)?
            .strip_prefix("app-runtime:")
            .ok_or_else(|| invalid("invalid: native run"))?;
        let saved = document(db, "business_commands", parent)?
            .ok_or_else(|| invalid("revoked: command missing"))?;
        if saved["status"] != "running"
            || saved["actor_id"] != command["actor"]["id"]
            || saved["app_id"] != command["app_id"]
            || (command["id"] == parent
                && (saved["type"] != command["type"] || saved["payload"] != command["payload"]))
        {
            return Err(invalid("revoked: committed command changed or cancelled"));
        }
        if actor_json(&a) != command["actor"] {
            return Err(invalid("revoked: actor changed"));
        }
        let native = super::super::commands::handles(text(command, "type").map_err(sql_error)?);
        let manifest = if native {
            json!({"app_id":command["app_id"],"app_ver":"native-platform"})
        } else if command["runtime_completion"] == true {
            self.version(
                db,
                text(command, "app_id").map_err(sql_error)?,
                text(command, "app_ver").map_err(sql_error)?,
            )?["manifest"]
                .clone()
        } else {
            self.manifest(db, text(command, "app_id").map_err(sql_error)?)?
        };
        if manifest["app_ver"] != command["app_ver"]
            || binding.get("manifest").is_some_and(|m| m != &manifest)
        {
            return Err(invalid("revoked: app version changed"));
        }
        let r = rights(db)?;
        if binding["fence"]["rights_ver"].as_u64() != Some(r.rights_ver) {
            return Err(invalid("revoked: rights changed"));
        }
        Ok(a)
    }
    pub(super) fn bytes(&self, hash: &str, size: u64) -> Result<Vec<u8>, StoreError> {
        if size > 64 * 1024 * 1024 {
            return Err(invalid("quota: CAS object"));
        }
        let object = self.cas.verified(hash, size)?;
        let mut bytes = Vec::new();
        self.cas
            .reader(&object)?
            .take(size + 1)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}
impl StoreCodec for RuntimeCodec {
    fn active_manifest(&self, db: &Connection, app: &str) -> Result<Value, StoreError> {
        self.manifest(db, app)
    }
    fn active_shell(&self, db: &Connection) -> Result<Option<Value>, StoreError> {
        let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM _collections WHERE name='business_apps' AND state='active')",[],|r|r.get(0))?;
        if !exists {
            return Ok(None);
        }
        match document(db, "business_apps", "shell")? {
            Some(v) if v["state"] == "active" => Ok(Some(self.manifest(db, "shell")?)),
            _ => Ok(None),
        }
    }
    fn check_fence(&self, tx: &Transaction<'_>, binding: &Value) -> Result<(), StoreError> {
        self.check(tx, binding)?;
        let f = &binding["fence"];
        let parent = text(f, "run_id")
            .map_err(sql_error)?
            .strip_prefix("app-runtime:")
            .ok_or_else(|| invalid("invalid: native execution run"))?;
        write_contract::check_executor_fence(
            tx,
            text(f, "participant").map_err(sql_error)?,
            parent,
            ctox_store::change_log::subscription::protocol::Seq(
                f["fence"]
                    .as_u64()
                    .ok_or_else(|| invalid("invalid: fence"))?,
            ),
            now(),
        )?
        .map_err(|e| invalid(format!("{}: {}", e.code, e.reason)))
    }
    fn read(&self, db: &Connection, binding: &Value, name: &str) -> Result<Vec<Value>, StoreError> {
        let a = self.check(db, binding)?;
        let r = rights(db)?;
        let app = text(&binding["command"], "app_id").map_err(sql_error)?;
        // Authorize the collection even when it contains no rows.
        let scope = r
            .resolve_app_collection(app, a.role.as_str(), a.id.as_deref().unwrap(), name)
            .map_err(sql_error)?;
        if scope.projection(name).is_none() {
            return Err(invalid("forbidden: collection read"));
        }
        let c = collection(db, name)?;
        let mut statement = db.prepare(&format!(
            "SELECT id,rev,data FROM {} WHERE deleted=0 ORDER BY id",
            quote(&c.table)
        ))?;
        let raw = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        let mut size = 0usize;
        for row in raw {
            let (id, rev, data) = row?;
            let mut v: Value = serde_json::from_str(&data).map_err(sql_error)?;
            v["id"] = json!(id);
            if let Ok(mut masked) = policy::read_sync_document(&a, &r, app, name, &v) {
                masked["rev"] = json!(rev);
                size = size.saturating_add(serde_json::to_vec(&masked).map_err(sql_error)?.len());
                if size > 6 * 1024 * 1024 {
                    return Err(invalid("quota: authorized snapshot exceeds IPC budget"));
                }
                out.push(masked);
            }
        }
        Ok(out)
    }
    fn snapshot(
        &self,
        db: &Connection,
        binding: &Value,
        names: &[String],
    ) -> Result<StoreSnapshot, StoreError> {
        let mut collections = BTreeMap::new();
        let mut size = 0usize;
        for name in names {
            let rows = self.read(db, binding, name)?;
            size = size.saturating_add(serde_json::to_vec(&rows).map_err(sql_error)?.len());
            if size > 6 * 1024 * 1024 {
                return Err(invalid("quota: transaction snapshot exceeds IPC budget"));
            }
            collections.insert(name.clone(), rows);
        }
        let (epoch,head):(String,u64)=db.query_row("SELECT epoch,v FROM _meta CROSS JOIN _seq WHERE _meta.singleton=1 AND _seq.singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
        Ok(StoreSnapshot {
            stamp: json!({"epoch":epoch,"head_seq":head,"rights_ver":rights(db)?.rights_ver}),
            collections,
        })
    }
    fn write(
        &self,
        tx: &Transaction<'_>,
        binding: &Value,
        stamp: Option<&Value>,
        operations: &[StoreWrite],
    ) -> Result<Vec<Value>, StoreError> {
        self.check_fence(tx, binding)?;
        let a = self.check(tx, binding)?;
        let r = rights(tx)?;
        let app = text(&binding["command"], "app_id").map_err(sql_error)?;
        if let Some(stamp) = stamp {
            let h = ctox_store::change_log::head(tx)?;
            if stamp["epoch"] != h.epoch
                || stamp["head_seq"] != h.head_seq
                || stamp["rights_ver"] != r.rights_ver
            {
                return Err(invalid("conflict: transaction snapshot changed"));
            }
        }
        let core = origin::delivery_permit(tx, &binding["command"], &a)?;
        let permit = if core {
            None
        } else {
            Some(permit(
                tx,
                app,
                &a,
                text(&binding["command"], "type").map_err(sql_error)?,
            )?)
        };
        let mut results = Vec::new();
        for operation in operations {
            let name = &operation.collection;
            if matches!(
                name.as_str(),
                "business_commands"
                    | "business_apps"
                    | "business_app_versions"
                    | "app_catalog"
                    | "business_users"
            ) {
                return Err(invalid("forbidden: platform collection"));
            }
            let c = collection(tx, name)?;
            if ctox_store::schema::preparing_target(tx, name, c.gen)?.is_some() {
                return Err(invalid("unavailable: generation migration in progress"));
            }
            let op = &operation.op;
            let kind = text(op, "type").map_err(sql_error)?;
            let id = if kind == "put" {
                text(&op["doc"], "id")
            } else {
                text(op, "id")
            }
            .map_err(sql_error)?;
            let row: Option<(String, Option<String>, bool)> = tx
                .query_row(
                    &format!(
                        "SELECT rev,data,deleted FROM {} WHERE id=?1",
                        quote(&c.table)
                    ),
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let before = row
                .as_ref()
                .filter(|v| !v.2)
                .and_then(|v| v.1.as_deref())
                .map(serde_json::from_str::<Value>)
                .transpose()
                .map_err(sql_error)?;
            let current_rev = row.as_ref().filter(|v| !v.2).map(|v| v.0.as_str());
            // Explicit base revision or an authorized transaction snapshot. No silent stale write.
            if op.get("base_rev").is_some() && op["base_rev"].as_str() != current_rev {
                return Err(invalid("conflict: document revision changed"));
            }
            let after = match kind {
                "put" => Some(op["doc"].clone()),
                "patch" => {
                    let mut v = before
                        .clone()
                        .ok_or_else(|| invalid("not_found: patch target"))?;
                    merge_patch(&mut v, &op["patch"]);
                    Some(v)
                }
                "remove" => {
                    if before.is_none() {
                        return Err(invalid("not_found: remove target"));
                    }
                    None
                }
                _ => return Err(invalid("invalid: write operation")),
            };
            if after.as_ref().is_some_and(|v| v["id"] != id) {
                return Err(invalid("invalid: document identity changed"));
            }
            policy::write_sync_document(
                &a,
                &r,
                app,
                name,
                before.as_ref(),
                after.as_ref(),
                permit.as_ref(),
            )
            .map_err(sql_error)?;
            let rev = format!("native:{}", uuid::Uuid::new_v4());
            if let Some(after) = &after {
                let schema: rxdb::types::RxJsonSchema =
                    serde_json::from_value(c.schema.clone()).map_err(sql_error)?;
                rxdb::rx_schema::validate_write_document(
                    &schema,
                    schema.primary_key.primary_field(),
                    after,
                )
                .map_err(sql_error)?;
                // P3's triggers own _seq/_changes. Preserve reconstructible revisions.
                if let Some((previous, Some(data), false)) = &row {
                    tx.execute("INSERT OR IGNORE INTO _revs(collection,id,rev,seq,gen,body,created_at) SELECT ?1,?2,?3,seq,?4,?5,?6 FROM _changes WHERE collection=?1 AND gen=?4 AND id=?2",params![name,id,previous,c.gen,data,(now()/1000) as i64])?;
                }
                tx.execute(&format!("INSERT INTO {}(id,rev,schema_ver,data,deleted) VALUES(?1,?2,?3,?4,0) ON CONFLICT(id) DO UPDATE SET rev=excluded.rev,schema_ver=excluded.schema_ver,data=excluded.data,deleted=0",quote(&c.table)),params![id,rev,c.schema_ver,after.to_string()])?;
                tx.execute("INSERT INTO _revs(collection,id,rev,seq,gen,body,created_at) SELECT ?1,?2,?3,seq,?4,?5,?6 FROM _changes WHERE collection=?1 AND gen=?4 AND id=?2",params![name,id,rev,c.gen,after.to_string(),(now()/1000) as i64])?;
                tx.execute("DELETE FROM _revs WHERE collection=?1 AND gen=?2 AND id=?3 AND (created_at<?4 OR rev NOT IN(SELECT rev FROM _revs WHERE collection=?1 AND gen=?2 AND id=?3 ORDER BY seq DESC LIMIT 20))",params![name,c.gen,id,(now().saturating_sub(30*86400*1000)/1000) as i64])?;
            } else {
                ctox_store::schema::remove_document(
                    tx,
                    name,
                    c.gen,
                    id,
                    current_rev.unwrap(),
                    &rev,
                )?;
            }
            let seq: u64 = tx.query_row(
                "SELECT seq FROM _changes WHERE collection=?1 AND gen=?2 AND id=?3",
                params![name, c.gen, id],
                |r| r.get(0),
            )?;
            results.push(json!({"status":"accepted","rev":rev,"seq":seq.to_string(),"gen":c.gen}));
        }
        Ok(results)
    }
    fn platform(
        &self,
        tx: &Transaction<'_>,
        binding: &Value,
        method: &str,
        args: &Value,
    ) -> Result<Value, StoreError> {
        self.check_fence(tx, binding)?;
        if method != "command.accept" {
            return Err(invalid("unavailable: native platform method"));
        }
        let app = text(args, "target_app").map_err(sql_error)?;
        let source = text(&binding["command"], "app_id").map_err(sql_error)?;
        let kind = text(args, "type").map_err(sql_error)?;
        let source_manifest = self.manifest(tx, source)?;
        let receiver = self.manifest(tx, app)?;
        ctox_app_runtime::ipc::check(&source_manifest, &receiver, kind, &args["options"])
            .map_err(sql_error)?;
        let a = self.check(tx, binding)?;
        permit(tx, app, &a, kind)?;
        let id = args["options"]["id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        if document(tx, "business_commands", &id)?.is_some() {
            return Err(invalid("conflict: command id already exists"));
        }
        let doc = json!({"id":id,"app_id":app,"source_app":source,"type":kind,"payload":args["payload"],"actor_id":a.id,"status":"queued","created_at":chrono::Utc::now().to_rfc3339(),"source_command_id":binding["command"]["id"],"record_id":args["options"]["record_id"],"client_context":args["options"]["client_context"]});
        super::super::commands::put(tx, "business_commands", &id, &doc)?;
        let c = collection(tx, "business_commands")?;
        let (rev,seq):(String,u64)=tx.query_row("SELECT rev,seq FROM _changes WHERE collection='business_commands' AND gen=?1 AND id=?2",params![c.gen,id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        write_contract::project_command(tx, &id, c.gen, &rev, seq, &doc)?;
        Ok(json!({"id":id,"status":"accepted"}))
    }
    fn version(&self, db: &Connection, app: &str, version: &str) -> Result<Value, StoreError> {
        let record = document(db, "business_app_versions", &format!("{app}@{version}"))?
            .ok_or_else(|| invalid("not_found: app version"))?;
        if !matches!(record["state"].as_str(), Some("published" | "archived")) {
            return Err(invalid("forbidden: unpublished app version"));
        }
        let manifest = &record["manifest"];
        let bytes = serde_json::to_vec(manifest).map_err(sql_error)?;
        if manifest["app_id"] != app
            || manifest["app_ver"] != version
            || record["manifest_sha256"] != digest(&bytes)
            || record["signature"]
                != super::super::commands::sign_manifest(self.signer.as_ref(), &bytes)?
        {
            return Err(invalid("forbidden: app signature differs"));
        }
        Ok(record)
    }
    fn artifact(&self, db: &Connection, hash: &str) -> Result<Vec<u8>, StoreError> {
        // Artifact hashes must be referenced by an admitted immutable app version.
        let c = collection(db, "business_app_versions")?;
        let size:Option<u64>=db.query_row(&format!("SELECT CAST(json_extract(f.data,'$.size') AS INTEGER) FROM {} f WHERE f.deleted=0 AND json_extract(f.data,'$.ref')=?1 LIMIT 1",quote(&collection(db,"files")?.table)),[hash],|r|r.get(0)).optional()?;
        let referenced:bool=db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {} v,json_each(json_extract(v.data,'$.manifest.files')) f WHERE v.deleted=0 AND json_extract(v.data,'$.state') IN ('published','archived') AND f.value=?1)",quote(&c.table)),[hash],|r|r.get(0))?;
        if !referenced {
            return Err(invalid("forbidden: unreferenced artifact"));
        }
        self.bytes(
            hash,
            size.ok_or_else(|| invalid("not_found: file metadata"))?,
        )
    }
}
fn merge_patch(target: &mut Value, patch: &Value) {
    if let Value::Object(fields) = patch {
        if !target.is_object() {
            *target = json!({});
        }
        for (k, v) in fields {
            if v.is_null() {
                target.as_object_mut().unwrap().remove(k);
            } else {
                merge_patch(
                    target
                        .as_object_mut()
                        .unwrap()
                        .entry(k)
                        .or_insert(Value::Null),
                    v,
                );
            }
        }
    } else {
        *target = patch.clone();
    }
}

pub(super) struct RuntimeBlobs {
    pub database: Database,
    pub runtime: Handle,
    pub codec: Arc<RuntimeCodec>,
    pub authority: Arc<RuntimeAuthority>,
}
impl crate::app_capabilities::cap2::blob::BlobAccess for RuntimeBlobs {
    fn read(
        &self,
        ctx: &crate::app_capabilities::CapCallContext,
        reference: &crate::app_capabilities::cap2::blob::BlobRef,
    ) -> Result<Vec<u8>, crate::app_capabilities::CtoxError> {
        use crate::app_capabilities::{CtoxError, ErrorCode};
        let ctx = ctx.clone();
        let r = reference.clone();
        let codec = self.codec.clone();
        let pool = self.database.readers.clone();
        self.authority
            .check_context(&ctx)
            .map_err(|_| CtoxError::new(ErrorCode::Revoked, "native invocation revoked"))?;
        wait(&self.runtime, async move {
            pool.read_job(ReadJob::new(JobOptions::default(), move |db| {
                let meta = document(db, "files", &r.transfer_id)?
                    .or_else(|| None)
                    .ok_or_else(|| invalid("not_found: scoped file"))?;
                if meta["owner_app"] != ctx.app_id
                    || meta["ref"] != r.sha256
                    || meta["size"] != r.size
                {
                    return Err(invalid("forbidden: file binding differs"));
                }
                codec.bytes(&r.sha256, r.size)
            }))
            .await
        })
        .map_err(|_| CtoxError::new(ErrorCode::NotFound, "scoped file unavailable"))
    }
    fn publish(
        &self,
        ctx: &crate::app_capabilities::CapCallContext,
        bytes: &[u8],
        mime: &str,
    ) -> Result<crate::app_capabilities::cap2::blob::BlobRef, crate::app_capabilities::CtoxError>
    {
        use crate::app_capabilities::{CtoxError, ErrorCode};
        self.authority
            .check_context(ctx)
            .map_err(|_| CtoxError::new(ErrorCode::Revoked, "native invocation revoked"))?;
        // Native producers use the canonical files owner; never manufacture a transfer ID.
        let meta = crate::app_capabilities::cap3::files::bound()?.put(
            &ctx.app_id,
            &ctx.actor.id,
            "artifact",
            mime,
            bytes,
        )?;
        Ok(crate::app_capabilities::cap2::blob::BlobRef {
            sha256: meta.reference,
            size: meta.size,
            transfer_id: meta.id,
        })
    }
}
