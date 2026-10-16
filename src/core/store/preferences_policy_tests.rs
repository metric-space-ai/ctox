use super::*;
use super::manifest_rules::ManifestRules;
use serde_json::json;
use std::collections::BTreeSet;

#[test]
fn prefs_app_boundary_uses_authenticated_user_and_app_even_for_admin() {
    let platform = ManifestRules::from_manifest("ctox-platform","4.0",
        [ctox_store::preferences::COLLECTION.into()].into(),&json!({})).unwrap();
    let calendar = ManifestRules::from_manifest("calendar","1",BTreeSet::new(),&json!({})).unwrap();
    let snapshot=RightsSnapshot{rights_ver:1,rules:vec![],manifests:vec![platform,calendar]};
    let own=ctox_store::preferences::Context::authenticated("calendar","alice",1).unwrap().document("theme",json!("light")).unwrap();
    assert_eq!(snapshot.authorize_app_read("calendar","admin","alice",ctox_store::preferences::COLLECTION,&own).unwrap(),own);
    for (app,user) in [("mail","alice"),("calendar","bob")] {
        let foreign=ctox_store::preferences::Context::authenticated(app,user,1).unwrap().document("theme",json!("secret")).unwrap();
        assert!(snapshot.authorize_app_read("calendar","admin","alice",ctox_store::preferences::COLLECTION,&foreign).is_err());
        assert!(snapshot.authorize_app_write("calendar","admin","alice",ctox_store::preferences::COLLECTION,None,Some(&foreign),None).is_err());
    }
    let a=snapshot.resolve_app_collection("calendar","admin","alice",ctox_store::preferences::COLLECTION).unwrap().hash().unwrap();
    let b=snapshot.resolve_app_collection("calendar","admin","bob",ctox_store::preferences::COLLECTION).unwrap().hash().unwrap();
    assert_ne!(a,b);
    assert_eq!(a,ctox_store::preferences::Context::authenticated("calendar","alice",1).unwrap().scope_hash());
}
