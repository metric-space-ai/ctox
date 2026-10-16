use super::*;
use crate::business_os::policy::sync_rights::{
    self,
    manifest_rules::ManifestRules,
    scope::{FieldMask, RowFilter},
    Operation, Right,
};
use ctox_store::schema::{self, CollectionDefinition};
use serde_json::json;
use sha2::{Digest, Sha256};

fn fixture(hash: &str, size: usize) -> anyhow::Result<Connection> {
    let mut db = Connection::open_in_memory()?;
    let tx = db.transaction()?;
    schema::initialize(&tx)?;
    for name in ["business_chats", "business_users", "files"] {
        let c = schema::create_collection(
            &tx,
            &CollectionDefinition {
                name: name.into(),
                app_id: "shell".into(),
                schema_ver: 1,
                schema_json: json!({"version":1,"type":"object","primaryKey":"id","properties":{"id":{"type":"string"}},"required":["id"]}),
                class: "full".into(),
                client_limit: 0,
                byte_budget: 1_048_576,
                rights_ver: 1,
                app_ver: "1".into(),
            },
        )?;
        let docs = match name {
            "business_chats" => vec![
                json!({"id":"chat","owner":"alice","attachments":[{"ref":hash}]}),
                json!({"id":"foreign","owner":"bob","attachments":[{"ref":hash}]}),
                json!({"id":"empty","owner":"alice"}),
            ],
            "business_users" => vec![json!({"id":"alice","active":true,"role":"user"})],
            _ => vec![json!({"id":"file","ref":hash,"size":size,"owner_app":"desktop"})],
        };
        for doc in docs {
            tx.execute(
                &format!(
                    "INSERT INTO \"{}\"(id,rev,schema_ver,data,deleted) VALUES(?1,'1',1,?2,0)",
                    c.table
                ),
                params![doc["id"].as_str().unwrap(), doc.to_string()],
            )?;
        }
    }
    sync_rights::initialize(&tx)?;
    let shell = ManifestRules::from_manifest(
        "shell",
        "1",
        [
            "business_chats".into(),
            "business_users".into(),
            "files".into(),
        ]
        .into(),
        &json!({}),
    )
    .map_err(anyhow::Error::msg)?;
    sync_rights::replace_with_manifests(
        &tx,
        1,
        &[Right {
            role: "user".into(),
            collection: "business_chats".into(),
            row_filter: RowFilter::Eq {
                path: "/owner".into(),
                value: sync_rights::scope::FilterValue::ActorId,
            },
            field_mask: FieldMask::All,
            ops: [Operation::Read].into(),
        }],
        &[shell],
    )?;
    tx.commit()?;
    Ok(db)
}
fn attachment(hash: &str, size: usize) -> Attachment {
    serde_json::from_value(json!({"ref":hash,"blob_id":"file","name":"proof.txt","mime_type":"text/plain","size_bytes":size,"content_hash":hash})).unwrap()
}

#[test]
fn chat_blob_e30_reference_and_cas_contract() -> anyhow::Result<()> {
    let bytes = b"chat CAS attachment";
    let hash = format!("{:x}", Sha256::digest(bytes));
    let db = fixture(&hash, bytes.len())?;
    let root = tempfile::tempdir()?;
    let cas = Cas::open(crate::paths::runtime_dir(root.path()).join("transfers"))?;
    let mut upload = cas.receive(
        &format!("{:x}", Sha256::digest(b"isolated-chat-transfer")),
        &hash,
        bytes.len() as u64,
    )?;
    upload.append(0, bytes)?;
    upload.finish()?;
    for (chat, code) in [
        ("unknown", "stale_base"),
        ("foreign", "forbidden"),
        ("empty", "forbidden"),
    ] {
        let error =
            authorize(&db, "shell", "alice", chat, attachment(&hash, bytes.len())).unwrap_err();
        assert!(error.to_string().contains(code), "{chat}: {error}");
        let wire = failure_outcome("command", &error).expect("typed native rejection");
        assert_eq!(wire["code"], code);
        assert_eq!(wire["status"], "rejected");
        assert!(!root
            .path()
            .join("runtime/business-os/chat-attachments")
            .exists());
    }
    // Fields masked out of the document cannot confer reference authority.
    db.execute(
        "UPDATE c_rights__g1 SET field_mask_json=?1 WHERE collection='business_chats'",
        [serde_json::to_string(&FieldMask::Fields {
            paths: ["/id".into(), "/owner".into()].into(),
        })?],
    )?;
    assert!(authorize(
        &db,
        "shell",
        "alice",
        "chat",
        attachment(&hash, bytes.len())
    )
    .unwrap_err()
    .to_string()
    .contains("forbidden"));
    db.execute(
        "UPDATE c_rights__g1 SET field_mask_json=?1 WHERE collection='business_chats'",
        [serde_json::to_string(&FieldMask::All)?],
    )?;
    let approved = authorize(
        &db,
        "shell",
        "alice",
        "chat",
        attachment(&hash, bytes.len()),
    )?;
    let result = materialize_verified(root.path(), "command", approved)?;
    assert_eq!(fs::read(&result.local_path)?, bytes);
    assert_eq!(result.content_hash, hash);
    assert_eq!(result.generation_id, "cas");
    // Neither a known hash alone nor a fabricated metadata size is sufficient.
    assert!(authorize(
        &db,
        "shell",
        "alice",
        "chat",
        attachment(&hash, bytes.len() + 1)
    )
    .is_err());
    assert!(authorize(
        &db,
        "unknown-app",
        "alice",
        "chat",
        attachment(&hash, bytes.len())
    )
    .is_err());
    db.execute(
        "UPDATE c_rights__g1 SET ops='[]' WHERE collection='business_chats'",
        [],
    )?;
    assert!(authorize(
        &db,
        "shell",
        "alice",
        "chat",
        attachment(&hash, bytes.len())
    )
    .is_err());
    // Corrupt bytes in the actual CAS fail before creating the output directory.
    fs::write(
        crate::paths::runtime_dir(root.path())
            .join("transfers/objects")
            .join(&hash),
        b"corrupt",
    )?;
    assert!(materialize_verified(
        root.path(),
        "corrupt-command",
        attachment(&hash, bytes.len())
    )
    .is_err());
    assert!(!root
        .path()
        .join("runtime/business-os/chat-attachments/corrupt-command")
        .exists());
    Ok(())
}

#[test]
fn chat_blob_intake_does_not_silently_drop_missing_or_invalid_references() {
    let mut command = BusinessCommand {
        id: None,
        module: "shell".into(),
        command_type: "business_os.chat.task".into(),
        record_id: None,
        payload: json!({"attachments":[{"kind":"blob","ref":"invalid"}]}),
        client_context: json!({}),
        origin: CommandOrigin::ReplicatedPeer,
    };
    assert_eq!(
        business_chat_attachment_refs(&command),
        command.payload["attachments"].as_array().unwrap().clone()
    );
    let root = tempfile::tempdir().unwrap();
    command.client_context = json!({"actor":{"id":"alice","role":"admin","trusted":true}});
    command.payload["instruction"] = json!("Inspect the attached document");
    let error = preflight(root.path(), "forged", &command, None).unwrap_err();
    assert_eq!(
        failure_outcome("forged", &error).unwrap()["code"],
        "forbidden"
    );
    let error =
        materialize_business_chat_attachments(root.path(), "forged", &command, None).unwrap_err();
    assert_eq!(
        failure_outcome("forged", &error).unwrap()["code"],
        "forbidden"
    );
    assert!(!root
        .path()
        .join("runtime/business-os/chat-attachments")
        .exists());
    command.payload = json!({"attachments":[{"kind":"desktop_file","file_id":"legacy"}]});
    assert_eq!(business_chat_attachment_refs(&command).len(), 1);
}
