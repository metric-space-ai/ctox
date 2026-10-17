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
    prove_matching_and_news(database, cas, false).await
}
pub(super) async fn prove_under_policy_pressure(database: &Database, cas: &Cas) -> Result<()> {
    prove_matching_and_news(database, cas, true).await
}
async fn prove_matching_and_news(database: &Database, cas: &Cas, pressure: bool) -> Result<()> {
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
    let policy_pressure =
        pressure.then(|| super::policy_pressure::install(database.writer.db_id().path()));
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
    if let Some(pressure) = &policy_pressure {
        anyhow::ensure!(
            pressure.attempts() > 0,
            "native policy decoding pressure was not exercised"
        );
        println!(
            "APP_PLATFORM_MATCHING_POLICY_RECOVERED checks={}",
            pressure.attempts()
        );
    }
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

/// Publish the real package and exercise the committed native visibility rules.
pub(super) async fn prove_knowledge(database: &Database, cas: &Cas) -> Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apps/business-os");
    let mut fields: Value = serde_json::from_slice(&std::fs::read(
        root.join("v4/manifests/knowledge.manifest.json"),
    )?)?;
    let mut files = Vec::new();
    for path in fields["files"]
        .as_object()
        .context("real Knowledge package files")?
        .keys()
    {
        files.push((
            path.clone(),
            std::fs::read(root.join("modules/knowledge").join(path))?,
        ));
    }
    let collections: serde_json::Map<String, Value> = fields["collections"].as_object().context("Knowledge collections")?.iter().map(|(name, def)| {
        let class = def["class"].as_str().unwrap_or("full");
        (name.clone(), json!({"schema":def["schema"],"class":class,"client_limit":if class=="window" {def["client_limit"].as_u64().unwrap_or(5000)} else {0},"byte_budget":1048576}))
    }).collect();
    // Generated draft annotations describe evidence, not runtime permission.
    // Keep every owner/operation and pass row_filters unchanged to the Core.
    if let Some(foreign) = fields["foreign_collections"].as_object_mut() {
        for value in foreign.values_mut() {
            let object = value
                .as_object_mut()
                .context("foreign collection declaration")?;
            anyhow::ensure!(
                object
                    .keys()
                    .all(|k| matches!(k.as_str(), "owner_app" | "ops" | "_evidence")),
                "unknown Knowledge foreign permission field"
            );
            object.remove("_evidence");
        }
    }
    install(
        database,
        cas,
        "knowledge",
        files,
        json!(collections),
        fields,
    )
    .await?;
    let (health, _) = tokio::sync::watch::channel(feed::FeedHealth::Healthy);
    tokio::time::timeout(Duration::from_secs(12), feed::resilient_read(database.readers.clone(),"knowledge.visibility",&health,&mut 1, |_| ReadJob::new(Default::default(),|db| {
        let actor = actor(db,"owner")?;
        let rules = rights(db)?;
        for name in ["knowledge_views","knowledge_table_rows"] {
            let own = json!({"id":"own","actor_id":"owner","table":"books","skip":0,"limit":10});
            let foreign = json!({"id":"foreign","actor_id":"another-user","table":"books","skip":0,"limit":10});
            policy::read_sync_document(&actor,&rules,"knowledge",name,&own).map_err(sql_error)?;
            if policy::read_sync_document(&actor,&rules,"knowledge",name,&foreign).is_ok() {
                return Err(invalid("forbidden: Knowledge foreign actor row was visible"));
            }
        }
        Ok(())
    }))).await??;
    println!("APP_PLATFORM_KNOWLEDGE_FILTER_PROOF published=true own_actor_visible=true foreign_actor_hidden=true");
    Ok(())
}
