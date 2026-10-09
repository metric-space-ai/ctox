#[path = "../../business_os/workjet_presentation_contract.generated.rs"]
mod contract;
use contract::WireValidate;
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/workjet-presentation-v1.json")).unwrap()
}

#[test]
fn presentation_native_accepts_and_rejects_shared_wire_cases() {
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
fn presentation_manifest_roundtrip_is_lossless() {
    let fixture = fixture();
    for case in fixture["valid_cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "PresentationManifest")
    {
        let manifest: contract::PresentationManifest =
            serde_json::from_value(case["value"].clone()).unwrap();
        manifest.validate().unwrap();
        assert_eq!(serde_json::to_value(manifest).unwrap(), case["value"]);
    }
}
