use std::path::PathBuf;
use std::process::Command;

#[test]
fn dach_scrape_targets_pass_fixture_gate() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let gate = manifest_dir.join("scrape-targets/tests/fixture-gate.test.mjs");
    let status = Command::new("node")
        .arg("--test")
        .arg(&gate)
        .current_dir(&manifest_dir)
        .status()
        .unwrap_or_else(|error| panic!("failed to start Node fixture gate: {error}"));
    assert!(status.success(), "scrape target fixture gate failed");
}
