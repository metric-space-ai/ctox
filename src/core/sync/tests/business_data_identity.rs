use ctox_sync::{
    authority::auth::SigningIdentity,
    business_data_contract::{NativeBusinessDataDeviceIdentity, NativeBusinessDataPrincipal},
    business_data_identity::{fresh_challenge, verify_peer_identity},
};
fn key() -> SigningIdentity {
    SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap()
}
fn principal() -> NativeBusinessDataPrincipal {
    NativeBusinessDataPrincipal {
        user_id: "user".into(),
        authorization_epoch: 3,
        device: Some(NativeBusinessDataDeviceIdentity {
            pairing_id: "pairing".into(),
            device_id: "device".into(),
            proof_key_thumbprint: "thumbprint".into(),
        }),
    }
}
#[test]
fn requires_pinned_key_instance_and_fresh_exchange_challenge() {
    let signer = key();
    let challenge = fresh_challenge().unwrap();
    let proof = signer
        .attest_business_data_identity("instance", &challenge, Some(principal()))
        .unwrap();
    verify_peer_identity(&proof, &signer.public_identity(), "instance", &challenge).unwrap();
    assert!(
        verify_peer_identity(&proof, &key().public_identity(), "instance", &challenge).is_err()
    );
    assert!(verify_peer_identity(&proof, &signer.public_identity(), "other", &challenge).is_err());
    assert!(verify_peer_identity(
        &proof,
        &signer.public_identity(),
        "instance",
        &fresh_challenge().unwrap()
    )
    .is_err());
    for field in ["user", "epoch", "device", "principal"] {
        let mut changed = proof.clone();
        match field {
            "user" => changed.principal.as_mut().unwrap().user_id = "other".into(),
            "epoch" => changed.principal.as_mut().unwrap().authorization_epoch += 1,
            "device" => {
                changed
                    .principal
                    .as_mut()
                    .unwrap()
                    .device
                    .as_mut()
                    .unwrap()
                    .device_id = "other".into()
            }
            _ => changed.principal = None,
        }
        assert!(
            verify_peer_identity(&changed, &signer.public_identity(), "instance", &challenge)
                .is_err()
        );
    }
}
#[test]
fn anonymous_proof_cannot_be_upgraded_to_a_principal() {
    let signer = key();
    let challenge = fresh_challenge().unwrap();
    let mut proof = signer
        .attest_business_data_identity("instance", &challenge, None)
        .unwrap();
    verify_peer_identity(&proof, &signer.public_identity(), "instance", &challenge).unwrap();
    assert!(proof.principal.is_none());
    proof.principal = Some(principal());
    assert!(
        verify_peer_identity(&proof, &signer.public_identity(), "instance", &challenge).is_err()
    );
    assert!(signer
        .attest_business_data_identity("instance", "not-a-nonce", None)
        .is_err());
    let mut unsafe_epoch = principal();
    unsafe_epoch.authorization_epoch = 9_007_199_254_740_992;
    assert!(signer
        .attest_business_data_identity("instance", &challenge, Some(unsafe_epoch))
        .is_err());
}
