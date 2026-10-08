// Origin: CTOX
// License: AGPL-3.0-only
//! Privileged local image selection. No paths or QEMU arguments enter through a model/peer request.
use super::*;
use serde::{Deserialize, Serialize};
#[cfg(all(test, target_os = "linux"))]
#[path = "guest_registry_machine_config_tests.rs"]
mod tests;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeGuestMachineConfiguration {
    program: PathBuf,
    base_raw: PathBuf,
    memory_mib: u32,
    vcpus: u8,
    acceleration: Acceleration,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Acceleration {
    Kvm,
    Tcg,
}
impl NativeGuestMachineConfiguration {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            cfg!(target_os = "linux"),
            "native machine restore requires Linux"
        );
        ensure!(
            (32..=4096).contains(&self.memory_mib) && (1..=2).contains(&self.vcpus),
            "native machine budget is invalid"
        );
        for path in [&self.program, &self.base_raw] {
            let metadata = std::fs::symlink_metadata(path)?;
            ensure!(
                path.is_absolute()
                    && std::fs::canonicalize(path)? == *path
                    && metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && (metadata.uid() == 0 || metadata.uid() == unsafe { libc::geteuid() })
                    && metadata.mode() & 0o022 == 0,
                "native machine path is not a canonical operator-owned file"
            );
        }
        let base = std::fs::metadata(&self.base_raw)?;
        ensure!(
            base.len() > 0
                && base.len() <= 16 * 1024 * 1024 * 1024
                && base.len() % 512 == 0
                && base.nlink() == 1
                && base.mode() & 0o222 == 0,
            "native independent raw base must be bounded, immutable and unaliased"
        );
        ensure!(
            std::fs::metadata(&self.program)?.mode() & 0o111 != 0,
            "native QEMU program is not executable"
        );
        Ok(())
    }
    #[cfg(target_os = "linux")]
    pub(super) fn prepared(
        &self,
        parent: PathBuf,
        disk: PathBuf,
    ) -> Result<super::super::guest_runtime::PreparedQemuGuest> {
        self.validate()?;
        Ok(super::super::guest_runtime::PreparedQemuGuest {
            program: self.program.clone(),
            runtime_parent: parent,
            base_raw: self.base_raw.clone(),
            overlay_qcow2: disk,
            memory_mib: self.memory_mib,
            vcpus: self.vcpus,
            acceleration: match self.acceleration {
                Acceleration::Kvm => super::super::guest_runtime::QemuAcceleration::Kvm,
                Acceleration::Tcg => super::super::guest_runtime::QemuAcceleration::Tcg,
            },
        })
    }
}
impl NativeGuestRegistry {
    /// Host startup consumes typed runtime-store configuration before admitting any enrollment.
    pub(crate) fn configure_machine(
        &self,
        config: Option<NativeGuestMachineConfiguration>,
    ) -> Result<()> {
        if let Some(config) = &config {
            config.validate()?;
        }
        ensure!(
            self.guests
                .lock()
                .map_err(|_| anyhow::anyhow!("native guest registry poisoned"))?
                .is_empty(),
            "machine configuration cannot change while controllers are enrolled"
        );
        *self
            .machine_configuration
            .lock()
            .map_err(|_| anyhow::anyhow!("machine configuration poisoned"))? = config;
        Ok(())
    }
}
