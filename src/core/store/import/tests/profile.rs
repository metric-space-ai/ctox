use super::*;
struct NoTimeseries;
impl core_import::DataframeImporter for NoTimeseries {
    fn import_timeseries(&mut self,_:&str,_:&[Value])->Result<usize> { Err(invalid("unexpected timeseries")) }
}
#[tokio::test]
async fn devops_profile_copy_imports_crew_and_replay_does_not_emit_changes() -> Result<()> {
    let input = std::env::var("IMPORT_INPUT_ROOT").expect("copied release inputs required");
    let mut source = rusqlite::Connection::open_with_flags(std::path::Path::new(&input).join("core-allowlisted-v2.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let tx = source.transaction()?;
    let snapshot = core_import::collect(&tx,&mut NoFiles,100000,128*1024*1024)?;
    let source_members:i64 = tx.query_row("SELECT count(*) FROM crew_members",[],|r|r.get(0))?;
    assert!(source_members > 0,"fixture must exercise populated core state");
    let defaults = schema::BaseReleaseDefaults {app_ver:"main@4794af03d+v4".into(),client_limit:100000,byte_budget:33554432,rights_ver:1};
    let plan = base_release::load(&std::path::Path::new(&input).join("v4"),&std::path::Path::new(&input).join("ctox-platform.manifest.json"),&defaults)?;
    let target = tempfile::tempdir()?;
    let writer = Registry.open(DbId::new(target.path().join("sync.sqlite3"))?,WriterOptions::default())?;
    writer.ready().await?;
    base_release::publish(&writer,&plan,&ReleaseMigration,1).await?;
    let report = core_import::apply(&writer,snapshot,&mut NoTimeseries,"devops-810-core").await?;
    assert_eq!(report.collections["ctox_crew_members"].source_rows,source_members as usize);
    assert_eq!(report.collections["ctox_crew_members"].target_documents,source_members as usize);
    let snapshot = core_import::collect(&tx,&mut NoFiles,100000,128*1024*1024)?;
    let before = writer.submit(WriteJob::new(JobOptions::default(),|tx|sql(crate::change_log::head(tx))))?.await?;
    let replay = core_import::apply(&writer,snapshot,&mut NoTimeseries,"devops-810-core").await?;
    assert_eq!(replay.collections["ctox_crew_members"].inserted,0);
    assert_eq!(replay.collections["ctox_crew_members"].already_present,source_members as usize);
    let after = writer.submit(WriteJob::new(JobOptions::default(),|tx|sql(crate::change_log::head(tx))))?.await?;
    assert_eq!(before.head_seq,after.head_seq);
    let evidence = serde_json::json!({"profile":"DevOps#810 allowlisted real daemon core copy","source_writes":0,"report":report,"replay":replay,"head_before":before.head_seq.to_string(),"head_after":after.head_seq.to_string(),"pass":true});
    let proof = std::env::var("IMPORT_PROOF_DIR").expect("private proof directory required");
    std::fs::write(std::path::Path::new(&proof).join("profile-import.json"),serde_json::to_vec_pretty(&evidence).unwrap())?;
    writer.shutdown().await?; Ok(())
}
