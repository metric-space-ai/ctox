#[test]
fn receipt_revision_matches_the_declared_engine_dependency() {
    let dependency = include_str!("../Cargo.toml")
        .lines()
        .find(|line| line.starts_with("aria2-rust = "))
        .expect("engine dependency must remain pinned");
    assert!(
        dependency.contains(&format!("rev = \"{}\"", ctox_transfers::ENGINE_REVISION)),
        "receipt provenance must name the actual dependency revision"
    );
}
