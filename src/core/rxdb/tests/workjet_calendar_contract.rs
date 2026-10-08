#[path = "../../business_os/workjet_calendar_contract.generated.rs"]
mod contract;
use contract::WireValidate;
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/workjet-calendar-v1.json")).unwrap()
}

#[test]
fn calendar_native_accepts_and_rejects_shared_wire_cases() {
    let fixture = fixture();
    assert_eq!(fixture["contract_version"], contract::CONTRACT_VERSION);
    assert_eq!(fixture["schema"], contract::CONTRACT_SCHEMA);
    for case in fixture["valid_cases"].as_array().unwrap() {
        contract::validate_fixture(case["type"].as_str().unwrap(), case["value"].clone()).unwrap();
    }
    for case in fixture["invalid_cases"].as_array().unwrap() {
        assert!(
            contract::validate_fixture(case["type"].as_str().unwrap(), case["value"].clone())
                .is_err(),
            "{}",
            case["reason"]
        );
    }
}

#[test]
fn calendar_event_roundtrip_keeps_wire_shape() {
    let fixture = fixture();
    for case in fixture["valid_cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "CalendarEvent")
    {
        let event: contract::CalendarEvent = serde_json::from_value(case["value"].clone()).unwrap();
        event.validate().unwrap();
        assert_eq!(serde_json::to_value(event).unwrap(), case["value"]);
    }
}
