use sha2::{Digest, Sha256};
use std::path::Path;

fn imported_files(root: &Path, directory: &Path, files: &mut std::collections::BTreeSet<String>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap();
        let kind = entry.file_type().unwrap();
        // Cargo output is disposable and cannot add auto-discovered source/build targets.
        if relative == Path::new("target") && kind.is_dir() {
            continue;
        }
        assert!(
            !kind.is_symlink(),
            "immutable engine source cannot contain symlinks: {relative:?}"
        );
        if kind.is_dir() {
            imported_files(root, &path, files);
        } else {
            assert!(kind.is_file(), "unexpected engine file type: {relative:?}");
            files.insert(
                relative
                    .to_str()
                    .unwrap()
                    .replace(std::path::MAIN_SEPARATOR, "/"),
            );
        }
    }
}

#[test]
fn receipt_revision_matches_the_in_tree_engine_source() {
    let dependency = include_str!("../Cargo.toml")
        .lines()
        .find(|line| line.starts_with("aria2-rust = "))
        .expect("engine dependency must remain declared");
    assert!(dependency.contains("path = \"aria2-rust\""));
    assert!(!dependency.contains("git ="));
    assert!(dependency.contains("default-features = false"));

    let manifest = include_bytes!("../aria2-rust/PROVENANCE.json");
    // Bind the entire immutable-source file list to this reviewed upstream pin.
    assert_eq!(
        format!("{:x}", Sha256::digest(manifest)),
        "64072edfdea01b427dcc47348396a23140c3b993a19096d2f3e04530117bc858",
        "engine source changes require an explicit provenance/receipt pin update"
    );
    let provenance: serde_json::Value = serde_json::from_slice(manifest).unwrap();
    assert_eq!(provenance["revision"], ctox_transfers::ENGINE_REVISION);
    assert_eq!(provenance["license"], "GPL-2.0-or-later");
    let files = provenance["files"].as_object().unwrap();
    assert_eq!(files.len(), 55, "retain the complete tracked upstream tree");
    let overlays = provenance["ctox_overlays"].as_object().unwrap();
    assert_eq!(
        overlays.len(),
        1,
        "only the reviewed lockfile overlay is allowed"
    );
    let lock_overlay = &overlays["Cargo.lock"];
    assert_eq!(lock_overlay["upstream_sha256"], files["Cargo.lock"]);
    assert_eq!(
        lock_overlay["source_commit"],
        "505771b49fe0f0754bb5d9fd7f082817da34481f"
    );
    let engine = Path::new(env!("CARGO_MANIFEST_DIR")).join("aria2-rust");
    let mut actual = std::collections::BTreeSet::new();
    imported_files(&engine, &engine, &mut actual);
    let mut expected: std::collections::BTreeSet<String> = files.keys().cloned().collect();
    expected.insert("PROVENANCE.json".into());
    assert_eq!(
        actual, expected,
        "reject unlisted engine source/build files"
    );
    for (name, expected) in files {
        let expected = if name == "Cargo.lock" {
            lock_overlay["installed_sha256"].as_str().unwrap()
        } else {
            expected.as_str().unwrap()
        };
        let bytes = std::fs::read(engine.join(name)).unwrap_or_else(|error| {
            panic!("imported engine file {name} must remain available: {error}")
        });
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            expected,
            "imported engine file {name} differs from the pinned upstream source"
        );
    }

    for lock in [
        include_str!("../Cargo.lock"),
        include_str!("../../../../Cargo.lock"),
    ] {
        assert!(
            !lock.contains("git+https://github.com/mkh-welsch/aria2-rust"),
            "public installer resolution must not require private engine Git access"
        );
    }
}
