//! Private, test-only native fixture control. Runs beside the production run()
//! future and borrows its StoreInputs. Never opens a second owner or HTTP route.
use super::*;
use crate::service::store_inputs::{store_inputs, StoreInputs};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

const MAX_REQUEST: u64 = 65536;

async fn dispatch(inputs: Arc<StoreInputs>, request: Value) -> Result<Value> {
    let cookie = request["cookie"].as_str().context("session required")?.to_owned();
    let root = inputs.root.clone();
    let actor = crate::persistence::run_core_sync(move || {
        let session = crate::business_os::store::session_from_cookie_token(&root, &cookie)?
            .context("unissued or expired session")?;
        anyhow::ensure!(session.authenticated, "session not authenticated");
        session.user.context("session has no user")
    }).await?;
    let operation = request["operation"].as_str().context("operation")?;
    let args = &request["args"];
    if operation == "authenticate" {
        anyhow::ensure!(args["userId"] == actor.id, "actor mismatch");
        // Device binding is checked against the actual E38 hello in the browser
        // adapter, never minted from this fixture socket.
        return Ok(json!({"userId":actor.id,"role":actor.role,"is_admin":actor.is_admin,
            "instanceId":inputs.instance_signer()?.instance_id()}));
    }
    anyhow::ensure!(actor.is_admin && actor.role == "admin", "admin session required");
    match operation {
        "processIdentity" => Ok(json!({"pid":std::process::id(),
            "instanceId":inputs.instance_signer()?.instance_id(),
            "syncGeneration":inputs.sync.writer.generation().0,
            "coreGeneration":inputs.core.writer.generation().0})),
        "readNative" => {
            let name = text(args, "collection")?.to_owned();
            let id = args["id"].as_str().map(str::to_owned);
            let rows = inputs.sync.readers.read_job(ReadJob::new(Default::default(), move |db| {
                // The active table comes only from the real registry. The socket
                // never accepts SQL or a physical table name.
                let table: String = db.query_row("SELECT table_name FROM _collections WHERE name=?1 AND state='active'", [&name], |r|r.get(0))?;
                let mut statement = db.prepare(&format!("SELECT data FROM {} WHERE deleted=0 AND (?1 IS NULL OR id=?1) ORDER BY id LIMIT 100001", quote(&table)))?;
                let rows=statement.query_map([id], |r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
                if rows.len()>100000 {return Err(invalid("control read budget exceeded"));}
                rows.into_iter().map(|s|serde_json::from_str::<Value>(&s).map_err(|_|invalid("invalid stored JSON"))).collect::<Result<Vec<_>,StoreError>>()
            })).await?;
            Ok(json!({"rows":rows,"source":"owner-reader"}))
        }
        "writeProbe" => {
            let id=text(args,"id")?.to_owned();
            anyhow::ensure!(id.starts_with("p6-probe-") && id.len()<=96,"private probe id required");
            let body=json!({"id":id,"n":args["n"].as_u64().context("n")?});
            let mut commits=inputs.sync.writer.subscribe_commits();
            let previous=commits.latest().map(|c|c.head_seq).unwrap_or(0);
            inputs.sync.writer.submit(WriteJob::new(Default::default(),move|tx|
                crate::business_os::app_platform::commands::put(tx,"p6_control_rows",&id,&body)))?.await?;
            let committed=tokio::time::timeout(Duration::from_secs(5),async {
                loop {let c=commits.changed().await?;if c.head_seq>previous{return Ok::<_,StoreError>(c);}}
            }).await??;
            Ok(json!({"seq":committed.head_seq.to_string(),"source":"owner-writer-commit",
                "generation":committed.generation.0}))
        }
        "setRights" => {
            let expected=args["expected_ver"].as_u64().context("expected_ver")?;
            let rules:Vec<sync_rights::Right>=serde_json::from_value(args["rules"].clone())?;
            let receipt=inputs.sync.writer.submit(WriteJob::new(Default::default(),move|tx|{
                let version=sync_rights::replace(tx,expected,&rules)?;
                let seq:u64=tx.query_row("SELECT v FROM _seq WHERE singleton=1",[],|r|r.get(0))?;
                Ok((version,seq))
            }))?.await?;
            Ok(json!({"rights_ver":receipt.0.to_string(),"seq":receipt.1.to_string(),"source":"owner-writer"}))
        }
        // These witnesses cannot be synthesized from an after-commit watch or
        // receive callbacks: watch heads coalesce and carry no commit timestamp.
        "commitReceipt" => anyhow::bail!("not_run(native-commit-timestamp): writer notifications contain no commit timestamp"),
        "sendTrace" => anyhow::bail!("not_run(native-final-send): no final-send frame observer is exposed"),
        "prepare" => {
            let scenario=text(args,"scenario")?;
            anyhow::ensure!(["K13","K21","K22"].contains(&scenario),"unknown scenario");
            Ok(json!({"scenario":scenario,"available":false,"reason":"not_run(native-scenario-fixtures): complete copied-app/46-app/external-participant fixtures not provisioned",
                "ownerBacked":true,"instanceId":inputs.instance_signer()?.instance_id()}))
        }
        _ => anyhow::bail!("not_run(native-control-operation): operation not implemented"),
    }
}

#[tokio::test(flavor="multi_thread",worker_threads=2)]
#[ignore="private lane-owned real foreground with a test-only Unix control socket"]
async fn p6_run_controlled_foreground() -> Result<()> {
    let config:Value=serde_json::from_slice(&std::fs::read(std::env::var_os("CTOX_P6_FIXTURE_CONFIG").context("private config")?)?)?;
    let root=std::path::PathBuf::from(text(&config,"root")?);
    let marker:Value=serde_json::from_slice(&std::fs::read(root.join(".sync-v4-e2e.json"))?)?;
    anyhow::ensure!(marker["owner"]=="01a14204-d613" && marker["synthetic"]==true && marker["source"]==config["revision"],"foreign fixture root");
    let mut foreground=tokio::spawn(crate::service::store_startup::run(root.clone()));
    let inputs=tokio::time::timeout(Duration::from_secs(30),async {
        loop {
            if foreground.is_finished(){anyhow::bail!("foreground exited before control attachment");}
            if let Ok(inputs)=store_inputs(&root){return Ok::<_,anyhow::Error>(inputs);}
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await??;
    let path=root.join("p6-control.sock");
    let listener=tokio::net::UnixListener::bind(&path)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o600))?;
    loop {
        tokio::select! {
            result=&mut foreground=>{let _=std::fs::remove_file(&path);return result?;},
            next=listener.accept()=>{
                let (stream,_)=next?;
                // Sequential admission, request byte cap and wall deadline;
                // peer cannot create an unbounded task or read pool.
                let inputs=inputs.clone();
                let result=tokio::time::timeout(Duration::from_secs(10),async move {
                    use tokio::io::AsyncReadExt;
                    let (read,mut write)=stream.into_split();
                    let mut reader=tokio::io::BufReader::new(read.take(MAX_REQUEST+1));
                    let mut line=String::new();reader.read_line(&mut line).await?;
                    let response=if line.len()>MAX_REQUEST as usize {json!({"ok":false,"error":"request budget exceeded"})}
                    else {match serde_json::from_str::<Value>(&line) {
                        Ok(request)=>match dispatch(inputs,request).await {
                            Ok(value)=>json!({"ok":true,"value":value}),
                            // Never serialize raw errors: request contains a cookie.
                            Err(error)=>{let reason=error.to_string();let public=if reason.starts_with("not_run("){reason}else{"control request denied".into()};json!({"ok":false,"error":public})}
                        },Err(_)=>json!({"ok":false,"error":"invalid request"})
                    }};
                    let bytes=serde_json::to_vec(&response)?;write.write_all(&bytes).await?;write.write_all(b"\n").await?;
                    Ok::<_,anyhow::Error>(())
                }).await;
                if !matches!(result,Ok(Ok(()))) {eprintln!("P6 private control request closed");}
            }
        }
    }
}
