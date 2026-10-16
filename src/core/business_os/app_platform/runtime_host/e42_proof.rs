//! E42.2: execute unchanged repository handlers with the real foreground host.
use super::native_proof::{submit, terminal};
use super::*;
use ctox_store::WriteJob;

async fn install(
    database: &Database,
    cas: &Cas,
    app: &str,
    files: Vec<(String, Vec<u8>)>,
    collections: Value,
    fields: Value,
) -> Result<()> {
    let mut refs = serde_json::Map::new();
    for (path, bytes) in files {
        let hash = digest(&bytes);
        let mut upload = cas.receive(
            &digest(format!("e42:{app}:{path}").as_bytes()),
            &hash,
            bytes.len() as u64,
        )?;
        for (index, chunk) in bytes.chunks(512 * 1024).enumerate() {
            upload.append((index * 512 * 1024) as u64, chunk)?;
        }
        upload.finish()?;
        let file = json!({"id":hash,"ref":hash,"size":bytes.len(),"owner_app":app,"name":path,"mime":"text/javascript"});
        let id = hash.clone();
        database
            .writer
            .submit(WriteJob::new(Default::default(), move |tx| {
                super::super::commands::put(tx, "files", &id, &file)
            }))?
            .await?;
        refs.insert(path, json!(hash));
    }
    let id = format!("e42-{app}-draft");
    let draft = json!({"id":id,"app_id":app,"author":"owner","files":refs,"collections_schema":collections,"manifest_fields":fields});
    let key = id.clone();
    database
        .writer
        .submit(WriteJob::new(Default::default(), move |tx| {
            super::super::commands::put(tx, "app_drafts", &key, &draft)
        }))?
        .await?;
    let result = submit(
        database,
        &format!("e42-{app}-publish"),
        "ctox-platform",
        "ctox.app.publish",
        json!({"app_id":app,"base_app_ver":null,"draft_id":id}),
    )
    .await?;
    anyhow::ensure!(
        result["status"] == "done",
        "E42.2 real handler publication {app}: {result}"
    );
    Ok(())
}
fn sources(
    root: &std::path::Path,
    dir: &std::path::Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() != "tests" {
                sources(root, &path, out)?;
            }
        } else if path.extension().is_some_and(|v| v == "js") {
            out.push((
                path.strip_prefix(root)?.to_string_lossy().into_owned(),
                std::fs::read(path)?,
            ));
        }
    }
    Ok(())
}
pub(super) async fn prove(database: &Database, cas: &Cas) -> Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apps/business-os");
    // The real News handler imports the actual pure shell export. No rewritten handler or stub context.
    install(database,cas,"shell",vec![("shared/prompt-template.js".into(),std::fs::read(root.join("shared/prompt-template.js"))?)],json!({}),json!({"title":"Native shell exports","handlers":{},"shared_exports":["shared/prompt-template.js"]})).await?;
    for (app, kind) in [
        ("news", "news.issue.generate"),
        ("matching", "matching.match"),
    ] {
        anyhow::ensure!(
            !routing::core_route(kind),
            "E42.2 app handler must not enter Core: {kind}"
        );
        let module = root.join("modules").join(app);
        let mut files = Vec::new();
        sources(&module, &module.join("handlers"), &mut files)?;
        let schema: Value =
            serde_json::from_slice(&std::fs::read(module.join("collections.schema.json"))?)?;
        let collections: serde_json::Map<String, Value> = schema["collections"]
            .as_object()
            .context("real collection definitions")?
            .iter()
            .map(|(name, schema)| {
                (
                    name.clone(),
                    json!({"schema":schema,"class":"full","client_limit":0,"byte_budget":1048576}),
                )
            })
            .collect();
        install(database,cas,app,files,json!(collections),json!({"title":format!("E42.2 {app}"),"handlers":{kind:format!("handlers/{kind}.js")},"commands_in":[{"type":kind,"from":["hot-app"]}],"command_rights":{kind:{"roles":["admin"]}}})).await?;
    }
    database.writer.submit(WriteJob::new(Default::default(),|tx|{
        super::super::commands::put(tx,"matching_requirements","req",&json!({"id":"req","kind":"requirement","title":"Python developer","status":"active","updated_at_ms":1,"data":{"requirement":{"id":"req","title":"Python developer","objectRequirements":"python sql"}}}))?;
        super::super::commands::put(tx,"matching_objects","obj",&json!({"id":"obj","kind":"object","title":"Engineer","status":"active","updated_at_ms":1,"data":{"id":"obj","name":"Eva","skills":["Python","SQL"]}}))
    }))?.await?;
    let matched = submit(
        database,
        "e42-matching",
        "hot-app",
        "matching.match",
        json!({"request":{"requirement_id":"req","object_id":"obj"}}),
    )
    .await?;
    anyhow::ensure!(
        matched["status"] == "done"
            && matched["app_id"] == "hot-app"
            && matched["result"]["records_count"] == 1
            && matched["result"]["score"].as_u64().is_some_and(|s| s > 0),
        "E42.2 native matching result must be a value: {matched}"
    );
    let computed = terminal(database, "matching_results", "|req|obj").await?;
    anyhow::ensure!(
        computed["data"]["command_id"] == "e42-matching",
        "E42.2 matching handler write missing: {computed}"
    );
    // A disabled/missing scheduled topic produces the ported handler's real skip value, never a queue task ID.
    let news=submit(database,"e42-news","hot-app","news.issue.generate",json!({"trigger":"schedule","topic_id":"disabled","schedule_name":"news:disabled","schedule_thread_key":"business-os/news/disabled","schedule_task_id":"clock","scheduled_for":"2026-10-17T00:00:00Z"})).await?;
    anyhow::ensure!(
        news["status"] == "done"
            && news["result"]["outcome"] == "skipped"
            && news["result"]["run_key"] == "news-skip:clock:2026-10-17T00:00:00Z",
        "E42.2 native News result must be a value: {news}"
    );
    println!(
        "APP_PLATFORM_E42_PROOF {}",
        json!({"unchanged_handlers":["matching.match","news.issue.generate"],"native":true,"sender_preserved":true,"matching_target_write":true,"results_as_values":true})
    );
    Ok(())
}
