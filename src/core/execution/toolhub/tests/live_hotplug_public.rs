//! Live check of the pure hot-plug path with a brand-new, credential-free service
//! (`specs/zippopotam.json`): no compiled adapter, loaded only through the
//! instance registry and hot reload.
//! `CTOX_TOOLHUB_LIVE_PUBLIC=1 cargo test --test live_hotplug_public -- --ignored --nocapture`
mod support;
use serde_json::json;

#[test]
#[ignore]
fn brand_new_public_service_via_registry_and_hot_reload() {
    let tools = [
        ("ZIPPOPOTAM_LOOKUP_POSTAL_CODE", json!({"country": "de", "postal_code": "10115"})),
        ("ZIPPOPOTAM_FIND_POSTAL_CODES", json!({"country": "us", "state": "ma", "city": "Belmont"})),
    ];
    let Some(out) = support::live_hotplug_public(include_str!("../specs/zippopotam.json"), &tools) else { return };
    let mut fails = vec![];
    for ((tool, _), r) in tools.iter().zip(&out) {
        match r {
            Ok(_) => println!("LIVE PASS {tool}"),
            Err(e) => { println!("LIVE FAIL {tool}: {e}"); fails.push(*tool); }
        }
    }
    let first = out[0].as_ref().unwrap();
    assert_eq!(first["postal_code"], json!("10115"));
    assert_eq!(first["places"][0]["place name"], json!("Berlin"));
    assert!(fails.is_empty(), "live failures: {fails:?}");
}
