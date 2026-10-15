//! `files.*` — content-addressed file records (CATALOG Nr. 1, §16.32,
//! §16.35 Nr. 1) on the platform `files` collection. `ref` = sha256 of the
//! content; records are owned by the calling app (owner_app from ctx).
//! Results never carry base64: text inline only for UTF-8 ≤ 512 KiB, else
//! metadata + ref (the client fetches bytes over the transfer lane).
use super::*;
use serde::Deserialize;

pub const MAX_PUT_BYTES: usize = 768 * 1024;
pub const MAX_INLINE_TEXT: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    pub id: String,
    pub reference: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub owner_app: String,
    pub created_at_ms: u64,
}
/// Core-owned files collection + transfer CAS (same owner as the v4 app
/// runtime's sync-store writer). Every method is scoped to `owner_app`.
pub trait FileStore: Send + Sync {
    fn put(
        &self,
        owner_app: &str,
        owner_user: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
    ) -> Result<FileMeta, CtoxError>;
    fn get(
        &self,
        owner_app: &str,
        id_or_ref: &str,
    ) -> Result<Option<(FileMeta, Vec<u8>)>, CtoxError>;
    fn list(&self, owner_app: &str, limit: usize) -> Result<Vec<FileMeta>, CtoxError>;
    fn delete(&self, owner_app: &str, id: &str) -> Result<bool, CtoxError>;
}
static FILE_STORE: std::sync::OnceLock<std::sync::Arc<dyn FileStore>> = std::sync::OnceLock::new();
/// Composition root only (together with ingest::bind_app_ingress).
pub fn bind_file_store(store: std::sync::Arc<dyn FileStore>) -> Result<(), CtoxError> {
    FILE_STORE
        .set(store)
        .map_err(|_| CtoxError::new(ErrorCode::Conflict, "file store already bound"))
}
pub fn bound() -> Result<&'static dyn FileStore, CtoxError> {
    FILE_STORE
        .get()
        .map(|store| store.as_ref())
        .ok_or_else(|| CtoxError::new(ErrorCode::Unavailable, "files collection not bound"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Put {
    name: String,
    #[serde(default = "octet")]
    mime: String,
    text: Option<String>,
    /// Input only; results never contain base64.
    base64: Option<String>,
}
fn octet() -> String {
    "application/octet-stream".into()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Get {
    id: Option<String>,
    #[serde(rename = "ref")]
    reference: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    limit: Option<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Delete {
    id: String,
}
fn meta_json(meta: &FileMeta) -> Value {
    json!({"id":meta.id,"ref":meta.reference,"name":meta.name,"mime":meta.mime,
           "size":meta.size,"owner_app":meta.owner_app,"created_at_ms":meta.created_at_ms})
}

pub(super) fn execute(
    store: &dyn FileStore,
    ctx: &CapCallContext,
    function: &str,
    args: &Value,
) -> Result<Value, CtoxError> {
    match function {
        "put" => {
            let request: Put = decode(args)?;
            text(&request.name, 255, false, "$.name")?;
            if request.name.contains(['/', '\\']) {
                return Err(invalid("$.name"));
            }
            text(&request.mime, 128, false, "$.mime")?;
            let bytes = match (request.text, request.base64) {
                (Some(text), None) => text.into_bytes(),
                (None, Some(encoded)) => {
                    use base64::Engine;
                    base64::engine::general_purpose::STANDARD
                        .decode(encoded.as_bytes())
                        .map_err(|_| invalid("$.base64"))?
                }
                _ => return Err(invalid("$.args")),
            };
            if bytes.len() > MAX_PUT_BYTES {
                return Err(CtoxError::new(
                    ErrorCode::Quota,
                    "file exceeds capability limit",
                ));
            }
            let meta = store.put(
                &ctx.app_id,
                &ctx.actor.id,
                &request.name,
                &request.mime,
                &bytes,
            )?;
            // The store must address content exactly; anything else is an
            // unreconciled effect, not a successful put.
            if meta.reference != sha256_hex(&bytes)
                || meta.size != bytes.len() as u64
                || meta.owner_app != ctx.app_id
            {
                return Err(CtoxError::uncertain());
            }
            Ok(json!({"id":meta.id,"ref":meta.reference}))
        }
        "get" => {
            let request: Get = decode(args)?;
            let key = match (&request.id, &request.reference) {
                (Some(id), None) => id,
                (None, Some(reference)) => reference,
                _ => return Err(invalid("$.args")),
            };
            crate::app_capabilities::protocol::identifier(key, "$.id")?;
            let Some((meta, bytes)) = store.get(&ctx.app_id, key)? else {
                return Err(CtoxError::new(ErrorCode::NotFound, "file not found"));
            };
            if meta.owner_app != ctx.app_id || sha256_hex(&bytes) != meta.reference {
                return Err(CtoxError::new(ErrorCode::NotFound, "file not found"));
            }
            let mut result = meta_json(&meta);
            if bytes.len() <= MAX_INLINE_TEXT {
                if let Ok(text) = String::from_utf8(bytes) {
                    result["text"] = json!(text);
                }
            }
            Ok(result)
        }
        "list" => {
            let request: List = decode(args)?;
            let limit = request.limit.unwrap_or(200);
            if limit == 0 || limit > 1000 {
                return Err(invalid("$.limit"));
            }
            let files = store
                .list(&ctx.app_id, limit)?
                .iter()
                .filter(|meta| meta.owner_app == ctx.app_id)
                .map(meta_json)
                .collect::<Vec<_>>();
            Ok(json!({ "files": files }))
        }
        "delete" => {
            let request: Delete = decode(args)?;
            crate::app_capabilities::protocol::identifier(&request.id, "$.id")?;
            Ok(json!({"id":request.id,"deleted":store.delete(&ctx.app_id, &request.id)?}))
        }
        _ => Err(CtoxError::new(
            ErrorCode::NotFound,
            "unknown files function",
        )),
    }
}
