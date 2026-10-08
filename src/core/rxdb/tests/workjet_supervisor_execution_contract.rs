#[path = "../../business_os/workjet_supervisor_execution_contract.generated.rs"]
mod contract;
use contract::WireValidate;
#[test]
fn supervisor_observer_contract_agrees_on_opt_in_bounds_and_unknown_fields() {
    let spec: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/workjet-supervisor-execution-v1.json"
    ))
    .unwrap();
    assert_eq!(spec["schema"], contract::CONTRACT_SCHEMA);
    assert_eq!(spec["contract_version"], contract::CONTRACT_VERSION);
    for case in spec["valid_cases"].as_array().unwrap() {
        contract::validate_fixture(case["type"].as_str().unwrap(), case["value"].clone()).unwrap();
    }
    for case in spec["invalid_cases"].as_array().unwrap() {
        assert!(
            contract::validate_fixture(case["type"].as_str().unwrap(), case["value"].clone())
                .is_err(),
            "{case}"
        );
    }
}
#[test]
fn native_execution_page_roundtrip_retains_real_keys_and_safe_cursor() {
    let page = contract::ExecutionPage {
        command_id: "canonical-command".into(),
        task_id: "native-task".into(),
        attempt: Some(contract::AttemptRef {
            attempt_id: "worker-attempt:actual".into(),
            run_id: None,
            attempt_index: Some(47),
            status: None,
            started_at_ms: Some(1),
            finished_at_ms: None,
        }),
        events: vec![contract::ExecutionEvent {
            id: "native-event".into(),
            sequence: 23,
            kind: "worker.turn_started".into(),
            title: "Started".into(),
            created_at_ms: 1,
            tool_name: None,
            call_id: None,
            success: None,
        }],
        next_cursor: Some(contract::EventCursor {
            after_sequence: 23,
            after_event_id: "native-event".into(),
        }),
        has_more: false,
    };
    page.validate().unwrap();
    let before = serde_json::to_value(page).unwrap();
    let restored: contract::ExecutionPage = serde_json::from_value(before.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), before);
}
