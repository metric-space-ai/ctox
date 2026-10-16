//! E51: platform preferences. Identity comes from the authenticated native
//! session, never from a document, selector, key, role or operation payload.
use crate::{change_log::identifier, schema::Collection};
use rusqlite::{params, Transaction};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const COLLECTION: &str = "app_user_preferences";
pub const MAX_KEYS: u64 = 64;
pub const MAX_VALUE_BYTES: usize = 16 * 1024;

/// Transport adapters construct this from their verified app launch/session.
/// Deliberately not Deserialize: a payload is not an authentication context.
#[derive(Clone, Debug)]
pub struct Context {
    app_id: String,
    user_id: String,
    rights_ver: u64,
}
impl Context {
    pub fn authenticated(app_id: &str, user_id: &str, rights_ver: u64) -> Result<Self, &'static str> {
        if rights_ver == 0 { return Err("forbidden"); }
        for part in [app_id, user_id] {
            if part.is_empty() || part.len() > 256 || part.contains('/') {
                return Err("forbidden");
            }
        }
        Ok(Self { app_id: app_id.into(), user_id: user_id.into(), rights_ver })
    }
    pub fn scope_hash(&self) -> String {
        fn ordered(v: Value) -> Value {
            match v {
                Value::Object(m) => {
                    let sorted: std::collections::BTreeMap<_,_> = m.into_iter().collect();
                    Value::Object(sorted.into_iter().map(|(k,v)| (k,ordered(v))).collect())
                }
                Value::Array(a) => Value::Array(a.into_iter().map(ordered).collect()),
                v => v,
            }
        }
        format!("{:x}", Sha256::digest(serde_json::to_vec(&ordered(self.resolved_scope())).expect("scope JSON")))
    }
    /// Standard ResolvedScope wire shape, shared with native token issuance.
    pub fn resolved_scope(&self) -> Value {
        serde_json::json!({"rights_ver": self.rights_ver, "projections": [{
            "collection": COLLECTION, "field_mask": {"mode":"all"},
            "row_filter": {"op":"and","filters":[
                {"op":"eq","path":"/app_id","value":{"kind":"literal","value":self.app_id}},
                {"op":"eq","path":"/user_id","value":{"kind":"literal","value":self.user_id}}
            ]}
        }]})
    }
    pub fn id(&self, key: &str) -> Result<String, &'static str> {
        if key.is_empty() || key.len() > 64 || !key.bytes().all(|b|
            b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b)) {
            return Err("invalid_preference_key");
        }
        Ok(format!("{}/{}/{key}", self.app_id, self.user_id))
    }
    pub fn owns_id(&self, id: &str) -> bool {
        id.rsplit_once('/').is_some_and(|(_, key)| self.id(key).is_ok_and(|own| own == id))
    }
    pub fn owns(&self, doc: &Value) -> bool {
        doc["app_id"].as_str() == Some(self.app_id.as_str())
            && doc["user_id"].as_str() == Some(self.user_id.as_str())
            && doc["key"].as_str().and_then(|key| self.id(key).ok()).as_deref() == doc["id"].as_str()
            && doc["id"].as_str().is_some_and(|id| self.owns_id(id))
    }
    pub fn document(&self, key: &str, value: Value) -> Result<Value, &'static str> {
        if serde_json::to_vec(&value).map_err(|_| "invalid_preference_value")?.len() > MAX_VALUE_BYTES {
            return Err("quota");
        }
        Ok(serde_json::json!({"id":self.id(key)?,"app_id":self.app_id,
            "user_id":self.user_id,"key":key,"value":value}))
    }
    /// Runs after merge, inside the SAME writer transaction as the effect. A
    /// replacement at the limit is allowed; tombstones do not consume quota.
    pub fn validate(&self, tx: &Transaction<'_>, collection: &Collection, id: &str, doc: &Value) -> rusqlite::Result<Option<&'static str>> {
        if !self.owns(doc) || doc["id"].as_str() != Some(id) {
            return Ok(Some("forbidden"));
        }
        if !doc.as_object().is_some_and(|m| m.len() == 5 && m.contains_key("value")) {
            return Ok(Some("invalid_preference_value"));
        }
        if serde_json::to_vec(&doc["value"]).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?.len() > MAX_VALUE_BYTES {
            return Ok(Some("quota"));
        }
        let table = identifier(&collection.table)?;
        let count: u64 = tx.query_row(&format!("SELECT COUNT(*) FROM {table} WHERE deleted=0 AND id<>?1 AND json_extract(data,'$.app_id')=?2 AND json_extract(data,'$.user_id')=?3"), params![id,self.app_id,self.user_id], |r| r.get(0))?;
        Ok((count >= MAX_KEYS).then_some("quota"))
    }
}

#[cfg(test)]
#[path = "preferences_tests.rs"]
mod tests;
