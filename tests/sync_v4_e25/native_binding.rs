// Included in the existing native foreground/quorum proof; reuse its actual
// writer, retained Raft authority, signed release and ordinary command feed.
pub(super) async fn assert_native_workjet_route(database: &Database) -> Result<()> {
    let started = Instant::now();
    let result = submit(
        database,
        "e25-project-list",
        "hot-app",
        "ctox.workjet.project.list",
        json!({}),
    )
    .await?;
    anyhow::ensure!(
        result["status"] == "done" && result["result"].is_object(),
        "E25 real Core feed failed to invoke the original native Workjet handler: {result}"
    );
    let saved = database
        .readers
        .read_job(ReadJob::new(Default::default(), |db| {
            document(db, "business_commands", "e25-project-list")
        }))
        .await?
        .context("E25 canonical result missing")?;
    anyhow::ensure!(
        saved["actor_id"] == "owner"
            && saved["type"] == "ctox.workjet.project.list"
            && saved["result"] == result["result"],
        "E25 command result or original actor changed"
    );
    println!(
        "E25_NATIVE_CORE_BINDING {}",
        json!({"original_actor":"owner","native_handler":"ctox.workjet.project.list","command_id":"e25-project-list","status":"done","roundtrip_ms":started.elapsed().as_secs_f64()*1000.,"real_foreground":true,"retained_quorum":true})
    );
    Ok(())
}
