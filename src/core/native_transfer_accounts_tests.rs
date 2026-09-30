use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

fn host(root: &std::path::Path) -> Arc<NativeTransferAccountHost> {
    NativeTransferAccountHost::new(
        root.to_owned(),
        Arc::new(|_| Box::pin(async { Err(host_error()) })),
    )
}

fn account(root: &std::path::Path) -> NativeTransferAccount {
    let scope = NativeDeviceKeyScope {
        target_id: "source-one".into(),
        source_instance_id: "instance-one".into(),
        source_public_identity: format!("ed25519:{}", "a".repeat(64)),
        account_epoch: 4,
    };
    let key = NativeDeviceProofKey::prepare(root, &scope).unwrap();
    NativeTransferAccount {
        version: 1,
        target_id: scope.target_id,
        instance_id: scope.source_instance_id,
        public_identity: scope.source_public_identity,
        account_epoch: scope.account_epoch,
        principal: NativeBusinessDataPrincipal {
            user_id: "native-enrolled-user".into(),
            authorization_epoch: 9,
            device: Some(key.device_identity()),
        },
        active: true,
    }
}

fn save_authority(root: &std::path::Path, account: &NativeTransferAccount) {
    let value = serde_json::to_string(account).unwrap();
    crate::secrets::write_secret_records(
        root,
        &[crate::secrets::SecretRecordWrite {
            scope: AUTHORITY_SCOPE,
            name: &authority_name(&account.target_id),
            value: &value,
            description: None,
            metadata: serde_json::json!({"version":1}),
        }],
    )
    .unwrap();
}

#[tokio::test]
async fn initial_pairing_cannot_replace_an_account_or_its_disconnect_tombstone() {
    let root = tempfile::tempdir().unwrap();
    let mut original = account(root.path());
    let host = host(root.path());
    host.require_new_target(&original.target_id).await.unwrap();
    save_authority(root.path(), &original);
    assert!(host
        .pairing_provider(original.key_scope(), "n".repeat(43))
        .await
        .is_err());
    original.active = false;
    save_authority(root.path(), &original);
    assert!(host.account(&original.target_id).await.unwrap().is_none());
    assert!(host.require_new_target(&original.target_id).await.is_err());
    assert!(host
        .pairing_provider(original.key_scope(), "n".repeat(43))
        .await
        .is_err());
}

#[tokio::test]
async fn malformed_pairing_secret_cannot_generate_a_native_key() {
    let root = tempfile::tempdir().unwrap();
    let scope = NativeDeviceKeyScope {
        target_id: "new-target".into(),
        source_instance_id: "source".into(),
        source_public_identity: format!("ed25519:{}", "a".repeat(64)),
        account_epoch: 1,
    };
    assert!(host(root.path())
        .pairing_provider(scope.clone(), "renderer.bearer.token".into())
        .await
        .is_err());
    assert!(NativeDeviceProofKey::load(root.path(), &scope).is_err());
}

fn save_credentials(
    root: &std::path::Path,
    expected: &NativeTransferAccount,
    bound: &NativeTransferAccount,
) {
    let value = serde_json::to_string(&StoredCredentials {
        version: 1,
        account: bound.clone(),
        capability_token: "isolated-native-fixture-capability".into(),
    })
    .unwrap();
    crate::secrets::write_secret_records(
        root,
        &[crate::secrets::SecretRecordWrite {
            scope: CREDENTIAL_SCOPE,
            name: &expected.credential_name().unwrap(),
            value: &value,
            description: None,
            metadata: serde_json::json!({"version":1}),
        }],
    )
    .unwrap();
}

#[tokio::test]
async fn reopened_host_resolves_pins_and_callbacks_without_loading_credentials() {
    let root = tempfile::tempdir().unwrap();
    let account = account(root.path());
    save_authority(root.path(), &account);
    let first = host(root.path());
    let reopened = host(root.path());
    let saved = first
        .saved_target(&account.target_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        saved,
        reopened
            .saved_target(&account.target_id)
            .await
            .unwrap()
            .unwrap()
    );
    assert_eq!(saved.account_epoch, 4);
    assert_eq!(
        reopened
            .current_principal(&account.target_id)
            .await
            .unwrap(),
        Some(account.principal.clone())
    );
    assert!(reopened
        .providers()
        .await
        .unwrap()
        .contains_key(&account.target_id));
    // No capability record exists: public resolution must still work.
    assert!(reopened.credentials(&account, &"n".repeat(43)).is_err());
}

#[test]
fn credentials_load_the_enrolled_signer_and_produce_a_verifiable_fresh_proof() {
    let root = tempfile::tempdir().unwrap();
    let account = account(root.path());
    save_authority(root.path(), &account);
    save_credentials(root.path(), &account, &account);
    let nonce = "n".repeat(43);
    let credentials = host(root.path()).credentials(&account, &nonce).unwrap();
    assert_eq!(
        credentials.capability_token,
        "isolated-native-fixture-capability"
    );
    let proof = credentials.device_proof.unwrap();
    let mut public = vec![4];
    public.extend(URL_SAFE_NO_PAD.decode(proof.public_x).unwrap());
    public.extend(URL_SAFE_NO_PAD.decode(proof.public_y).unwrap());
    ring::signature::UnparsedPublicKey::new(&ring::signature::ECDSA_P256_SHA256_FIXED, public)
        .verify(
            nonce.as_bytes(),
            &URL_SAFE_NO_PAD.decode(proof.signature).unwrap(),
        )
        .unwrap();
}

#[tokio::test]
async fn inactive_or_switched_account_denies_the_original_credential_binding() {
    let root = tempfile::tempdir().unwrap();
    let original = account(root.path());
    save_authority(root.path(), &original);
    save_credentials(root.path(), &original, &original);
    let host = host(root.path());
    let mut changed = original.clone();
    changed.active = false;
    save_authority(root.path(), &changed);
    assert!(host
        .saved_target(&original.target_id)
        .await
        .unwrap()
        .is_none());
    assert!(host.credentials(&original, &"n".repeat(43)).is_err());
    changed.active = true;
    changed.account_epoch += 1;
    save_authority(root.path(), &changed);
    assert!(host.credentials(&original, &"n".repeat(43)).is_err());
    assert_ne!(
        host.saved_target(&original.target_id)
            .await
            .unwrap()
            .unwrap()
            .account_epoch,
        original.account_epoch
    );
}

#[test]
fn foreign_credential_tuple_and_missing_signer_fail_without_reenrollment() {
    let root = tempfile::tempdir().unwrap();
    let original = account(root.path());
    save_authority(root.path(), &original);
    let mut foreign = original.clone();
    foreign.principal.user_id = "another-account".into();
    save_credentials(root.path(), &original, &foreign);
    assert!(host(root.path())
        .credentials(&original, &"n".repeat(43))
        .is_err());
    let mut missing_key = original.clone();
    missing_key.account_epoch += 1;
    save_authority(root.path(), &missing_key);
    save_credentials(root.path(), &missing_key, &missing_key);
    assert!(host(root.path())
        .credentials(&missing_key, &"n".repeat(43))
        .is_err());
    assert!(NativeDeviceProofKey::load(root.path(), &missing_key.key_scope()).is_err());
}

#[tokio::test]
async fn corrupt_public_record_does_not_fall_back_to_credentials_or_an_old_account() {
    let root = tempfile::tempdir().unwrap();
    let account = account(root.path());
    save_authority(root.path(), &account);
    save_credentials(root.path(), &account, &account);
    crate::secrets::write_secret_records(
        root.path(),
        &[crate::secrets::SecretRecordWrite {
            scope: AUTHORITY_SCOPE,
            name: &authority_name(&account.target_id),
            value: "{broken",
            description: None,
            metadata: serde_json::json!({"version":1}),
        }],
    )
    .unwrap();
    let host = host(root.path());
    assert!(host.saved_target(&account.target_id).await.is_err());
    assert!(host.providers().await.is_err());
    assert!(host.credentials(&account, &"n".repeat(43)).is_err());
}

fn provision_reply(account: &NativeTransferAccount) -> NativeTransferProvisionReply {
    let now = chrono::Utc::now().timestamp_millis();
    let token = "source-browser-role-token".to_owned();
    NativeTransferProvisionReply {
        version: 1, source_public_identity: account.public_identity.clone(),
        source_instance_id: account.instance_id.clone(), principal: account.principal.clone(),
        capability_token: "source-renewed-native-capability".into(), capability_expires_at_ms: now + 60_000,
        routing: NativeTransferRouting {
            room: format!("ctox-business-os:{}:source-room", account.instance_id),
            signaling_urls: vec!["wss://signaling.ctox.dev/signal?role=ctox_instance&token=foreign&native_peer_id=foreign".into()],
            browser_token_hash: format!("{:x}", Sha256::digest(token.as_bytes())), browser_token: token,
            native_token_hash: "a".repeat(64), auth_version: "ctox-role-bound-v1".into(),
            ice_servers: Vec::new(), refreshed_at_ms: now, refresh_after_ms: now + 120_000,
            expires_at_ms: now + 240_000,
        },
    }
}

#[tokio::test]
async fn authenticated_tuple_reopens_and_stale_refresh_cannot_undo_disconnect() {
    let root = tempfile::tempdir().unwrap();
    let account = account(root.path());
    let host = host(root.path());
    host.commit_authenticated(&account.key_scope(), None, None, provision_reply(&account))
        .unwrap();
    let before = serde_json::to_string(&account).unwrap();
    let route = host
        .read_record(ROUTING_SCOPE, &account.credential_name().unwrap())
        .unwrap()
        .unwrap();
    let ice = host.routing(&account.target_id).await.unwrap().ice();
    // NativeSyncOptions treats [] as unspecified, enabling default public STUN.
    // An authenticated source with no ICE servers must explicitly disable it.
    assert_eq!(ice.len(), 1);
    assert!(ice[0].urls.is_empty());
    assert!(ice[0].username.is_empty());
    assert!(ice[0].credential.is_empty());
    assert_eq!(
        host.read_routing(&account).unwrap().room,
        format!("ctox-business-os:{}:source-room", account.instance_id)
    );
    assert_eq!(
        host.credentials(&account, &"n".repeat(43))
            .unwrap()
            .capability_token,
        "source-renewed-native-capability"
    );
    host.revoke(account.clone()).await.unwrap();
    assert!(host.read_account(&account.target_id).unwrap().is_none());
    assert!(!crate::secrets::secret_exists(
        root.path(),
        CREDENTIAL_SCOPE,
        &account.credential_name().unwrap()
    )
    .unwrap());
    assert!(!crate::secrets::secret_exists(
        root.path(),
        ROUTING_SCOPE,
        &account.credential_name().unwrap()
    )
    .unwrap());
    assert!(host
        .commit_authenticated(
            &account.key_scope(),
            Some(&before),
            Some(&route),
            provision_reply(&account)
        )
        .is_err());
    assert!(host.read_account(&account.target_id).unwrap().is_none());
}

#[test]
fn route_cas_prevents_older_refresh_overwriting_new_credentials() {
    let root = tempfile::tempdir().unwrap();
    let account = account(root.path());
    let host = host(root.path());
    host.commit_authenticated(&account.key_scope(), None, None, provision_reply(&account))
        .unwrap();
    let before = serde_json::to_string(&account).unwrap();
    let route = host
        .read_record(ROUTING_SCOPE, &account.credential_name().unwrap())
        .unwrap()
        .unwrap();
    let mut newer = provision_reply(&account);
    newer.routing.room.push_str("-rotated");
    newer.capability_token = "newer-source-credential".into();
    host.commit_authenticated(&account.key_scope(), Some(&before), Some(&route), newer)
        .unwrap();
    assert!(host
        .commit_authenticated(
            &account.key_scope(),
            Some(&before),
            Some(&route),
            provision_reply(&account)
        )
        .is_err());
    assert_eq!(
        host.credentials(&account, &"n".repeat(43))
            .unwrap()
            .capability_token,
        "newer-source-credential"
    );
}

#[test]
fn foreign_source_missing_key_and_changed_principal_never_persist_authority() {
    let root = tempfile::tempdir().unwrap();
    let account = account(root.path());
    let host = host(root.path());
    let mut wrong = provision_reply(&account);
    wrong.source_instance_id = "another-instance".into();
    assert!(host
        .commit_authenticated(&account.key_scope(), None, None, wrong)
        .is_err());
    let mut scope = account.key_scope();
    scope.account_epoch += 1;
    assert!(host
        .commit_authenticated(&scope, None, None, provision_reply(&account))
        .is_err());
    assert!(host.read_account(&account.target_id).unwrap().is_none());
    host.commit_authenticated(&account.key_scope(), None, None, provision_reply(&account))
        .unwrap();
    let before = serde_json::to_string(&account).unwrap();
    let route = host
        .read_record(ROUTING_SCOPE, &account.credential_name().unwrap())
        .unwrap()
        .unwrap();
    let mut wrong = provision_reply(&account);
    wrong.principal.user_id = "another-user".into();
    assert!(host
        .commit_authenticated(&account.key_scope(), Some(&before), Some(&route), wrong)
        .is_err());
    assert!(host.read_account(&account.target_id).unwrap().as_ref() == Some(&account));
}

#[test]
fn signaling_refresh_preserves_browser_role_and_rejects_expiry_and_bad_commitments() {
    let root = tempfile::tempdir().unwrap();
    let account = account(root.path());
    let mut routing = provision_reply(&account).routing;
    let now = routing.refreshed_at_ms;
    let first = url::Url::parse(
        &routing
            .signaling_at(&account.instance_id, "recipient-1", now)
            .unwrap()[0],
    )
    .unwrap();
    let later = url::Url::parse(
        &routing
            .signaling_at(&account.instance_id, "recipient-1", now + 2000)
            .unwrap()[0],
    )
    .unwrap();
    let query: std::collections::BTreeMap<_, _> = first.query_pairs().collect();
    let second: std::collections::BTreeMap<_, _> = later.query_pairs().collect();
    assert_eq!(first.path(), "/v2");
    assert_eq!(query["role"], "browser");
    assert_eq!(query["token"], "source-browser-role-token");
    assert!(!query.contains_key("native_peer_id"));
    assert_ne!(query["token_iat"], second["token_iat"]);
    assert!(routing
        .signaling_at(&account.instance_id, "recipient-1", routing.expires_at_ms)
        .is_err());
    routing.native_token_hash = routing.browser_token_hash.clone();
    assert!(routing
        .signaling_at(&account.instance_id, "recipient-1", now)
        .is_err());
}

#[tokio::test]
async fn explicit_newer_account_erases_old_tuple_and_rejects_stale_disconnect() {
    let root = tempfile::tempdir().unwrap();
    let original = account(root.path());
    let host = host(root.path());
    host.commit_authenticated(
        &original.key_scope(),
        None,
        None,
        provision_reply(&original),
    )
    .unwrap();
    let before = serde_json::to_string(&original).unwrap();
    let old_name = original.credential_name().unwrap();
    let route = host.read_record(ROUTING_SCOPE, &old_name).unwrap().unwrap();
    let mut next = original.clone();
    next.account_epoch += 1;
    next.principal.user_id = "explicit-next-user".into();
    next.principal.device = Some(
        NativeDeviceProofKey::prepare(root.path(), &next.key_scope())
            .unwrap()
            .device_identity(),
    );
    host.commit_authenticated(
        &next.key_scope(),
        Some(&before),
        Some(&route),
        provision_reply(&next),
    )
    .unwrap();
    assert!(host.revoke(original.clone()).await.is_err());
    assert!(host.read_account(&next.target_id).unwrap().as_ref() == Some(&next));
    assert!(!crate::secrets::secret_exists(root.path(), CREDENTIAL_SCOPE, &old_name).unwrap());
    assert!(!crate::secrets::secret_exists(root.path(), ROUTING_SCOPE, &old_name).unwrap());
    assert!(host.credentials(&original, &"n".repeat(43)).is_err());
}

fn save_route_snapshot(
    root: &std::path::Path,
    account: &NativeTransferAccount,
    routing: NativeTransferRouting,
) {
    crate::secrets::write_secret_record(
        root,
        ROUTING_SCOPE,
        &account.credential_name().unwrap(),
        &serde_json::to_string(&StoredRouting {
            account: account.clone(),
            routing,
        })
        .unwrap(),
        None,
        serde_json::json!({"version":1}),
    )
    .unwrap();
}

fn expired_snapshot(account: &NativeTransferAccount) -> NativeTransferRouting {
    let mut routing = provision_reply(account).routing;
    let now = chrono::Utc::now().timestamp_millis();
    routing.refreshed_at_ms = now - 120_000;
    routing.refresh_after_ms = now - 90_000;
    routing.expires_at_ms = now - 60_000;
    routing.ice_servers = vec![crate::native_transfer_routing::NativeTransferIceServer {
        urls: vec!["turn:127.0.0.1:3478".into()],
        username: format!("{}:recipient", (now - 30_000) / 1000),
        credential: "expired-turn-secret".into(),
    }];
    routing
}

#[tokio::test]
async fn expired_routing_bootstraps_without_ice_or_mutating_authority_and_stops_on_disconnect() {
    let root = tempfile::tempdir().unwrap();
    let account = account(root.path());
    let options_root = root.path().to_owned();
    let host = NativeTransferAccountHost::new(
        root.path().to_owned(),
        Arc::new(move |_| {
            let root = options_root.clone();
            Box::pin(async move {
                crate::transfers_native::test_query_options(&root)
                    .await
                    .map_err(|_| host_error())
            })
        }),
    );
    host.commit_authenticated(&account.key_scope(), None, None, provision_reply(&account))
        .unwrap();
    assert!(host
        .recovery_options(&account.target_id)
        .await
        .unwrap()
        .is_none());
    save_route_snapshot(root.path(), &account, expired_snapshot(&account));
    let before = host
        .read_record(ROUTING_SCOPE, &account.credential_name().unwrap())
        .unwrap();
    assert!(host
        .native_options_with_deadline(&account.target_id)
        .await
        .is_err());
    let options = host
        .recovery_options(&account.target_id)
        .await
        .unwrap()
        .unwrap();
    assert!(options.local_session_provider.is_none());
    assert!(options.collections.is_empty());
    assert_eq!(options.ice_servers.len(), 1);
    assert!(options.ice_servers[0].urls.is_empty());
    assert!(options.ice_servers[0].username.is_empty());
    assert!(options.ice_servers[0].credential.is_empty());
    let urls = (options.signaling_urls)();
    assert_eq!(urls.len(), 1);
    let url = url::Url::parse(&urls[0]).unwrap();
    let query = url
        .query_pairs()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(query.get("role").map(|v| v.as_ref()), Some("browser"));
    assert_eq!(
        query.get("instance_id").map(|v| v.as_ref()),
        Some(account.instance_id.as_str())
    );
    assert!(!urls[0].contains("expired-turn-secret"));
    assert_eq!(
        host.read_record(ROUTING_SCOPE, &account.credential_name().unwrap())
            .unwrap(),
        before
    );
    assert_eq!(
        host.account(&account.target_id).await.unwrap(),
        Some(account.clone())
    );
    assert!(
        host.native_options_with_deadline(&account.target_id)
            .await
            .is_err(),
        "bootstrap cannot extend payload expiry"
    );
    let mut changed = expired_snapshot(&account);
    changed.room.push_str("-rotated");
    save_route_snapshot(root.path(), &account, changed);
    assert!(
        (options.signaling_urls)().is_empty(),
        "in-flight bootstrap cannot switch rendezvous"
    );
    save_route_snapshot(root.path(), &account, expired_snapshot(&account));
    host.revoke(account.clone()).await.unwrap();
    assert!((options.signaling_urls)().is_empty());
    assert!(host.recovery_options(&account.target_id).await.is_err());
    options.database.close().await.unwrap();
}

#[tokio::test]
async fn invalid_retained_routing_cannot_be_laundered_into_recovery() {
    let root = tempfile::tempdir().unwrap();
    let account = account(root.path());
    save_authority(root.path(), &account);
    let host = host(root.path());
    let valid = expired_snapshot(&account);
    assert!(valid
        .retained_rendezvous(&account.instance_id, chrono::Utc::now().timestamp_millis())
        .is_ok());
    assert!(valid
        .validate(&account.instance_id, chrono::Utc::now().timestamp_millis())
        .is_err());
    for mutation in 0..4 {
        let mut route = valid.clone();
        match mutation {
            0 => route.browser_token_hash = "b".repeat(64),
            1 => route.room = "ctox-business-os:foreign:room".into(),
            2 => route.refresh_after_ms = route.expires_at_ms,
            _ => route.ice_servers[0].username = "1:expired-before-snapshot".into(),
        }
        assert!(route
            .retained_rendezvous(&account.instance_id, chrono::Utc::now().timestamp_millis())
            .is_err());
        save_route_snapshot(root.path(), &account, route);
        assert!(host.recovery_options(&account.target_id).await.is_err());
    }
}
