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

fn worker<F, Fut>(
    run: F,
) -> (
    mpsc::Sender<ClientCommand>,
    mpsc::Receiver<InProcessServerEvent>,
    ShutdownWorker,
)
where
    F: FnOnce(mpsc::Receiver<ClientCommand>) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let (command_tx, command_rx) = mpsc::channel(1);
    let (_, event_rx) = mpsc::channel(1);
    (
        command_tx,
        event_rx,
        ShutdownWorker(tokio::spawn(run(command_rx))),
    )
}

async fn stopped(receiver: oneshot::Receiver<()>) {
    timeout(Duration::from_secs(1), receiver)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn client_shutdown_requires_ack_even_when_worker_finishes() {
    for acknowledge in [true, false] {
        let (tx, rx, worker) = worker(move |mut receiver| async move {
            let Some(ClientCommand::Shutdown { response_tx }) = receiver.recv().await else {
                panic!("expected owned shutdown request");
            };
            if acknowledge {
                let _ = response_tx.send(Ok(()));
            } else {
                drop(response_tx);
            }
        });
        let result = finish_client_shutdown(tx, rx, worker, Duration::from_secs(1)).await;
        if acknowledge {
            result.unwrap();
        } else {
            assert_eq!(result.unwrap_err().kind(), ErrorKind::BrokenPipe);
        }
    }
}

#[tokio::test]
async fn client_shutdown_error_response_still_waits_for_owned_cleanup() {
    let (drop_tx, drop_rx) = oneshot::channel();
    let (sent_tx, sent_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let (tx, rx, worker) = worker(|mut receiver| async move {
        let _dropped = Dropped(Some(drop_tx));
        let Some(ClientCommand::Shutdown { response_tx }) = receiver.recv().await else {
            panic!("expected owned shutdown request");
        };
        let _ = response_tx.send(Err(IoError::new(
            ErrorKind::PermissionDenied,
            "checked runtime shutdown refused",
        )));
        let _ = sent_tx.send(());
        release_rx.await.unwrap();
    });
    let mut shutdown = tokio::spawn(finish_client_shutdown(
        tx,
        rx,
        worker,
        Duration::from_secs(1),
    ));
    sent_rx.await.unwrap();
    assert!(
        timeout(Duration::from_millis(10), &mut shutdown)
            .await
            .is_err(),
        "an error acknowledgement cannot detach cleanup still in flight",
    );
    release_tx.send(()).unwrap();
    let error = shutdown.await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::PermissionDenied);
    stopped(drop_rx).await;
}

#[tokio::test]
async fn client_shutdown_does_not_mask_worker_panic_after_ack() {
    let (tx, rx, worker) = worker(|mut receiver| async move {
        let Some(ClientCommand::Shutdown { response_tx }) = receiver.recv().await else {
            panic!("expected owned shutdown request");
        };
        let _ = response_tx.send(Ok(()));
        panic!("private-client-panic-fixture");
    });
    let error = finish_client_shutdown(tx, rx, worker, Duration::from_secs(1))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Other);
    assert!(!error.to_string().contains("private-client-panic-fixture"));
}

#[tokio::test]
async fn client_shutdown_timeout_stops_owned_worker_and_returns_error() {
    let (drop_tx, drop_rx) = oneshot::channel();
    let (tx, rx, worker) = worker(|mut receiver| async move {
        let _dropped = Dropped(Some(drop_tx));
        let _request = receiver.recv().await;
        std::future::pending::<()>().await;
    });
    let error = finish_client_shutdown(tx, rx, worker, Duration::from_millis(10))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::TimedOut);
    stopped(drop_rx).await;
}

#[tokio::test]
async fn client_shutdown_unpolled_future_retains_worker_ownership() {
    let (drop_tx, drop_rx) = oneshot::channel();
    let (ready_tx, ready_rx) = oneshot::channel();
    let (tx, rx, worker) = worker(|_receiver| async move {
        let _dropped = Dropped(Some(drop_tx));
        let _ = ready_tx.send(());
        std::future::pending::<()>().await;
    });
    ready_rx.await.unwrap();
    drop(finish_client_shutdown(
        tx,
        rx,
        worker,
        Duration::from_secs(1),
    ));
    stopped(drop_rx).await;
}
