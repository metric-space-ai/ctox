use super::*;
use ctox_store::WriteJob;
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn launch_auth_uses_current_user_on_retained_bos_reader() -> anyhow::Result<()> {
    let _scope = crate::service::store_startup::isolate_test_roots().await;
    anyhow::ensure!(std::env::var_os("CTOX_STATE_ROOT").is_none());
    let root = tempfile::Builder::new().prefix("c8a-").tempdir_in("/tmp")?;
    let mut owner = crate::service::store_startup::Foreground::start(
        root.path().to_owned(),
        tokio::runtime::Handle::current(),
    )
    .await?;
    let bos = owner.inputs.databases.get("bos").unwrap().clone();
    let hash = URL_SAFE_NO_PAD.encode(Sha256::digest(b"synthetic-cookie"));
    let saved_hash = hash.clone();
    bos.writer.submit(WriteJob::new(Default::default(),move |tx| {
        tx.execute_batch("CREATE TABLE IF NOT EXISTS business_users(user_id TEXT PRIMARY KEY,display_name TEXT NOT NULL,role TEXT NOT NULL,active INTEGER NOT NULL,capability_epoch INTEGER NOT NULL,created_at_ms INTEGER NOT NULL,updated_at_ms INTEGER NOT NULL,profile_json TEXT NOT NULL); CREATE TABLE IF NOT EXISTS business_sessions(token_hash TEXT PRIMARY KEY,user_id TEXT NOT NULL,role TEXT NOT NULL,display_name TEXT NOT NULL,created_at_ms INTEGER NOT NULL,expires_at_ms INTEGER NOT NULL,revoked INTEGER NOT NULL,last_seen_ms INTEGER NOT NULL);")?;
        tx.execute("INSERT INTO business_users(user_id,display_name,role,active,capability_epoch,created_at_ms,updated_at_ms,profile_json) VALUES('actor','Actor','user',1,0,0,0,'{}')",[])?;
        tx.execute("INSERT INTO business_sessions(token_hash,user_id,role,display_name,created_at_ms,revoked,expires_at_ms,last_seen_ms) VALUES(?1,'actor','admin','Actor',0,0,9223372036854775807,0)",[hash])?;Ok(())
    }))?.await?;
    let user = authenticated_user(&owner.inputs, saved_hash.clone())
        .await?
        .unwrap();
    assert_eq!(user.role, "user", "launch used the stale cookie role");
    assert!(!matches!(user.role.as_str(), "chef" | "admin"));
    bos.writer
        .submit(WriteJob::new(Default::default(), |tx| {
            tx.execute(
                "UPDATE business_users SET active=0 WHERE user_id='actor'",
                [],
            )?;
            Ok(())
        }))?
        .await?;
    assert!(
        authenticated_user(&owner.inputs, saved_hash.clone())
            .await?
            .is_none(),
        "launch authenticated a disabled user"
    );
    owner.shutdown().await?;
    assert!(authenticated_user(&owner.inputs, saved_hash).await.is_err());
    Ok(())
}
