// Origin: CTOX
// License: AGPL-3.0-only
//! E41 bootstrap content. HTTP/authentication and P4 admission remain core seams.
//! No business documents, private keys or native signaling token cross this API.
use super::{
    policy::{self, sync_rights, BusinessOsActor, BusinessOsPermission, BusinessOsScope},
    store::{self, BusinessOsUser},
};
use crate::service::store_inputs::StoreInputs;
use anyhow::{anyhow, ensure, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ring::{
    rand::{SecureRandom, SystemRandom},
    signature::{UnparsedPublicKey, ED25519},
};
use rusqlite::{Connection, OptionalExtension, Transaction};
use rxdb::plugins::replication_webrtc::index_mod::hello::{
    ApiRange, Codec, DeviceProof, Hello, HelloFrame, TransportOffer, MAX_FRAME_BYTES,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct V4LaunchContext {
    pub v4: Launch,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Launch {
    pub instance_id: String,
    pub user_id: String,
    pub actor: Actor,
    pub signaling: Signaling,
    pub hello: Offer,
    pub package_keys: Vec<PackageKey>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Actor {
    pub id: String,
    pub role: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Signaling {
    pub urls: Vec<String>,
    pub room: String,
    pub peer_id: String,
    pub native_peer_id: String,
    pub token: String,
    pub ice_servers: Vec<IceServer>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IceServer {
    pub urls: IceUrls,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum IceUrls {
    One(String),
    Many(Vec<String>),
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Offer {
    pub offer: HelloFrame,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PackageKey {
    pub key_id: String,
    pub alg: String,
    pub public_key: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Device {
    actor: Actor,
    peer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    device_pubkey: Option<String>,
}

/// `user` is the route's authenticated principal, never a browser body. Its role
/// and active flag are reloaded from the current owner snapshot, not trusted.
pub(crate) async fn build(
    owners: &Arc<StoreInputs>,
    user: &BusinessOsUser,
) -> Result<V4LaunchContext> {
    build_principal(owners, user, false, None).await
}
/// E54.4: called only after the pairing owner verified the one-time proof.
/// The device key is inside the instance-signed offer and E50's ticket digest;
/// no alternate signer, ticket, peer role or admission path is introduced.
pub(crate) async fn build_for_device(
    owners: &Arc<StoreInputs>,
    user: &BusinessOsUser,
    device_pubkey: &str,
) -> Result<V4LaunchContext> {
    let raw = URL_SAFE_NO_PAD.decode(device_pubkey)?;
    ensure!(
        raw.len() == 32 && URL_SAFE_NO_PAD.encode(&raw) == device_pubkey,
        "invalid paired device public key"
    );
    build_principal(owners, user, false, Some(device_pubkey)).await
}
/// E50: only the HTTP boundary may authenticate the implicit loopback admin.
/// No persisted local-dev account or credential is created.
pub(crate) async fn build_local(owners: &Arc<StoreInputs>) -> Result<V4LaunchContext> {
    let user = BusinessOsUser {
        id: "local-dev".into(),
        display_name: "Local CTOX".into(),
        role: "admin".into(),
        active: true,
        profile: serde_json::json!({}),
        created_at_ms: 0,
        updated_at_ms: 0,
    };
    build_principal(owners, &user, true, None).await
}
async fn build_principal(
    owners: &Arc<StoreInputs>,
    user: &BusinessOsUser,
    local: bool,
    device_pubkey: Option<&str>,
) -> Result<V4LaunchContext> {
    ensure!(
        user.active && !user.id.is_empty(),
        "forbidden: inactive principal"
    );
    let signer = owners.instance_signer()?;
    let raw_key = public_key(signer.key_id())?;
    let id = user.id.clone();
    let key = raw_key;
    let snapshot = owners
        .activation_snapshot(move |tx| {
            snapshot(tx, &id, &key, local)
                .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))
        })
        .await
        .context("read launch identity and grants")?
        .value;
    let root = owners.root.clone();
    let peer = format!("browser-{}", uuid::Uuid::new_v4());
    let session = peer.clone();
    // Reuse the V3 browser credential and configured/ctox.dev TURN route. This
    // existing helper performs network/secret-owner I/O outside the Sync snapshot.
    let config =
        crate::persistence::run_core_sync(move || store::sync_config_for_browser(&root, &session))
            .await
            .context("resolve browser signaling and TURN")?;
    ensure!(
        config.instance_id == signer.instance_id(),
        "instance identity differs"
    );
    let mut signaling = Signaling {
        urls: config.signaling_urls,
        room: config.sync_room,
        peer_id: peer,
        native_peer_id: config.native_peer_id,
        token: config.signaling_browser_token,
        ice_servers: config
            .ice_servers
            .into_iter()
            .map(serde_json::from_value)
            .collect::<std::result::Result<_, _>>()?,
    };
    validate_signaling(&mut signaling)?;
    let actor = snapshot.0;
    let device = serde_json::to_string(&Device {
        actor: actor.clone(),
        peer: signaling.peer_id.clone(),
        device_pubkey: device_pubkey.map(str::to_owned),
    })?;
    ensure!(
        device.len() <= 256,
        "invalid: bootstrap device identity too long"
    );
    let mut nonce = [0u8; 32];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| anyhow!("nonce unavailable"))?;
    let mut hello = Hello {
        protocol_ver: 4,
        bootstrap_ver: 1,
        app_api_ver: ApiRange {
            min: "4.0".into(),
            max: "4.0".into(),
        },
        instance_id: signer.instance_id().into(),
        capabilities: snapshot.2,
        device_proof: DeviceProof {
            device_id: device,
            public_key: URL_SAFE_NO_PAD.encode(raw_key),
            nonce: URL_SAFE_NO_PAD.encode(nonce),
            signature: String::new(),
        },
        scopes_wanted: snapshot.1,
        transport: TransportOffer {
            codecs: vec![Codec::Gzip],
            max_frame_bytes: MAX_FRAME_BYTES,
            fragmentation: true,
            reassembly: true,
        },
    };
    let signature = signer
        .sign(&signing_bytes(&hello)?)
        .map_err(|_| anyhow!("instance signing authority unavailable"))?;
    ensure!(signature.len() == 64, "invalid Ed25519 signature");
    hello.device_proof.signature = URL_SAFE_NO_PAD.encode(signature);
    // Check owner generation again after async work/signing. No stale owner is
    // returned when the daemon has retired this signer or Store generation.
    owners.instance_signer()?;
    let offer = HelloFrame::Hello(hello);
    offer
        .encode()
        .map_err(|e| anyhow!("invalid P4 offer: {e:?}"))?;
    let context = V4LaunchContext {
        v4: Launch {
            instance_id: signer.instance_id().into(),
            user_id: actor.id.clone(),
            actor,
            signaling,
            hello: Offer { offer },
            package_keys: vec![PackageKey {
                key_id: signer.key_id().into(),
                alg: "Ed25519".into(),
                public_key: URL_SAFE_NO_PAD.encode(raw_key),
            }],
        },
    };
    owners
        .browser_admission()?
        .register(&context, local)
        .await?;
    Ok(context)
}
fn validate_signaling(s: &mut Signaling) -> Result<()> {
    ensure!(
        !s.urls.is_empty()
            && !s.room.is_empty()
            && !s.token.is_empty()
            && !s.native_peer_id.is_empty()
            && s.peer_id != s.native_peer_id,
        "invalid browser signaling"
    );
    for endpoint in &s.urls {
        let url = url::Url::parse(endpoint)?;
        ensure!(
            matches!(url.scheme(), "ws" | "wss")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none(),
            "invalid signaling URL"
        );
    }
    for server in &s.ice_servers {
        let urls = match &server.urls {
            IceUrls::One(v) => std::slice::from_ref(v),
            IceUrls::Many(v) => v.as_slice(),
        };
        ensure!(!urls.is_empty(), "empty ICE urls");
        for url in urls {
            ensure!(
                ["stun:", "stuns:", "turn:", "turns:"]
                    .iter()
                    .any(|p| url.starts_with(p))
                    && url.len() > 6,
                "invalid ICE URL"
            );
        }
    }
    Ok(())
}
fn public_key(identity: &str) -> Result<[u8; 32]> {
    let hex = identity
        .strip_prefix("ed25519:")
        .context("invalid instance key ID")?;
    ensure!(
        hex.len() == 64
            && hex
                .bytes()
                .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)),
        "invalid instance public key"
    );
    let mut key = [0; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)?;
    }
    Ok(key)
}
fn signing_bytes(hello: &Hello) -> Result<Vec<u8>> {
    let mut frame = serde_json::to_value(HelloFrame::Hello(hello.clone()))?;
    frame["device_proof"]
        .as_object_mut()
        .unwrap()
        .remove("signature");
    // Domain separation, full frame including scopes/capabilities and the
    // authenticated actor + browser peer embedded in device_id.
    let mut bytes = b"ctox.launch-context.v4\0".to_vec();
    bytes.extend(sync_rights::scope::canonical_json(&frame).map_err(anyhow::Error::msg)?);
    Ok(bytes)
}
/// P4 verifier seam: trust the daemon's published key, bind the WebRTC peer,
/// then reload this actor and its rights through the existing P6 authenticator.
/// This is proof verification, NOT a substitute for nonce consumption/revocation
/// by the connection admission owner.
pub(crate) fn verify_offer(hello: &Hello, peer: &str, key: &[u8; 32]) -> Result<Actor> {
    ensure!(
        hello.device_proof.public_key == URL_SAFE_NO_PAD.encode(key),
        "proof key differs"
    );
    let signature = URL_SAFE_NO_PAD.decode(&hello.device_proof.signature)?;
    UnparsedPublicKey::new(&ED25519, key)
        .verify(&signing_bytes(hello)?, &signature)
        .map_err(|_| anyhow!("invalid launch proof"))?;
    let device: Device = serde_json::from_str(&hello.device_proof.device_id)?;
    ensure!(
        device.peer == peer && !device.actor.id.is_empty(),
        "launch peer differs"
    );
    Ok(device.actor)
}
fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn documents(db: &Connection, collection: &str) -> Result<Vec<Value>> {
    let table: String = db.query_row(
        "SELECT table_name FROM _collections WHERE name=?1 AND state='active'",
        [collection],
        |r| r.get(0),
    )?;
    let mut stmt = db.prepare(&format!(
        "SELECT data FROM {} WHERE deleted=0 ORDER BY id",
        quote(&table)
    ))?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|s| serde_json::from_str(&s).map_err(Into::into))
        .collect()
}
fn document(db: &Connection, collection: &str, id: &str) -> Result<Value> {
    // Point lookup; bootstrap never enumerates other users' business data.
    let table: String = db.query_row(
        "SELECT table_name FROM _collections WHERE name=?1 AND state='active'",
        [collection],
        |r| r.get(0),
    )?;
    let raw: Option<String> = db
        .query_row(
            &format!(
                "SELECT data FROM {} WHERE id=?1 AND deleted=0",
                quote(&table)
            ),
            [id],
            |r| r.get(0),
        )
        .optional()?;
    serde_json::from_str(&raw.context("missing bootstrap metadata")?).map_err(Into::into)
}
fn snapshot(
    tx: &Transaction<'_>,
    id: &str,
    key: &[u8; 32],
    local: bool,
) -> Result<(Actor, Vec<String>, Vec<String>)> {
    let user = if local && id == "local-dev" {
        serde_json::json!({"id":"local-dev","role":"admin","active":true})
    } else {
        document(tx, "business_users", id)?
    };
    ensure!(
        user["active"] == true
            && user["id"] == id
            && user.get("user_id").and_then(Value::as_str).unwrap_or(id) == id,
        "forbidden: principal revoked"
    );
    let role = user["role"].as_str().context("missing role")?;
    ensure!(
        matches!(role, "chef" | "admin" | "founder" | "user"),
        "forbidden: invalid role"
    );
    let actor = BusinessOsActor::new(Some(id.into()), role);
    let rights = sync_rights::read(tx)?;
    let mut held = BTreeSet::new();
    let mut capabilities = BTreeSet::new();
    let mut can_code = false;
    for app in documents(tx, "business_apps")? {
        if app["state"] != "active" {
            continue;
        }
        let app_id = app["id"].as_str().context("missing app ID")?;
        let assigned = app["assigned_user_ids"]
            .as_array()
            .is_some_and(|v| v.iter().any(|v| v == id))
            || app["provenance"]["owner_id"] == id;
        let scope = BusinessOsScope::module(app_id, assigned);
        if !policy::evaluate_with_rights(&rights, &actor, BusinessOsPermission::AppsView, &scope)
            .allowed
        {
            continue;
        }
        // Native coding authorizes the requested TARGET app by provenance,
        // not mere assignment or edit access to the calling shell (draft_turn.rs).
        let source_owned = app["provenance"]["owner_id"] == id;
        can_code |= policy::evaluate_with_rights(
            &rights,
            &actor,
            BusinessOsPermission::AppsSourceEdit,
            &BusinessOsScope::module(app_id, source_owned),
        )
        .allowed;
        // An installed app with only a draft has no active manifest yet.
        let Some(ver) = app["active_ver"].as_str().filter(|v| !v.is_empty()) else {
            continue;
        };
        let version = document(tx, "business_app_versions", &format!("{app_id}@{ver}"))?;
        ensure!(
            matches!(version["state"].as_str(), Some("published" | "archived")),
            "unpublished active manifest"
        );
        let manifest = &version["manifest"];
        let bytes = serde_json::to_vec(manifest)?;
        ensure!(
            manifest["app_id"] == app_id
                && manifest["app_ver"] == ver
                && version["manifest_sha256"] == format!("{:x}", Sha256::digest(&bytes)),
            "active manifest differs"
        );
        let hex = version["signature"].as_str().context("unsigned manifest")?;
        ensure!(
            hex.len() == 128 && hex.bytes().all(|v| v.is_ascii_hexdigit()),
            "invalid manifest signature"
        );
        let signature = (0..64)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        UnparsedPublicKey::new(&ED25519, key)
            .verify(&bytes, &signature)
            .map_err(|_| anyhow!("active manifest signature differs"))?;
        held.insert(app_id.to_owned());
        let approved = app["capabilities_approved"].as_array();
        for required in manifest["capabilities_required"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let cap = required.as_str().context("invalid required capability")?;
            if !approved.is_some_and(|v| v.iter().any(|v| v == cap)) {
                continue;
            }

            capabilities.insert(cap.to_owned());
        }
    }
    if !can_code {
        capabilities.retain(|cap| !cap.starts_with("agent.coding."));
    }
    let resolved = rights.resolve(role, id).map_err(anyhow::Error::msg)?;
    let mut scopes = BTreeSet::new();
    for projection in resolved.projections {
        if projection.row_filter == sync_rights::scope::RowFilter::None {
            continue;
        }
        let owner: Option<String> = tx
            .query_row(
                "SELECT app_id FROM _collections WHERE name=?1 AND state='active'",
                [&projection.collection],
                |r| r.get(0),
            )
            .optional()?;
        if owner.is_some_and(|owner| owner == "ctox-platform" || held.contains(&owner)) {
            scopes.insert(projection.collection);
        }
    }
    Ok((
        Actor {
            id: id.into(),
            role: role.into(),
        },
        scopes.into_iter().collect(),
        capabilities.into_iter().collect(),
    ))
}
#[cfg(all(test, unix))]
#[path = "launch_context_v4_tests.rs"]
pub(crate) mod tests;

/// Existing role-bound native routing provider, retained in memory. The fixed
/// protocol changes to V4; the official issuer refreshes its token window.
pub(crate) fn native_transport(
    root: &std::path::Path,
) -> Result<(
    store::BusinessOsSyncConfig,
    Arc<dyn Fn() -> Vec<String> + Send + Sync>,
)> {
    let config = store::sync_config(root)?;
    let auth = store::signaling_auth_config(root, &config.signaling_room_password)?;
    let urls = super::rxdb_peer::native_signaling_url_provider(
        config.signaling_urls.clone(),
        config.sync_room.clone(),
        config.native_peer_id.clone(),
        auth,
    );
    let provider = Arc::new(move || {
        urls()
            .into_iter()
            .filter_map(|s| {
                let mut u = url::Url::parse(&s).ok()?;
                let pairs = u
                    .query_pairs()
                    .filter(|(k, _)| k != "protocol")
                    .map(|(k, v)| (k.into_owned(), v.into_owned()))
                    .collect::<Vec<_>>();
                u.set_query(None);
                u.query_pairs_mut()
                    .extend_pairs(pairs)
                    .append_pair("protocol", "ctox-v4");
                Some(u.to_string())
            })
            .collect()
    });
    Ok((config, provider))
}
