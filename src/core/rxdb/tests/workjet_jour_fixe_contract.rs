#[path = "../../business_os/workjet_jour_fixe_contract.generated.rs"]
mod contract;
use contract::WireValidate;
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/workjet-jour-fixe-v1.json")).unwrap()
}

#[test]
fn jour_fixe_native_accepts_and_rejects_shared_wire_cases() {
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
fn jour_fixe_meeting_roundtrip_retains_feedback_audio_and_goal_revision() {
    let fixture = fixture();
    for case in fixture["valid_cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "Meeting")
    {
        let meeting: contract::Meeting = serde_json::from_value(case["value"].clone()).unwrap();
        meeting.validate().unwrap();
        assert_eq!(serde_json::to_value(meeting).unwrap(), case["value"]);
    }
}
