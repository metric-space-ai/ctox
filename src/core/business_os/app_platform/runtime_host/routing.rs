//! E39: immutable sender, independently authorized execution target.
use super::*;
use ctox_app_runtime::core_host::StoreCodec;

pub(super) fn core_route(kind: &str) -> bool {
    static INVENTORY: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    let inventory = INVENTORY.get_or_init(|| serde_json::from_str(include_str!("../../business_command_inventory.json")).expect("generated command inventory"));
    super::super::commands::handles(kind)
        || kind.starts_with("ctox.app.") || kind.starts_with("ctox.coding.")
        || matches!(kind,"business_os.chat.task"|"business_os.context.ask"|"business_os.data.modify")
        || ["exact_control_types","predicate_control_types","browser_runtime_types","browser_literal_types"].iter().any(|key| inventory[key].as_array().is_some_and(|types| types.iter().any(|v| v == kind)))
        || inventory["control_prefixes"].as_array().is_some_and(|prefixes| prefixes.iter().filter_map(Value::as_str).any(|prefix| kind.starts_with(prefix)))
        || ctox_store::change_log::subscription::write_contract::routes::owns_type(kind)
        || ctox_store::change_log::subscription::write_contract::routes::resolve(kind).is_ok()
}

// E27 validation is repeated inside the existing command writer transaction.
// This existence check grants no authority; signed manifests and policy are
// checked by RuntimeCodec before the lease and again before every effect.
pub(super) fn known_target(db: &Connection, doc: &Value) -> Result<(), StoreError> {
    let kind = text(doc, "type").map_err(sql_error)?;
    if core_route(kind) { return Ok(()); }
    let source = text(doc, "app_id").map_err(sql_error)?;
    let sender = document(db,"business_apps",source)?.ok_or_else(|| invalid("unknown_command: sender app missing"))?;
    let version = sender["active_ver"].as_str().ok_or_else(|| invalid("unknown_command: sender version missing"))?;
    let release = document(db,"business_app_versions",&format!("{source}@{version}"))?.ok_or_else(|| invalid("unknown_command: sender version missing"))?;
    let requested = doc.get("target_app").map(|v|v.as_str().filter(|s|!s.is_empty()).ok_or_else(||invalid("invalid: target_app"))).transpose()?;
    let target = ctox_app_runtime::declared_target(&release["manifest"],kind,requested).map_err(sql_error)?;
    if document(db,"business_apps",target)?.is_none() {
        return Err(invalid("unknown_command: target app missing"));
    }
    if target == source && release["manifest"]["handlers"].get(kind).is_none() {
        return Err(invalid("unknown_command: own command handler missing"));
    }
    Ok(())
}

impl RuntimeCodec {
    pub(super) fn route(&self, db: &Connection, doc: &Value) -> Result<Value, StoreError> {
        let source = text(doc, "app_id").map_err(sql_error)?;
        let kind = text(doc, "type").map_err(sql_error)?;
        let a = actor(db, text(doc, "actor_id").map_err(sql_error)?)?;
        if core_route(kind) {
            if super::super::commands::handles(kind) {
                return Ok(json!({"app_id":source,"app_ver":"native-platform"}));
            }
            // Core routes never consult app handlers or commands_out.
            let manifest = self.manifest(db, source)?;
            origin::delivery_permit(db, doc, &a)?;
            return Ok(manifest);
        }
        if doc["runtime_completion"] == true {
            if !origin::native_event(db, doc)? { return Err(invalid("forbidden: native completion origin required")); }
            let manifest = self.version(db, source, text(doc, "native_version").map_err(sql_error)?)?["manifest"].clone();
            origin::delivery_permit(db, doc, &a)?;
            return Ok(manifest);
        }
        if origin::native_event(db, doc)? {
            let manifest = self.manifest(db, source)?;
            origin::delivery_permit(db, doc, &a)?;
            return Ok(manifest);
        }
        known_target(db, doc)?;
        let sender = self.manifest(db, source)?;
        let requested = doc.get("target_app").map(|v| v.as_str().ok_or_else(|| invalid("invalid: target_app"))).transpose()?;
        let target = ctox_app_runtime::declared_target(&sender, kind, requested).map_err(sql_error)?;
        let receiver = if target == source { sender.clone() } else { self.manifest(db, target)? };
        if target != source {
            if receiver["handlers"].get(kind).is_none() {
                return Err(invalid("forbidden: target handler missing"));
            }
            let exported = receiver["commands_in"].as_array().is_some_and(|rules| rules.iter().any(|r| r["type"] == kind && (r["from"] == "*" || r["from"].as_array().is_some_and(|apps| apps.iter().any(|v| v == source)))));
            if !exported { return Err(invalid("forbidden: receiver commands_in missing")); }
        }
        origin::delivery_permit_for(db, doc, &a, &rights(db)?, target)?;
        Ok(receiver)
    }
}
