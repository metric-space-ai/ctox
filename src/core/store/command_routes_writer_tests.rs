use super::*;
#[tokio::test]
async fn e25_raw_secret_never_enters_order_buffer_or_command_history() {
    let (_dir, writer, readers, ingress) = commands().await;
    let before = readers.read(|tx| change_log::head(tx)).await.unwrap();
    for seq in [2, 1] {
        let mut w = command(seq, seq as u16, None, "queued", None, Value::Null);
        w.write.body.as_mut().unwrap()["type"] = json!("ctox.workjet.speech.settings.v1");
        w.write.body.as_mut().unwrap()["payload"] =
            json!({"action":"speech.settings.key","secret":"synthetic-never-persist"});
        let result = ingress.submit("alice".into(), w, NOW).await;
        let batch = result.unwrap();
        assert_eq!(batch.results[0].result.status, WriteStatus::Rejected);
        assert_eq!(batch.results[0].result.code.as_deref(), Some("invalid"));
        assert_eq!(
            batch.results[0].result.reason.as_deref(),
            Some("credential_ref_required")
        );
        assert_eq!(batch.next_seq, Seq(if seq == 2 { 1 } else { 3 }));
    }
    let after = readers
        .read(|tx| {
            for name in ["_write_pending"] {
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name=?1)",
                    [name],
                    |r| r.get(0),
                )?;
                if exists {
                    let n: i64 =
                        tx.query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| r.get(0))?;
                    assert_eq!(n, 0, "credential-bearing request reached {name}");
                }
            }
            let identities: Vec<String> = tx
                .prepare("SELECT request_json FROM _ops")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            assert!(!identities.is_empty());
            assert!(
                identities
                    .iter()
                    .all(|s| !s.contains("synthetic-never-persist")),
                "credential copied into the dedup ledger"
            );
            change_log::head(tx)
        })
        .await
        .unwrap();
    assert_eq!(before.head_seq, after.head_seq);
    let mut safe = command(3, 10, None, "queued", None, Value::Null);
    safe.write.body.as_mut().unwrap()["type"] = json!("ctox.workjet.speech.settings.v1");
    safe.write.body.as_mut().unwrap()["payload"] =
        json!({"action":"speech.settings.read","commandId":"command-1"});
    assert_eq!(
        send(&ingress, "alice", safe, NOW).await.status,
        WriteStatus::Accepted,
        "typed secret rejection must consume each ordered slot exactly once"
    );
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}

#[tokio::test]
async fn e25_workjet_inventory_is_registered_before_write() {
    let (_dir, writer, readers, ingress) = commands().await;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/sync_v4_e25/wjb-command-types.json"
    ))
    .unwrap();
    let types: std::collections::BTreeSet<String> = fixture
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| {
            r["command_types"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_owned())
        })
        .collect();
    assert!(!types.is_empty());
    let mut seq = 1;
    for kind in types {
        let route = routes::resolve(&kind).unwrap_or_else(|e| panic!("{kind}: {}", e.reason));
        assert_eq!(route.lane, 1);
        let mut w = command(seq, seq as u16, None, "queued", None, Value::Null);
        w.write.id = format!("wjb-{seq}");
        let body = w.write.body.as_mut().unwrap();
        body["id"] = json!(w.write.id);
        body["type"] = json!(kind);
        assert_eq!(
            send(&ingress, "alice", w, NOW).await.status,
            WriteStatus::Accepted,
            "{kind}"
        );
        seq += 1;
    }
    let mut unknown = command(seq, 999, None, "queued", None, Value::Null);
    unknown.write.id = "unknown-native".into();
    unknown.write.body.as_mut().unwrap()["id"] = json!("unknown-native");
    unknown.write.body.as_mut().unwrap()["type"] = json!("ctox.workjet.not_published");
    let result = send(&ingress, "alice", unknown, NOW).await;
    assert_eq!(result.status, WriteStatus::Rejected);
    assert_eq!(result.code.as_deref(), Some("unknown_command"));
    assert_eq!(result.reason.as_deref(), Some("unknown_command"));
    readers
        .read(|tx| {
            assert_eq!(
                tx.query_row("SELECT COUNT(*) FROM _write_pending", [], |r| r
                    .get::<_, i64>(0))?,
                0
            );
            let table = crate::schema::active_collection(tx, "business_commands")?
                .unwrap()
                .table;
            assert_eq!(
                tx.query_row(
                    &format!(
                        "SELECT COUNT(*) FROM {} WHERE id='unknown-native'",
                        change_log::identifier(&table)?
                    ),
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                0
            );
            Ok(())
        })
        .await
        .unwrap();
    readers.shutdown().await.unwrap();
    writer.shutdown().await.unwrap();
}
