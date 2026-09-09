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
fn requires_pinned_key_instance_fresh_challenge_and_the_actual_channel() {
    let signer = key();
    let challenge = fresh_challenge().unwrap();
    let channel = "a".repeat(64);
    let proof = signer
        .attest_business_data_identity("instance", &challenge, &channel, Some(principal()))
        .unwrap();
    verify_peer_identity(
        &proof,
        &signer.public_identity(),
        "instance",
        &challenge,
        &channel,
    )
    .unwrap();
    assert!(verify_peer_identity(
        &proof,
        &key().public_identity(),
        "instance",
        &challenge,
        &channel
    )
    .is_err());
    assert!(verify_peer_identity(
        &proof,
        &signer.public_identity(),
        "other",
        &challenge,
        &channel
    )
    .is_err());
    assert!(verify_peer_identity(
        &proof,
        &signer.public_identity(),
        "instance",
        &fresh_challenge().unwrap(),
        &channel
    )
    .is_err());
    // Even a genuine fresh signature relayed from the pinned source cannot
    // authenticate the attacker's different DTLS channel.
    assert!(verify_peer_identity(
        &proof,
        &signer.public_identity(),
        "instance",
        &challenge,
        &"b".repeat(64)
    )
    .is_err());
    for field in ["user", "epoch", "device", "principal", "channel"] {
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
            "channel" => changed.channel_binding = "b".repeat(64),
            _ => changed.principal = None,
        }
        assert!(verify_peer_identity(
            &changed,
            &signer.public_identity(),
            "instance",
            &challenge,
            &changed.channel_binding
        )
        .is_err());
    }
}
#[test]
fn anonymous_proof_cannot_be_upgraded_to_a_principal() {
    let signer = key();
    let challenge = fresh_challenge().unwrap();
    let channel = "a".repeat(64);
    let mut proof = signer
        .attest_business_data_identity("instance", &challenge, &channel, None)
        .unwrap();
    verify_peer_identity(
        &proof,
        &signer.public_identity(),
        "instance",
        &challenge,
        &channel,
    )
    .unwrap();
    assert!(proof.principal.is_none());
    proof.principal = Some(principal());
    assert!(verify_peer_identity(
        &proof,
        &signer.public_identity(),
        "instance",
        &challenge,
        &channel
    )
    .is_err());
    assert!(signer
        .attest_business_data_identity("instance", "not-a-nonce", &channel, None)
        .is_err());
    assert!(signer
        .attest_business_data_identity("instance", &challenge, "not-a-channel", None)
        .is_err());
    let mut unsafe_epoch = principal();
    unsafe_epoch.authorization_epoch = 9_007_199_254_740_992;
    assert!(signer
        .attest_business_data_identity("instance", &challenge, &channel, Some(unsafe_epoch))
        .is_err());
}
