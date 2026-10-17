use super::*;
use serde_json::{json, Value};

pub(crate) fn hello() -> Hello {
    let fixture: Value = serde_json::from_str(include_str!("hello_fixture.json")).unwrap();
    match serde_json::from_value::<HelloFrame>(fixture["hello"].clone()).unwrap() {
        HelloFrame::Hello(value) => value,
        _ => panic!("fixture type"),
    }
}
pub(crate) fn policy() -> HelloPolicy {
    HelloPolicy {
        instance_id: "test-instance".into(),
        bootstrap_min: 3,
        bootstrap_max: 4,
        app_api_ver: ApiRange {
            min: "4.0".into(),
            max: "4.1".into(),
        },
        capabilities: vec!["terminal".into(), "worktree".into()],
        required_capabilities: vec!["terminal".into()],
        transport: TransportOffer {
            codecs: vec![Codec::Zstd, Codec::Gzip],
            max_frame_bytes: MAX_FRAME_BYTES,
            fragmentation: true,
            reassembly: true,
        },
    }
}
#[test]
fn v4_hello_fixture_roundtrips_all_control_messages_without_numeric_seq() {
    let fixture: Value = serde_json::from_str(include_str!("hello_fixture.json")).unwrap();
    for frame in fixture.as_object().unwrap().values() {
        let decoded = HelloFrame::decode(&serde_json::to_vec(frame).unwrap()).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&decoded.encode().unwrap()).unwrap(),
            *frame
        );
    }
    for seq in [0, 9_007_199_254_740_993, u64::MAX] {
        let encoded = serde_json::to_value(WireSeq(seq)).unwrap();
        assert_eq!(encoded, json!(seq.to_string()));
        assert_eq!(serde_json::from_value::<WireSeq>(encoded).unwrap().0, seq);
    }
    for invalid in [
        json!(1),
        json!(-1),
        json!(1.5),
        json!("01"),
        json!("+1"),
        json!("-1"),
        json!("1e3"),
        json!("18446744073709551616"),
        json!(" 1"),
    ] {
        assert!(serde_json::from_value::<WireSeq>(invalid).is_err());
    }
}
#[test]
fn v4_hello_selects_api_codec_capabilities_and_bounded_frame() {
    let selected = policy().negotiate(&hello()).unwrap();
    assert_eq!(selected.app_api_ver, "4.1");
    assert_eq!(selected.transport.codec, Codec::Gzip);
    assert_eq!(selected.transport.frame_bytes, 8192);
    assert_eq!(selected.capabilities, ["terminal", "worktree"]);
    let mut peer = hello();
    peer.bootstrap_ver = 3;
    peer.transport.max_frame_bytes = u32::MAX;
    assert_eq!(
        policy().negotiate(&peer).unwrap().transport.frame_bytes,
        MAX_FRAME_BYTES
    );
}
#[test]
fn v4_hello_rejects_every_incompatible_dimension() {
    let mut peer = hello();
    peer.protocol_ver = 3;
    assert_eq!(
        policy().negotiate(&peer).unwrap_err().reason,
        RejectReason::ProtocolIncompatible
    );
    let mut peer = hello();
    peer.bootstrap_ver = 2;
    assert_eq!(
        policy().negotiate(&peer).unwrap_err().reason,
        RejectReason::BootstrapIncompatible
    );
    let mut peer = hello();
    peer.instance_id = "other-instance".into();
    assert_eq!(
        policy().negotiate(&peer).unwrap_err().reason,
        RejectReason::InstanceMismatch
    );
    for range in [
        ("5.0", "5.1"),
        ("4.2", "4.0"),
        ("04.0", "4.1"),
        ("4", "4.1"),
    ] {
        let mut peer = hello();
        peer.app_api_ver = ApiRange {
            min: range.0.into(),
            max: range.1.into(),
        };
        assert_eq!(
            policy().negotiate(&peer).unwrap_err().reason,
            RejectReason::AppApiIncompatible
        );
    }
    let mut peer = hello();
    peer.capabilities.clear();
    assert_eq!(
        policy().negotiate(&peer).unwrap_err().reason,
        RejectReason::CapabilityMissing
    );
    for mutation in 0..4 {
        let mut peer = hello();
        match mutation {
            0 => peer.transport.codecs.clear(),
            1 => peer.transport.fragmentation = false,
            2 => peer.transport.reassembly = false,
            _ => peer.transport.max_frame_bytes = 1023,
        }
        assert_eq!(
            policy().negotiate(&peer).unwrap_err().reason,
            RejectReason::TransportIncompatible
        );
    }
}
#[test]
fn v4_hello_decode_bounds_and_proof_challenge_are_strict_and_redacted() {
    assert!(HelloFrame::decode(&vec![b' '; MAX_HELLO_BYTES + 1]).is_err());
    assert!(HelloFrame::decode(b"{}").is_err());
    let proof = hello().device_proof;
    assert!(proof.matches_challenge(&"A".repeat(43)));
    assert!(!proof.matches_challenge(&"D".repeat(43)));
    assert!(!format!("{proof:?}").contains(&proof.signature));
    let fixture: Value = serde_json::from_str(include_str!("hello_fixture.json")).unwrap();
    let ok: HelloFrame = serde_json::from_value(fixture["hello_ok"].clone()).unwrap();
    assert!(!format!("{ok:?}").contains("test-issued-scope"));
}

#[test]
fn v4_hello_native_initiator_rejects_unoffered_server_selections() {
    let fixture: Value = serde_json::from_str(include_str!("hello_fixture.json")).unwrap();
    let response: HelloOk = match serde_json::from_value(fixture["hello_ok"].clone()).unwrap() {
        HelloFrame::HelloOk(ok) => ok,
        _ => panic!("fixture type"),
    };
    hello()
        .validate_response(&response, &["terminal".into()])
        .unwrap();
    let mut wrong = response.clone();
    wrong.instance_id = "foreign".into();
    assert_eq!(
        hello().validate_response(&wrong, &[]).unwrap_err().reason,
        RejectReason::InstanceMismatch
    );
    let mut wrong = response.clone();
    wrong.app_api_ver = "5.0".into();
    assert_eq!(
        hello().validate_response(&wrong, &[]).unwrap_err().reason,
        RejectReason::AppApiIncompatible
    );
    let mut wrong = response.clone();
    wrong.transport.frame_bytes = 16384;
    assert_eq!(
        hello().validate_response(&wrong, &[]).unwrap_err().reason,
        RejectReason::TransportIncompatible
    );
    let mut wrong = response.clone();
    wrong.transport.codec = Codec::Zstd;
    assert_eq!(
        hello().validate_response(&wrong, &[]).unwrap_err().reason,
        RejectReason::TransportIncompatible
    );
    let mut wrong = response;
    wrong.capabilities.clear();
    assert_eq!(
        hello()
            .validate_response(&wrong, &["terminal".into()])
            .unwrap_err()
            .reason,
        RejectReason::CapabilityMissing
    );
}

#[test]
fn v4_hello_bootstrap_control_cannot_assume_fragmentation_before_negotiation() {
    let mut input = hello();
    input.instance_id = "x".repeat(MAX_HELLO_BYTES);
    assert_eq!(
        HelloFrame::Hello(input).encode().unwrap_err().reason,
        RejectReason::InvalidHello
    );
}
