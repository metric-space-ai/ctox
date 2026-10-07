#[path = "../../business_os/workjet_project_kpis_contract.generated.rs"]
mod contract;
use contract::WireValidate;
use serde_json::Value;
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/workjet-project-kpis-v1.json")).unwrap()
}
#[test]
fn prompted_kpis_agree_on_evidence_revision_and_missing_source() {
    let fixture = fixture();
    assert_eq!(fixture["schema"], contract::CONTRACT_SCHEMA);
    assert_eq!(fixture["contract_version"], contract::CONTRACT_VERSION);
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
fn prompted_kpis_roundtrip_keeps_source_computation_and_freshness() {
    let fixture = fixture();
    let project: contract::ProjectKpis =
        serde_json::from_value(fixture["valid_cases"][0]["value"].clone()).unwrap();
    project.validate().unwrap();
    assert_eq!(
        serde_json::to_value(project).unwrap(),
        fixture["valid_cases"][0]["value"]
    );
    for case in fixture["valid_cases"].as_array().unwrap() {
        if case["type"] == "KpiSnapshot" {
            let snapshot: contract::KpiSnapshot =
                serde_json::from_value(case["value"].clone()).unwrap();
            snapshot.validate().unwrap();
            assert_eq!(serde_json::to_value(snapshot).unwrap(), case["value"]);
        }
    }
}
