use super::*;
use crate::business_data_contract::{
    NativeBusinessDataCredentialReply, NativeBusinessDataErrorCode, NativeBusinessDataOperation,
    NativeBusinessDataResult,
};
use std::sync::atomic::{AtomicBool, Ordering};

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
