// E39 case matrix follows stub f967bafa, using real native owners and processes.
use super::*;

async fn intent(
    database: &Database,
    id: &str,
    kind: &str,
    target: Option<&str>,
    payload: Value,
) -> Result<Value> {
    let mut doc = json!({"id":id,"app_id":"hot-app","type":kind,"actor_id":"owner","status":"queued","created_at":chrono::Utc::now().to_rfc3339(),"payload":payload});
    if let Some(target) = target {
        doc["target_app"] = json!(target);
    }
    let written = database
        .writer
        .submit(WriteJob::new(Default::default(), move |tx| {
            store_command(tx, &doc)
        }))?
        .await;
    match written {
        Ok(()) => terminal(database, "business_commands", id).await,
        Err(error) => {
            let code = match &error {
                StoreError::Invalid(reason) => error_code(reason),
                _ => "unavailable",
            };
            Ok(json!({"status":"rejected","error":{"code":code,"detail":error.to_string()}}))
        }
    }
}
fn denied(doc: &Value, case: &str) -> Result<()> {
    anyhow::ensure!(
        matches!(doc["status"].as_str(), Some("failed" | "rejected"))
            && doc["error"]["code"] == "forbidden",
        "E39 {case} must be forbidden: {doc}"
    );
    Ok(())
}
async fn install(database: &Database, cas: &Cas) -> Result<()> {
    let source=b"export async function handle(command,ctx){if(command.payload.write!==false)await ctx.put('receiver_rows',{id:command.payload.id,n:command.payload.n});return {sender:command.app_id,n:command.payload.n,target:'receiver'};}";
    let hash = digest(source);
    let mut receiver = cas.receive(&digest(b"e39-receiver"), &hash, source.len() as u64)?;
    receiver.append(0, source)?;
    receiver.finish()?;
    let size = source.len();
    let types = [
        "hot.cross",
        "hot.denied",
        "hot.ambiguous",
        "hot.own",
        "hot.wildcard",
        "hot.restricted",
        "hot.pre.run",
        "hot.allow.run",
        "hot.fallback",
    ];
    let handlers: serde_json::Map<String, Value> = types
        .iter()
        .map(|kind| (kind.to_string(), json!("handlers/run.js")))
        .collect();
    let incoming:Vec<Value>=types.iter().filter(|kind|**kind!="hot.denied").map(|kind|json!({"type":kind,"from":if *kind=="hot.wildcard"{json!("*")}else{json!(["hot-app"])}})).chain(std::iter::once(json!({"type":"hot.nohandler","from":["hot-app"]}))).collect();
    let draft = json!({"id":"receiver-draft","app_id":"receiver","author":"owner","files":{"handlers/run.js":hash},"collections_schema":{"receiver_rows":{"schema":{"version":1,"type":"object","primaryKey":"id","properties":{"id":{"type":"string"},"n":{"type":"integer"}},"required":["id","n"]},"class":"full","client_limit":0,"byte_budget":1048576}},"manifest_fields":{"title":"E39 receiver","handlers":handlers,"commands_in":incoming,"command_rights":{"hot.denied":{"roles":["admin"]},"hot.restricted":{"roles":["user"]},"hot.pre*":{"roles":["admin"]},"hot.pre.run*":{"roles":[],"assigned_users":false},"hot.allow*":{"roles":[],"assigned_users":false},"hot.allow.run":{"roles":["admin"]}}}});
    database.writer.submit(WriteJob::new(Default::default(),move|tx|{
        super::super::super::commands::put(tx,"files",&hash,&json!({"id":hash,"ref":hash,"size":size,"owner_app":"receiver","mime":"text/javascript"}))?;
        super::super::super::commands::put(tx,"app_drafts","receiver-draft",&draft)
    }))?.await?;
    let published = submit(
        database,
        "receiver-publish",
        "ctox-platform",
        "ctox.app.publish",
        json!({"app_id":"receiver","base_app_ver":null,"draft_id":"receiver-draft"}),
    )
    .await?;
    anyhow::ensure!(
        published["status"] == "done",
        "E39 receiver publication: {published}"
    );
    Ok(())
}

pub(super) async fn prove(database: &Database, cas: &Cas) -> Result<()> {
    install(database, cas).await?;
    let result = intent(
        database,
        "e39-direct",
        "hot.cross",
        None,
        json!({"id":"cross-row","n":39,"app_id":"attacker","target_app":"other"}),
    )
    .await?;
    anyhow::ensure!(
        result["status"] == "done" && result["result"]["target"] == "receiver",
        "E39 target selection failed: {result}"
    );
    anyhow::ensure!(
        result["app_id"] == "hot-app" && result["result"]["sender"] == "hot-app",
        "E39 sender rewritten: {result}"
    );
    anyhow::ensure!(
        terminal(database, "receiver_rows", "cross-row").await?["n"] == 39,
        "E39 target scope write missing"
    );
    let forwarded = submit(
        database,
        "e39-forward",
        "hot-app",
        "hot.forward",
        json!({"id":"callback-target","n":40}),
    )
    .await?;
    anyhow::ensure!(
        forwarded["status"] == "done",
        "E39 ctx.command failed: {forwarded}"
    );
    let child = terminal(database, "business_commands", "e39-child").await?;
    anyhow::ensure!(
        child["app_id"] == "hot-app"
            && child["target_app"] == "receiver"
            && child["result"]["sender"] == "hot-app",
        "E39 child sender/result route changed: {child}"
    );
    anyhow::ensure!(
        terminal(database, "hot_rows", "e39-callback").await?["n"] == 40,
        "E39 completion was not routed to sender"
    );
    denied(
        &intent(
            database,
            "e39-single-mismatch",
            "hot.cross",
            Some("other"),
            json!({}),
        )
        .await?,
        "undeclared target",
    )?;
    denied(
        &intent(database, "e39-ambiguous", "hot.ambiguous", None, json!({})).await?,
        "ambiguous target",
    )?;
    let matched = intent(
        database,
        "e39-matched",
        "hot.ambiguous",
        Some("receiver"),
        json!({"id":"matched","n":41}),
    )
    .await?;
    anyhow::ensure!(
        matched["status"] == "done" && matched["result"]["target"] == "receiver",
        "E39 matching target rejected: {matched}"
    );
    let own = intent(
        database,
        "e39-own",
        "hot.own",
        Some("hot-app"),
        json!({"n":42}),
    )
    .await?;
    anyhow::ensure!(
        own["status"] == "done"
            && own["app_id"] == "hot-app"
            && own["result"]["n"] == 42
            && own["result"]["target"] == "hot-app",
        "E39 explicit own target changed: {own}"
    );
    for (id, kind, case) in [
        (
            "e39-denied",
            "hot.denied",
            "missing inbound despite explicit target right",
        ),
        ("e39-nohandler", "hot.nohandler", "missing receiver handler"),
        ("e39-user-denied", "hot.restricted", "target user right"),
        (
            "e39-prefix-denied",
            "hot.pre.run",
            "longest prefix overrides broad allow",
        ),
    ] {
        denied(
            &intent(
                database,
                id,
                kind,
                None,
                json!({"id":"must-not-write","n":-1}),
            )
            .await?,
            case,
        )?;
    }
    let absent = database
        .readers
        .read_job(ReadJob::new(Default::default(), |db| {
            Ok(document(db, "receiver_rows", "must-not-write")?.is_none())
        }))
        .await?;
    anyhow::ensure!(absent, "E39 forbidden target executed an effect");
    for (id, kind) in [
        ("e39-wildcard", "hot.wildcard"),
        ("e39-exact-allow", "hot.allow.run"),
        ("e39-write-fallback", "hot.fallback"),
    ] {
        let result = intent(database, id, kind, None, json!({"id":id,"n":43})).await?;
        anyhow::ensure!(
            result["status"] == "done" && result["result"]["target"] == "receiver",
            "E39 receiver consent/right precedence/fallback failed: {result}"
        );
    }
    for (id, kind) in [
        ("e39-unknown-target", "hot.unknown"),
        ("e39-unknown-type", "unregistered.command"),
    ] {
        let result = intent(database, id, kind, None, json!({})).await?;
        anyhow::ensure!(
            result["status"] == "rejected" && result["error"]["code"] == "unknown_command",
            "E39 unknown type/target accepted at write: {result}"
        );
    }
    for (index, kind) in [
        "ctox.app_store.install",
        "ats.consent.check",
        "browser.back",
        "ctox.appsec.probe",
        "business_os.chat.task",
        "business_os.context.ask",
        "business_os.data.modify",
        "ctox.workjet.new.route.v1",
        "ctox.plan.step.retry",
    ]
    .into_iter()
    .enumerate()
    {
        let result = intent(
            database,
            &format!("e39-platform-{index}"),
            kind,
            None,
            json!({}),
        )
        .await?;
        anyhow::ensure!(
            result["status"] != "rejected"
                && feed::core_route_probe::observed(&format!("e39-platform-{index}")),
            "E42 route fell into app admission instead of Core: {result}"
        );
    }
    // Real current user/assignment data; authority and signer remain native owners.
    database
        .writer
        .submit(WriteJob::new(Default::default(), |tx| {
            super::super::super::commands::put(
                tx,
                "business_users",
                "target-user",
                &json!({"id":"target-user","user_id":"target-user","role":"user","active":true}),
            )?;
            let mut app = document(tx, "business_apps", "receiver")?.unwrap();
            app["assigned_user_ids"] = json!(["target-user"]);
            super::super::super::commands::put(tx, "business_apps", "receiver", &app)
        }))?
        .await?;
    let readonly = json!({"id":"e39-user-readonly-fallback","app_id":"hot-app","type":"hot.fallback","actor_id":"target-user","status":"queued","created_at":chrono::Utc::now().to_rfc3339(),"payload":{"write":false,"n":44}});
    database
        .writer
        .submit(WriteJob::new(Default::default(), move |tx| {
            store_command(tx, &readonly)
        }))?
        .await?;
    denied(
        &terminal(database, "business_commands", "e39-user-readonly-fallback").await?,
        "read-only user cannot borrow sender or admin app-write fallback",
    )?;
    let user_doc = json!({"id":"e39-user-allowed","app_id":"hot-app","type":"hot.restricted","actor_id":"target-user","status":"queued","created_at":chrono::Utc::now().to_rfc3339(),"payload":{"write":false,"n":44}});
    database
        .writer
        .submit(WriteJob::new(Default::default(), move |tx| {
            store_command(tx, &user_doc)
        }))?
        .await?;
    let allowed = terminal(database, "business_commands", "e39-user-allowed").await?;
    anyhow::ensure!(
        allowed["status"] == "done" && allowed["result"]["sender"] == "hot-app",
        "E39 actual target user role denied: {allowed}"
    );
    receiver_hotplug_and_forgery(database).await?;
    println!(
        "APP_PLATFORM_E39_PROOF {}",
        json!({"matrix":"stub:f967bafa","target":"receiver","sender":"hot-app","target_scope_write":true,"sender_callback":true,"mandatory_inbound":true,"handler_and_target_right":true,"ambiguous_matching_and_own_targets":true,"payload_sender_ignored":true,"wildcard_and_right_precedence":true,"unknown_type_write_rejected":true,"platform_inventory_to_core":true})
    );
    Ok(())
}

async fn republish_receiver(database: &Database, id: &str, incoming: Value) -> Result<()> {
    let id = id.to_owned();
    let draft_id = id.clone();
    let base = database
        .writer
        .submit(WriteJob::new(Default::default(), move |tx| {
            let app = document(tx, "business_apps", "receiver")?.unwrap();
            let mut draft = document(tx, "app_drafts", "receiver-draft")?.unwrap();
            draft["id"] = json!(draft_id);
            draft["manifest_fields"]["commands_in"] = incoming;
            super::super::super::commands::put(tx, "app_drafts", &draft_id, &draft)?;
            Ok(app["active_ver"].clone())
        }))?
        .await?;
    let result = submit(
        database,
        &format!("publish-{id}"),
        "ctox-platform",
        "ctox.app.publish",
        json!({"app_id":"receiver","base_app_ver":base,"draft_id":id}),
    )
    .await?;
    anyhow::ensure!(
        result["status"] == "done",
        "E39 live receiver publication failed: {result}"
    );
    Ok(())
}
async fn receiver_hotplug_and_forgery(database: &Database) -> Result<()> {
    republish_receiver(database, "receiver-no-inbound", json!([])).await?;
    denied(
        &intent(
            database,
            "e39-live-no-in",
            "hot.cross",
            None,
            json!({"id":"live-denied","n":-1}),
        )
        .await?,
        "live inbound revocation",
    )?;
    // Queue and untrusted version edit commit together, so the real feed cannot
    // observe a permissive cache before the forged canonical snapshot.
    let original=database.writer.submit(WriteJob::new(Default::default(),|tx|{
        let app=document(tx,"business_apps","receiver")?.unwrap();
        let id=format!("receiver@{}",app["active_ver"].as_str().unwrap());
        let original=document(tx,"business_app_versions",&id)?.unwrap();let mut forged=original.clone();
        forged["manifest"]["commands_in"]=json!([{"type":"hot.cross","from":"*"}]);
        store_command(tx,&json!({"id":"e39-forged-version","app_id":"hot-app","type":"hot.cross","actor_id":"owner","status":"queued","created_at":chrono::Utc::now().to_rfc3339(),"payload":{"id":"forged-row","n":-1}}))?;
        super::super::super::commands::put(tx,"business_app_versions",&id,&forged)?;Ok((id,original))
    }))?.await?;
    let denied_command = terminal(database, "business_commands", "e39-forged-version").await;
    database
        .writer
        .submit(WriteJob::new(Default::default(), move |tx| {
            super::super::super::commands::put(
                tx,
                "business_app_versions",
                &original.0,
                &original.1,
            )
        }))?
        .await?;
    denied(
        &denied_command?,
        "forged receiver declaration cannot grant authority",
    )?;
    republish_receiver(
        database,
        "receiver-wildcard",
        json!([{"type":"hot.cross","from":"*"}]),
    )
    .await?;
    let result = intent(
        database,
        "e39-live-wildcard",
        "hot.cross",
        None,
        json!({"id":"live-wildcard","n":45}),
    )
    .await?;
    anyhow::ensure!(
        result["status"] == "done" && result["result"]["target"] == "receiver",
        "E39 live wildcard activation failed: {result}"
    );
    Ok(())
}
