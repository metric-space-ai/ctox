use super::*;

#[tokio::test]
async fn turn_refresh_uses_core_writer_and_query_only_snapshot() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = crate::paths::core_db(dir.path());
    let denied = lcm::LcmClient::attach(&path)
        .err()
        .expect("unadmitted caller denied");
    assert!(denied.to_string().contains("run_core_sync"));
    assert!(!path.exists());
    std::fs::create_dir_all(path.parent().expect("Core parent"))?;
    drop(lcm::LcmEngine::open(&path, lcm::LcmConfig::default())?);
    let (writer, readers) = ctox_store::testkit::StoreFixture::open(ctox_store::DbId::new(&path)?)
        .await?
        .into_parts();
    let adapter =
        lcm::owner_adapter::OwnerAdapter::attach(&path, writer.clone(), readers.clone()).await?;
    writer.submit(ctox_store::WriteJob::new(Default::default(),|tx|{
        ensure_turn_refresh_schema(tx).map_err(|e|ctox_store::StoreError::Invalid(e.to_string()))?;
        tx.execute_batch("CREATE TEMP TABLE turn_owner_audit(value INTEGER); CREATE TEMP TRIGGER audit_turn_refresh AFTER INSERT ON main.continuity_refresh_status BEGIN INSERT INTO turn_owner_audit VALUES(1); END;")?;
        tx.execute_batch("CREATE TABLE ticket_self_work_items(state TEXT,updated_at TEXT); CREATE TABLE ticket_knowledge_entries(created_at TEXT);")?;
        Ok(())
    }))?.await?;
    let work = path.clone();
    let root = dir.path().to_owned();
    crate::persistence::run_core_sync(move || {
        let client=lcm::LcmClient::attach(&work)?;
        assert!(lcm::LcmEngine::open(&work,lcm::LcmConfig::default()).is_err());
        client.continuity_init_documents(77)?;
        for turn in 1..8 {assert!(record_durable_refresh_demand(&work,77,false,0,120,&format!("turn:{turn}"))?.is_empty());}
        let due=record_durable_refresh_demand(&work,77,false,0,120,"turn:8")?;
        assert!(due.contains("narrative")&&due.contains("anchors")&&!due.contains("focus"));
        let observed=lcm::with_lcm_write(&work,|conn|Ok(conn.query_row("SELECT COUNT(*) FROM temp.turn_owner_audit",[],|r|r.get::<_,i64>(0))?))?;
        assert_eq!(observed,3,"refresh writes escaped the actual owner connection");
        let only=lcm::with_lcm_read(&work,|conn|Ok(conn.query_row("PRAGMA query_only",[],|r|r.get::<_,i64>(0))?))?;
        assert_eq!(only,1,"inspection escaped the snapshot reader");
        mark_durable_refresh_failed(&work,77,"narrative","model unavailable")?;
        let state=lcm::with_lcm_read(&work,|conn|Ok(conn.query_row("SELECT successful_turn_count,failure_attempt_count,last_error FROM continuity_refresh_status WHERE conversation_id=77 AND continuity_kind='narrative'",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?)))?))?;
        assert_eq!(state,(8,1,"model unavailable".into()));
        assert!(mark_durable_refresh_consumed(&work,77,"narrative","same","same").is_err());
        mark_durable_refresh_consumed(&work,77,"anchors","a","b")?;
        let status=lcm::with_lcm_read(&work,|conn|Ok(conn.query_row("SELECT status,successful_turn_count FROM continuity_refresh_status WHERE conversation_id=77 AND continuity_kind='anchors'",[],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))?))?;
        assert_eq!(status,("consumed".into(),0));
        let none=detect_durable_state_transition(&root,&work,77,"9999-01-01T00:00:00Z","9999-01-01T00:00:00Z")?;
        assert!(!none.detected&&none.probe_errors.is_empty());
        lcm::with_lcm_write(&work,|conn|{conn.execute("INSERT INTO ticket_self_work_items VALUES('closed','2026-10-16T00:00:00Z')",[])?;Ok(())})?;
        assert!(detect_durable_state_transition(&root,&work,77,"2026-01-01T00:00:00Z","2026-01-01T00:00:00Z")?.detected);
        let steps=[lcm::TaskExecutionPlanStepInput{label:"authorized step".into(),status:"in_progress".into()}];
        let input=lcm::TaskExecutionPlanUpdate{work_key:"guarded-work",task_id:"task",command_id:"command",attempt_id:"attempt",explanation:None,steps:&steps};
        client.record_task_execution_plan_guarded(lcm::TaskExecutionPlanUpdate{work_key:"guarded-work",task_id:"task",command_id:"command",attempt_id:"attempt",explanation:None,steps:&steps},|conn|{
            assert_eq!(conn.query_row("SELECT COUNT(*) FROM temp.turn_owner_audit",[],|r|r.get::<_,i64>(0))?,3);Ok(())
        })?;
        let rejected=client.record_task_execution_plan_guarded(input,|_|anyhow::bail!("authority denied")).unwrap_err();
        assert!(rejected.to_string().contains("authority denied"));
        let record=client.increment_mission_agent_failure_count(77)?;assert_eq!(record.agent_failure_count,1);
        assert_eq!(client.reset_mission_agent_failure_count(77)?.record.agent_failure_count,0);
        assert_eq!(client.continuity_show_all(77)?.conversation_id,77);
        Ok(())
    }).await?;
    let committed=readers.read_job(ctox_store::ReadJob::new(Default::default(),|conn|Ok(conn.query_row("SELECT status FROM continuity_refresh_status WHERE conversation_id=77 AND continuity_kind='anchors'",[],|r|r.get::<_,String>(0))?))).await?;
    assert_eq!(committed, "consumed");
    drop(adapter);
    drop(readers);
    writer.shutdown().await?;
    Ok(())
}
