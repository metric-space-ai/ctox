use super::*;
use futures::FutureExt;

#[tokio::test]
async fn exact_turn_interrupt_session_exit_rejects_unprocessed_request() {
    let (sess, _, events) = make_session_and_context_with_rx().await;
    let (mut client, submissions) = interrupt_client(Arc::clone(&sess), events);
    let (exit_tx, exit_rx) = tokio::sync::oneshot::channel();
    Arc::get_mut(&mut client).unwrap().session_loop_termination = async move {
        let _ = exit_rx.await;
    }
    .boxed()
    .shared();
    let caller = Arc::clone(&client);
    let request = tokio::spawn(async move { caller.interrupt_turn("unknown".into()).await });
    let _queued = submissions.recv().await.unwrap();
    exit_tx.send(()).unwrap();
    assert!(
        tokio::time::timeout(StdDuration::from_secs(5), request)
            .await
            .expect("session exit must release caller")
            .unwrap()
            .is_err()
    );
    assert!(sess.interrupt_receipts.lock().unwrap().is_empty());
}

fn interrupt_client(
    session: Arc<Session>,
    rx_event: async_channel::Receiver<Event>,
) -> (Arc<Codex>, async_channel::Receiver<Submission>) {
    let (tx_sub, rx_sub) = async_channel::bounded(4);
    let (_, agent_status) = watch::channel(AgentStatus::PendingInit);
    let codex = Arc::new(Codex {
        tx_sub,
        rx_event,
        agent_status,
        session,
        session_loop_termination: std::future::pending::<()>().boxed().shared(),
    });
    (codex, rx_sub)
}

async fn dispatch_interrupt(session: &Arc<Session>, sub: Submission) -> bool {
    let Op::InterruptTurn { turn_id } = sub.op else {
        panic!("expected targeted interrupt");
    };
    handlers::interrupt_turn(session, &sub.id, &turn_id).await
}

#[tokio::test]
async fn exact_turn_interrupt_rejects_delayed_request_after_replacement() {
    let (sess, tc, events) = make_session_and_context_with_rx().await;
    sess.spawn_task(
        Arc::clone(&tc),
        vec![],
        NeverEndingTask {
            kind: TaskKind::Regular,
            listen_to_cancellation_token: true,
        },
    )
    .await;
    let (client, submissions) = interrupt_client(Arc::clone(&sess), events.clone());
    let caller = Arc::clone(&client);
    let old_turn = tc.sub_id.clone();
    let request = tokio::spawn(async move { caller.interrupt_turn(old_turn).await });
    let delayed = submissions.recv().await.expect("queued request");

    let successor = sess.new_default_turn_with_sub_id("successor".into()).await;
    sess.spawn_task(
        Arc::clone(&successor),
        vec![],
        NeverEndingTask {
            kind: TaskKind::Regular,
            listen_to_cancellation_token: true,
        },
    )
    .await;
    let replaced = events.recv().await.expect("replacement event");
    assert!(matches!(replaced.msg, EventMsg::TurnAborted(ref e)
        if e.turn_id.as_deref() == Some(tc.sub_id.as_str()) && e.reason == TurnAbortReason::Replaced));
    assert!(
        !request.is_finished(),
        "unrelated abort must not acknowledge the request"
    );
    assert!(!dispatch_interrupt(&sess, delayed).await);
    assert!(
        !request
            .await
            .expect("caller joined")
            .expect("interrupt receipt")
    );
    {
        let active = sess.active_turn.lock().await;
        let task = &active.as_ref().expect("successor remains active").tasks["successor"];
        assert!(!task.cancellation_token.is_cancelled());
    }
    assert!(
        events.try_recv().is_err(),
        "rejected interrupt emits no abort"
    );
    assert!(sess.abort_turn("successor").await);
}

#[tokio::test]
async fn exact_turn_interrupt_rejects_unknown_empty_and_finished_turns() {
    let (sess, tc, events) = make_session_and_context_with_rx().await;
    sess.spawn_task(
        Arc::clone(&tc),
        vec![],
        NeverEndingTask {
            kind: TaskKind::Regular,
            listen_to_cancellation_token: true,
        },
    )
    .await;
    let (client, submissions) = interrupt_client(Arc::clone(&sess), events.clone());
    for turn_id in ["unknown", ""] {
        let caller = Arc::clone(&client);
        let turn_id = turn_id.to_owned();
        let request = tokio::spawn(async move { caller.interrupt_turn(turn_id).await });
        assert!(!dispatch_interrupt(&sess, submissions.recv().await.unwrap()).await);
        assert!(!request.await.unwrap().unwrap());
        assert!(sess.active_turn.lock().await.is_some());
        assert!(events.try_recv().is_err());
    }
    assert!(sess.abort_turn(&tc.sub_id).await);
    let _ = events.recv().await.unwrap();
    let caller = Arc::clone(&client);
    let turn_id = tc.sub_id.clone();
    let request = tokio::spawn(async move { caller.interrupt_turn(turn_id).await });
    assert!(!dispatch_interrupt(&sess, submissions.recv().await.unwrap()).await);
    assert!(!request.await.unwrap().unwrap());
    assert!(events.try_recv().is_err());
}

struct GatedAbortTask {
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl SessionTask for GatedAbortTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Regular
    }
    fn span_name(&self) -> &'static str {
        "session_task.gated_abort"
    }
    async fn run(
        self: Arc<Self>,
        _: Arc<SessionTaskContext>,
        _: Arc<TurnContext>,
        _: Vec<UserInput>,
        token: CancellationToken,
    ) -> Option<String> {
        token.cancelled().await;
        None
    }
    async fn abort(&self, _: Arc<SessionTaskContext>, _: Arc<TurnContext>) {
        self.entered.notify_one();
        self.release.notified().await;
    }
}

#[tokio::test]
async fn exact_turn_interrupt_receipt_waits_for_its_own_stop() {
    let (sess, tc, events) = make_session_and_context_with_rx().await;
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    sess.spawn_task(
        Arc::clone(&tc),
        vec![],
        GatedAbortTask {
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
        },
    )
    .await;
    let (client, submissions) = interrupt_client(Arc::clone(&sess), events.clone());
    let caller = Arc::clone(&client);
    let turn_id = tc.sub_id.clone();
    let request = tokio::spawn(async move { caller.interrupt_turn(turn_id).await });
    let sub = submissions.recv().await.unwrap();
    let core = Arc::clone(&sess);
    let stop = tokio::spawn(async move { dispatch_interrupt(&core, sub).await });
    entered.notified().await;
    sess.send_event_raw(Event {
        id: "unrelated".into(),
        msg: EventMsg::TurnAborted(crate::protocol::TurnAbortedEvent {
            turn_id: Some("unrelated".into()),
            reason: TurnAbortReason::Interrupted,
        }),
    })
    .await;
    let unrelated = events.recv().await.unwrap();
    assert_eq!(unrelated.id, "unrelated");
    assert!(
        !request.is_finished(),
        "broadcast abort cannot resolve a receipt"
    );
    assert_eq!(sess.interrupt_receipts.lock().unwrap().len(), 1);
    release.notify_one();
    assert!(stop.await.unwrap());
    assert!(request.await.unwrap().unwrap());
    let stopped = events
        .try_recv()
        .expect("matching stop published before receipt");
    assert!(matches!(stopped.msg, EventMsg::TurnAborted(ref e)
        if e.turn_id.as_deref() == Some(tc.sub_id.as_str()) && e.reason == TurnAbortReason::Interrupted));
    assert!(sess.active_turn.lock().await.is_none());
    assert!(sess.interrupt_receipts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn exact_turn_interrupt_removes_cancelled_and_failed_receipts() {
    let (sess, _, events) = make_session_and_context_with_rx().await;
    let (client, submissions) = interrupt_client(Arc::clone(&sess), events);
    let caller = Arc::clone(&client);
    let request = tokio::spawn(async move { caller.interrupt_turn("unknown".into()).await });
    let queued = submissions.recv().await.unwrap();
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert!(sess.interrupt_receipts.lock().unwrap().is_empty());
    assert!(!dispatch_interrupt(&sess, queued).await);
    drop(submissions);
    assert!(client.interrupt_turn("unknown".into()).await.is_err());
    assert!(sess.interrupt_receipts.lock().unwrap().is_empty());
}
