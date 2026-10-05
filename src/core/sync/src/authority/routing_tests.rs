#[test]
fn authority_failure_contract_rejects_legacy_strings_and_unsafe_ids() {
    for value in [
        serde_json::json!("has to forward request to: None, None"),
        serde_json::json!({"type": "notLeader", "leader": 9_007_199_254_740_992u64}),
        serde_json::json!({"type": "notLeader", "leader": -1}),
        serde_json::json!({"type": "rejected", "reason": "revoked", "retry": true}),
    ] {
        assert!(serde_json::from_value::<AuthorityFailure>(value).is_err());
    }
    for value in [
        serde_json::json!({"type": "notLeader", "leader": null}),
        serde_json::json!({"type": "notLeader"}),
    ] {
        assert_eq!(
            serde_json::from_value::<AuthorityFailure>(value).unwrap(),
            AuthorityFailure::NotLeader { leader: None }
        );
    }
}

use super::*;
use crate::authority::{Command, ExecutionSpec, Job, Ownership, Receipt, Request};
use std::cell::{Cell, RefCell};

fn voters() -> BTreeMap<NodeId, Peer> {
    (1..=3)
        .map(|id| {
            (
                id,
                Peer {
                    identity: format!("pinned-{id}"),
                    executor: true,
                    data_replica: true,
                },
            )
        })
        .collect()
}
fn request() -> Rpc {
    Rpc::Propose(Request {
        request_id: "durable-effect-request".into(),
        actor: 4,
        command: Command::BeginEffect {
            job_id: "job".into(),
            ownership: Ownership {
                node_id: 4,
                generation: 1,
            },
            effect_id: "publish".into(),
        },
    })
}
fn replay() -> Reply {
    Reply::Propose(Ok(Receipt::Replayed(Job {
        spec: ExecutionSpec {
            job_id: "job".into(),
            session_id: "session".into(),
            scope_id: "scope".into(),
            harness: "codex".into(),
            harness_version: "fixture".into(),
            model_route_id: "route".into(),
            gateway_account_id: "account".into(),
            model_id: "model".into(),
            required_capabilities: Default::default(),
        },
        ownership: Ownership {
            node_id: 4,
            generation: 1,
        },
        checkpoint: None,
        checkpoint_requires_refresh: true,
        pending_effects: ["publish".into()].into(),
        completed_effects: Default::default(),
        stopped: false,
    })))
}

#[tokio::test(start_paused = true)]
async fn election_gap_revisits_voters_with_the_identical_request() {
    let sent = RefCell::new(Vec::new());
    let original = serde_json::to_value(request()).unwrap();
    let (id, reply) = route(
        request(),
        &voters(),
        None,
        || false,
        |id, _, rpc| {
            sent.borrow_mut()
                .push((id, serde_json::to_value(rpc).unwrap()));
            let result = if sent.borrow().len() <= 3 {
                Reply::Propose(Err(AuthorityFailure::NotLeader { leader: None }))
            } else {
                replay()
            };
            std::future::ready(Ok(result))
        },
    )
    .await
    .unwrap();
    assert_eq!(id, 1);
    assert!(matches!(reply, Reply::Propose(Ok(Receipt::Replayed(_)))));
    assert_eq!(
        sent.borrow().iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [1, 2, 3, 1]
    );
    assert!(sent.borrow().iter().all(|(_, rpc)| rpc == &original));
}

#[tokio::test(start_paused = true)]
async fn redirects_only_reorder_untried_pinned_voters() {
    for hint in [Some(3), Some(99), Some(1)] {
        let sent = RefCell::new(Vec::new());
        route(
            request(),
            &voters(),
            None,
            || false,
            |id, peer, _| {
                assert_eq!(peer.identity, format!("pinned-{id}"));
                sent.borrow_mut().push(id);
                std::future::ready(Ok(if sent.borrow().len() == 1 {
                    Reply::Propose(Err(AuthorityFailure::NotLeader { leader: hint }))
                } else {
                    replay()
                }))
            },
        )
        .await
        .unwrap();
        assert_eq!(
            *sent.borrow(),
            if hint == Some(3) {
                vec![1, 3]
            } else {
                vec![1, 2]
            }
        );
    }
}

#[tokio::test(start_paused = true)]
async fn typed_rejection_is_terminal_without_another_attempt_or_delay() {
    let calls = Cell::new(0);
    let started = Instant::now();
    let error = route(
        request(),
        &voters(),
        None,
        || false,
        |_, _, _| {
            calls.set(calls.get() + 1);
            std::future::ready(Ok(Reply::Propose(Err(AuthorityFailure::Rejected {
                reason: "revoked executor".into(),
            }))))
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(calls.get(), 1);
    assert_eq!(Instant::now(), started);
}

#[tokio::test(start_paused = true)]
async fn minority_or_permanent_election_gap_exhausts_the_original_deadline() {
    let calls = Cell::new(0);
    let started = Instant::now();
    let error = route(
        request(),
        &voters(),
        None,
        || false,
        |id, _, _| {
            calls.set(calls.get() + 1);
            std::future::ready(Ok(Reply::Propose(Err(if id == 1 {
                AuthorityFailure::Unavailable {
                    reason: "no confirmed quorum".into(),
                }
            } else {
                AuthorityFailure::NotLeader { leader: Some(1) }
            }))))
        },
    )
    .await
    .unwrap_err();
    assert_eq!(Instant::now() - started, Duration::from_secs(5));
    assert!(
        (4..=69).contains(&calls.get()),
        "bounded backoff, not a busy loop"
    );
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("no confirmed quorum"));
    assert!(diagnostic.contains("retain the request ID"));
}

#[tokio::test(start_paused = true)]
async fn in_flight_transport_is_cancelled_at_the_shared_deadline() {
    let calls = Cell::new(0);
    let started = Instant::now();
    assert!(route(
        request(),
        &voters(),
        None,
        || false,
        |_, _, _| {
            calls.set(calls.get() + 1);
            std::future::pending::<io::Result<Reply>>()
        }
    )
    .await
    .is_err());
    assert_eq!(Instant::now() - started, Duration::from_secs(5));
    assert_eq!(calls.get(), 3);
}

#[tokio::test(start_paused = true)]
async fn shutdown_during_confirmation_never_returns_a_grant() {
    let stopped = Cell::new(false);
    let error = route(
        request(),
        &voters(),
        None,
        || stopped.get(),
        |_, _, _| {
            stopped.set(true);
            std::future::ready(Ok(replay()))
        },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("stopped before confirmation"));
}
