use super::*;
use crate::authority::{network::Rpc, Ownership};
use std::sync::Mutex;

fn identity() -> Arc<SigningIdentity> {
    Arc::new(SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap())
}

#[test]
fn instance_packet_signing_requires_encoder_output() {
    let key = identity();
    let value = serde_json::json!({"z":1,"a":{"z":2,"a":3}});
    let packet = CanonicalInstancePacket::encode_stream(value).unwrap();
    let signature = key.sign_instance_packet(&packet);
    UnparsedPublicKey::new(&ED25519, public_key(&key.public_identity()).unwrap())
        .verify(&packet.0, &signature)
        .unwrap();
    assert!(CanonicalInstancePacket::from_stream_bytes(
        b"ctox.sync.v4.stream\0{ \"z\": 1, \"a\": 2 }"
    )
    .is_err());
    assert!(key
        .sign_canonical_bytes(b"{ \"z\": 1, \"a\": 2 }")
        .is_empty());
    assert_eq!(key.sign_canonical_bytes(&packet.0), signature);
}
fn body(sender: &SigningIdentity, recipient: &SigningIdentity) -> Body {
    Body {
        version: 1,
        sender: sender.public_identity(),
        recipient: recipient.public_identity(),
        scope_id: "scope".into(),
        nonce: "a".repeat(32),
        kind: "request".into(),
        data: serde_json::json!({"x":1,"nested":{"b":2,"a":1}}),
    }
}
#[test]
fn signatures_bind_data_sender_recipient_scope_and_protocol() {
    let a = identity();
    let b = identity();
    let c = identity();
    let signed = a.sign(body(&a, &b)).unwrap();
    verify(&signed, &b.public_identity(), "scope", "request").unwrap();
    assert!(verify(&signed, &c.public_identity(), "scope", "request").is_err());
    assert!(verify(&signed, &b.public_identity(), "another-scope", "request").is_err());
    assert!(verify(&signed, &b.public_identity(), "scope", "reply").is_err());
    let mut tampered = a.sign(body(&a, &b)).unwrap();
    tampered.body.data["x"] = 2.into();
    assert!(verify(&tampered, &b.public_identity(), "scope", "request").is_err());
    let forged = c.sign(body(&a, &b)).unwrap();
    assert!(verify(&forged, &b.public_identity(), "scope", "request").is_err());
}
#[test]
fn json_key_order_does_not_change_signed_meaning() {
    let a = identity();
    let b = identity();
    let mut signed = a.sign(body(&a, &b)).unwrap();
    signed.body.data = serde_json::from_str(r#"{"nested":{"a":1,"b":2},"x":1}"#).unwrap();
    verify(&signed, &b.public_identity(), "scope", "request").unwrap();
}
struct ReplayChannel {
    identity: Arc<SigningIdentity>,
    previous: Mutex<Option<Value>>,
}
#[async_trait]
impl ControlChannel for ReplayChannel {
    async fn request(&self, _target: &str, value: Value) -> io::Result<Value> {
        let mut previous = self.previous.lock().unwrap();
        if let Some(value) = previous.as_ref() {
            return Ok(value.clone());
        }
        let request: Envelope = serde_json::from_value(value).unwrap();
        verify(
            &request,
            &self.identity.public_identity(),
            "scope",
            "request",
        )?;
        let response = self.identity.sign(Body {
            version: 1,
            sender: self.identity.public_identity(),
            recipient: request.body.sender,
            scope_id: "scope".into(),
            nonce: request.body.nonce,
            kind: "reply".into(),
            data: serde_json::to_value(Reply::Validate(Err(
                crate::contracts::AuthorityFailure::Rejected {
                    reason: "fixture".into(),
                },
            )))
            .unwrap(),
        })?;
        let value = serde_json::to_value(response).unwrap();
        *previous = Some(value.clone());
        Ok(value)
    }
}
#[tokio::test]
async fn previously_valid_reply_cannot_authorize_a_fresh_request() {
    let a = identity();
    let b = identity();
    let channel = Arc::new(ReplayChannel {
        identity: b.clone(),
        previous: Mutex::new(None),
    });
    let transport = SignedTransport::new(a, "scope".into(), channel);
    let target = Peer {
        identity: b.public_identity(),
        executor: true,
        data_replica: true,
    };
    let packet = Packet {
        version: crate::authority::network::CONTROL_PROTOCOL,
        scope_id: "scope".into(),
        from: 1,
        rpc: Rpc::Validate {
            job_id: "job".into(),
            ownership: Ownership {
                node_id: 1,
                generation: 1,
            },
        },
    };
    assert!(matches!(
        transport.exchange(&target, packet.clone()).await.unwrap(),
        Reply::Validate(Err(_))
    ));
    assert_eq!(
        transport
            .exchange(&target, packet)
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
}
