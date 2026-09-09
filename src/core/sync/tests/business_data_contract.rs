use ctox_sync::{
    business_data::decode_request, business_data_contract::NativeBusinessDataOperation,
};
use serde_json::{json, Value};

fn decode(
    operation: Value,
) -> std::io::Result<ctox_sync::business_data_contract::NativeBusinessDataRequest> {
    decode_request(
        &serde_json::to_vec(&json!({
            "version": 1, "requestId": "stable-request", "operation": operation,
        }))
        .unwrap(),
    )
}
fn query() -> Value {
    json!({"type":"query", "session":{"handle":"native-handle","generation":1},
        "query":{"collection":"workjet_project_chats", "scope":{"type":"project","projectId":"project"},
            "query":{"selector":{}}, "pageSize":200}})
}
#[test]
fn caller_cannot_supply_authentication_or_rebind_a_session() {
    for injected in [
        json!({"type":"open","targetId":"saved","actor":"owner"}),
        json!({"type":"open","targetId":"saved","capabilityToken":"token"}),
        json!({"type":"open","targetId":"saved","instanceId":"other","userId":"owner"}),
        json!({"type":"status","session":{"handle":"native-handle","generation":1,"userId":"owner"}}),
    ] {
        assert!(decode(injected).is_err());
    }
    // A syntactically valid target is still not an authenticated or ready session.
    assert!(matches!(
        decode(json!({"type":"open","targetId":"saved"}))
            .unwrap()
            .operation,
        NativeBusinessDataOperation::Open { .. }
    ));
}
#[test]
fn pages_generations_and_opaque_cursors_have_explicit_bounds() {
    assert!(decode(query()).is_ok());
    for size in [0, 201] {
        let mut input = query();
        input["query"]["pageSize"] = json!(size);
        assert!(decode(input).is_err());
    }
    for generation in [0u64, 9_007_199_254_740_992] {
        let mut input = query();
        input["session"]["generation"] = json!(generation);
        assert!(decode(input).is_err());
    }
    for cursor in [String::new(), "x".repeat(4097)] {
        let mut input = query();
        input["pageCursor"] = json!(cursor);
        assert!(decode(input).is_err());
    }
    let mut input = query();
    input["query"]["scope"]["projectId"] = json!("");
    assert!(decode(input).is_err());
    assert!(decode_request(&vec![b' '; ctox_sync::ipc::IPC_MAX_FRAME_BYTES + 1]).is_err());
}
#[test]
fn command_submission_preserves_the_callers_durable_id() {
    let request = decode(json!({"type":"submitCommand",
        "session":{"handle":"native-handle","generation":2},
        "command":{"commandId":"existing-command-id","commandType":"project.chat.create","payload":{}}})).unwrap();
    let NativeBusinessDataOperation::SubmitCommand { command, .. } = request.operation else {
        panic!()
    };
    assert_eq!(command.command_id, "existing-command-id");
    assert!(decode(json!({"type":"observeCommand",
        "session":{"handle":"native-handle","generation":2},"commandId":""}))
    .is_err());
}
