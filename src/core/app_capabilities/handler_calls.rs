//! Shared source scanner and registry comparison, without daemon dependencies.
use super::CapabilityRegistry;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn app_calls() -> Vec<Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors()
        .find(|path| path.join("src/apps/business-os/modules").is_dir())
        .expect("repository app handlers must be available");
    let script = r#"import {scanCalls} from './src/apps/business-os/v4/os-primitives/scan-calls.mjs'; console.log(JSON.stringify(scanCalls('src/apps/business-os',['modules'])));"#;
    let output = std::process::Command::new("node").args(["--input-type=module", "--eval", script])
        .current_dir(root).output().expect("Node is required for the app-handler registry guard");
    assert!(output.status.success(), "handler scanner failed: {}", String::from_utf8_lossy(&output.stderr));
    let calls: Vec<Value> = serde_json::from_slice(&output.stdout).expect("scanner JSON");
    assert!(!calls.is_empty(), "handler inventory must not silently disappear");
    calls
}

pub(super) fn registered_paths(registry: CapabilityRegistry) -> BTreeSet<String> {
    registry.descriptors().as_array().expect("registry descriptors").iter().map(|entry| {
        let name = entry["name"].as_str().expect("capability name");
        let function = entry["fn"].as_str().expect("capability function");
        if name.rsplit('.').next() == Some(function) || (name == "doc.html" && function == "query") {
            name.to_owned()
        } else { format!("{name}.{function}") }
    }).collect()
}

pub(super) fn missing_calls(calls: &[Value], paths: &BTreeSet<String>) -> Vec<String> {
    calls.iter().filter_map(|call| {
        let path = call["path"].as_str().expect("call path");
        (!paths.contains(path)).then(|| format!("{}:{} ctx.cap.{}", call["file"].as_str().unwrap_or("?"),call["line"],path))
    }).collect()
}
