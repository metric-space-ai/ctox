//! E44 native responses. No browser-supplied credentials or filesystem paths.
use super::*;

pub(super) fn is_control_response(command: &str) -> bool {
    matches!(command, "browser.http_auth.respond" | "browser.observer.grant"
        | "browser.observer.revoke" | "browser.upload.select" | "browser.webauthn.respond")
}

pub(super) fn require_owned_session(session: &Value, actor: &str) -> anyhow::Result<()> {
    anyhow::ensure!(session.is_object(), "browser session not found");
    anyhow::ensure!(session.get("owner_user_id").and_then(Value::as_str) == Some(actor),
        "browser session belongs to another user");
    Ok(())
}

/// Owner-only sharing of the existing read-only browser projection. This is
/// independent of the controller lease; a grantee never receives control.
pub(super) async fn update_observers(
    database: &Arc<RxDatabase>, session: &Value, actor: &str, command: &str, payload: &Value,
) -> anyhow::Result<Value> {
    require_owned_session(session, actor)?;
    let user = payload.get("user_id").and_then(Value::as_str).map(str::trim)
        .filter(|user| !user.is_empty() && user.len() <= 256)
        .context("observer user_id is required")?;
    anyhow::ensure!(user != actor, "the session owner is not an observer");
    let mut observers: std::collections::BTreeSet<String> = session
        .get("allowed_observer_user_ids").and_then(Value::as_array)
        .into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
    match command {
        "browser.observer.grant" => { observers.insert(user.to_owned()); }
        "browser.observer.revoke" => { observers.remove(user); }
        _ => anyhow::bail!("unsupported observer command"),
    }
    anyhow::ensure!(observers.len() <= 100, "browser observer limit exceeded");
    let observers = serde_json::to_value(observers)?;
    let id = session.get("id").and_then(Value::as_str).context("browser session id missing")?;
    // Called under BROWSER_RUNTIME_COMMAND_LOCK, like new frame/tab writes.
    // Re-read related rows by session rather than trusting IDs supplied by a client.
    for name in ["browser_tabs", "browser_frames", "browser_sessions"] {
        let collection = database.collection(name).with_context(|| format!("{name} not registered"))?;
        let rows = if name == "browser_sessions" { json!([session]) } else {
            collection.find(Some(MangoQuery {
                selector: Some(json!({"session_id":{"$eq":id}})), ..Default::default()
            }))?.exec(false).await?
        };
        for row in rows.as_array().context("browser observer query returned no array")? {
            anyhow::ensure!(row.get("owner_user_id").and_then(Value::as_str) == Some(actor),
                "browser observer row belongs to another user");
            let mut next = row.clone();
            let object = next.as_object_mut().context("browser observer row is invalid")?;
            object.remove("_rev");
            object.remove("_meta");
            object.insert("allowed_observer_user_ids".to_owned(), observers.clone());
            object.insert("updated_at_ms".to_owned(), Value::from(now_ms() as u64));
            collection.incremental_upsert(next).await?;
        }
    }
    Ok(observers)
}

/// Only display-safe challenge descriptors enter the replicated session.
/// Private credential exports from the runner are deliberately not copied.
pub(super) fn prompt_metadata(metadata: Option<&Value>, nav: &Value) -> Value {
    let mut out = metadata.filter(|v| v.is_object()).cloned().unwrap_or_else(|| json!({}));
    for (source, target, fields) in [
        ("http_auth_request", "pending_http_auth", &["origin", "scheme", "realm"][..]),
        ("webauthn_request", "pending_webauthn", &["type", "rp_id", "origin"][..]),
    ] {
        if let Some(object) = out.as_object_mut() { object.remove(target); }
        if let Some(value) = nav.get(source) {
            out[target] = if value.is_object() {
                Value::Object(fields.iter().filter_map(|key| value.get(*key).and_then(Value::as_str)
                    .map(|text| ((*key).to_owned(), Value::String(text.chars().take(1024).collect())))).collect())
            } else { Value::Null };
        }
    }
    out
}

pub(super) fn prepare_response(
    root: &Path, token: &str, command_id: &str, command: &str, payload: &Value,
) -> anyhow::Result<Option<(&'static str, Value)>> {
    let response = match command {
        "browser.http_auth.respond" | "browser.webauthn.respond" => {
            anyhow::ensure!(payload.get("confirmed").and_then(Value::as_bool) == Some(true),
                "browser authentication requires explicit confirmation");
            let accept = payload.get("accept").and_then(Value::as_bool)
                .context("browser authentication accept must be boolean")?;
            anyhow::ensure!(!["username", "password", "credentials"].iter().any(|key| payload.get(*key).is_some()),
                "browser authentication accepts secret references only");
            if command == "browser.webauthn.respond" {
                // The runner retains its virtual authenticator, including private keys.
                // Its exported credentials are never put into replicated command results.
                ("webauthn_respond", json!({"accept":accept}))
            } else if !accept {
                ("http_auth_respond", json!({"accept":false}))
            } else {
                anyhow::ensure!(store::webrtc_capability_allows_workspace_permission(
                    root, token, BusinessOsPermission::SecretsManage),
                    "HTTP auth requires secrets.manage permission");
                let name = payload.get("secret_name").and_then(Value::as_str)
                    .filter(|name| is_safe_browser_credential_name(name))
                    .context("HTTP auth secret name is invalid")?;
                let stored = crate::secrets::read_secret_value(root, crate::secrets::credential_scope(), name)
                    .context("configured HTTP auth credential is unavailable")?;
                ("http_auth_respond", json!({"accept":true,
                    "username":resolve_browser_credential_field(&stored, "username")?,
                    "password":resolve_browser_credential_field(&stored, "password")?}))
            }
        }
        "browser.upload.select" => {
            anyhow::ensure!(!["path", "filePath", "file_path"].iter().any(|key| payload.get(*key).is_some()),
                "browser upload accepts file references only");
            let file_id = payload.get("file_id").and_then(Value::as_str)
                .map(str::trim).filter(|id| !id.is_empty()).context("upload file_id is required")?;
            let document = store::rxdb_desktop_file_document(root, file_id)?;
            anyhow::ensure!(super::super::threads::may_replicate_document(root, token, "desktop_files", &document),
                "browser upload file read is forbidden");
            let attachment = store::materialize_business_chat_attachment(
                root, command_id, &json!({}), file_id, None)?;
            ("upload", json!({"filePath":attachment.local_path,
                "selector":payload.get("selector").and_then(Value::as_str).unwrap_or("input[type=file]")}))
        }
        _ => return Ok(None),
    };
    Ok(Some(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::rxdb_peer::tests::open_test_database;
    use super::super::super::rxdb_peer::business_os_schema;
    use rxdb::rx_database::RxCollectionCreator;

    #[test]
    fn e44_prompt_projection_copies_descriptors_and_clears_settled_challenges() {
        let patch = prompt_metadata(None, &json!({
            "http_auth_request":{"origin":"https://example.test","realm":"fixture","password":"private"},
            "webauthn_request":{"type":"get","rp_id":"example.test","credentials":["private"]},
            "credentials":["private"]}));
        assert_eq!(patch["pending_http_auth"],json!({"origin":"https://example.test","realm":"fixture"}));
        assert_eq!(patch["pending_webauthn"],json!({"type":"get","rp_id":"example.test"}));
        assert!(!patch.to_string().contains("private"));
        let cleared = prompt_metadata(Some(&patch), &json!({"http_auth_request":null,"webauthn_request":null}));
        assert!(cleared["pending_http_auth"].is_null());
        assert!(cleared["pending_webauthn"].is_null());
    }

    #[test]
    fn e44_auth_responses_require_confirmation_and_never_accept_raw_credentials() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        for command in ["browser.http_auth.respond", "browser.webauthn.respond"] {
            assert!(is_browser_runtime_command(command));
            assert!(prepare_response(root.path(), "", "id", command, &json!({"accept":false})).is_err());
            assert!(prepare_response(root.path(), "", "id", command,
                &json!({"accept":true,"confirmed":true,"password":"forged"})).is_err());
            let (op, args) = prepare_response(root.path(), "", "id", command,
                &json!({"accept":false,"confirmed":true}))?.context("response")?;
            assert_eq!(op, if command == "browser.http_auth.respond" {"http_auth_respond"} else {"webauthn_respond"});
            assert_eq!(args, json!({"accept":false}));
        }
        assert!(prepare_response(root.path(), "", "id", "browser.http_auth.respond",
            &json!({"accept":true,"confirmed":true,"secret_name":"AUTH"})).is_err());
        assert_eq!(prepare_response(root.path(), "", "id", "browser.webauthn.respond",
            &json!({"accept":true,"confirmed":true}))?.context("response")?,
            ("webauthn_respond", json!({"accept":true})));
        Ok(())
    }

    #[test]
    fn e44_http_auth_resolves_authorized_secret_without_accepting_inline_values() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().to_path_buf();
        crate::app_capabilities::cap3::tests::run_with_core_owners(root.clone(), move || -> anyhow::Result<()> {
            let (token, _) = store::issue_business_os_capability_token_for_managed_user(
                &root,"e44-owner","E44 fixture","chef",now_ms() as i64)?;
            crate::secrets::write_secret_record(&root,crate::secrets::credential_scope(),"E44_AUTH",
                r#"{"username":"fixture-user","password":"fixture-password"}"#,None,json!({}))?;
            let payload = json!({"accept":true,"confirmed":true,"secret_name":"E44_AUTH"});
            assert!(prepare_response(&root,"", "auth","browser.http_auth.respond",&payload).is_err());
            let (operation, args) = prepare_response(&root,&token,"auth","browser.http_auth.respond",&payload)?
                .context("HTTP auth response")?;
            assert_eq!(operation,"http_auth_respond");
            assert_eq!(args["username"],"fixture-user");
            assert_eq!(args["password"],"fixture-password");
            assert_eq!(args["accept"],true);
            Ok(())
        })
    }

    #[test]
    fn e44_upload_rejects_client_paths_before_file_materialization() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        assert!(is_browser_runtime_command("browser.upload.select"));
        for field in ["path", "filePath", "file_path"] {
            let mut payload = json!({"file_id":"known"});
            payload[field] = json!("/etc/passwd");
            let err = prepare_response(root.path(), "", "upload", "browser.upload.select", &payload)
                .err().context("forged path must fail")?;
            assert!(err.to_string().contains("file references only"));
        }
        Ok(())
    }

    #[test]
    fn e44_upload_materializes_verified_bytes_only_after_file_read_authorization() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().to_path_buf();
        crate::app_capabilities::cap3::tests::run_with_core_owners(root.clone(), move || -> anyhow::Result<()> {
            let (token, _) = store::issue_business_os_capability_token_for_managed_user(
                &root,"e44-owner","E44 fixture","chef",now_ms() as i64)?;
            let bytes = b"verified browser upload fixture";
            let hash = format!("{:x}",sha2::Sha256::digest(bytes));
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            let chunk_hash = format!("{:x}",sha2::Sha256::digest(encoded.as_bytes()));
            std::fs::create_dir_all(crate::paths::runtime_dir(&root))?;
            let conn = rusqlite::Connection::open(store::rxdb_store_path(&root))?;
            for table in ["ctox_business_os__desktop_files__v0","ctox_business_os__desktop_file_chunks__v0"] {
                rxdb::storage::sqlite::sql::ensure_collection_table(&conn,table)?;
            }
            let doc = json!({"id":"upload-file","name":"fixture.txt","kind":"file",
                "content_state":"available","size_bytes":bytes.len(),"content_hash":hash,
                "content_hash_scheme":"sha256-bytes-v1","content_generation_id":"generation"});
            let chunk = json!({"id":"upload-file_generation_0","file_id":"upload-file",
                "generation_id":"generation","content_hash":hash,"content_hash_scheme":"sha256-bytes-v1",
                "idx":0,"total":1,"encoding":"base64","data":encoded,"chunk_hash":chunk_hash,
                "chunk_hash_scheme":"sha256-base64-chunk-v1"});
            conn.execute("INSERT INTO ctox_business_os__desktop_files__v0(id,revision,deleted,lastWriteTime,data) VALUES(?1,'1-fixture',0,1,?2)",
                rusqlite::params!["upload-file",doc.to_string()])?;
            conn.execute("INSERT INTO ctox_business_os__desktop_file_chunks__v0(id,revision,deleted,lastWriteTime,data) VALUES(?1,'1-fixture',0,1,?2)",
                rusqlite::params!["upload-file_generation_0",chunk.to_string()])?;
            let payload = json!({"file_id":"upload-file"});
            let denied = prepare_response(&root,"", "upload","browser.upload.select",&payload)
                .err().context("missing file grant must fail")?;
            assert!(denied.to_string().contains("file read is forbidden"));
            assert!(!root.join("runtime/business-os/chat-attachments/upload").exists());
            let (operation, args) = prepare_response(&root,&token,"upload","browser.upload.select",&payload)?
                .context("upload response")?;
            assert_eq!(operation,"upload");
            let path = Path::new(args["filePath"].as_str().context("native materialized path")?);
            assert!(path.starts_with(root.join("runtime/business-os/chat-attachments/upload")));
            assert_eq!(std::fs::read(path)?,bytes);
            conn.execute("UPDATE ctox_business_os__desktop_file_chunks__v0 SET data=json_set(data,'$.chunk_hash','bad')",[])?;
            assert!(prepare_response(&root,&token,"corrupt","browser.upload.select",&payload).is_err());
            Ok(())
        })
    }

    #[tokio::test]
    async fn e44_observer_grant_revoke_persist_without_control_and_reject_foreign_owner() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let db = open_test_database(root.path().join("e44.sqlite3")).await?;
        let mut collections = HashMap::new();
        for name in ["browser_sessions", "browser_tabs", "browser_frames"] {
            collections.insert(name.to_owned(), RxCollectionCreator {
                schema: business_os_schema(name, "id"), conflict_handler: None, options: HashMap::new(),
            });
        }
        db.add_collections(collections).await?;
        let session = json!({"id":"session","owner_user_id":"alice","tenant_id":"tenant",
            "controller_user_id":"alice","controller_lease_id":"lease","status":"active",
            "allowed_observer_user_ids":[]});
        db.collection("browser_sessions").context("sessions")?.incremental_upsert(session.clone()).await?;
        upsert_browser_tab(&db,"tab","session","Title","https://example.test/","active",false,false,false,None,0,None).await?;
        upsert_browser_frame(&db,"frame","session","tab",1,"image/jpeg","",10,10,0,"fixture",None).await?;
        let bob = json!({"user_id":"bob"});
        assert!(update_observers(&db,&session,"bob","browser.observer.grant",&bob).await.is_err());
        for (command, expected) in [("browser.observer.grant", json!(["bob"])),("browser.observer.revoke",json!([]))] {
            assert!(is_browser_runtime_command(command));
            let current = find_browser_document(&db,"browser_sessions","session").await?;
            assert_eq!(update_observers(&db,&current,"alice",command,&bob).await?, expected);
            let stored = find_browser_document(&db,"browser_sessions","session").await?;
            assert_eq!(stored["allowed_observer_user_ids"],expected);
            assert_eq!(stored["controller_user_id"],"alice");
            assert_eq!(stored["controller_lease_id"],"lease");
            assert_eq!(find_browser_document(&db,"browser_tabs","tab").await?["allowed_observer_user_ids"],expected);
            assert_eq!(find_browser_document(&db,"browser_frames","frame").await?["allowed_observer_user_ids"],expected);
            // A later projection refresh must preserve the authoritative grant.
            upsert_browser_session(&db,"session","tab","active","active","https://example.test/","Title",
                1280,720,None,0,"browser.input",0,None,None,None).await?;
            assert_eq!(find_browser_document(&db,"browser_sessions","session").await?["allowed_observer_user_ids"],expected);
        }
        assert!(require_owned_session(&Value::Null,"alice").is_err());
        db.close().await?;
        Ok(())
    }
}
