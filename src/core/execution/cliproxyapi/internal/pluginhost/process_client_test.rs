// Origin: CTOX
// License: AGPL-3.0-only

use super::{
    abi::{PluginCall, PluginClientError},
    callback_contexts::{CallbackAuthority, CallbackContextRegistry},
    host_callbacks::{HostCallbackFuture, HostCallbackHandler, HostCallbackRouter},
    process_client::exchange_unary,
    process_transport::{read_process_message, write_process_message},
    rpc_schema::{ProcessMessage, PROCESS_PROTOCOL_VERSION},
};
use crate::sdk::pluginabi::Envelope;
use serde_json::{
    json,
    value::{to_raw_value, RawValue},
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct Echo(Arc<AtomicUsize>);
impl HostCallbackHandler for Echo {
    fn call<'a>(
        &'a self,
        _: &'a CallbackAuthority,
        payload: &'a RawValue,
    ) -> HostCallbackFuture<'a> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Envelope::success(Some(payload.to_owned())))
        })
    }
}

async fn callback_case(
    parent_plugin: &str,
    lease_plugin: &str,
    wrong_context: bool,
    expect_allowed: bool,
) {
    let contexts = CallbackContextRegistry::new();
    let lease = contexts.open(CallbackAuthority::new(lease_plugin, None));
    let foreign = contexts.open(CallbackAuthority::new(lease_plugin, None));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut router = HostCallbackRouter::new(contexts);
    router
        .register("host.log", Arc::new(Echo(calls.clone())))
        .unwrap();
    let id = lease.id().to_owned();
    let supplied = if wrong_context {
        foreign.id().to_owned()
    } else {
        id.clone()
    };
    let (mut host, mut child) = tokio::io::duplex(4096);
    let peer = tokio::spawn(async move {
        let ProcessMessage::Request { request_id, .. } =
            read_process_message(&mut child).await.unwrap().unwrap()
        else {
            panic!("request");
        };
        write_process_message(
            &mut child,
            &ProcessMessage::Request {
                protocol_version: PROCESS_PROTOCOL_VERSION,
                request_id: "child-callback".into(),
                method: "host.log".into(),
                deadline_unix_ms: None,
                payload: to_raw_value(&json!({"host_callback_id":supplied,"message":"safe"}))
                    .unwrap(),
            },
        )
        .await
        .unwrap();
        let ProcessMessage::Response {
            request_id: callback_id,
            envelope,
            ..
        } = read_process_message(&mut child).await.unwrap().unwrap()
        else {
            panic!("callback response");
        };
        assert_eq!(callback_id, "child-callback");
        assert_eq!(envelope.ok, expect_allowed);
        if expect_allowed {
            assert_eq!(envelope.result.unwrap().get(), r#"{"message":"safe"}"#);
        } else {
            assert_eq!(envelope.error.unwrap().code, "callback_denied");
        }
        write_process_message(
            &mut child,
            &ProcessMessage::Response {
                protocol_version: PROCESS_PROTOCOL_VERSION,
                request_id,
                envelope: Envelope::success(Some(to_raw_value(&json!({"done":true})).unwrap())),
            },
        )
        .await
        .unwrap();
    });
    let call = PluginCall {
        method: "quota.fetch".into(),
        deadline_unix_ms: None,
        payload: to_raw_value(&json!({"host_callback_id":id})).unwrap(),
    };
    assert!(
        exchange_unary(&mut host, call, parent_plugin, &router)
            .await
            .unwrap()
            .ok
    );
    peer.await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), usize::from(expect_allowed));
}

#[tokio::test]
async fn process_callback_uses_bound_call_and_strips_context_before_handler() {
    callback_case("quota", "quota", false, true).await;
}
#[tokio::test]
async fn process_callback_rejects_another_live_call_in_same_plugin() {
    callback_case("quota", "quota", true, false).await;
}
#[tokio::test]
async fn process_callback_rejects_other_plugin_identity() {
    callback_case("foreign", "quota", false, false).await;
}

#[tokio::test]
async fn process_mismatched_response_id_is_terminal() {
    let (mut host, mut child) = tokio::io::duplex(4096);
    let peer = tokio::spawn(async move {
        read_process_message(&mut child).await.unwrap().unwrap();
        write_process_message(
            &mut child,
            &ProcessMessage::Response {
                protocol_version: PROCESS_PROTOCOL_VERSION,
                request_id: "stale-call".into(),
                envelope: Envelope::success(None),
            },
        )
        .await
        .unwrap();
    });
    let router = HostCallbackRouter::new(CallbackContextRegistry::new());
    let result = exchange_unary(
        &mut host,
        PluginCall {
            method: "quota.describe".into(),
            deadline_unix_ms: None,
            payload: to_raw_value(&json!({})).unwrap(),
        },
        "quota",
        &router,
    )
    .await;
    assert!(matches!(result, Err(PluginClientError::InvalidResponse)));
    peer.await.unwrap();
}
