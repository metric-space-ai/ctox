//! Test-only provisioning through the real foreground owners. Included in native_proof.
use super::*;
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "P6 lane runner supplies a private fresh tenant configuration"]
async fn p6_prepare_enrolled_profiles() -> Result<()> {
    let _scope = crate::service::store_startup::isolate_test_roots().await;
    anyhow::ensure!(std::env::var_os("CTOX_STATE_ROOT").is_none(), "installed state forbidden");
    let path = std::env::var_os("CTOX_P6_FIXTURE_CONFIG").context("private fixture config required")?;
    let config: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let root = std::path::PathBuf::from(config["root"].as_str().context("root")?);
    let roots = vec![root.clone(), root.with_extension("voter2"), root.with_extension("voter3")];
    let mut owners = Vec::new();
    let mut peers = BTreeMap::new();
    let mut identities = Vec::new();
    for (i, path) in roots.iter().enumerate() {
        let owner = Foreground::start(path.clone(), tokio::runtime::Handle::current()).await?;
        owner.inputs.sync.writer.submit(WriteJob::new(Default::default(), |tx| {
            seed(tx)?;
            schema::create_collection(tx, &CollectionDefinition {
                name:"p6_control_rows".into(),app_id:"p6-fixture".into(),schema_ver:1,
                schema_json:json!({"version":1,"type":"object","primaryKey":"id","properties":{"id":{"type":"string"},"n":{"type":"integer"}},"required":["id","n"]}),
                class:"full".into(),client_limit:0,byte_budget:1048576,rights_ver:1,app_ver:"1".into(),
            })?;Ok(())
        }))?.await?;
        peers.insert(i as u64 + 1, Peer { identity: owner.inputs.instance_signer()?.key_id().into(), executor: true, data_replica: true });
        owners.push(owner);
    }
    let result: Result<()> = async {
        for (i, owner) in owners.iter_mut().enumerate() {
            let host = ctox_sync::host_config::HostConfiguration {
                version: 1, scope_id: "p6-k-browser".into(),
                local: HostMember::Voter { node_id: i as u64 + 1 },
                voters: peers.clone(), timing: Default::default(),
            };
            host.validate()?;
            let host = serde_json::to_string(&host)?;
            let web_addr = format!("127.0.0.1:{}",config["port"].as_u64().context("HTTP port")?);
            let autostart = if i==0 { "true" } else { "false" };
            owner.inputs.databases["runtime"].writer.submit(WriteJob::new(Default::default(), move |tx| {
                tx.execute_batch("CREATE TABLE ctox_sync_host(singleton INTEGER PRIMARY KEY CHECK(singleton=1),configuration TEXT NOT NULL)")?;
                tx.execute("INSERT INTO ctox_sync_host VALUES(1,?1)", [host])?;
                tx.execute("INSERT OR REPLACE INTO runtime_env_kv VALUES('CTOX_BUSINESS_OS_WEB_AUTOSTART',?1)",[autostart])?;
                tx.execute("INSERT OR REPLACE INTO runtime_env_kv VALUES('CTOX_BUSINESS_OS_WEB_ADDR',?1)",[web_addr])?;
                Ok(())
            }))?.await?;
            let path = owner.inputs.root.clone();
            let url = format!("ws://127.0.0.1:{}?role=ctox_instance", config["signalingPort"].as_u64().context("signaling port")?);
            crate::persistence::run_core_sync(move || crate::secrets::write_secret_record(
                &path, crate::sync_host::SIGNING_IDENTITY_SECRET_KEY.0, "transport:p6-k-browser",
                &json!({"signalingUrls":[url],"iceServers":[]}).to_string(), None, json!({"source":"p6-private-enrollment"}),
            )).await?;
        }
        for role in ["admin", "chef", "user"] {
            let id = format!("p6-{role}");
            let path = root.clone();
            let role = role.to_owned();
            let user = id.clone();
            let token = crate::persistence::run_core_sync(move || {
                use crate::business_os::store::{BusinessOsSession, BusinessOsSessionUser};
                let session = BusinessOsSession { ok:true, authenticated:true, auth_required:false, login_url:None, reason:None,
                    user:Some(BusinessOsSessionUser { id:user.clone(), display_name:user.clone(), is_admin:role=="admin", role:role.clone() }) };
                crate::business_os::store::remember_authenticated_session_user(&path, &session)?;
                let token=crate::business_os::store::create_business_session(&path, &user, &role, &user, 3600)?;
                let resolved=crate::business_os::store::session_from_cookie_token(&path,&token)?.context("fixture session not resolved")?;
                anyhow::ensure!(resolved.authenticated && resolved.user.as_ref().is_some_and(|v|v.id==user && v.role==role),"fixture cookie resolved to another identity");
                anyhow::ensure!(crate::business_os::store::session_from_cookie_token(&path,"p6-invalid-unissued-cookie")?.is_none(),"unissued cookie granted authority");
                Ok::<_,anyhow::Error>(token)
            }).await?;
            let role = id.strip_prefix("p6-").unwrap().to_owned();
            let sync_user = id.clone();
            let sync_role = role.clone();
            owners[0].inputs.sync.writer.submit(WriteJob::new(Default::default(), move |tx| {
                super::super::super::commands::put(tx, "business_users", &sync_user,
                    &json!({"id":sync_user,"user_id":sync_user,"role":sync_role,"active":true}))
            }))?.await?;
            identities.push(json!({"id":id,"name":role,"role":role,"deviceId":null,"cookie":token}));
        }
        let instance_id = owners[0].inputs.instance_signer()?.instance_id().to_owned();
        let fixture = json!({"version":1,"source":"foreground-writer-secret-store","root":root,"roots":roots,
            "revision":config["revision"],"instanceId":instance_id,"identities":identities,
            "nativeVoters":3,"browserAdmission":"must-be-proven-by-E38-hello"});
        use std::os::unix::fs::OpenOptionsExt;
        use std::io::Write;
        let mut out = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600)
            .open(config["fixturePrivate"].as_str().context("private output")?)?;
        out.write_all(&serde_json::to_vec(&fixture)?)?;
        Ok(())
    }.await;
    for owner in &mut owners { owner.shutdown().await?; }
    result
}

#[path="control-provider.rs"]
mod control_provider;
