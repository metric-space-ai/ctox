use p6_e55_contract::rights::{self, app_grant::*, scope::*, manifest_rules::ManifestRules, Operation, Right, StoreRightsAuthority};
use ctox_store::{testkit::StoreFixture, DbId, WriteJob, StoreError};
use ctox_store::change_log::subscription::protocol::InstanceSigner;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

// Only cryptographic test custody is replaced. Production app rights, writer,
// registry, canonical encoder, verifier, projection and session fences are real.
struct Key { pair: Ed25519KeyPair, pin: String, active: AtomicBool }
impl Key {
    fn new() -> Arc<Self> {
        let doc = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(doc.as_ref()).unwrap();
        let pin = format!("ed25519:{}", pair.public_key().as_ref().iter().map(|b| format!("{b:02x}")).collect::<String>());
        Arc::new(Self { pair, pin, active: AtomicBool::new(true) })
    }
}
impl InstanceSigner for Key {
    fn instance_id(&self) -> &str { "instance" }
    fn key_id(&self) -> &str { &self.pin }
    fn sign(&self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        if !self.active.load(Ordering::Acquire) { return Err("retired".into()); }
        assert!(bytes.starts_with(DOMAIN));
        let claims: AppGrantClaims = serde_json::from_slice(&bytes[DOMAIN.len()..]).unwrap();
        assert_eq!(signing_bytes(&claims).unwrap(), bytes);
        Ok(self.pair.sign(bytes).as_ref().to_vec())
    }
}
struct CurrentPeer;
impl GrantPeerAuthority for CurrentPeer {
    fn validate(&self, tx: &rusqlite::Transaction<'_>, user: &str, role: &str, device: &str) -> Result<(), String> {
        let live: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM c_business_users WHERE id=?1 AND json_extract(data,'$.role')=?2 AND json_extract(data,'$.active')=1 AND deleted=0)",rusqlite::params![user,role],|r|r.get(0)).map_err(|_|"principal unavailable")?;
        if !live || device != "device" { return Err("revoked".into()); }
        Ok(())
    }
}
struct Fixture { _dir: tempfile::TempDir, store: StoreFixture, auth: Arc<StoreRightsAuthority>, key: Arc<Key>, session: Arc<AppGrantSession> }
fn manifests() -> Vec<ManifestRules> {
    vec![ManifestRules::from_manifest("a", "1", ["alpha".into()].into(), &json!({})).unwrap(),
         ManifestRules::from_manifest("b", "1", ["beta".into()].into(), &json!({})).unwrap()]
}
fn rules() -> Vec<Right> {
    ["alpha", "beta"].into_iter().map(|name| Right { role:"user".into(), collection:name.into(),
        row_filter:RowFilter::Eq { path:"/owner".into(), value:FilterValue::ActorId },
        field_mask:FieldMask::Fields { paths:["/owner".into(), "/name".into()].into() },
        ops:[Operation::Read, Operation::Write].into() }).collect()
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = StoreFixture::open(DbId::new(dir.path().join("sync.sqlite3")).unwrap()).await.unwrap();
        store.writer().submit(WriteJob::new(Default::default(), |tx| {
            ctox_store::schema::initialize(tx)?;
            for (name, app) in [("alpha", "a"), ("beta", "b"), ("business_users", "ctox-platform")] {
                ctox_store::schema::create_collection(tx, &ctox_store::schema::CollectionDefinition {
                    name:name.into(), app_id:app.into(), schema_ver:1,
                    schema_json:json!({"type":"object", "properties":{"id":{"type":"string"}, "owner":{"type":"string"}, "name":{"type":"string"}}, "required":["id"]}),
                    class:"replicated".into(), client_limit:100, byte_budget:65536, rights_ver:1, app_ver:"1".into()
                })?;
            }
            tx.execute("INSERT INTO c_business_users (id,rev,data,deleted) VALUES('alice','1',?1,0)", [json!({"id":"alice","role":"user","active":true}).to_string()])?;
            Ok(())
        })).unwrap().await.unwrap();
        let auth = StoreRightsAuthority::attach(store.writer(), "instance".into()).await.unwrap();
        auth.replace(rules(), manifests()).await.unwrap();
        let key = Key::new();
        let session = AppGrantSession::new(auth.clone(), key.clone(), Arc::new(CurrentPeer), "alice".into(), "user".into(), "device".into()).unwrap();
        Self { _dir:dir, store, auth, key, session }
    }
    async fn read(&self, token: &str, collection: &str, now: u64) -> Result<Value, StoreError> {
        let session = self.session.clone(); let token = token.to_owned(); let collection = collection.to_owned();
        self.store.writer().submit(WriteJob::new(Default::default(), move |tx| {
            session.with_read_in_transaction(tx, &token, &collection, now,
                &json!({"id":"doc", "owner":"alice", "name":"visible", "secret":"hidden"}), Ok)
                .map_err(StoreError::Invalid)
        })).unwrap().await
    }
    async fn close(self) { self.store.shutdown().await.unwrap(); }
}
fn claims(token: &str) -> Value { serde_json::from_slice::<Value>(&URL_SAFE_NO_PAD.decode(token).unwrap()).unwrap()["claims"].clone() }

#[tokio::test]
async fn p6_e55_foreign_app_collections_never_enter_the_grant() {
    let f = Fixture::new().await;
    let token = f.session.issue("a", 1000).await.unwrap();
    let c = claims(&token);
    assert_eq!(c["collections"].as_array().unwrap().len(), 1, "foreign app collection entered grant");
    assert_eq!(c["collections"][0]["collection"], "alpha@g1");
    assert_eq!(c["collections"][0]["rights"], json!(["read", "write"]));
    assert!(c["rights_ver"].is_string());
    assert_eq!(f.read(&token, "alpha@g1", 1001).await.unwrap(), json!({"owner":"alice", "name":"visible"}));
    assert!(f.read(&token, "beta@g1", 1001).await.is_err());
    // A second app gets its own token, not an aggregate grant.
    let b = f.session.issue("b",1000).await.unwrap();
    assert_eq!(claims(&b)["collections"][0]["collection"], "beta@g1");
    let no_right = AppGrantSession::new(f.auth.clone(), f.key.clone(), Arc::new(CurrentPeer), "alice".into(), "unknown".into(), "device".into()).unwrap();
    assert!(no_right.issue("a",1000).await.is_err());
    let reply = no_right.handle_request(GrantRequest::Request{app_id:"a".into()}, 1000).await;
    assert_eq!(reply, json!({"type":"forbidden", "app_id":"a", "reason":"forbidden"}));
    let mut forged: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&token).unwrap()).unwrap();
    forged["claims"]["app_id"] = json!("b");
    let forged = URL_SAFE_NO_PAD.encode(canonical_json(&forged).unwrap());
    assert!(f.read(&forged, "beta@g1",1001).await.is_err(), "signature bypass");
    f.close().await;
}
#[tokio::test]
async fn p6_e55_expiry_is_closed_at_the_exact_boundary() {
    let f = Fixture::new().await;
    let token = f.session.issue("a", 1000).await.unwrap();
    let exp = claims(&token)["exp"].as_u64().unwrap();
    assert!(f.read(&token, "alpha@g1", exp-1).await.is_ok());
    assert!(f.read(&token, "alpha@g1", exp).await.is_err(), "expired grant remained readable");
    assert!(f.read(&token, "alpha@g1", exp+1).await.is_err());
    f.close().await;
}
#[tokio::test]
async fn p6_e55_revoked_app_is_unreadable_before_reauthentication() {
    let f = Fixture::new().await;
    let token = f.session.issue("a", 1000).await.unwrap();
    let b = f.session.issue("b",1000).await.unwrap();
    assert!(f.read(&token, "alpha@g1",1001).await.is_ok());
    let frames = f.session.rebind("alice".into(), "other".into(), "device".into()).unwrap();
    assert_eq!(frames.iter().map(|v|v.app_id.as_str()).collect::<Vec<_>>(), vec!["a","b"]);
    assert_eq!(serde_json::to_value(&frames[0]).unwrap(), json!({"type":"scope_revoke", "app_id":"a", "reason":"revoked"}));
    assert!(f.read(&token, "alpha@g1",1001).await.is_err(), "revoked app remained readable");
    assert!(f.read(&b, "beta@g1",1001).await.is_err());
    f.session.rebind("alice".into(),"user".into(),"device".into()).unwrap();
    let renewed = f.session.issue("a",1000).await.unwrap();
    assert_ne!(renewed, token, "same-second reauthentication revived old token");
    assert!(f.read(&token,"alpha@g1",1001).await.is_err());
    assert!(f.read(&renewed,"alpha@g1",1001).await.is_ok());
    f.session.revoke_app("a").unwrap();
    assert!(f.read(&renewed,"alpha@g1",1001).await.is_err(), "revoked app remained readable");
    let token = f.session.issue("a",1001).await.unwrap();
    // Publish policy through the same real writer. Old grants must fail even
    // before the commit-driven control notification has been consumed.
    f.auth.replace(vec![], manifests()).await.unwrap();
    assert!(f.read(&token,"alpha@g1",1002).await.is_err());
    let owner=f.session.clone();
    let revoked=f.store.writer().submit(WriteJob::new(Default::default(), move |tx| {
        owner.revocations_in_transaction(tx,1002).map_err(StoreError::Invalid)
    })).unwrap().await.unwrap();
    assert_eq!(revoked.len(),1);
    let fresh = Fixture::new().await;
    let token = fresh.session.issue("a",1000).await.unwrap();
    fresh.key.active.store(false,Ordering::Release);
    assert!(fresh.read(&token,"alpha@g1",1001).await.is_err(), "retired signer remained usable");
    fresh.close().await;
    f.close().await;
}
