//! Checked-in incident app -> actual native guard grammar, without a policy stand-in.
use super::super::manifest_rules::ManifestRules;
use serde_json::{json,Value};
const LEADS:&str="outbound_lead_generation_leads";
fn candidate()->Value{serde_json::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"),"/src/apps/business-os/v4/manifests/outbound-lead-generation.manifest.json"))).unwrap()}
#[test]
fn p6_fields_outbound_manifest_guard_is_loadable(){
    let manifest=candidate();
    let owned=manifest["collections"].as_object().unwrap().keys().cloned().collect();
    let result=ManifestRules::from_manifest("outbound-lead-generation","p6-contract",owned,&manifest);
    assert!(result.is_ok(),"checked-in Outbound native field guard is not loadable: {:?}",result.err());
}
#[test]
fn p6_fields_outbound_contact_native_status_is_guarded(){
    let manifest=candidate();
    // Isolate the declared field paths from the independent metadata-load
    // failure. This neither fixes the source manifest nor weakens its guard.
    let paths=manifest["write_guard"][LEADS]["command_only_fields"].clone();
    let guard=ManifestRules::from_manifest("outbound-lead-generation","p6-contract",[LEADS.into()].into(),
        &json!({"write_guard":{"outbound_lead_generation_leads":{"command_only_fields":paths}}})).unwrap();
    let before=json!({"id":"synthetic-lead","contacts":[{"id":"synthetic-contact","name":"A","field_status":{"email":{"revision":"native-1","review":"approved"}}}]});
    let mut peer=before.clone();peer["contacts"][0]["name"]=json!("peer name");
    assert!(guard.guard_direct_write(LEADS,Some(&before),Some(&peer)).is_ok(),"peer contact name was denied by native-only guard");
    let mut changed=before.clone();changed["contacts"][0]["field_status"]["email"]["review"]=json!("rejected");
    assert_eq!(guard.guard_direct_write(LEADS,Some(&before),Some(&changed)),Err("command_only_field".into()),
        "declared contact native field-status was writable");
}
