use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ctox_store::change_log::subscription::protocol::InstanceSigner;
use ctox_store::{testkit::StoreFixture, DbId, StoreError, WriteJob};
use p6_e55_contract::rights::{
    app_grant::*,
    manifest_rules::ManifestRules,
    scope::{canonical_json, FieldMask, FilterValue, RowFilter},
    Operation, Right, StoreRightsAuthority,
};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

// Only cryptographic test custody is replaced. Production app rights, writer,
// registry, canonical encoder, verifier, projection and session fences are real.
struct Key {
    identity: ctox_sync::authority::auth::SigningIdentity,
    pair: Ed25519KeyPair,
    pin: String,
    active: AtomicBool,
}
impl Key {
    fn new() -> Arc<Self> {
        let doc = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(doc.as_ref()).unwrap();
        let pin = format!(
            "ed25519:{}",
            pair.public_key()
                .as_ref()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        Arc::new(Self {
            identity: ctox_sync::authority::auth::SigningIdentity::from_pkcs8(doc.as_ref())
                .unwrap(),
            pair,
            pin,
            active: AtomicBool::new(true),
        })
    }
}
impl InstanceSigner for Key {
    fn instance_id(&self) -> &str {
        "instance"
    }
    fn key_id(&self) -> &str {
        &self.pin
    }
    fn sign(&self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        if !self.active.load(Ordering::Acquire) {
            return Err("retired".into());
        }
        let packet = if bytes.starts_with(DOMAIN) {
            let claims: AppGrantClaims = serde_json::from_slice(&bytes[DOMAIN.len()..]).unwrap();
            assert_eq!(signing_bytes(&claims).unwrap(), bytes);
            ctox_sync::authority::auth::CanonicalInstancePacket::from_scope_bytes(bytes)
        } else {
            ctox_sync::authority::auth::CanonicalInstancePacket::from_stream_bytes(bytes)
        }
        .map_err(|_| "canonical E22 encoder denied")?;
        Ok(self.identity.sign_instance_packet(&packet))
    }
}
struct CurrentPeer;
impl GrantPeerAuthority for CurrentPeer {
    fn validate(
        &self,
        tx: &rusqlite::Transaction<'_>,
        user: &str,
        role: &str,
        device: &str,
        app_id: &str,
    ) -> Result<(), String> {
        let live: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM c_business_users WHERE id=?1 AND json_extract(data,'$.role')=?2 AND json_extract(data,'$.active')=1 AND deleted=0)",rusqlite::params![user,role],|r|r.get(0)).map_err(|_|"principal unavailable")?;
        let may_use: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM c_business_apps WHERE id=?1 AND json_extract(data,'$.state')='active' AND EXISTS(SELECT 1 FROM json_each(json_extract(data,'$.assigned_user_ids')) WHERE value=?2) AND deleted=0)",rusqlite::params![app_id,user],|r|r.get(0)).map_err(|_|"app authority unavailable")?;
        if !live || !may_use || device != "device" {
            return Err("revoked".into());
        }
        Ok(())
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    store: StoreFixture,
    auth: Arc<StoreRightsAuthority>,
    key: Arc<Key>,
    session: Arc<AppGrantSession>,
}
fn manifests() -> Vec<ManifestRules> {
    vec![
        ManifestRules::from_manifest(
            "ctox-platform",
            "1",
            [
                "business_users".into(),
                "business_apps".into(),
                "app_user_preferences".into(),
            ]
            .into(),
            &json!({}),
        )
        .unwrap(),
        ManifestRules::from_manifest("a", "1", ["alpha".into()].into(), &json!({})).unwrap(),
        ManifestRules::from_manifest("b", "1", ["beta".into()].into(), &json!({})).unwrap(),
        ManifestRules::from_manifest("shell", "1", ["shell_docs".into()].into(), &json!({}))
            .unwrap(),
    ]
}
fn rules() -> Vec<Right> {
    ["alpha", "beta", "shell_docs"]
        .into_iter()
        .map(|name| Right {
            role: "user".into(),
            collection: name.into(),
            row_filter: RowFilter::Eq {
                path: "/owner".into(),
                value: FilterValue::ActorId,
            },
            field_mask: FieldMask::Fields {
                paths: ["/owner".into(), "/name".into()].into(),
            },
            ops: [Operation::Read, Operation::Write].into(),
        })
        .collect()
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = StoreFixture::open(DbId::new(dir.path().join("sync.sqlite3")).unwrap())
            .await
            .unwrap();
        store.writer().submit(WriteJob::new(Default::default(), |tx| {
            ctox_store::schema::initialize(tx)?;
            for (name, app) in [("alpha", "a"), ("beta", "b"), ("shell_docs", "shell"), ("business_users", "ctox-platform"), ("business_apps", "ctox-platform"), ("app_user_preferences", "ctox-platform")] {
                ctox_store::schema::create_collection(tx, &ctox_store::schema::CollectionDefinition {
                    name:name.into(), app_id:app.into(), schema_ver:1,
                    schema_json:json!({"type":"object", "properties":{"id":{"type":"string"}, "owner":{"type":"string"}, "name":{"type":"string"}}, "required":["id"]}),
                    class:"replicated".into(), client_limit:100, byte_budget:65536, rights_ver:1, app_ver:"1".into()
                })?;
            }
            tx.execute("INSERT INTO c_business_users (id,rev,schema_ver,data,deleted) VALUES('alice','1',1,?1,0)", [json!({"id":"alice","role":"user","active":true}).to_string()])?;
            for app in ["a", "b", "shell"] {
                tx.execute("INSERT INTO c_business_apps (id,rev,schema_ver,data,deleted) VALUES(?1,'1',1,?2,0)", rusqlite::params![app,json!({"id":app,"state":"active","assigned_user_ids":["alice"]}).to_string()])?;
            }
            Ok(())
        })).unwrap().await.unwrap();
        let auth = StoreRightsAuthority::attach(store.writer(), "instance".into())
            .await
            .unwrap();
        auth.replace(rules(), manifests()).await.unwrap();
        let key = Key::new();
        let session = AppGrantSession::new(
            auth.clone(),
            key.clone(),
            Arc::new(CurrentPeer),
            "alice".into(),
            "user".into(),
            "device".into(),
        )
        .unwrap();
        Self {
            _dir: dir,
            store,
            auth,
            key,
            session,
        }
    }
    async fn read(&self, token: &str, collection: &str, now: u64) -> Result<Value, StoreError> {
        let session = self.session.clone();
        let token = token.to_owned();
        let collection = collection.to_owned();
        self.store
            .writer()
            .submit(WriteJob::new(Default::default(), move |tx| {
                session
                    .with_read_in_transaction(
                        tx,
                        &token,
                        &collection,
                        now,
                        &json!({"id":"doc", "owner":"alice", "name":"visible", "secret":"hidden"}),
                        Ok,
                    )
                    .map_err(StoreError::Invalid)
            }))
            .unwrap()
            .await
    }
    async fn close(self) {
        self.store.shutdown().await.unwrap();
    }
}
fn claims(token: &str) -> Value {
    serde_json::from_slice::<Value>(&URL_SAFE_NO_PAD.decode(token).unwrap()).unwrap()["claims"]
        .clone()
}

#[tokio::test]
async fn p6_e55_foreign_app_collections_never_enter_the_grant() {
    let f = Fixture::new().await;
    let token = f.session.issue("a", 1000).await.unwrap();
    let c = claims(&token);
    assert_eq!(
        c["collections"].as_array().unwrap().len(),
        2,
        "foreign app collection entered grant"
    );
    assert_eq!(c["collections"][0]["collection"], "alpha");
    assert_eq!(c["collections"][0]["rights"], json!(["read", "write"]));
    assert!(c["rights_ver"].is_string());
    assert_eq!(
        f.read(&token, "alpha", 1001).await.unwrap(),
        json!({"owner":"alice", "name":"visible"})
    );
    assert!(f.read(&token, "beta", 1001).await.is_err());
    // A second app gets its own token, not an aggregate grant.
    let b = f.session.issue("b", 1000).await.unwrap();
    assert!(claims(&b)["collections"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["collection"] == "beta"));
    let no_right = AppGrantSession::new(
        f.auth.clone(),
        f.key.clone(),
        Arc::new(CurrentPeer),
        "alice".into(),
        "unknown".into(),
        "device".into(),
    )
    .unwrap();
    assert!(no_right.issue("a", 1000).await.is_err());
    let reply = no_right
        .handle_request(GrantRequest::Request { app_id: "a".into() }, 1000)
        .await;
    assert_eq!(
        reply,
        json!({"type":"forbidden", "app_id":"a", "reason":"forbidden"})
    );
    let mut forged: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&token).unwrap()).unwrap();
    forged["claims"]["app_id"] = json!("b");
    let forged = URL_SAFE_NO_PAD.encode(canonical_json(&forged).unwrap());
    assert!(
        f.read(&forged, "beta", 1001).await.is_err(),
        "signature bypass"
    );
    f.close().await;
}
#[tokio::test]
async fn p6_e55_expiry_is_closed_at_the_exact_boundary() {
    let f = Fixture::new().await;
    let token = f.session.issue("a", 1000).await.unwrap();
    let exp = claims(&token)["exp"].as_u64().unwrap();
    assert!(f.read(&token, "alpha", exp - 1).await.is_ok());
    assert!(
        f.read(&token, "alpha", exp).await.is_err(),
        "expired grant remained readable"
    );
    assert!(f.read(&token, "alpha", exp + 1).await.is_err());
    f.close().await;
}
#[tokio::test]
async fn p6_e55_revoked_app_is_unreadable_before_reauthentication() {
    let f = Fixture::new().await;
    let token = f.session.issue("a", 1000).await.unwrap();
    let b = f.session.issue("b", 1000).await.unwrap();
    assert!(f.read(&token, "alpha", 1001).await.is_ok());
    let frames = f
        .session
        .rebind("alice".into(), "other".into(), "device".into())
        .unwrap();
    assert_eq!(
        frames.iter().map(|v| v.app_id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(
        serde_json::to_value(&frames[0]).unwrap(),
        json!({"type":"scope_revoke", "app_id":"a", "reason":"revoked"})
    );
    assert!(
        f.read(&token, "alpha", 1001).await.is_err(),
        "revoked app remained readable"
    );
    assert!(f.read(&b, "beta", 1001).await.is_err());
    f.session
        .rebind("alice".into(), "user".into(), "device".into())
        .unwrap();
    assert!(
        f.read(&token, "alpha", 1001).await.is_err(),
        "identity round trip revived old grant"
    );
    let renewed = f.session.issue("a", 1000).await.unwrap();
    assert_ne!(
        renewed, token,
        "same-second reauthentication revived old token"
    );
    assert!(f.read(&token, "alpha", 1001).await.is_err());
    assert!(f.read(&renewed, "alpha", 1001).await.is_ok());
    f.session.revoke_app("a").unwrap();
    assert!(
        f.read(&renewed, "alpha", 1001).await.is_err(),
        "revoked app remained readable"
    );
    let token = f.session.issue("a", 1001).await.unwrap();
    // A principal change is fenced even before the transport has rebound the
    // cached peer. The mandatory adapter check runs in this actual writer TX.
    f.store
        .writer()
        .submit(WriteJob::new(Default::default(), |tx| {
            tx.execute(
                "UPDATE c_business_users SET data=json_set(data,'$.active',0) WHERE id='alice'",
                [],
            )?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    assert!(
        f.read(&token, "alpha", 1002).await.is_err(),
        "changed principal retained grant"
    );
    assert!(
        f.session.issue("a", 1002).await.is_err(),
        "changed principal obtained grant"
    );
    f.store
        .writer()
        .submit(WriteJob::new(Default::default(), |tx| {
            tx.execute(
                "UPDATE c_business_users SET data=json_set(data,'$.active',1) WHERE id='alice'",
                [],
            )?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    // Publish policy through the same real writer. Old grants must fail even
    // before the commit-driven control notification has been consumed.
    f.auth.replace(vec![], manifests()).await.unwrap();
    assert!(f.read(&token, "alpha", 1002).await.is_err());
    let owner = f.session.clone();
    let revoked = f
        .store
        .writer()
        .submit(WriteJob::new(Default::default(), move |tx| {
            owner
                .revocations_in_transaction(tx, 1002)
                .map_err(StoreError::Invalid)
        }))
        .unwrap()
        .await
        .unwrap();
    assert_eq!(revoked.len(), 1);
    let fresh = Fixture::new().await;
    let token = fresh.session.issue("a", 1000).await.unwrap();
    fresh.key.active.store(false, Ordering::Release);
    assert!(
        fresh.read(&token, "alpha", 1001).await.is_err(),
        "retired signer remained usable"
    );
    fresh.close().await;
    f.close().await;
}

#[tokio::test]
async fn p6_e55_route_forbidden_duplicate_and_kernel_key_pin() {
    use p6_e55_contract::rights::app_grant_route::AppGrantRoute;
    let f = Fixture::new().await;
    f.store.writer().submit(WriteJob::new(Default::default(), |tx| {
        tx.execute("UPDATE c_business_apps SET data=json_set(data,'$.assigned_user_ids',json('[]')) WHERE id='b'",[])?;
        Ok(())
    })).unwrap().await.unwrap();
    let route = Arc::new(AppGrantRoute::new(f.session.clone(), f.auth.clone()));
    let shell = route.shell_token(1000).await.unwrap();
    assert_eq!(
        claims(&shell)["app_id"],
        "shell",
        "hello grant is not shell"
    );
    assert_eq!(
        claims(&shell)["collections"].as_array().unwrap().len(),
        2,
        "hello leaked other app collections"
    );
    let denied = route
        .handle(json!({"type":"scope.grant.request","app_id":"b"}), 1000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        denied["type"], "forbidden",
        "unauthorized app received grant"
    );
    assert!(
        route
            .handle(json!({"type":"scope.grant.request","app_id":"b"}), 1000)
            .await
            .unwrap()
            .is_none(),
        "duplicate denial answered twice"
    );
    let (first, second) = tokio::join!(
        route.handle(json!({"type":"scope.grant.request","app_id":"a"}), 1000),
        route.handle(json!({"type":"scope.grant.request","app_id":"a"}), 1000)
    );
    let replies = [first.unwrap(), second.unwrap()];
    assert_eq!(
        replies.iter().filter(|r| r.is_some()).count(),
        1,
        "duplicate app request answered twice"
    );
    let response = replies.into_iter().flatten().next().unwrap();
    assert_eq!(response["type"], "scope.grant");
    let token = response["token"].as_str().unwrap();
    assert!(f.read(token, "alpha", 1001).await.is_ok());
    let mut forged: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(token).unwrap()).unwrap();
    let wrong = Key::new();
    let c: AppGrantClaims = serde_json::from_value(forged["claims"].clone()).unwrap();
    forged["signature"] =
        json!(URL_SAFE_NO_PAD.encode(wrong.pair.sign(&signing_bytes(&c).unwrap()).as_ref()));
    let wrong_token = URL_SAFE_NO_PAD.encode(canonical_json(&forged).unwrap());
    if let Ok(path) = std::env::var("P6_E55_VECTOR") {
        std::fs::write(
            path,
            json!({"token":token,"wrong_token":wrong_token,"public":f.key.pin,"claims":c})
                .to_string(),
        )
        .unwrap();
    }
    assert!(
        f.read(&wrong_token, "alpha", 1001).await.is_err(),
        "foreign signing key accepted"
    );
    f.store.writer().submit(WriteJob::new(Default::default(), |tx| {
        tx.execute("UPDATE c_business_apps SET data=json_set(data,'$.assigned_user_ids',json('[]')) WHERE id='a'",[])?;
        Ok(())
    })).unwrap().await.unwrap();
    assert!(
        f.read(token, "alpha", 1002).await.is_err(),
        "app assignment revoked but remained readable"
    );
    let session = f.session.clone();
    let revoked = f
        .store
        .writer()
        .submit(WriteJob::new(Default::default(), move |tx| {
            session
                .revocations_in_transaction(tx, 1002)
                .map_err(StoreError::Invalid)
        }))
        .unwrap()
        .await
        .unwrap();
    assert_eq!(
        revoked
            .iter()
            .map(|v| v.app_id.as_str())
            .collect::<Vec<_>>(),
        vec!["a"]
    );
    assert_eq!(
        route
            .handle(json!({"type":"scope.grant.request","app_id":"a"}), 1240)
            .await
            .unwrap()
            .unwrap()["type"],
        "forbidden"
    );
    f.close().await;
}

#[tokio::test]
async fn p6_shell_boot_grant_contains_actor_bound_preferences() {
    let f = Fixture::new().await;
    let token = f.session.issue("shell", 1000).await.unwrap();
    assert!(
        claims(&token)["collections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["collection"] == "app_user_preferences"),
        "shell boot preferences grant missing"
    );
    let session = f.session.clone();
    f.store.writer().submit(WriteJob::new(Default::default(), move |tx| {
        let scope=session.scope_in_transaction(tx,&token,"app_user_preferences",1001).unwrap();
        let projection=scope.projection("app_user_preferences").unwrap();
        let filter=serde_json::to_value(&projection.row_filter).unwrap();
        assert!(filter.to_string().contains("/user_id") && filter.to_string().contains("alice"), "shell preference filter lost actor");
        for (user,allowed) in [("alice",true),("bob",false)] {
            let doc=json!({"id":format!("shell/{user}/layout"),"app_id":"shell","user_id":user,"key":"layout","value":1});
            assert_eq!(session.with_read_in_transaction(tx,&token,"app_user_preferences",1001,&doc,Ok).is_ok(),allowed,"foreign user preferences leaked");
        }
        Ok(())
    })).unwrap().await.unwrap();
    f.close().await;
}

#[tokio::test]
async fn p6_browser_binding_uses_app_hash_and_retracts_retained_tokens() {
    let f = Fixture::new().await;
    let token = f.session.issue("a", 1000).await.unwrap();
    let hash = claims(&token)["scope_hash"].as_str().unwrap().to_owned();
    assert_eq!(
        f.session
            .bound_token(Some(&hash), "alpha", 1, None)
            .unwrap(),
        token
    );
    assert!(
        f.session
            .bound_token(Some("foreign-app-scope"), "alpha", 1, None)
            .is_err(),
        "foreign scope admitted"
    );
    assert!(
        f.session.bound_token(Some(&hash), "beta", 1, None).is_err(),
        "foreign collection admitted"
    );
    assert!(
        f.session.bound_token(None, "alpha", 1, Some("b")).is_err(),
        "foreign app write admitted"
    );
    assert!(
        f.session.bound_token(None, "alpha", 2, Some("a")).is_err(),
        "foreign generation admitted"
    );
    f.session.token_current(&token, 1001).unwrap();
    f.session.revoke_app("a").unwrap().unwrap();
    assert!(
        f.session
            .bound_token(Some(&hash), "alpha", 1, None)
            .is_err(),
        "revoked subscription survived"
    );
    assert!(
        f.session.token_current(&token, 1001).is_err(),
        "queued publication survived revoke"
    );
    assert!(f.read(&token, "alpha", 1001).await.is_err());
    f.close().await;
}

#[tokio::test]
async fn p6_shell_boot_preference_cursor_uses_real_p8_and_e55_reader() {
    use ctox_store::change_log::subscription::{Subscription, SubscriptionKey};
    let f = Fixture::new().await;
    f.store.writer().submit(WriteJob::new(Default::default(),|tx| {
        for (app,user) in [("shell","alice"),("shell","bob"),("b","alice")] {
            let doc=json!({"id":format!("{app}/{user}/layout"),"app_id":app,"user_id":user,"key":"layout","value":7});
            tx.execute("INSERT INTO c_app_user_preferences(id,rev,schema_ver,data,deleted) VALUES(?1,'1',1,?2,0)",rusqlite::params![doc["id"].as_str().unwrap(),doc.to_string()])?;
        }
        Ok(())
    })).unwrap().await.unwrap();
    let token = f.session.issue("shell", 1000).await.unwrap();
    let hash = claims(&token)["scope_hash"].as_str().unwrap().to_owned();
    let ctx = ctox_store::preferences::Context::authenticated(
        "shell",
        "alice",
        claims(&token)["rights_ver"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
    )
    .unwrap();
    assert_ne!(
        hash,
        ctx.scope_hash(),
        "fixture must exercise aggregate E55 scope"
    );
    let key = SubscriptionKey {
        participant: "alice".into(),
        collection: "app_user_preferences".into(),
        scope_hash: hash,
        gen: 1,
    };
    let reader =
        Arc::new(reader::AppGrantReader::new(f.session.clone(), key.clone(), || 1001).unwrap());
    let mut cursor = Subscription::new(f.store.readers(), &f.store.writer(), key, None).unwrap();
    let page = cursor
        .next_page(100, reader, &*f.key, || 1_001_000)
        .await
        .expect("shell boot preferences cursor rejected aggregate grant");
    let mut ids = page.docs.iter().map(|d| d.id.as_str()).collect::<Vec<_>>();
    ids.sort();
    assert_eq!(
        ids,
        vec!["b/alice/layout", "shell/alice/layout"],
        "Shell lost own-app preferences or leaked foreign user"
    );
    assert!(!page.sig.is_empty(), "P8 cursor must use held E22 signer");
    f.close().await;
}

#[tokio::test]
async fn p6_browser_publication_follows_the_fresh_app_grant_version() {
    let f=Fixture::new().await;
    let epoch=f.auth.hello_snapshot().await.unwrap().0.epoch;
    let old=f.session.issue("a",1000).await.unwrap();
    f.session.with_publication(&old,1001,&epoch,||Ok(())).expect("initial grant did not publish");
    f.auth.replace(rules(),manifests()).await.unwrap();
    assert!(f.session.with_publication(&old,1001,&epoch,||Ok(())).is_err(),"old version survived publication before revoke notification");
    let fresh=f.session.issue("a",1001).await.unwrap();
    f.session.with_publication(&fresh,1002,&epoch,||Ok(())).expect("fresh grant stuck on original hello version");
    f.session.revoke_app("a").unwrap();
    assert!(f.session.with_publication(&fresh,1002,&epoch,||Ok(())).is_err(),"revoked publication retained");
    f.close().await;
}

#[tokio::test]
async fn p6_revoked_preference_write_keeps_its_journaled_reason() {
    use ctox_store::change_log::subscription::{WriteAuthority,WritePermission,protocol::{OrderedWrite,Write},write_contract::OrderedWriter};
    struct Revoked;
    impl WriteAuthority for Revoked {
        fn authorize(&self,_:&rusqlite::Transaction<'_>,_:&str,_:&Write)->rusqlite::Result<WritePermission>{Ok(WritePermission::Reject("revoked".into()))}
        fn validate_document(&self,_:&rusqlite::Transaction<'_>,_:&str,_:&ctox_store::schema::Collection,_:&Value)->rusqlite::Result<WritePermission>{panic!("revoked write reached document validation")}
    }
    let f=Fixture::new().await;
    let now=1_001_000u64;
    let mut bytes=[0u8;16];bytes[..6].copy_from_slice(&now.to_be_bytes()[2..]);bytes[6]=0x70;bytes[8]=0x80;bytes[15]=1;
    let hex=bytes.iter().map(|b|format!("{b:02x}")).collect::<String>();
    let op=format!("{}-{}-{}-{}-{}",&hex[..8],&hex[8..12],&hex[12..16],&hex[16..20],&hex[20..]);
    let write:OrderedWrite=serde_json::from_value(json!({"client_seq":"1","op_id":op,"collection":"app_user_preferences","id":"shell/alice/layout","gen":1,"schema_ver":1,"app_ver":"1","op":"put","body":{"id":"shell/alice/layout","app_id":"shell","user_id":"alice","key":"layout","value":7}})).unwrap();
    let batch=OrderedWriter::new(f.store.writer(),Arc::new(Revoked)).submit("device".into(),write,now).await.unwrap();
    assert_eq!(batch.results[0].result.reason.as_deref(),Some("revoked"),"revoked preference rejection lost its reason");
    f.store.readers().read(|tx|{
        let stored:String=tx.query_row("SELECT result_json FROM _write_results WHERE participant='device' AND client_seq=1",[],|r|r.get(0))?;
        assert_eq!(serde_json::from_str::<Value>(&stored).unwrap()["reason"],"revoked","durable E14 journal lost revocation");
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM c_app_user_preferences",[],|r|r.get::<_,u64>(0))?,0);
        Ok(())
    }).await.unwrap();
    f.close().await;
}
