use super::*;
use crate::business_data_contract::{
    NativeBusinessDataCredentialReply, NativeBusinessDataErrorCode, NativeBusinessDataOperation,
    NativeBusinessDataResult,
};
use std::sync::atomic::{AtomicBool, Ordering};

#[tokio::test]
async fn response_release_waits_for_complete_private_frame_write() {
    use tokio::io::AsyncReadExt;
    struct ReleaseDispatcher {
        dispatched: Arc<tokio::sync::Notify>,
        released: Arc<tokio::sync::Notify>,
        release_called: Arc<AtomicBool>,
    }
    impl BusinessDataDispatcher for ReleaseDispatcher {
        fn dispatch(&self, request: Request) -> BusinessDataDispatchFuture {
            let dispatched = self.dispatched.clone();
            Box::pin(async move {
                dispatched.notify_one();
                Ok(Response {
                    version: 1,
                    request_id: request.request_id,
                    result: NativeBusinessDataResult::Rejected {
                        code: NativeBusinessDataErrorCode::Unsupported,
                        message: "fixture response".into(),
                        retryable: false,
                    },
                })
            })
        }
        fn response_sent<'a>(&'a self, response: &'a Response) -> BusinessDataShutdownFuture<'a> {
            // Record invocation as well as future completion: neither may
            // precede the complete frame write.
            assert_eq!(response.request_id, "open");
            self.release_called.store(true, Ordering::SeqCst);
            Box::pin(async move {
                self.released.notify_one();
                Ok(())
            })
        }
    }
    let dispatched = Arc::new(tokio::sync::Notify::new());
    let released = Arc::new(tokio::sync::Notify::new());
    let release_called = Arc::new(AtomicBool::new(false));
    let service = BusinessDataIpc::new(Arc::new({
        let dispatched = dispatched.clone();
        let released = released.clone();
        let release_called = release_called.clone();
        move |_, _| {
            Ok(Arc::new(ReleaseDispatcher {
                dispatched: dispatched.clone(),
                released: released.clone(),
                release_called: release_called.clone(),
            }))
        }
    }));
    // The response cannot fit even its header without the client reading.
    let (native, mut client) = tokio::io::duplex(1);
    let exchange = async {
        write_host_frame(&mut client, &open_request())
            .await
            .unwrap();
        dispatched.notified().await;
        tokio::task::yield_now().await;
        assert!(!release_called.load(Ordering::SeqCst));
        let mut header = [0u8; 4];
        client.read_exact(&mut header).await.unwrap();
        tokio::task::yield_now().await;
        assert!(
            !release_called.load(Ordering::SeqCst),
            "header alone must not release events"
        );
        let size = u32::from_be_bytes(header) as usize;
        assert!(size > 1 && size < 4096);
        let mut body = vec![0; size];
        client.read_exact(&mut body).await.unwrap();
        assert!(matches!(serde_json::from_slice::<Frame>(&body).unwrap(),
            Frame::Response { response } if response.request_id == "open"));
        released.notified().await;
        assert!(release_called.load(Ordering::SeqCst));
        drop(client);
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(service.serve(Box::new(native)), exchange)
    })
    .await
    .expect("bounded response release regression");
    assert!(result.is_err());
}

struct TestDispatcher {
    credentials: CredentialRequester,
}
impl BusinessDataDispatcher for TestDispatcher {
    fn dispatch(&self, request: Request) -> BusinessDataDispatchFuture {
        let credentials = self.credentials.clone();
        Box::pin(async move {
            credentials.request("target", "connection", 0, None).await?;
            Ok(Response {
                version: 1,
                request_id: request.request_id,
                result: NativeBusinessDataResult::Rejected {
                    code: NativeBusinessDataErrorCode::Unsupported,
                    message: "test dispatcher".into(),
                    retryable: false,
                },
            })
        })
    }
}
fn open_request() -> Frame {
    Frame::Request {
        request: Request {
            version: 1,
            request_id: "open".into(),
            operation: NativeBusinessDataOperation::Open {
                target_id: "target".into(),
            },
        },
    }
}

#[tokio::test]
async fn service_reads_credentials_while_a_dispatch_is_waiting() {
    let service = BusinessDataIpc::new(Arc::new(|credentials, _events| {
        Ok(Arc::new(TestDispatcher { credentials }))
    }));
    let (native, mut main) = tokio::io::duplex(64);
    let serve = service.serve(Box::new(native));
    let client = async {
        write_host_frame(&mut main, &open_request()).await.unwrap();
        let challenge = match read_host_frame(&mut main).await.unwrap() {
            Frame::CredentialChallenge { challenge } => challenge,
            _ => panic!("missing credential challenge"),
        };
        write_host_frame(
            &mut main,
            &Frame::CredentialReply {
                reply: NativeBusinessDataCredentialReply {
                    version: 1,
                    request_id: challenge.request_id,
                    connection_id: challenge.connection_id,
                    session_epoch: 0,
                    capability_token: Some("test-token".into()),
                    device_proof: None,
                },
            },
        )
        .await
        .unwrap();
        assert!(matches!(read_host_frame(&mut main).await.unwrap(),
            Frame::Response { response } if response.request_id == "open"));
        drop(main);
    };
    let (result, ()) = tokio::join!(serve, client);
    assert!(result.is_err()); // EOF closes the service and its credential owner.
}

#[tokio::test]
async fn queued_event_rechecks_exact_watch_lifetime_before_writing() {
    let old_alive = Arc::new(AtomicBool::new(true));
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let service = BusinessDataIpc::new(Arc::new({
        let old_alive = old_alive.clone();
        let entered = entered.clone();
        let release = release.clone();
        move |credentials, events| {
            let delayed_authority: crate::business_data_remote::EventAuthorityCheck = {
                let entered = entered.clone();
                let release = release.clone();
                Arc::new(move || {
                    let entered = entered.clone();
                    let release = release.clone();
                    Box::pin(async move {
                        entered.notify_one();
                        release.notified().await;
                        true
                    })
                })
            };
            for sequence in [1, 2] {
                let queued = QueuedBusinessDataEvent {
                    event: Event {
                        version: 1,
                        session: crate::business_data_contract::NativeBusinessDataSessionRef {
                            handle: "same-session".into(),
                            generation: 1,
                        },
                        subscription_id: "same-subscription".into(),
                        sequence,
                        payload:
                            crate::business_data_contract::NativeBusinessDataEventPayload::Reset {
                                code: NativeBusinessDataErrorCode::ResetRequired,
                            },
                    },
                    alive: if sequence == 1 {
                        old_alive.clone()
                    } else {
                        Arc::new(AtomicBool::new(true))
                    },
                    failed: Arc::new(AtomicBool::new(false)),
                    authority: if sequence == 1 {
                        delayed_authority.clone()
                    } else {
                        Arc::new(|| Box::pin(async { true }))
                    },
                };
                assert!(events.try_send(queued).is_ok());
            }
            Ok(Arc::new(TestDispatcher { credentials }))
        }
    }));
    let (native, mut main) = tokio::io::duplex(64);
    let client = async {
        entered.notified().await;
        old_alive.store(false, Ordering::SeqCst);
        release.notify_one();
        match read_host_frame(&mut main).await.unwrap() {
            Frame::Event { event } => {
                assert_eq!(event.subscription_id, "same-subscription");
                assert_eq!(
                    event.sequence, 2,
                    "old watch must not inherit replacement authority"
                );
            }
            _ => panic!("expected event from new watch only"),
        }
        drop(main);
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(service.serve(Box::new(native)), client)
    })
    .await
    .expect("bounded IPC lifetime regression");
    assert!(result.is_err());
}

struct BackpressuredStream {
    inner: tokio::io::DuplexStream,
    blocked: Arc<tokio::sync::Notify>,
}

impl tokio::io::AsyncRead for BackpressuredStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        tokio::io::AsyncRead::poll_read(Pin::new(&mut self.inner), cx, buf)
    }
}

impl tokio::io::AsyncWrite for BackpressuredStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        let result = tokio::io::AsyncWrite::poll_write(Pin::new(&mut self.inner), cx, buf);
        if result.is_pending() {
            self.blocked.notify_one();
        }
        result
    }
    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        tokio::io::AsyncWrite::poll_flush(Pin::new(&mut self.inner), cx)
    }
    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        tokio::io::AsyncWrite::poll_shutdown(Pin::new(&mut self.inner), cx)
    }
}

#[tokio::test]
async fn writer_backpressure_drops_stale_events_without_truncating_started_frame() {
    // Exercise lifetime invalidation and a current-authority denial separately.
    for revoke_lifetime in [true, false] {
        let alive = Arc::new(AtomicBool::new(true));
        let authorized = Arc::new(AtomicBool::new(true));
        let blocked = Arc::new(tokio::sync::Notify::new());
        let service = BusinessDataIpc::new(Arc::new({
            let alive = alive.clone();
            let authorized = authorized.clone();
            move |credentials, events| {
                for sequence in [1, 2, 3] {
                    let current = authorized.clone();
                    assert!(events.try_send(QueuedBusinessDataEvent {
                        event: Event {
                            version: 1,
                            session: crate::business_data_contract::NativeBusinessDataSessionRef {
                                handle: "session".into(), generation: 1,
                            },
                            subscription_id: "reused-id".into(), sequence,
                            payload: crate::business_data_contract::NativeBusinessDataEventPayload::CaughtUp {
                                cursor: format!("cursor-{sequence}"),
                            },
                        },
                        alive: if sequence == 2 { alive.clone() } else { Arc::new(AtomicBool::new(true)) },
                        failed: Arc::new(AtomicBool::new(false)),
                    authority: Arc::new(move || {
                            let current = current.clone();
                            Box::pin(async move { sequence != 2 || current.load(Ordering::SeqCst) })
                        }),
                    }).is_ok());
                }
                Ok(Arc::new(TestDispatcher { credentials }))
            }
        }));
        let (native, mut main) = tokio::io::duplex(1);
        let native = BackpressuredStream {
            inner: native,
            blocked: blocked.clone(),
        };
        let client = async {
            // Observe actual Poll::Pending from the frame writer, not a timer.
            blocked.notified().await;
            if revoke_lifetime {
                alive.store(false, Ordering::SeqCst);
            } else {
                authorized.store(false, Ordering::SeqCst);
            }
            for expected in if revoke_lifetime {
                vec![1, 3]
            } else {
                vec![1, 2, 3]
            } {
                match read_host_frame(&mut main).await.unwrap() {
                    Frame::Event { event } => {
                        use crate::business_data_contract::NativeBusinessDataEventPayload as Payload;
                        assert_eq!(event.sequence, expected);
                        if expected == 2 {
                            assert!(
                                matches!(
                                    event.payload,
                                    Payload::Reset {
                                        code: NativeBusinessDataErrorCode::ResetRequired,
                                    }
                                ),
                                "revoked authority must replace queued completion with Reset"
                            );
                        } else {
                            assert!(
                                matches!(event.payload, Payload::CaughtUp { ref cursor }
                                if cursor == &format!("cursor-{expected}")),
                                "authorized events must retain their original payload"
                            );
                        }
                    }
                    _ => panic!("expected complete event frame"),
                }
            }
            drop(main);
        };
        let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(service.serve(Box::new(native)), client)
        })
        .await
        .expect("bounded writer backpressure regression");
        assert!(result.is_err());
    }
}

#[tokio::test]
async fn authority_failure_resets_once_and_cannot_later_complete_snapshot() {
    use crate::business_data_contract::NativeBusinessDataEventPayload as Payload;
    let service = BusinessDataIpc::new(Arc::new(|credentials, events| {
        let failed = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        for sequence in [1, 2, 3, 4] {
            let mut event = reset_event(sequence);
            event.payload = if sequence == 2 {
                Payload::SnapshotEnd {
                    snapshot_id: "snapshot".into(),
                    cursor: "cursor".into(),
                }
            } else {
                Payload::CaughtUp {
                    cursor: "cursor".into(),
                }
            };
            if sequence == 4 {
                event.subscription_id = "independent-watch".into();
            }
            assert!(events
                .try_send(QueuedBusinessDataEvent {
                    event,
                    alive: alive.clone(),
                    failed: if sequence == 4 {
                        Arc::new(AtomicBool::new(false))
                    } else {
                        failed.clone()
                    },
                    authority: Arc::new(move || Box::pin(async move { sequence != 1 })),
                })
                .is_ok());
        }
        Ok(Arc::new(TestDispatcher { credentials }))
    }));
    let (native, mut main) = tokio::io::duplex(64);
    let client = async {
        assert!(matches!(read_host_frame(&mut main).await.unwrap(),
            Frame::Event { event } if event.sequence == 1 && matches!(event.payload, Payload::Reset { .. })));
        assert!(matches!(read_host_frame(&mut main).await.unwrap(),
            Frame::Event { event } if event.sequence == 4 && event.subscription_id == "independent-watch"));
        drop(main);
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(service.serve(Box::new(native)), client)
    })
    .await
    .expect("bounded snapshot failure regression");
    assert!(result.is_err());
}

fn reset_event(sequence: u64) -> Event {
    Event {
        version: 1,
        session: crate::business_data_contract::NativeBusinessDataSessionRef {
            handle: "session".into(),
            generation: 1,
        },
        subscription_id: "subscription".into(),
        sequence,
        payload: crate::business_data_contract::NativeBusinessDataEventPayload::Reset {
            code: NativeBusinessDataErrorCode::ResetRequired,
        },
    }
}

#[tokio::test]
async fn event_authority_can_exchange_credentials_without_blocking_ipc() {
    let service = BusinessDataIpc::new(Arc::new(|credentials, events| {
        let requester = credentials.clone();
        assert!(events
            .try_send(QueuedBusinessDataEvent {
                event: reset_event(1),
                alive: Arc::new(AtomicBool::new(true)),
                failed: Arc::new(AtomicBool::new(false)),
                authority: Arc::new(move || {
                    let requester = requester.clone();
                    Box::pin(async move {
                        requester
                            .request("target", "connection", 0, None)
                            .await
                            .is_ok()
                    })
                }),
            })
            .is_ok());
        Ok(Arc::new(TestDispatcher { credentials }))
    }));
    let (native, mut main) = tokio::io::duplex(64);
    let client = async {
        let challenge = match read_host_frame(&mut main).await.unwrap() {
            Frame::CredentialChallenge { challenge } => challenge,
            _ => panic!("authority must obtain credentials before its event"),
        };
        write_host_frame(
            &mut main,
            &Frame::CredentialReply {
                reply: NativeBusinessDataCredentialReply {
                    version: 1,
                    request_id: challenge.request_id,
                    connection_id: challenge.connection_id,
                    session_epoch: 0,
                    capability_token: Some("test-token".into()),
                    device_proof: None,
                },
            },
        )
        .await
        .unwrap();
        assert!(
            matches!(read_host_frame(&mut main).await.unwrap(), Frame::Event { event } if event.sequence == 1)
        );
        drop(main);
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(service.serve(Box::new(native)), client)
    })
    .await
    .expect("event validation must not deadlock credential replies");
    assert!(result.is_err());
}

#[tokio::test]
async fn closing_ipc_cancels_pending_event_authority() {
    let entered = Arc::new(tokio::sync::Notify::new());
    let dropped = Arc::new(AtomicBool::new(false));
    let service = BusinessDataIpc::new(Arc::new({
        let entered = entered.clone();
        let dropped = dropped.clone();
        move |credentials, events| {
            let entered = entered.clone();
            let dropped = dropped.clone();
            assert!(events
                .try_send(QueuedBusinessDataEvent {
                    event: reset_event(1),
                    alive: Arc::new(AtomicBool::new(true)),
                    failed: Arc::new(AtomicBool::new(false)),
                    authority: Arc::new(move || {
                        let entered = entered.clone();
                        let guard = DropSignal(dropped.clone());
                        Box::pin(async move {
                            let _guard = guard;
                            entered.notify_one();
                            std::future::pending::<bool>().await
                        })
                    }),
                })
                .is_ok());
            Ok(Arc::new(TestDispatcher { credentials }))
        }
    }));
    let (native, main) = tokio::io::duplex(64);
    let client = async {
        entered.notified().await;
        drop(main);
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(service.serve(Box::new(native)), client)
    })
    .await
    .expect("pending event authority must cancel with IPC");
    assert!(result.is_err());
    assert!(dropped.load(Ordering::SeqCst));
}

struct DropSignal(Arc<AtomicBool>);
impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
struct WaitingDispatcher {
    entered: Arc<tokio::sync::Notify>,
    dropped: Arc<AtomicBool>,
}
impl BusinessDataDispatcher for WaitingDispatcher {
    fn dispatch(&self, _: Request) -> BusinessDataDispatchFuture {
        let guard = DropSignal(self.dropped.clone());
        let entered = self.entered.clone();
        Box::pin(async move {
            let _guard = guard;
            entered.notify_one();
            std::future::pending().await
        })
    }
}
#[tokio::test]
async fn closing_stream_drops_pending_dispatch_without_detached_tasks() {
    let entered = Arc::new(tokio::sync::Notify::new());
    let dropped = Arc::new(AtomicBool::new(false));
    let service = BusinessDataIpc::new(Arc::new({
        let entered = entered.clone();
        let dropped = dropped.clone();
        move |_, _| {
            Ok(Arc::new(WaitingDispatcher {
                entered: entered.clone(),
                dropped: dropped.clone(),
            }))
        }
    }));
    let (native, mut main) = tokio::io::duplex(1024);
    let serve = service.serve(Box::new(native));
    let client = async {
        write_host_frame(&mut main, &open_request()).await.unwrap();
        entered.notified().await;
        drop(main);
    };
    let (result, ()) = tokio::join!(serve, client);
    assert!(result.is_err());
    assert!(dropped.load(Ordering::SeqCst));
}
