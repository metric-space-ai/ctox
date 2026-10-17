use super::*;
use ctox_store::{
    schema::{self, CollectionDefinition},
    testkit::StoreFixture,
    DbId,
};
use serde_json::json;

pub(super) async fn fixture() -> (tempfile::TempDir, StoreFixture, Invites) {
    let dir = tempfile::tempdir().unwrap();
    let owner = StoreFixture::open(DbId::new(dir.path().join("pairing.sqlite3")).unwrap())
        .await
        .unwrap();
    owner.write(Default::default(),|tx| {
        schema::create_collection(tx,&CollectionDefinition{name:"business_users".into(),app_id:"ctox-platform".into(),schema_ver:1,schema_json:json!({"type":"object","primaryKey":"id","properties":{"id":{"type":"string"}}}),class:"full".into(),client_limit:100,byte_budget:1_000_000,rights_ver:1,app_ver:"1".into()})?;
        let table=schema::active_collection(tx,"business_users")?.unwrap().table;
        tx.execute(&format!("INSERT INTO \"{table}\"(id,rev,schema_ver,data,deleted) VALUES('owner','1',1,?1,0)"),[json!({"id":"owner","role":"admin","active":true,"display_name":"Owner"}).to_string()])?;
        Ok(())
    }).await.unwrap();
    let invites = Invites::attach(owner.writer()).await.unwrap();
    (dir, owner, invites)
}
fn request(invite: &Invitation) -> Redeem {
    let public = [7; 32];
    let secret = URL_SAFE_NO_PAD.decode(&invite.one_time_secret).unwrap();
    let proof = hmac::sign(
        &hmac::Key::new(hmac::HMAC_SHA256, &key(&secret)),
        &message(&invite.invite_id, &public),
    );
    Redeem {
        invite_id: invite.invite_id.clone(),
        proof: URL_SAFE_NO_PAD.encode(proof.as_ref()),
        device_pubkey: URL_SAFE_NO_PAD.encode(public),
        device_label: "Workjet fixture".into(),
    }
}
async fn redeem(invites: &Invites, r: Redeem) -> Result<(), Reject> {
    let p = invites.prepare(r).await?;
    invites.consume(p, "fixture-device-id".into()).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn single_use_is_atomic_across_prepared_competing_connections_and_restart() {
    let (_dir, owner, invites) = fixture().await;
    let invite = invites
        .create("owner".into(), "admin".into(), 60_000)
        .await
        .unwrap();
    let a = invites.prepare(request(&invite)).await.unwrap();
    let b = invites.prepare(request(&invite)).await.unwrap();
    let (first, second) = tokio::join!(
        invites.consume(a, "device-a".into()),
        invites.consume(b, "device-b".into())
    );
    assert!(first.is_ok() ^ second.is_ok(), "second use accepted");
    assert_eq!(first.err().or(second.err()), Some(Reject::InviteUsed));
    drop(invites);
    owner.shutdown().await.unwrap();
    let reopened = StoreFixture::open(DbId::new(_dir.path().join("pairing.sqlite3")).unwrap())
        .await
        .unwrap();
    let restarted = Invites::attach(reopened.writer()).await.unwrap();
    assert_eq!(
        redeem(&restarted, request(&invite)).await.err(),
        Some(Reject::InviteUsed)
    );
    drop(restarted);
    reopened.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expired_invite_cannot_be_prepared_or_consumed() {
    let (_dir, owner, invites) = fixture().await;
    let invite = invites
        .create("owner".into(), "admin".into(), 60_000)
        .await
        .unwrap();
    let prepared = invites.prepare(request(&invite)).await.unwrap();
    let id = invite.invite_id.clone();
    let now = clock().unwrap();
    owner
        .write(Default::default(), move |tx| {
            tx.execute(
                "UPDATE _workjet_invites_v4 SET created_ms=?2,expires_ms=?3 WHERE invite_id=?1",
                params![id, now - 60_000, now - 1],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let outcome = invites.consume(prepared, "expired-device".into()).await;
    assert!(outcome.is_err(), "expired invite accepted");
    assert_eq!(outcome.err(), Some(Reject::InviteExpired));
    assert_eq!(
        redeem(&invites, request(&invite)).await.err(),
        Some(Reject::InviteExpired)
    );
    drop(invites);
    owner.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_proof_and_changed_device_key_are_denied_without_spending_the_invite() {
    let (_dir, owner, invites) = fixture().await;
    let invite = invites
        .create("owner".into(), "admin".into(), 60_000)
        .await
        .unwrap();
    let mut wrong = request(&invite);
    wrong.proof = URL_SAFE_NO_PAD.encode([0; 32]);
    let outcome = redeem(&invites, wrong).await;
    assert!(outcome.is_err(), "wrong proof accepted");
    assert_eq!(outcome.err(), Some(Reject::ProofInvalid));
    let mut changed = request(&invite);
    changed.device_pubkey = URL_SAFE_NO_PAD.encode([8; 32]);
    assert_eq!(
        redeem(&invites, changed).await.err(),
        Some(Reject::ProofInvalid)
    );
    redeem(&invites, request(&invite)).await.unwrap();
    drop(invites);
    owner.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn current_role_ttl_bounds_unknown_and_attempt_budget_are_enforced() {
    let (_dir, owner, invites) = fixture().await;
    assert_eq!(
        invites
            .create("owner".into(), "user".into(), 60_000)
            .await
            .err(),
        Some(Reject::DeviceRejected)
    );
    for ttl in [0, MAX_TTL_MS + 1] {
        assert_eq!(
            invites
                .create("owner".into(), "admin".into(), ttl)
                .await
                .err(),
            Some(Reject::DeviceRejected)
        );
    }
    let invite = invites
        .create("owner".into(), "admin".into(), MAX_TTL_MS)
        .await
        .unwrap();
    let mut unknown = request(&invite);
    unknown.invite_id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        redeem(&invites, unknown).await.err(),
        Some(Reject::InviteUnknown)
    );
    owner.write(Default::default(),|tx| {let table=schema::active_collection(tx,"business_users")?.unwrap().table;tx.execute(&format!("UPDATE \"{table}\" SET data=json_set(data,'$.active',json('false')) WHERE id='owner'"),[])?;Ok(())}).await.unwrap();
    assert_eq!(
        redeem(&invites, request(&invite)).await.err(),
        Some(Reject::DeviceRejected)
    );
    owner.write(Default::default(),|tx| {tx.execute("INSERT INTO _workjet_pairing_budget_v4 VALUES(1,?1,120) ON CONFLICT(singleton) DO UPDATE SET window_ms=excluded.window_ms,attempts=120",[clock().unwrap()])?;Ok(())}).await.unwrap();
    assert_eq!(
        redeem(&invites, request(&invite)).await.err(),
        Some(Reject::RateLimited)
    );
    drop(invites);
    owner.shutdown().await.unwrap();
}
