//! Native endpoint regressions against real encrypted issuer, target enrollment
//! and policy stores. The existing target fixture's source offer is synthetic;
//! these do not claim source capture, networking or checkpoint activation.
use super::super::super::session_handoff_enrollment::target::tests::{gate_fixture, GateFixture};
use super::*;
use ctox_sync::authority::auth::handoff_wire::SignedHandoffRequest;

type Peer = (&'static str, u64);
fn server(f: &GateFixture) -> Arc<Server<Peer>> {
    Arc::new(Server {
        gate: NativeSessionHandoffGate {
            root: f.root.path().into(),
            issuer_identity: f.identity.public_identity(),
            permit_ttl_ms: PERMIT_TTL_MS,
        },
        scope: f.request.audience.clone(),
        ledger: Mutex::new(Ledger {
            alive: true,
            pending: HashMap::new(),
        }),
    })
}
fn fresh_fixture() -> GateFixture {
    let mut f = gate_fixture();
    f.request.nonce = fresh_nonce().unwrap();
    f
}
fn probe(
    f: &GateFixture,
    server: &Arc<Server<Peer>>,
    peer: Peer,
) -> (String, GuardedAuxiliaryResponse) {
    let sent = SignedHandoffRequest::new(
        &f.source,
        SessionHandoffWireRequest::Probe {
            request: f.request.clone(),
        },
    )
    .unwrap();
    let prepared = server
        .clone()
        .answer(peer, vec![sent.envelope.clone()])
        .unwrap();
    prepared.publication.with_current(&mut || Ok(())).unwrap();
    let SessionHandoffWireReply::Challenge { challenge } = sent
        .verify_reply(prepared.result.clone(), now_ms() as u64)
        .unwrap()
    else {
        panic!("challenge expected")
    };
    (challenge, prepared)
}
fn signed_authorize(f: &GateFixture, challenge: String) -> SignedHandoffRequest {
    SignedHandoffRequest::new(
        &f.source,
        SessionHandoffWireRequest::Authorize {
            request: f.request.clone(),
            challenge,
        },
    )
    .unwrap()
}
#[test]
fn native_handoff_transport_binds_challenge_to_exact_connection_and_consumes_once() {
    let f = fresh_fixture();
    let server = server(&f);
    let peer = ("same-route", 1);
    let (challenge, probe_response) = probe(&f, &server, peer);
    let signed = signed_authorize(&f, challenge);
    // Same signaling label and valid source signature still cannot move the
    // receiver-issued challenge to a reconnected or second native connection.
    assert_eq!(
        server
            .clone()
            .answer(("same-route", 2), vec![signed.envelope.clone()])
            .err()
            .unwrap()
            .reason_code,
        "challenge_unknown"
    );
    let response = server
        .clone()
        .answer(peer, vec![signed.envelope.clone()])
        .unwrap();
    response.publication.with_current(&mut || Ok(())).unwrap();
    let SessionHandoffWireReply::Authorized { permit } = signed
        .verify_reply(response.result, now_ms() as u64)
        .unwrap()
    else {
        panic!("permit expected")
    };
    assert_eq!(permit.binding_digest, f.request.binding_digest);
    assert_eq!(
        server
            .clone()
            .answer(peer, vec![signed.envelope])
            .err()
            .unwrap()
            .reason_code,
        "challenge_changed"
    );
    assert!(
        probe_response
            .publication
            .with_current(&mut || Ok(()))
            .is_err(),
        "a consumed challenge cannot be published again"
    );
}
#[test]
fn native_handoff_transport_rejects_unsigned_foreign_and_changed_request_without_setup() {
    let f = fresh_fixture();
    let server = server(&f);
    assert!(server
        .clone()
        .answer(("p", 1), vec![serde_json::json!({"approved":true})])
        .is_err());
    let foreign = SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap();
    let signed = SignedHandoffRequest::new(
        &foreign,
        SessionHandoffWireRequest::Probe {
            request: f.request.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        server
            .clone()
            .answer(("p", 1), vec![signed.envelope])
            .err()
            .unwrap()
            .reason_code,
        "peer_not_bound"
    );
    assert!(server.ledger.lock().unwrap().pending.is_empty());
    let (challenge, _) = probe(&f, &server, ("p", 1));
    let mut changed = f.request.clone();
    changed.checkpoint_sequence += 1;
    let signed = SignedHandoffRequest::new(
        &f.source,
        SessionHandoffWireRequest::Authorize {
            request: changed,
            challenge,
        },
    )
    .unwrap();
    assert!(server
        .clone()
        .answer(("p", 1), vec![signed.envelope])
        .is_err());
    assert!(!server.ledger.lock().unwrap().pending[&("p", 1)].used);
}
#[test]
fn native_handoff_transport_rechecks_policy_between_pending_physical_polls() {
    for change in ["grant", "epoch", "binding", "issuer", "retire"] {
        let f = fresh_fixture();
        let server = server(&f);
        let (challenge, _) = probe(&f, &server, ("p", 1));
        let sent = signed_authorize(&f, challenge);
        let response = server
            .clone()
            .answer(("p", 1), vec![sent.envelope])
            .unwrap();
        let mut polls = 0;
        response
            .publication
            .with_current(&mut || {
                let conn = Connection::open(business_os_store_path(f.root.path())).unwrap();
                conn.busy_timeout(std::time::Duration::ZERO).unwrap();
                assert!(
                    conn.execute_batch("BEGIN IMMEDIATE").is_err(),
                    "policy mutation fence must cover the physical poll"
                );
                assert!(
                    server.ledger.try_lock().is_err(),
                    "host retirement fence escaped poll"
                );
                polls += 1; // Simulated zero-byte Pending.
                Ok(())
            })
            .unwrap();
        match change {
            "grant" => {
                Connection::open(business_os_store_path(f.root.path()))
                    .unwrap()
                    .execute("UPDATE business_permission_grants SET active=0", [])
                    .unwrap();
            }
            "epoch" => {
                Connection::open(business_os_store_path(f.root.path()))
                    .unwrap()
                    .execute(
                        "UPDATE business_users SET capability_epoch=capability_epoch+1",
                        [],
                    )
                    .unwrap();
            }
            "binding" => {
                Connection::open(business_os_store_path(f.root.path()))
                    .unwrap()
                    .execute(
                        "UPDATE business_session_handoff_bindings SET revision=revision+1",
                        [],
                    )
                    .unwrap();
            }
            "issuer" => {
                crate::secrets::delete_secret_record(
                    f.root.path(),
                    "ctox-sync-host",
                    "identity-pkcs8",
                )
                .unwrap();
            }
            "retire" => drop(NativeHandoffHost {
                server: server.clone(),
            }),
            _ => unreachable!(),
        }
        assert!(
            response
                .publication
                .with_current(&mut || {
                    polls += 1;
                    Ok(())
                })
                .is_err(),
            "{change}"
        );
        assert_eq!(polls, 1, "revoked prepared response escaped at {change}");
    }
}
#[test]
fn native_handoff_transport_new_probe_retires_old_response_and_bounds_expiry() {
    let f = fresh_fixture();
    let server = server(&f);
    let (_, old) = probe(&f, &server, ("p", 1));
    let (_, new) = probe(&f, &server, ("p", 1));
    assert!(old.publication.with_current(&mut || Ok(())).is_err());
    new.publication.with_current(&mut || Ok(())).unwrap();
    server
        .ledger
        .lock()
        .unwrap()
        .pending
        .get_mut(&("p", 1))
        .unwrap()
        .permit
        .expires_at_ms = 0;
    // The retained challenge record is part of authority too.
    assert!(new.publication.with_current(&mut || Ok(())).is_err());
}
