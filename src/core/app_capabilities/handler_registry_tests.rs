//! E17.1: every statically identifiable app-handler capability call must be registered.
use super::*;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

fn app_calls() -> Vec<Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = r#"import {scanCalls} from './src/apps/business-os/v4/os-primitives/scan-calls.mjs'; console.log(JSON.stringify(scanCalls('src/apps/business-os',['modules'])));"#;
    let output = std::process::Command::new("node")
        .args(["--input-type=module", "--eval", script]).current_dir(root).output()
        .expect("Node is required for the app-handler registry guard");
    assert!(output.status.success(), "handler scanner failed: {}", String::from_utf8_lossy(&output.stderr));
    let calls: Vec<Value> = serde_json::from_slice(&output.stdout).expect("scanner JSON");
    assert!(!calls.is_empty(), "handler inventory must not silently disappear");
    calls
}

fn registered_paths() -> BTreeSet<String> {
    cap3::registry().descriptors().as_array().expect("registry descriptors").iter().map(|entry| {
        let name = entry["name"].as_str().expect("capability name");
        let function = entry["fn"].as_str().expect("capability function");
        if name.rsplit('.').next() == Some(function) || (name == "doc.html" && function == "query") {
            name.to_owned()
        } else { format!("{name}.{function}") }
    }).collect()
}

fn missing_calls(calls: &[Value], paths: &BTreeSet<String>) -> Vec<String> {
    calls.iter().filter_map(|call| {
        let path = call["path"].as_str().expect("call path");
        (!paths.contains(path)).then(|| format!("{}:{} ctx.cap.{}", call["file"].as_str().unwrap_or("?"),call["line"],path))
    }).collect()
}

#[test]
fn app_handler_capability_calls_exist_in_native_registry() {
    let missing = missing_calls(&app_calls(), &registered_paths());
    assert!(missing.is_empty(), "unregistered app-handler capabilities:\n{}", missing.join("\n"));
}

#[test]
fn knowledge_app_consumers_keep_their_native_capability() {
    let calls: Vec<_> = app_calls().into_iter().filter(|call| call["path"].as_str().is_some_and(|path| path.starts_with("knowledge."))).collect();
    assert!(!calls.is_empty(), "real Knowledge consumers must remain covered");
    let missing = missing_calls(&calls, &registered_paths());
    assert!(missing.is_empty(), "Knowledge consumer regression:\n{}", missing.join("\n"));
}
