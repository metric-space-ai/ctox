// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use std::os::unix::fs::PermissionsExt;
#[test]
fn native_machine_configuration_is_local_immutable_and_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("qemu");
    let base = dir.path().join("base.raw");
    std::fs::write(&program, b"operator fixture; never executed").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o500)).unwrap();
    std::fs::write(&base, [0u8; 512]).unwrap();
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o400)).unwrap();
    let config = NativeGuestMachineConfiguration {
        program,
        base_raw: base,
        memory_mib: 768,
        vcpus: 2,
        acceleration: Acceleration::Kvm,
    };
    config.validate().unwrap();
    for mutation in 0..4 {
        let mut bad = config.clone();
        match mutation {
            0 => bad.memory_mib = 0,
            1 => bad.memory_mib = 4097,
            2 => bad.vcpus = 3,
            _ => bad.program = PathBuf::from("relative/qemu"),
        }
        assert!(bad.validate().is_err());
    }
    std::fs::set_permissions(&config.base_raw, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(config.validate().is_err(), "writable base is not admitted");
    std::fs::set_permissions(&config.base_raw, std::fs::Permissions::from_mode(0o400)).unwrap();
    let alias = dir.path().join("alias");
    std::os::unix::fs::symlink(&config.base_raw, &alias).unwrap();
    let mut bad = config.clone();
    bad.base_raw = alias;
    assert!(bad.validate().is_err());
    std::fs::hard_link(&config.base_raw, dir.path().join("hardlink")).unwrap();
    assert!(config.validate().is_err());
    let mut json = serde_json::to_value(&config).unwrap();
    json["arguments"] = serde_json::json!(["-net", "user"]);
    assert!(serde_json::from_value::<NativeGuestMachineConfiguration>(json).is_err());
}
