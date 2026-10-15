// Packaging input only. Production never discovers Node from PATH or downloads it.
fn main() {
    println!("cargo:rerun-if-env-changed=CTOX_APP_RUNTIME_NODE_BUNDLE");
    if std::env::var_os("CARGO_FEATURE_BUNDLED_NODE").is_none() {
        return;
    }
    let input = std::env::var_os("CTOX_APP_RUNTIME_NODE_BUNDLE").expect(
        "bundled-node requires the platform Node build artifact CTOX_APP_RUNTIME_NODE_BUNDLE",
    );
    let input = std::path::PathBuf::from(input);
    assert!(
        input.is_absolute() && input.is_file(),
        "Node build input must be an absolute file"
    );
    println!("cargo:rerun-if-changed={}", input.display());
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("app-node");
    std::fs::copy(input, output).expect("copy Node packaging input");
}
