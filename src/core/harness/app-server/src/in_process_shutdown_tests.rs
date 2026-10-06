use super::*;
use std::future::Future;

struct Dropped(Option<oneshot::Sender<()>>);
impl Drop for Dropped {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

fn runtime<F, Fut>(run: F) -> InProcessClientHandle
where
    F: FnOnce(mpsc::Receiver<InProcessClientMessage>) -> Fut,
    Fut: Future<Output = IoResult<()>> + Send + 'static,
{
    let (client_tx, client_rx) = mpsc::channel(1);
    let (_, event_rx) = mpsc::channel(1);
    InProcessClientHandle {
        client: InProcessClientSender { client_tx },
        event_rx,
        runtime_handle: Some(tokio::spawn(run(client_rx))),
    }
}

fn acknowledge(message: Option<InProcessClientMessage>) {
    let Some(InProcessClientMessage::Shutdown { done_tx }) = message else {
        panic!("expected the owned shutdown request");
    };
    let _ = done_tx.send(());
}

async fn stopped(receiver: oneshot::Receiver<()>) {
    timeout(Duration::from_secs(1), receiver)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn runtime_shutdown_requires_checked_result_after_ack() {
    let good = runtime(|mut receiver| async move {
        acknowledge(receiver.recv().await);
        Ok(())
    });
    good.shutdown().await.unwrap();

    let failed = runtime(|mut receiver| async move {
        acknowledge(receiver.recv().await);
        Err(IoError::new(
            ErrorKind::TimedOut,
            "checked thread drain failed",
        ))
    });
    assert_eq!(
        failed.shutdown().await.unwrap_err().kind(),
        ErrorKind::TimedOut
    );
}

#[tokio::test]
async fn runtime_shutdown_does_not_mask_panic_after_ack() {
    let client = runtime(|mut receiver| async move {
        acknowledge(receiver.recv().await);
        panic!("private-runtime-panic-fixture");
    });
    let error = client.shutdown().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Other);
    assert!(!error.to_string().contains("private-runtime-panic-fixture"));
}

#[tokio::test]
async fn runtime_shutdown_timeout_aborts_and_joins_owned_task() {
    let (drop_tx, drop_rx) = oneshot::channel();
    let client = runtime(|mut receiver| async move {
        let _dropped = Dropped(Some(drop_tx));
        acknowledge(receiver.recv().await);
        std::future::pending::<IoResult<()>>().await
    });
    let error = client
        .shutdown_with_timeout(Duration::from_millis(10))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::TimedOut);
    stopped(drop_rx).await;
}

#[tokio::test]
async fn runtime_shutdown_closed_request_still_finishes_owned_cleanup() {
    let (drop_tx, drop_rx) = oneshot::channel();
    let (ready_tx, ready_rx) = oneshot::channel();
    let client = runtime(|receiver| async move {
        let _dropped = Dropped(Some(drop_tx));
        drop(receiver);
        let _ = ready_tx.send(());
        std::future::pending::<IoResult<()>>().await
    });
    ready_rx.await.unwrap();
    let error = client
        .shutdown_with_timeout(Duration::from_millis(10))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::BrokenPipe);
    stopped(drop_rx).await;
}

#[tokio::test]
async fn runtime_shutdown_cancelled_future_aborts_owned_children() {
    let (parent_drop_tx, parent_drop_rx) = oneshot::channel();
    let (child_drop_tx, child_drop_rx) = oneshot::channel();
    let (ready_tx, ready_rx) = oneshot::channel();
    let client = runtime(|mut receiver| async move {
        let _parent = Dropped(Some(parent_drop_tx));
        let (child_ready_tx, child_ready_rx) = oneshot::channel();
        let _child = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
            let _child = Dropped(Some(child_drop_tx));
            let _ = child_ready_tx.send(());
            std::future::pending::<()>().await;
        }));
        child_ready_rx.await.unwrap();
        acknowledge(receiver.recv().await);
        let _ = ready_tx.send(());
        std::future::pending::<IoResult<()>>().await
    });
    let mut shutdown = Box::pin(client.shutdown());
    tokio::select! {
        result = &mut shutdown => panic!("unexpected terminal shutdown: {result:?}"),
        result = ready_rx => result.unwrap(),
    };
    drop(shutdown);
    stopped(parent_drop_rx).await;
    stopped(child_drop_rx).await;
}

#[tokio::test]
async fn runtime_shutdown_unpolled_future_retains_abort_ownership() {
    let (drop_tx, drop_rx) = oneshot::channel();
    let (ready_tx, ready_rx) = oneshot::channel();
    let client = runtime(|_receiver| async move {
        let _dropped = Dropped(Some(drop_tx));
        let _ = ready_tx.send(());
        std::future::pending::<IoResult<()>>().await
    });
    ready_rx.await.unwrap();
    drop(client.shutdown());
    stopped(drop_rx).await;
}
