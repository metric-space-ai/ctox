use super::*;
#[test]
fn e25_all_routes_complete_without_changing_method_payload_or_command_id() {
    println!(
        "E25_REGISTRY_SOURCE_SHA256 {:x}",
        Sha256::digest(include_bytes!("command_routes.rs"))
    );
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let inventory: Value =
        serde_json::from_str(include_str!("../../../tests/sync_v4_e25/e25-methods.json")).unwrap();
    for item in inventory["methods"].as_array().unwrap() {
        let route = resolve(item["method"].as_str().unwrap()).expect("missing E25 route");
        assert_eq!(
            resolve(route.method).unwrap(),
            route,
            "missing E25 route: {}",
            route.method
        );
        assert_eq!(route.lane, 1);
        let command = NativeCommand {
            command_id: "command-a".into(),
            kind: route.method.into(),
            payload: json!({"action":"fixture"}),
            app_id: "app".into(),
            actor_id: "sender".into(),
        };
        let native = if route.streaming {
            json!({"state":"finished","text":"final"})
        } else {
            json!({"native":"result"})
        };
        let got = completion(&tx, &command, native.clone(), None).unwrap();
        assert_eq!(
            serde_json::to_value(got).unwrap(),
            json!({"command_id":"command-a","status":"done","result":native})
        );
    }
    assert_eq!(
        resolve("not.a.native.method").unwrap_err().reason,
        "unknown_command"
    );
}
#[test]
fn e25_large_and_audio_results_require_verified_files_ref_in_same_transaction() {
    struct Sink;
    impl ResultFiles for Sink {
        fn put_json(
            &self,
            tx: &Transaction<'_>,
            command: &NativeCommand,
            bytes: &[u8],
        ) -> Result<FileRef, CommandError> {
            assert_eq!(command.app_id, "sender-app");
            let hash = format!("{:x}", Sha256::digest(bytes));
            tx.execute(
                "INSERT INTO files VALUES(?1,?2,?3)",
                rusqlite::params![hash, command.app_id, bytes],
            )
            .unwrap();
            Ok(FileRef { r#ref: hash })
        }
    }
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE files(ref TEXT,owner_app TEXT,data BLOB)")
        .unwrap();
    let mut command = NativeCommand {
        command_id: "cmd".into(),
        kind: ROUTES[0].method.into(),
        payload: json!({}),
        app_id: "sender-app".into(),
        actor_id: "user".into(),
    };
    {
        let tx = db.transaction().unwrap();
        for value in [
            json!({"long":"x".repeat(INLINE_RESULT_LIMIT)}),
            json!({"audioBase64":"AQID"}),
            json!({"state":"open","events":[{"partial":"x".repeat(INLINE_RESULT_LIMIT)}]}),
        ] {
            command.kind=if value.get("state").is_some(){"ctox.workjet.speech.dictation.v1"}else{"ctox.workjet.speech.settings.v1"}.into();
            assert_eq!(
                completion(&tx, &command, value.clone(), None)
                    .unwrap_err()
                    .reason,
                "files_sink_missing"
            );
            let result = completion(&tx, &command, value, Some(&Sink))
                .unwrap()
                .result
                .unwrap();
            let result=if command.kind=="ctox.workjet.speech.dictation.v1" {
                assert!(result.get("partial").is_some(),"large partial lost its running envelope");
                result["partial"].clone()
            } else {result};
            assert_eq!(result.as_object().unwrap().len(), 1);
            assert_eq!(result["ref"].as_str().unwrap().len(), 64);
            let exists: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM files WHERE ref=?1 AND owner_app='sender-app')",
                    [result["ref"].as_str().unwrap()],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(exists);
        }
        // Dropping the completion transaction must also roll back files metadata.
    }
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM files", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn e25_stream_partials_keep_the_command_id_and_never_finish_on_open() {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    for method in [
        "ctox.workjet.speech.dictation.v1",
        "ctox.workjet.jour_fixe.speech.v1",
    ] {
        let command = NativeCommand {
            command_id: "original-stream-command".into(),
            kind: method.into(),
            payload: json!({"op":"open"}),
            app_id: "sender-app".into(),
            actor_id: "alice".into(),
        };
        for state in ["open", "finishing", "running"] {
            let native = json!({"state":state,"events":[{"partial":"coalesced text"}]});
            let result = completion(&tx, &command, native.clone(), None).unwrap();
            assert_eq!(result.command_id, command.command_id);
            assert_eq!(result.status, "running");
            assert_eq!(result.result, Some(json!({"partial":native})));
        }
        let mut chunk=command.clone();
        chunk.command_id="chunk-control-command".into();chunk.payload=json!({"op":"write"});
        let ack=completion(&tx,&chunk,json!({"state":"open","streamId":"active-stream"}),None).unwrap();
        assert_eq!(ack.status,"done","chunk acknowledgement cannot own an endless stream");
        assert_eq!(ack.command_id,"chunk-control-command");
        assert_eq!(ack.result.unwrap()["state"],"open");
        assert_eq!(
            completion(
                &tx,
                &command,
                json!({"state":"finished","text":"final"}),
                None
            )
            .unwrap()
            .status,
            "done"
        );
        assert_eq!(
            completion(
                &tx,
                &command,
                json!({"state":"failed","text":"must not escape"}),
                None
            )
            .unwrap()
            .result,
            None
        );
    }
}
#[test]
fn e25_protected_control_results_never_become_command_or_files_payloads() {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let mut command = NativeCommand {
        command_id: "cmd".into(),
        kind: "ctox.workjet.device.v1".into(),
        payload: json!({"action":"invite.create"}),
        app_id: "sender-app".into(),
        actor_id: "alice".into(),
    };
    assert_eq!(
        command.validate().unwrap_err().reason,
        "protected_invite_result_adapter_required"
    );
    command.payload = json!({"action":"binding.list"});
    assert_eq!(
        completion(
            &tx,
            &command,
            json!({"invite":{"fixture":"not-a-real-invite"}}),
            None
        )
        .unwrap_err()
        .reason,
        "protected_result_adapter_required"
    );
    command.kind = "ctox.workjet.grok.v1".into();
    assert_eq!(
        completion(
            &tx,
            &command,
            json!({"login":{"userCode":"synthetic-only"}}),
            None
        )
        .unwrap_err()
        .reason,
        "protected_result_adapter_required"
    );
}
