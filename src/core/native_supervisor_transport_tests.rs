use super::*;
use crate::native_data_device::{NativeDeviceKeyScope, NativeDeviceProofKey};
use ctox_sync::business_data_contract::NativeBusinessDataPrincipal;
use sha2::{Digest, Sha256};

fn enrolled(root: &Path) -> NativeTransferAccount {
    let scope = NativeDeviceKeyScope {
        target_id: "source-fixture".into(),
        source_instance_id: "holder-fixture".into(),
        source_public_identity: format!("ed25519:{}", "a".repeat(64)),
        account_epoch: 3,
    };
    let key = NativeDeviceProofKey::prepare(root, &scope).unwrap();
    NativeTransferAccount {
        version: 1,
        target_id: scope.target_id,
        public_identity: scope.source_public_identity,
        instance_id: scope.source_instance_id,
        account_epoch: scope.account_epoch,
        principal: NativeBusinessDataPrincipal {
            user_id: "native-source-user".into(),
            authorization_epoch: 4,
            device: Some(key.device_identity()),
        },
        active: true,
    }
}
fn store(
    root: &Path,
    authority: &NativeTransferAccount,
    credential_account: &NativeTransferAccount,
    token: &str,
) {
    let authority_name = format!("{:x}", Sha256::digest(authority.target_id.as_bytes()));
    let credential_name = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(credential_account).unwrap())
    );
    let authority = serde_json::to_string(authority).unwrap();
    let credential =
        json!({"version":1,"account":credential_account,"capabilityToken":token}).to_string();
    crate::secrets::write_secret_records(
        root,
        &[
            crate::secrets::SecretRecordWrite {
                scope: "ctox-native-business-data-accounts",
                name: &authority_name,
                value: &authority,
                description: None,
                metadata: json!({"version":1}),
            },
            crate::secrets::SecretRecordWrite {
                scope: "ctox-native-business-data-account-credentials",
                name: &credential_name,
                value: &credential,
                description: None,
                metadata: json!({"version":1}),
            },
        ],
    )
    .unwrap();
}
fn guard(root: &Path, original: &NativeTransferAccount) -> Arc<EnrollmentGuard> {
    let host = NativeTransferAccountHost::new(
        root.to_owned(),
        Arc::new(|_| {
            Box::pin(async {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "fixture has no transport",
                ))
            })
        }),
    );
    EnrollmentGuard::capture(
        host,
        original.clone(),
        chrono::Utc::now().timestamp_millis() + 60_000,
    )
    .unwrap()
}

#[tokio::test]
async fn credential_rotation_fences_a_zero_byte_pending_publication() {
    let root = tempfile::tempdir().unwrap();
    let account = enrolled(root.path());
    store(root.path(), &account, &account, "isolated-fixture-original");
    let guard = guard(root.path(), &account);
    let (mut writer, mut reader) = tokio::io::duplex(1);
    writer.write_all(b"x").await.unwrap(); // Fill physical capacity before attempting this frame.
    let mut publish = Box::pin(writer.write_all(b"late-frame"));
    std::future::poll_fn(|cx| {
        assert!(guard
            .current(|| Ok(publish.as_mut().poll(cx)))
            .unwrap()
            .is_pending());
        Poll::Ready(())
    })
    .await;
    store(root.path(), &account, &account, "isolated-fixture-rotated");
    let mut previous = [0];
    reader.read_exact(&mut previous).await.unwrap();
    std::future::poll_fn(|cx| {
        assert!(guard.current(|| Ok(publish.as_mut().poll(cx))).is_err());
        Poll::Ready(())
    })
    .await;
    drop(publish);
    drop(writer);
    let mut remaining = Vec::new();
    reader.read_to_end(&mut remaining).await.unwrap();
    assert!(remaining.is_empty());
}

#[test]
fn account_epoch_pairing_and_explicit_retirement_never_rebind_the_guard() {
    let root = tempfile::tempdir().unwrap();
    let original = enrolled(root.path());
    store(
        root.path(),
        &original,
        &original,
        "isolated-fixture-original",
    );
    let captured = guard(root.path(), &original);
    assert_eq!(captured.current(|| Ok(7)).unwrap(), 7);
    let mut changed = original.clone();
    changed.account_epoch += 1;
    store(
        root.path(),
        &changed,
        &original,
        "isolated-fixture-original",
    );
    assert!(captured
        .current(|| panic!("substituted account published"))
        .is_err());
    store(
        root.path(),
        &original,
        &original,
        "isolated-fixture-original",
    );
    let active = guard(root.path(), &original);
    active.retire();
    assert!(active
        .current(|| panic!("retired source published"))
        .is_err());
    let mut unpaired = original.clone();
    unpaired
        .principal
        .device
        .as_mut()
        .unwrap()
        .pairing_id
        .push_str("-other");
    store(
        root.path(),
        &unpaired,
        &original,
        "isolated-fixture-original",
    );
    assert!(captured
        .current(|| panic!("substituted pairing published"))
        .is_err());
}

#[test]
fn consumer_reply_must_name_the_original_native_device_not_a_computer_label() {
    let root = tempfile::tempdir().unwrap();
    let original = enrolled(root.path());
    let device = original.principal.device.as_ref().unwrap();
    let valid = json!({"version":1,"consumer":{"actorUserId":original.principal.user_id,"actorEpoch":original.principal.authorization_epoch,
        "pairingId":device.pairing_id,"deviceId":device.device_id,"proofKeyThumbprint":device.proof_key_thumbprint,
        "ownerUserId":"owner-fixture","computerId":"native-computer","computerRevision":"1-current","pairingRevision":"original-association"}});
    assert!(association_matches(&original, &valid));
    for field in [
        "actorUserId",
        "actorEpoch",
        "pairingId",
        "deviceId",
        "proofKeyThumbprint",
        "ownerUserId",
        "computerId",
        "computerRevision",
        "pairingRevision",
    ] {
        let mut changed = valid.clone();
        changed["consumer"][field] = Value::Null;
        assert!(!association_matches(&original, &changed), "{field}");
    }
    let mut replaced = valid.clone();
    replaced["consumer"]["pairingId"] = json!("desktop-guest-pairing");
    assert!(!association_matches(&original, &replaced));
}

#[tokio::test]
async fn missing_native_enrollment_opens_no_database_or_ipc_listener() {
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    assert!(Source::open(root.path(), "not-enrolled", private.path())
        .await
        .is_err());
    assert!(!private.path().join("peer.sqlite3").exists());
    assert!(!private.path().join("control.sock").exists());
}

#[test]
fn ipc_envelope_cannot_select_an_account_or_construct_consumer_authority() {
    let valid = json!({"version":1,"requestId":"actual-correlation","params":[{"version":1,"action":"poll"}]});
    assert!(serde_json::from_value::<SourceRequest>(valid.clone())
        .unwrap()
        .valid());
    for field in [
        "targetId",
        "consumer",
        "capabilityToken",
        "account",
        "computerId",
        "ownerUserId",
    ] {
        let mut supplied = valid.clone();
        supplied[field] = json!("caller-supplied");
        assert!(serde_json::from_value::<SourceRequest>(supplied).is_err());
    }
}
