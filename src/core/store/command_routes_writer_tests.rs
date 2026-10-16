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
