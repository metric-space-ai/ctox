use ctox_office_engine::{delimited_text_to_xlsx, sha256_hex};
use serde_json::{json, Value};
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn native_cli_reads_and_patches_without_overwriting_existing_files() {
    let folder = std::env::temp_dir().join(format!(
        "ctox-office-cli-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&folder).unwrap();
    let input = folder.join("input.xlsx");
    let output = folder.join("output.xlsx");
    let patch_path = folder.join("patch.json");
    let bytes = delimited_text_to_xlsx(b"Keep,10\n").unwrap();
    fs::write(&input, &bytes).unwrap();
    let bin = env!("CARGO_BIN_EXE_ctox-office-engine");
    let capabilities = Command::new(bin).arg("capabilities").output().unwrap();
    assert!(capabilities.status.success());
    let capabilities: Value = serde_json::from_slice(&capabilities.stdout).unwrap();
    assert!(capabilities["operations"]["spreadsheet-patch"].is_string());
    let read = Command::new(bin)
        .args(["read", "spreadsheet"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        read.status.success(),
        "{}",
        String::from_utf8_lossy(&read.stderr)
    );
    let read: Value = serde_json::from_slice(&read.stdout).unwrap();
    let sheet = read["worksheets"][0]["name"].as_str().unwrap();
    let patch = json!({"base_sha256":sha256_hex(&bytes),"cells":[
        {"sheet":sheet,"cell":"B1","value":{"type":"number","value":20}},
        {"sheet":sheet,"cell":"C12","value":{"type":"text","value":"=literal & text"}}
    ]});
    fs::write(&patch_path, serde_json::to_vec(&patch).unwrap()).unwrap();
    let apply = || {
        Command::new(bin)
            .arg("spreadsheet-patch")
            .arg(&input)
            .arg(&output)
            .arg(&patch_path)
            .output()
            .unwrap()
    };
    let applied = apply();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let report: Value = serde_json::from_slice(&applied.stdout).unwrap();
    assert_eq!(report["updated_cells"], 2);
    assert_eq!(report["business_os_writeback"], false);
    let saved = fs::read(&output).unwrap();
    let repeated = apply();
    assert!(!repeated.status.success());
    assert_eq!(fs::read(&output).unwrap(), saved);
    assert_eq!(fs::read(&input).unwrap(), bytes);
    let read = Command::new(bin)
        .args(["read", "spreadsheet"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(read.status.success());
    let read: Value = serde_json::from_slice(&read.stdout).unwrap();
    assert!(read["worksheets"][0]["cells"]
        .as_array()
        .unwrap()
        .iter()
        .any(|cell| cell["reference"] == "C12"
            && cell["display"] == "=literal & text"
            && cell.get("formula").is_none()));
    // This directory was created exclusively by this test, never reused.
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn native_cli_csv_xlsx_edit_export_and_fresh_process_reopen() {
    let folder = std::env::temp_dir().join(format!(
        "ctox-office-roundtrip-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&folder).unwrap();
    let canonical = folder.join("canonical.xlsx");
    let patched = folder.join("patched.xlsx");
    let editor = folder.join("edited.xlsy");
    let exported = folder.join("exported.xlsx");
    let patch_path = folder.join("patch.json");
    let csv = b"Before edit,Retained CSV marker\n";
    let canonical_bytes = delimited_text_to_xlsx(csv).unwrap();
    assert!(canonical_bytes.starts_with(b"PK"));
    assert_ne!(sha256_hex(csv), sha256_hex(&canonical_bytes));
    fs::write(&canonical, &canonical_bytes).unwrap();

    let bin = env!("CARGO_BIN_EXE_ctox-office-engine");
    let run = |args: &[&std::ffi::OsStr]| -> Value {
        // Every operation opens the file in a fresh native process.
        let result = Command::new(bin).args(args).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&result.stdout).unwrap()
    };
    let initial = run(&[
        "read".as_ref(),
        "spreadsheet".as_ref(),
        canonical.as_os_str(),
    ]);
    let sheet = initial["worksheets"][0]["name"].as_str().unwrap();
    let patch = json!({
        "base_sha256": sha256_hex(&canonical_bytes),
        "cells": [{
            "sheet": sheet, "cell": "A1",
            "value": {"type": "text", "value": "Edited & saved Grüße"}
        }]
    });
    fs::write(&patch_path, serde_json::to_vec(&patch).unwrap()).unwrap();
    let report = run(&[
        "spreadsheet-patch".as_ref(),
        canonical.as_os_str(),
        patched.as_os_str(),
        patch_path.as_os_str(),
    ]);
    assert_eq!(report["updated_cells"], 1);
    assert_eq!(report["business_os_writeback"], false);

    run(&[
        "prepare-editor".as_ref(),
        "spreadsheet".as_ref(),
        patched.as_os_str(),
        editor.as_os_str(),
    ]);
    let editor_bytes = fs::read(&editor).unwrap();
    assert!(editor_bytes.starts_with(b"XLSY;v10;0;"));
    assert_ne!(sha256_hex(&editor_bytes), sha256_hex(&canonical_bytes));
    let exported_report = run(&[
        "export".as_ref(),
        "spreadsheet".as_ref(),
        editor.as_os_str(),
        canonical.as_os_str(),
        exported.as_os_str(),
    ]);
    let exported_bytes = fs::read(&exported).unwrap();
    assert!(exported_bytes.starts_with(b"PK"));
    assert_eq!(exported_report["sha256"], sha256_hex(&exported_bytes));
    assert_eq!(exported_report["byte_size"], exported_bytes.len());
    assert_ne!(sha256_hex(&exported_bytes), sha256_hex(&canonical_bytes));
    assert_eq!(fs::read(&canonical).unwrap(), canonical_bytes);

    let reopened = run(&[
        "read".as_ref(),
        "spreadsheet".as_ref(),
        exported.as_os_str(),
    ]);
    let cells = reopened["worksheets"][0]["cells"].as_array().unwrap();
    for (reference, expected) in [
        ("A1", "Edited & saved Grüße"),
        ("B1", "Retained CSV marker"),
    ] {
        let cell = cells
            .iter()
            .find(|cell| cell["reference"] == reference)
            .unwrap();
        assert_eq!(cell["display"], expected);
        assert!(cell.get("formula").is_none());
    }
    // Native conversion/file persistence only; no Business OS SaveACK or sync claim.
    fs::remove_dir_all(folder).unwrap();
}
