use super::*;
use crate::app_capabilities::registry::AuditEntry;
use crate::app_capabilities::{Actor, CapabilityGrant, ExecutorFence};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
};
struct Host {
    ctx: CapCallContext,
    guard: Cell<bool>,
    deny: Cell<bool>,
    audit: Cell<usize>,
}
impl Host {
    fn new() -> Self {
        Self {
            ctx: CapCallContext {
                app_id: "test-app".into(),
                app_ver: "1".into(),
                actor: Actor {
                    id: "actor".into(),
                    role: "owner".into(),
                },
                command_id: "command".into(),
                call_id: "call".into(),
                fence: ExecutorFence {
                    run_id: "run".into(),
                    participant: "core".into(),
                    fence: 3,
                    expires_at_core_ms: 30000,
                },
            },
            guard: Cell::new(false),
            deny: Cell::new(false),
            audit: Cell::new(0),
        }
    }
}
impl CapabilityHost for Host {
    fn authorize(
        &self,
        ctx: &CapCallContext,
        _: &str,
        _: &str,
        _: &Value,
    ) -> Result<CapabilityGrant, CtoxError> {
        if self.deny.get() || ctx != &self.ctx {
            return Err(CtoxError::new(ErrorCode::Forbidden, "denied"));
        }
        Ok(CapabilityGrant {
            app_id: ctx.app_id.clone(),
            app_ver: ctx.app_ver.clone(),
            actor: ctx.actor.clone(),
            command_id: ctx.command_id.clone(),
            capabilities_required: ROUTES
                .iter()
                .chain(mail::ROUTES)
                .map(|r| r.capability.to_owned())
                .collect::<BTreeSet<_>>(),
            allowed_roles: ["owner".into()].into_iter().collect(),
        })
    }
    fn now_ms(&self) -> u64 {
        1000
    }
    fn with_fence(
        &self,
        _: &CapCallContext,
        _: &str,
        _: &str,
        _: &Value,
        effect: &mut dyn FnMut() -> Result<Value, CtoxError>,
    ) -> Result<Value, CtoxError> {
        self.guard.set(true);
        let result = effect();
        self.guard.set(false);
        result
    }
    fn audit(&self, _: &AuditEntry) -> Result<(), CtoxError> {
        self.audit.set(self.audit.get() + 1);
        Ok(())
    }
}
struct Blobs<'a> {
    host: &'a Host,
    bytes: RefCell<BTreeMap<String, Vec<u8>>>,
    reads: Cell<usize>,
    writes: Cell<usize>,
    corrupt: Cell<bool>,
}
impl<'a> Blobs<'a> {
    fn new(host: &'a Host) -> Self {
        Self {
            host,
            bytes: RefCell::new(BTreeMap::new()),
            reads: Cell::new(0),
            writes: Cell::new(0),
            corrupt: Cell::new(false),
        }
    }
    fn seed(&self, bytes: &[u8]) -> blob::BlobRef {
        let hash = format!("{:x}", Sha256::digest(bytes));
        self.bytes.borrow_mut().insert(hash.clone(), bytes.to_vec());
        blob::BlobRef {
            sha256: hash.clone(),
            size: bytes.len() as u64,
            transfer_id: format!("transfer-{hash}"),
        }
    }
}
impl blob::BlobAccess for Blobs<'_> {
    fn read(&self, ctx: &CapCallContext, reference: &blob::BlobRef) -> Result<Vec<u8>, CtoxError> {
        assert!(self.host.guard.get());
        assert_eq!(ctx, &self.host.ctx);
        self.reads.set(self.reads.get() + 1);
        if self.corrupt.get() {
            return Ok(vec![0]);
        }
        self.bytes
            .borrow()
            .get(&reference.sha256)
            .cloned()
            .ok_or_else(|| CtoxError::new(ErrorCode::NotFound, "missing"))
    }
    fn publish(
        &self,
        ctx: &CapCallContext,
        bytes: &[u8],
        _: &str,
    ) -> Result<blob::BlobRef, CtoxError> {
        assert!(self.host.guard.get());
        assert_eq!(ctx, &self.host.ctx);
        self.writes.set(self.writes.get() + 1);
        Ok(self.seed(bytes))
    }
}
fn send_request() -> mail::SendRequest {
    decode(&json!({"account_id":"sender@example.test","to":["to@example.test"],"subject":"Subject","text":"body","idempotency_key":"key"})).unwrap()
}
fn channels_fixture(root: &Path) {
    crate::mission::channels::ensure_store(root).unwrap();
}
fn document_fixture() -> Vec<u8> {
    use std::io::{Cursor, Write};
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in [
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "word/document.xml",
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Capability Mail</w:t></w:r></w:p></w:body></w:document>"#,
        ),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn app_capabilities_cap2_file_boundary_contract() {
    {
        let root = tempfile::tempdir().unwrap();
        let host = Host::new();
        let blobs = Blobs::new(&host);
        let reference = blobs.seed(b"hello");
        let backend = Cap2Backend {
            root: root.path(),
            blobs: &blobs,
        };
        for function in ["materialize", "export"] {
            assert_eq!(
                invoke(
                    &host,
                    &backend,
                    &host.ctx,
                    "blob",
                    function,
                    json!({"blob_id":reference})
                )
                .unwrap()["sha256"],
                reference.sha256
            );
        }
        let reads = blobs.reads.get();
        for request in [
            json!({"blob_id":"/etc/passwd"}),
            json!({"blob_id":reference,"path":"/etc/passwd"}),
            json!({"blob_id":{"sha256":"../x","size":1,"transfer_id":"x"}}),
        ] {
            assert_eq!(
                invoke(&host, &backend, &host.ctx, "blob", "materialize", request)
                    .unwrap_err()
                    .code,
                ErrorCode::Invalid
            );
        }
        assert_eq!(blobs.reads.get(), reads);
        blobs.corrupt.set(true);
        assert_eq!(
            invoke(
                &host,
                &backend,
                &host.ctx,
                "blob",
                "materialize",
                json!({"blob_id":reference})
            )
            .unwrap_err()
            .code,
            ErrorCode::Conflict
        );
    }
    {
        let root = tempfile::tempdir().unwrap();
        let ctx = Host::new().ctx;
        let store = ctox_transfers::Store::open(
            root.path().join("native.sqlite3"),
            root.path().join("artifacts"),
        )
        .unwrap();
        let hash = "a".repeat(64);
        store
            .enqueue(ctox_transfers::DownloadRequest {
                id: "transfer".into(),
                sources: vec![],
                peer_source: Some(ctox_transfers::PeerSource {
                    instance_id: "instance".into(),
                    public_key: "key".into(),
                    collection: "desktop_files".into(),
                    file_id: "file".into(),
                    account_binding: Some(ctox_transfers::PeerAccountBinding {
                        target_id: "target".into(),
                        account_epoch: 1,
                        grant_id: "grant".into(),
                        principal_sha256: "b".repeat(64),
                    }),
                }),
                storage: None,
                sha256: hash.clone(),
                size: 1,
            })
            .unwrap();
        let reference = blob::BlobRef {
            sha256: hash,
            size: 1,
            transfer_id: "transfer".into(),
        };
        let calls = Cell::new(0);
        let error = blob::read_completed_transfer(&store, &ctx, &reference, &|actual, _| {
            assert_eq!(actual, &ctx);
            calls.set(calls.get() + 1);
            Err(CtoxError::new(ErrorCode::Forbidden, "revoked"))
        })
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::Forbidden);
        assert_eq!(calls.get(), 1);
    }
}
#[test]
fn app_capabilities_cap2_office_version_and_formats_contract() {
    {
        let root = tempfile::tempdir().unwrap();
        let host = Host::new();
        let blobs = Blobs::new(&host);
        let source = blobs.seed(b"Name,Value\nAlpha,2\n");
        let backend = Cap2Backend {
            root: root.path(),
            blobs: &blobs,
        };
        let prepared = invoke(
            &host,
            &backend,
            &host.ctx,
            "office.edit",
            "prepare",
            json!({"kind":"spreadsheet","blob_id":source}),
        )
        .unwrap();
        assert_eq!(prepared["protocol"], "euro-office-cell-binary-v10");
        let session: OfficeSession = decode(&prepared["session"]).unwrap();
        let applied=invoke(&host,&backend,&host.ctx,"office.edit","apply",json!({"session":session,"changes":session.editor,"base_version":session.editor.sha256})).unwrap();
        assert_eq!(applied["manifest"]["package_sha256"], applied["sha256"]);
        let exported = invoke(
            &host,
            &backend,
            &host.ctx,
            "office.render",
            "render",
            json!({"session":applied["session"],"to":"xlsx"}),
        )
        .unwrap();
        let reference: blob::BlobRef = decode(&exported["blob_id"]).unwrap();
        assert!(blobs.bytes.borrow()[&reference.sha256].starts_with(b"PK"));
        let reads_before = blobs.reads.get();
        let writes_before = blobs.writes.get();
        let error = invoke(
            &host,
            &backend,
            &host.ctx,
            "office.edit",
            "apply",
            json!({"session":session,"changes":session.editor,"base_version":"stale"}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::Conflict);

        assert_eq!(blobs.reads.get(), reads_before);
        assert_eq!(blobs.writes.get(), writes_before);
    }
    {
        let root = tempfile::tempdir().unwrap();
        let host = Host::new();
        let blobs = Blobs::new(&host);
        let source = blobs.seed(&document_fixture());
        let backend = Cap2Backend {
            root: root.path(),
            blobs: &blobs,
        };
        let prepared = invoke(
            &host,
            &backend,
            &host.ctx,
            "office.edit",
            "prepare",
            json!({"kind":"document","blob_id":source}),
        )
        .unwrap();
        let exported = invoke(
            &host,
            &backend,
            &host.ctx,
            "office.render",
            "render",
            json!({"session":prepared["session"],"to":"docx"}),
        )
        .unwrap();
        assert_eq!(exported["mime_type"], OfficeKind::Document.canonical_mime());
        let email = invoke(
            &host,
            &backend,
            &host.ctx,
            "office.render",
            "render",
            json!({"session":prepared["session"],"to":"email_html"}),
        )
        .unwrap();
        let reference: blob::BlobRef = decode(&email["blob_id"]).unwrap();
        assert!(
            String::from_utf8(blobs.bytes.borrow()[&reference.sha256].clone())
                .unwrap()
                .contains("Capability Mail")
        );
        let reference: blob::BlobRef = decode(&email["text_blob"]).unwrap();
        assert!(
            String::from_utf8(blobs.bytes.borrow()[&reference.sha256].clone())
                .unwrap()
                .contains("Capability Mail")
        );
    }
}
#[test]
fn app_capabilities_cap2_mail_delivery_once_contract() {
    {
        let root = tempfile::tempdir().unwrap();
        let host = Host::new();
        let request = send_request();
        let calls = Cell::new(0);
        let receipt = mail::send_once(root.path(), &host.ctx, &request, || {
            calls.set(calls.get() + 1);
            Ok(json!({"message_id":"native-id","status":"accepted"}))
        })
        .unwrap();
        assert_eq!(
            receipt,
            json!({"message_id":"native-id","status":"accepted"})
        );
        let mut retry = host.ctx.clone();
        retry.command_id = "new-command".into();
        retry.call_id = "new-call".into();
        retry.app_ver = "2".into();
        assert_eq!(
            mail::send_once(root.path(), &retry, &request, || panic!("duplicate send")).unwrap(),
            receipt
        );
        let mut changed = request.clone();
        changed.text = "different".into();
        assert!(mail::send_once(root.path(), &retry, &changed, || panic!(
            "changed immutable intent sent"
        ))
        .is_err());
        assert_eq!(calls.get(), 1);
    }
    {
        let root = tempfile::tempdir().unwrap();
        let host = Host::new();
        let request = send_request();
        assert!(
            mail::send_once(root.path(), &host.ctx, &request, || Err(
                CtoxError::uncertain()
            ))
            .unwrap_err()
            .uncertain
        );
        assert!(
            mail::send_once(root.path(), &host.ctx, &request, || panic!(
                "uncertain resend"
            ))
            .unwrap_err()
            .uncertain
        );
    }
    {
        let root = tempfile::tempdir().unwrap();
        let host = Host::new();
        for key in ["", "\nkey"] {
            let mut request = send_request();
            request.idempotency_key = key.into();
            assert_eq!(
                mail::send_once(root.path(), &host.ctx, &request, || panic!("invalid send"))
                    .unwrap_err()
                    .code,
                ErrorCode::Invalid
            );
        }
        assert!(!crate::paths::core_db(root.path()).exists());
        assert!(mail::validate_legacy(&json!({"config":{"password":"not-persisted"}})).is_err());
        assert!(mail::validate_legacy(&json!({"attachment_path":"/etc/passwd"})).is_err());
        assert!(
            mail::validate_legacy(&json!({"config":{"password_ref":"secret-reference"}})).is_ok()
        );
        assert_eq!(
            mail::redact_paths(json!({"dbPath":"/native","result":{"path":"/native","ok":true}})),
            json!({"result":{"ok":true}})
        );
    }
    {
        // Sender resolution is a synchronous Core caller: daemon owners for
        // this root, on the bounded run_core_sync executor.
        let root = tempfile::tempdir().unwrap();
        let path = root.path().to_path_buf();
        crate::app_capabilities::cap3::tests::run_with_core_owners(path.clone(), move || {
            let root = path.as_path();
            let ctx = Host::new().ctx;
            let mut settings = std::collections::BTreeMap::new();
            settings.insert("CTO_EMAIL_ADDRESS".into(), "instance@example.test".into());
            settings.insert("CTO_EMAIL_PROVIDER".into(), "imap".into());
            crate::inference::runtime_env::save_runtime_env_map(root, &settings).unwrap();
            let sender = mail::resolve_sender(root, &ctx, "instance@example.test").unwrap();
            assert_eq!(sender.address, "instance@example.test");
            assert_eq!(sender.provider.as_deref(), Some("imap"));
            assert_eq!(
                mail::resolve_sender(root, &ctx, "unconfigured@example.test")
                    .err()
                    .unwrap()
                    .code,
                ErrorCode::NotFound
            );
        });
    }
    {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Barrier,
        };
        let root = tempfile::tempdir().unwrap();
        channels_fixture(root.path());
        let barrier = Arc::new(Barrier::new(2));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut threads = Vec::new();
        for _ in 0..2 {
            let root = root.path().to_path_buf();
            let barrier = barrier.clone();
            let calls = calls.clone();
            threads.push(std::thread::spawn(move || {
                let ctx = Host::new().ctx;
                let request = send_request();
                barrier.wait();
                mail::send_once(&root, &ctx, &request, || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    Ok(json!({"message_id":"one-effect","status":"accepted"}))
                })
            }));
        }
        let outcomes = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(outcomes.iter().any(Result::is_ok));
        for result in outcomes {
            if let Err(error) = result {
                assert!(error.uncertain);
            }
        }
    }
}
#[test]
fn app_capabilities_cap2_parquet_window_contract() {
    {
        let root = tempfile::tempdir().unwrap();
        let host = Host::new();
        let blobs = Blobs::new(&host);
        crate::knowledge::seed_knowledge_table_for_test(
            root.path(),
            "table",
            "domain",
            "values",
            &[json!({"value":1}), json!({"value":2}), json!({"value":3})],
            None,
        )
        .unwrap();
        let backend = Cap2Backend {
            root: root.path(),
            blobs: &blobs,
        };
        let value = invoke(
            &host,
            &backend,
            &host.ctx,
            "dataframe",
            "window",
            json!({"table":"table","skip":1,"limit":1}),
        )
        .unwrap();
        assert_eq!(value["row_count"], 3);
        assert_eq!(value["rows"].as_array().unwrap().len(), 1);
        assert_eq!(value["rows"][0]["value"], 2);
        assert!(value["content_hash"]
            .as_str()
            .is_some_and(|s| !s.is_empty()));
        assert_eq!(
            invoke(
                &host,
                &backend,
                &host.ctx,
                "dataframe",
                "window",
                json!({"table":"missing","limit":1})
            )
            .unwrap_err()
            .code,
            ErrorCode::NotFound
        );
        assert_eq!(
            invoke(
                &host,
                &backend,
                &host.ctx,
                "dataframe",
                "window",
                json!({"table":"table","limit":1,"filter":{"value":2}})
            )
            .unwrap_err()
            .code,
            ErrorCode::Invalid
        );
    }
}
#[test]
fn app_capabilities_cap2_mail_reads_contract() {
    {
        let root = tempfile::tempdir().unwrap();
        channels_fixture(root.path());
        let mut connection =
            rusqlite::Connection::open(crate::paths::core_db(root.path())).unwrap();
        for (key, thread, body) in [
            ("message-a", "thread-a", "Complete body A"),
            ("message-b", "thread-b", "Foreign body B"),
        ] {
            crate::communication_store::upsert_communication_message(
                &mut connection,
                crate::communication_store::UpsertMessage {
                    message_key: key,
                    channel: "email",
                    account_key: "account",
                    thread_key: thread,
                    remote_id: key,
                    direction: "inbound",
                    folder_hint: "inbox",
                    sender_display: "Sender",
                    sender_address: "sender@example.test",
                    recipient_addresses_json: "[]",
                    cc_addresses_json: "[]",
                    bcc_addresses_json: "[]",
                    subject: "Subject",
                    preview: "Preview",
                    body_text: body,
                    body_html: "<p>Body</p>",
                    raw_payload_ref: "/private/native/source",
                    trust_level: "known",
                    status: "received",
                    seen: false,
                    has_attachments: false,
                    external_created_at: "2026-10-15T00:00:00Z",
                    observed_at: "2026-10-15T00:00:00Z",
                    metadata_json: "{}",
                },
            )
            .unwrap();
        }
        let host = Host::new();
        let blobs = Blobs::new(&host);
        let backend = Cap2Backend {
            root: root.path(),
            blobs: &blobs,
        };
        let result = invoke(
            &host,
            &backend,
            &host.ctx,
            "mail.read",
            "read",
            json!({"thread_key":"thread-a"}),
        )
        .unwrap();
        assert_eq!(result["messages"].as_array().unwrap().len(), 1);
        assert_eq!(result["messages"][0]["body_text"], "Complete body A");
        assert!(!result.to_string().contains("/private/native/source"));
    }
    {
        let root = tempfile::tempdir().unwrap();
        channels_fixture(root.path());
        let db = crate::paths::core_db(root.path());
        let store = ctox_mailserver::store::sqlite::SqliteStore::new(db.to_str().unwrap());
        store.init().unwrap();
        store
            .record_delivery_outcome("wanted", "sender", "recipient", "delivered", None, 10)
            .unwrap();
        store
            .record_delivery_outcome("other", "sender", "recipient", "failed", None, 20)
            .unwrap();
        let host = Host::new();
        let blobs = Blobs::new(&host);
        let backend = Cap2Backend {
            root: root.path(),
            blobs: &blobs,
        };
        let result = invoke(
            &host,
            &backend,
            &host.ctx,
            "mail.delivery_log",
            "query",
            json!({"message_ids":["wanted"],"since":5}),
        )
        .unwrap();
        assert_eq!(
            result["deliveries"],
            json!([{"message_id":"wanted","status":"delivered","at":10}])
        );
        let empty = invoke(
            &host,
            &backend,
            &host.ctx,
            "mail.delivery_log",
            "query",
            json!({"since":21}),
        )
        .unwrap();
        assert_eq!(empty["deliveries"], json!([]));
    }
}
#[test]
fn app_capabilities_cap2_transport_contract() {
    {
        use crate::app_capabilities::protocol::{CapMessage, CapOutcome};
        let root = tempfile::tempdir().unwrap();
        let host = Host::new();
        let blobs = Blobs::new(&host);
        let reference = blobs.seed(b"blob");
        let backend = Cap2Backend {
            root: root.path(),
            blobs: &blobs,
        };
        let message = CapMessage::Invoke {
            request_id: host.ctx.call_id.clone(),
            ctx: host.ctx.clone(),
            name: "blob".into(),
            function: "materialize".into(),
            args: json!({"blob_id":reference}),
        };
        let decoded = CapMessage::decode(&message.encode().unwrap()).unwrap();
        let response = handle(&host, &backend, decoded).unwrap();
        assert!(
            matches!(response,CapMessage::Result{request_id,outcome:CapOutcome::Ok{..}} if request_id=="call")
        );

        let reads = blobs.reads.get();
        host.deny.set(true);
        assert_eq!(
            invoke(
                &host,
                &backend,
                &host.ctx,
                "blob",
                "materialize",
                json!({"blob_id":reference})
            )
            .unwrap_err()
            .code,
            ErrorCode::Forbidden
        );
        assert_eq!(blobs.reads.get(), reads);
        assert_eq!(resolve("mail", "send").unwrap().capability, "mail.send");
        assert_eq!(
            resolve("office", "render").unwrap().capability,
            "office.render"
        );
        assert_eq!(
            resolve("mail.server.config", "get").unwrap().function,
            "config.get"
        );
        assert_eq!(resolve("doc", "extract").unwrap().capability, "doc.extract");
        assert_eq!(resolve("doc", "html").unwrap().capability, "doc.html");
        assert!(resolve("mail", "arbitrary").is_none());
    }
}
#[test]
fn app_capabilities_cap2_document_boundary_contract() {
    {
        let root = tempfile::tempdir().unwrap();
        let host = Host::new();
        let blobs = Blobs::new(&host);
        let reference=blobs.seed(b"<html><p class='x'>Hello <b>World</b></p><p class='x'>Second</p><a href='/a'>A</a></html>");
        let backend = Cap2Backend {
            root: root.path(),
            blobs: &blobs,
        };
        let text = invoke(
            &host,
            &backend,
            &host.ctx,
            "doc.html",
            "query",
            json!({"blob_id":reference,"selector":"p.x","limit":1}),
        )
        .unwrap();
        assert_eq!(text["matches"], json!(["Hello World"]));
        let attr = invoke(
            &host,
            &backend,
            &host.ctx,
            "doc",
            "html",
            json!({"blob_id":reference,"selector":"a","attribute":"href"}),
        )
        .unwrap();
        assert_eq!(attr["matches"], json!(["/a"]));
        let plain = blobs.seed(b"body text");
        assert_eq!(
            invoke(
                &host,
                &backend,
                &host.ctx,
                "doc",
                "extract",
                json!({"blob_id":plain,"kind":"text"})
            )
            .unwrap()["text"],
            "body text"
        );
        assert_eq!(
            invoke(
                &host,
                &backend,
                &host.ctx,
                "doc.html",
                "query",
                json!({"blob_id":reference,"selector":"["})
            )
            .unwrap_err()
            .code,
            ErrorCode::Invalid
        );
    }
    {
        let root = tempfile::tempdir().unwrap();
        let host = Host::new();
        let blobs = Blobs::new(&host);
        let backend = Cap2Backend {
            root: root.path(),
            blobs: &blobs,
        };
        for url in ["file:///etc/passwd", "https://name:secret@example.test/"] {
            assert_eq!(
                invoke(
                    &host,
                    &backend,
                    &host.ctx,
                    "web",
                    "fetch",
                    json!({"url":url})
                )
                .unwrap_err()
                .code,
                ErrorCode::Invalid
            );
        }
        assert_eq!(blobs.reads.get() + blobs.writes.get(), 0);
    }
}
