//! E46: native chat attachments use a visible canonical document reference,
//! never possession of a CAS hash or a caller-supplied local path as authority.
use super::*;
use crate::business_os::app_platform::runtime_host::{actor, document, rights};
use ctox_store::cas::Cas;
use std::io::Read;

const MAX_CHAT_BLOB_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug)]
struct Rejection {
    code: &'static str,
    reason: &'static str,
}
impl std::fmt::Display for Rejection {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{}: {}", self.code, self.reason)
    }
}
impl std::error::Error for Rejection {}
fn rejection(code: &'static str, reason: &'static str) -> anyhow::Error {
    Rejection { code, reason }.into()
}
pub(crate) fn failure_outcome(command_id: &str, error: &anyhow::Error) -> Option<Value> {
    let failure = error.downcast_ref::<Rejection>()?;
    Some(
        serde_json::json!({"ok":false,"command_id":command_id,"status":"rejected",
        "code":failure.code,"reason":failure.reason,"path":"payload.attachments"}),
    )
}

#[derive(Debug, serde::Deserialize)]
struct Attachment {
    #[serde(rename = "ref")]
    reference: String,
    blob_id: String,
    name: String,
    mime_type: String,
    size_bytes: u64,
    content_hash: String,
}

pub(super) fn materialize(
    root: &Path,
    command_id: &str,
    command: &BusinessCommand,
    native_authorization: Option<&Value>,
    reference: &Value,
) -> anyhow::Result<MaterializedBusinessChatAttachment> {
    // This receipt is a native argument, not a field accepted from payload.
    // Revalidate it, or use the existing origin-aware native authentication; a peer actor claim never grants authority.
    let session = if let Some(authorization) = native_authorization {
        revalidate_queue_native_authorization(root, command, authorization).map(|v| v.0)
    } else {
        rxdb_authenticated_session(root, command)
    }
    .map_err(|_| rejection("forbidden", "attachment_actor_unauthenticated"))?;
    let user_id = session_user_id(&session)
        .ok_or_else(|| rejection("forbidden", "attachment_actor_missing"))?
        .to_owned();
    let app = command.module.clone();
    let chat_id = business_chat_id(command, command_id);
    let attachment: Attachment = serde_json::from_value(reference.clone())
        .map_err(|_| rejection("invalid", "blob_attachment_metadata"))?;
    let attachment = crate::service::store_inputs::owned_read(root, true, move |db| {
        Ok(authorize(db, &app, &user_id, &chat_id, attachment))
    })
    .map_err(|_| rejection("unavailable", "attachment_store_unavailable"))??;
    materialize_verified(root, command_id, attachment)
}

fn has_reference(value: &Value, reference: &str) -> bool {
    match value {
        Value::Object(fields) => {
            fields.get("ref").and_then(Value::as_str) == Some(reference)
                || fields.values().any(|v| has_reference(v, reference))
        }
        Value::Array(items) => items.iter().any(|v| has_reference(v, reference)),
        _ => false,
    }
}

fn authorize(
    db: &Connection,
    app: &str,
    user_id: &str,
    chat_id: &str,
    attachment: Attachment,
) -> anyhow::Result<Attachment> {
    anyhow::ensure!(
        attachment.reference.len() == 64
            && attachment
                .reference
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && attachment.content_hash == attachment.reference,
        "invalid: blob attachment hash"
    );
    anyhow::ensure!(
        attachment.size_bytes <= MAX_CHAT_BLOB_BYTES,
        "quota: chat attachment"
    );
    anyhow::ensure!(
        !attachment.blob_id.is_empty()
            && !attachment.name.is_empty()
            && attachment.name.len() <= 255
            && attachment.mime_type.len() <= 128,
        "invalid: blob attachment metadata"
    );
    // The read pool holds ONE current WAL snapshot for user, rights, referring
    // document and file metadata. Missing/deleted chat is a sequencing error.
    let chat = document(db, "business_chats", chat_id)?
        .ok_or_else(|| rejection("stale_base", "referencing_chat_unknown"))?;
    let current_actor =
        actor(db, user_id).map_err(|_| rejection("revoked", "attachment_actor_inactive"))?;
    let current_rights = rights(db)?;
    let visible = policy::read_sync_document(
        &current_actor,
        &current_rights,
        app,
        "business_chats",
        &chat,
    )
    .map_err(|_| rejection("forbidden", "referencing_chat_unreadable"))?;
    if !has_reference(&visible, &attachment.reference) {
        return Err(rejection("forbidden", "blob_reference_not_visible"));
    }
    let file = document(db, "files", &attachment.blob_id)?
        .context("not_found: referenced file metadata")?;
    anyhow::ensure!(
        file["ref"] == attachment.reference && file["size"].as_u64() == Some(attachment.size_bytes),
        "invalid: blob attachment file binding differs"
    );
    Ok(attachment)
}

fn materialize_verified(
    root: &Path,
    command_id: &str,
    attachment: Attachment,
) -> anyhow::Result<MaterializedBusinessChatAttachment> {
    // Repoint to the existing Transfer CAS verifier; no private blob store or
    // unchecked file read. Verification happens before any output is created.
    let cas = Cas::open(crate::paths::runtime_dir(root).join("transfers"))?;
    let verified = cas.verified(&attachment.reference, attachment.size_bytes)?;
    let mut bytes = Vec::new();
    cas.reader(&verified)?
        .take(MAX_CHAT_BLOB_BYTES + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 == attachment.size_bytes,
        "invalid: CAS attachment size"
    );
    let dir = root
        .join("runtime/business-os/chat-attachments")
        .join(sanitize_filename(command_id));
    fs::create_dir_all(&dir)?;
    let local_path = dir.join(materialized_attachment_filename(
        &attachment.reference,
        &attachment.name,
    ));
    fs::write(&local_path, bytes)?;
    Ok(MaterializedBusinessChatAttachment {
        attachment_id: attachment.blob_id.clone(),
        file_id: attachment.blob_id,
        generation_id: "cas".into(),
        name: attachment.name,
        mime_type: attachment.mime_type,
        size_bytes: attachment.size_bytes,
        content_hash: attachment.reference,
        content_hash_scheme: BUSINESS_OS_LEGACY_SHA256_CONTENT_HASH_SCHEME.into(),
        virtual_path: String::new(),
        local_path: local_path.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
#[path = "store_chat_blob_tests.rs"]
mod tests;
