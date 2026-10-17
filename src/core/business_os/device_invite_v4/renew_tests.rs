use super::*;
use ring::signature::{Ed25519KeyPair, KeyPair};

fn device_key() -> Ed25519KeyPair {
    Ed25519KeyPair::from_seed_unchecked(&(0u8..32).collect::<Vec<_>>()).unwrap()
}
fn signed(key: &Ed25519KeyPair, challenge: &Challenge, device: &str) -> Renew {
    let mut request = Renew {
        instance_id: "inst-1".into(),
        device_id: device.into(),
        nonce: challenge.nonce.clone(),
        signature: String::new(),
    };
    request.signature = URL_SAFE_NO_PAD.encode(
        key.sign(&renew_message(
            &request.instance_id,
            &request.device_id,
            &request.nonce,
        ))
        .as_ref(),
    );
    request
}
async fn paired() -> (
    tempfile::TempDir,
    ctox_store::testkit::StoreFixture,
    Invites,
    Ed25519KeyPair,
) {
    let (root, owner, invites) = super::super::tests::fixture().await;
    let keypair = device_key();
    let public = keypair.public_key().as_ref();
    let invite = invites
        .create("owner".into(), "admin".into(), 60_000)
        .await
        .unwrap();
    let secret = URL_SAFE_NO_PAD.decode(&invite.one_time_secret).unwrap();
    let request = Redeem {
        invite_id: invite.invite_id.clone(),
        proof: URL_SAFE_NO_PAD.encode(
            hmac::sign(
                &hmac::Key::new(hmac::HMAC_SHA256, &key(&secret)),
                &message(&invite.invite_id, public),
            )
            .as_ref(),
        ),
        device_pubkey: URL_SAFE_NO_PAD.encode(public),
        device_label: "Real key fixture".into(),
    };
    let prepared = invites.prepare(request).await.unwrap();
    invites.consume(prepared, "device-1".into()).await.unwrap();
    (root, owner, invites, keypair)
}
#[test]
fn workjet_811_812_proof_and_renew_vectors_match() {
    let secret = (0u8..32).collect::<Vec<_>>();
    let id = "0b6f6f5e-3c1a-4d2e-9f00-123456789abc";
    for (public, expected) in [
        ([7; 32], "o7CVwRHD3c7qpy0Jdm9TYcNJLtqSKmAkrncPh5cdVTQ"),
        ([8; 32], "M6LQyzG7EqbxYwbHa9bLGvpDJwD_S5ql-9bTg7_QRJQ"),
    ] {
        assert_eq!(
            URL_SAFE_NO_PAD.encode(
                hmac::sign(
                    &hmac::Key::new(hmac::HMAC_SHA256, &key(&secret)),
                    &message(id, &public)
                )
                .as_ref()
            ),
            expected
        );
    }
    let challenge = Challenge {
        kind: "pairing_challenge",
        nonce: "bm9uY2UtMQ".into(),
        expires_at_ms: 0,
    };
    let request = signed(&device_key(), &challenge, "device-1");
    assert_eq!(
        renew_message("inst-1", "device-1", "bm9uY2UtMQ"),
        b"ctox-v4-device-renew\ninst-1device-1bm9uY2UtMQ"
    );
    assert_eq!(
        request.signature,
        "i9cO5j1_sH4UompVuzH64v2JaLVWRxRuVtE0_LAxo6JKt-rTonx6tAyEefg_EbH-X4F__dfGmD4JhA5hf0R7AA"
    );
    let raw: [u8; 32] = device_key().public_key().as_ref().try_into().unwrap();
    check_signature(&request, &raw).unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn renew_requires_registered_offer_id_and_signature_and_consumes_once() {
    let (_root, owner, invites, keypair) = paired().await;
    let challenge = invites.challenge("inst-1".into()).await.unwrap();
    let mut forged = signed(&keypair, &challenge, "device-1");
    forged.signature = URL_SAFE_NO_PAD.encode([0; 64]);
    assert_eq!(
        invites.prepare_renew("inst-1".into(), forged).await.err(),
        Some(RenewReject::SignatureInvalid),
        "wrong signature accepted"
    );
    assert_eq!(
        invites
            .prepare_renew(
                "inst-1".into(),
                signed(&keypair, &challenge, "not-the-issued-offer-id")
            )
            .await
            .err(),
        Some(RenewReject::DeviceUnknown),
        "unissued device identity accepted"
    );
    let a = invites
        .prepare_renew("inst-1".into(), signed(&keypair, &challenge, "device-1"))
        .await
        .unwrap();
    let b = invites
        .prepare_renew("inst-1".into(), signed(&keypair, &challenge, "device-1"))
        .await
        .unwrap();
    let (first, second) = tokio::join!(
        invites.consume_renew(a, "device-2".into()),
        invites.consume_renew(b, "device-3".into())
    );
    assert!(first.is_ok() ^ second.is_ok(), "renew challenge reused");
    assert_eq!(
        first.err().or(second.err()),
        Some(RenewReject::ChallengeExpired)
    );
    let challenge = invites.challenge("inst-1".into()).await.unwrap();
    // The successful response might have been lost. The last context actually
    // received remains usable with a fresh challenge and possession proof.
    invites
        .prepare_renew("inst-1".into(), signed(&keypair, &challenge, "device-1"))
        .await
        .unwrap();
    drop(invites);
    owner.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revoked_device_cannot_renew_even_after_preparation() {
    let (_root, owner, invites, keypair) = paired().await;
    let challenge = invites.challenge("inst-1".into()).await.unwrap();
    let prepared = invites
        .prepare_renew("inst-1".into(), signed(&keypair, &challenge, "device-1"))
        .await
        .unwrap();
    invites.revoke_device("device-1".into()).await.unwrap();
    assert_eq!(
        invites
            .consume_renew(prepared, "device-2".into())
            .await
            .err(),
        Some(RenewReject::DeviceRevoked),
        "revoked device renewed"
    );
    drop(invites);
    owner.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expired_challenge_is_rechecked_before_publish() {
    let (_root, owner, invites, keypair) = paired().await;
    let challenge = invites.challenge("inst-1".into()).await.unwrap();
    let prepared = invites
        .prepare_renew("inst-1".into(), signed(&keypair, &challenge, "device-1"))
        .await
        .unwrap();
    let nonce = challenge.nonce.clone();
    let now = clock().unwrap();
    owner.write(Default::default(),move |tx| {tx.execute("UPDATE _workjet_renew_challenges_v4 SET created_ms=?2,expires_ms=?3 WHERE nonce=?1",params![nonce,now-CHALLENGE_TTL_MS-1,now-1])?;Ok(())}).await.unwrap();
    assert_eq!(
        invites
            .consume_renew(prepared, "device-2".into())
            .await
            .err(),
        Some(RenewReject::ChallengeExpired),
        "expired challenge accepted"
    );
    drop(invites);
    owner.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn renew_attempt_budget_limits_unauthenticated_work() {
    let (_root, owner, invites, keypair) = paired().await;
    let challenge = invites.challenge("inst-1".into()).await.unwrap();
    let now = clock().unwrap();
    owner
        .write(Default::default(), move |tx| {
            tx.execute(
                "UPDATE _workjet_pairing_budget_v4 SET window_ms=?1,attempts=120 WHERE singleton=1",
                [now],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(
        invites
            .prepare_renew("inst-1".into(), signed(&keypair, &challenge, "device-1"))
            .await
            .err(),
        Some(RenewReject::RateLimited),
        "renew attempt budget bypassed"
    );
    drop(invites);
    owner.shutdown().await.unwrap();
}
