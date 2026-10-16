// E39 proof uses the foreground's real signer, policy, writer and process host.
use super::*;

async fn install(database: &Database, cas: &Cas) -> Result<()> {
    let source = b"export async function handle(command,ctx){await ctx.put('receiver_rows',{id:command.payload.id,n:command.payload.n});return {sender:command.app_id,n:command.payload.n,target:'receiver'};}";
    let hash = digest(source);
    let mut receiver = cas.receive(&digest(b"e39-receiver"), &hash, source.len() as u64)?;
    receiver.append(0, source)?; receiver.finish()?;
    let size = source.len();
    let draft = json!({"id":"receiver-draft","app_id":"receiver","author":"owner","files":{"handlers/run.js":hash},"collections_schema":{"receiver_rows":{"schema":{"version":1,"type":"object","primaryKey":"id","properties":{"id":{"type":"string"},"n":{"type":"integer"}},"required":["id","n"]},"class":"full","client_limit":0,"byte_budget":1048576}},"manifest_fields":{"title":"E39 receiver","handlers":{"hot.cross":"handlers/run.js","hot.denied":"handlers/run.js"},"commands_in":[{"type":"hot.cross","from":["hot-app"]}]}});
    database.writer.submit(WriteJob::new(Default::default(), move |tx| {
        super::super::super::commands::put(tx,"files",&hash,&json!({"id":hash,"ref":hash,"size":size,"owner_app":"receiver","mime":"text/javascript"}))?;
        super::super::super::commands::put(tx,"app_drafts","receiver-draft",&draft)
    }))?.await?;
    let published = submit(database,"receiver-publish","ctox-platform","ctox.app.publish",json!({"app_id":"receiver","base_app_ver":null,"draft_id":"receiver-draft"})).await?;
    anyhow::ensure!(published["status"]=="done", "E39 receiver publication: {published}");
    Ok(())
}

pub(super) async fn prove(database: &Database, cas: &Cas) -> Result<()> {
    install(database, cas).await?;
    let result = submit(database,"e39-direct","hot-app","hot.cross",json!({"id":"cross-row","n":39})).await?;
    anyhow::ensure!(result["status"]=="done" && result["result"]["target"]=="receiver", "E39 target selection failed: {result}");
    anyhow::ensure!(result["app_id"]=="hot-app" && result["result"]["sender"]=="hot-app", "E39 sender rewritten: {result}");
    let row = terminal(database,"receiver_rows","cross-row").await?;
    anyhow::ensure!(row["n"]==39, "E39 target scope write missing");
    let forwarded = submit(database,"e39-forward","hot-app","hot.forward",json!({"id":"callback-target","n":40})).await?;
    anyhow::ensure!(forwarded["status"]=="done", "E39 ctx.command failed: {forwarded}");
    let child = terminal(database,"business_commands","e39-child").await?;
    anyhow::ensure!(child["app_id"]=="hot-app" && child["target_app"]=="receiver" && child["result"]["sender"]=="hot-app", "E39 child sender/result route changed: {child}");
    let callback = terminal(database,"hot_rows","e39-callback").await?;
    anyhow::ensure!(callback["n"]==40, "E39 completion was not routed to sender");
    let denied = submit(database,"e39-denied","hot-app","hot.denied",json!({"id":"must-not-write","n":-1})).await?;
    anyhow::ensure!(denied["status"]=="failed" && denied["error"]["code"]=="forbidden", "E39 unexported command delivered: {denied}");
    let absent = database.readers.read_job(ReadJob::new(Default::default(), |db| Ok(document(db,"receiver_rows","must-not-write")?.is_none()))).await?;
    anyhow::ensure!(absent, "E39 forbidden target executed an effect");
    let unknown = database.writer.submit(WriteJob::new(Default::default(), |tx| {
        store_command(tx,&json!({"id":"e39-unknown","app_id":"hot-app","type":"hot.unknown","actor_id":"owner","status":"queued","payload":{}}))
    }))?.await;
    anyhow::ensure!(unknown.as_ref().err().is_some_and(|e| e.to_string().contains("unknown_command")), "E39 unknown target accepted at write: {unknown:?}");
    println!("APP_PLATFORM_E39_PROOF {}",json!({"target":"receiver","sender":"hot-app","target_scope_write":true,"sender_callback":true,"unexported_forbidden":true,"unknown_target_write_rejected":true}));
    Ok(())
}
