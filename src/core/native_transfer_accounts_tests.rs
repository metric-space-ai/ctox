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
