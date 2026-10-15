use super::*;

#[tokio::test]
async fn actual_a9_base_is_activated_through_registry_and_replay_has_no_new_seq() -> Result<()> {
    let inputs = std::env::var("IMPORT_INPUT_ROOT").expect("copied immutable release required");
    let defaults = schema::BaseReleaseDefaults {app_ver:"main@4794af03d+v4".into(),client_limit:100000,byte_budget:33554432,rights_ver:1};
    let plan = base_release::load(&std::path::Path::new(&inputs).join("v4"),&std::path::Path::new(&inputs).join("ctox-platform.manifest.json"),&defaults)?;
    assert!(plan.collections["ctox-platform"].iter().any(|d|d.name == "app_drafts"));
    assert!(!plan.removed_sync_settings.is_empty());
    for definitions in plan.collections.values() { for definition in definitions {
        assert!(definition.schema_json["properties"].is_object(),"A9 schema envelope must be unwrapped: {}",definition.name);
        assert!(definition.schema_json.get("primaryKey").is_some(),"primary key missing: {}",definition.name);
    }}
    let root = tempfile::tempdir()?;
    let writer = Registry.open(DbId::new(root.path().join("all-apps.sqlite3"))?,WriterOptions::default())?;
    writer.ready().await?;
    base_release::publish(&writer,&plan,&ReleaseMigration,1).await?;
    let collections = plan.collections.clone();
    let head = writer.submit(WriteJob::new(JobOptions::default(),move |tx| {
        for definitions in collections.values() { for def in definitions {
            let actual = sql(schema::active_collection(tx,&def.name))?.expect("published collection must be active");
            assert_eq!(actual.definition,*def);
        }}
        sql(crate::change_log::head(tx))
    }))?.await?;
    base_release::publish(&writer,&plan,&ReleaseMigration,2).await?;
    let after = writer.submit(WriteJob::new(JobOptions::default(),|tx|sql(crate::change_log::head(tx))))?.await?;
    assert_eq!(head.head_seq,after.head_seq);
    writer.shutdown().await?; Ok(())
}
