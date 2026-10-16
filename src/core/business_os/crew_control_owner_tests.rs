use super::*;

#[tokio::test]
async fn crew_control_owner_memory_and_member_touch_are_one_commit() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = crate::paths::core_db(root.path());
    std::fs::create_dir_all(path.parent().unwrap())?;
    drop(crate::lcm::LcmEngine::open(
        &path,
        crate::lcm::LcmConfig::default(),
    )?);
    let (writer, readers) = ctox_store::testkit::StoreFixture::open(ctox_store::DbId::new(&path)?)
        .await?
        .into_parts();
    let adapter =
        crate::lcm::owner_adapter::OwnerAdapter::attach(&path, writer.clone(), readers.clone())
            .await?;
    let work = root.path().to_owned();
    crate::persistence::run_core_sync(move || {
        let path = crate::paths::core_db(&work);
        crate::lcm::with_lcm_write(&path, |conn| {
            conn.execute_batch("CREATE TABLE communication_routing_state(message_key TEXT PRIMARY KEY,route_status TEXT,leased_at TEXT);")?;
            crew::ensure_schema(conn)?;
            conn.execute_batch("CREATE TRIGGER fail_member_touch BEFORE UPDATE OF updated_at ON crew_members BEGIN SELECT RAISE(ABORT,'member touch rejected'); END;")?;
            Ok(())
        })?;
        let payload = json!({"member_id":"crew-milo","kind":"anchors","mode":"diff","diff":"## Entries\n+ anchor_id: control_entry\n+ anchor_type: hypothesis\n+ statement: Schema prüfen\n"});
        let error = apply_control_core(&work, "ctox.crew.memory.update", &payload).unwrap_err();
        assert!(error.to_string().contains("member touch rejected"), "{error:#}");
        let conversation = crew::member_conversation_id("crew-milo");
        let count = crate::lcm::with_lcm_read(&path, move |conn| Ok(conn.query_row("SELECT COUNT(*) FROM continuity_documents WHERE conversation_id=?1", [conversation], |r| r.get::<_,i64>(0))?))?;
        assert_eq!(count, 0, "a failed member touch committed Crew memory");
        crate::lcm::with_lcm_write(&path, |conn| {conn.execute_batch("DROP TRIGGER fail_member_touch; CREATE TEMP TABLE control_owner_audit(n INTEGER); CREATE TEMP TRIGGER audit_member_touch AFTER UPDATE OF updated_at ON main.crew_members BEGIN INSERT INTO control_owner_audit VALUES(1); END;")?; Ok(())})?;
        let result = apply_control_core(&work, "ctox.crew.memory.update", &payload)?;
        assert_eq!(result["member_id"], "crew-milo");
        let count = crate::lcm::with_lcm_write(&path, |conn| Ok(conn.query_row("SELECT COUNT(*) FROM temp.control_owner_audit", [], |r| r.get::<_,i64>(0))?))?;
        assert_eq!(count, 1, "member touch escaped the existing owner writer");
        Ok(())
    }).await?;
    drop(adapter);
    readers.shutdown().await?;
    writer.shutdown().await?;
    Ok(())
}
