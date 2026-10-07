// Origin: CTOX
// License: AGPL-3.0-only

//! A registered build endpoint, revalidated at every short SSH operation.
//! Browser capability names and caller-supplied host lists grant no authority.

use super::computer_capabilities::{BuildAvailability, BuildCapability, ComputerCapability};
use super::computer_endpoints::{
    resolve_computer_endpoint, with_current_computer_endpoint, ComputerEndpoint,
    ComputerEndpointRequest, EndpointUse,
};
use anyhow::{ensure, Context, Result};
use ctox_transfers::ssh_exec::{exec, SshExecOptions, SshExecOutput};
use std::path::{Path, PathBuf};

pub(crate) struct NativeBuildSsh {
    root: PathBuf,
    request: ComputerEndpointRequest,
    fingerprint: String,
    grant: BuildCapability,
}

impl NativeBuildSsh {
    /// Identity must come from a verified native session or its durable job.
    /// New jobs bind once; resumes retain this fingerprint rather than rebinding.
    pub(crate) fn bind(root: &Path, request: ComputerEndpointRequest) -> Result<Self> {
        ensure!(
            request.usage == EndpointUse::Build,
            "a build endpoint use is required"
        );
        let endpoint = resolve_computer_endpoint(root, &request)?;
        let ComputerCapability::Build(grant) = endpoint.grant else {
            anyhow::bail!("a typed build grant is required");
        };
        ensure!(
            matches!(endpoint.connection, ComputerEndpoint::Ssh { .. }),
            "build requires SSH"
        );
        Ok(Self {
            root: root.to_owned(),
            request,
            fingerprint: endpoint.fingerprint,
            grant,
        })
    }

    /// Reconstruct a persisted job binding without accepting a rotated grant/key.
    pub(crate) fn resume(
        root: &Path,
        request: ComputerEndpointRequest,
        saved_fingerprint: &str,
    ) -> Result<Self> {
        ensure!(
            !saved_fingerprint.is_empty(),
            "job endpoint fingerprint is missing"
        );
        let bound = Self::bind(root, request)?;
        ensure!(
            bound.fingerprint == saved_fingerprint,
            "build endpoint changed since job admission"
        );
        Ok(bound)
    }

    pub(crate) fn grant(&self) -> &BuildCapability {
        &self.grant
    }

    pub(crate) fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Only generated/quoted native planner commands belong here. Persistent
    /// job admission, detached launch and result reconciliation are caller-owned.
    pub(crate) fn execute_generated(&self, command: &str, input: &[u8]) -> Result<SshExecOutput> {
        self.with_current(|options| exec(options, command, input))
    }

    fn with_current<T>(
        &self,
        apply: impl for<'a> FnOnce(SshExecOptions<'a>) -> Result<T>,
    ) -> Result<T> {
        with_current_computer_endpoint(
            &self.root,
            &self.request,
            &self.fingerprint,
            |endpoint, credentials| {
                ensure!(
                    matches!(endpoint.grant, ComputerCapability::Build(_)),
                    "build grant removed"
                );
                let ComputerEndpoint::Ssh {
                    host,
                    port,
                    username,
                    host_key_sha256,
                    host_key_algorithm,
                    passphrase,
                    ..
                } = &endpoint.connection
                else {
                    anyhow::bail!("build endpoint is no longer SSH");
                };
                ensure!(
                    credentials.len() == if passphrase.is_some() { 2 } else { 1 },
                    "invalid build credential layout"
                );
                let private_key =
                    std::str::from_utf8(credentials[0]).context("build SSH key must be UTF-8")?;
                let passphrase = if passphrase.is_some() {
                    Some(
                        std::str::from_utf8(credentials[1])
                            .context("build SSH passphrase must be UTF-8")?,
                    )
                } else {
                    None
                };
                apply(SshExecOptions {
                    host: host.clone(),
                    port: *port,
                    username: username.clone(),
                    host_key_sha256: host_key_sha256.clone(),
                    host_key_algorithm: host_key_algorithm
                        .map(|algorithm| algorithm.as_str().to_owned()),
                    private_key,
                    passphrase,
                })
            },
        )
    }

    /// Nonblocking observation using the prototype's actual lock files.
    /// The runner must still obtain a lease; this result is never a reservation.
    pub(crate) fn availability(&self) -> Result<BuildAvailability> {
        let script = format!(
            r#"set -euo pipefail
lane={}
slots={}
floor={}
[ -d "$lane" ] && [ "$(readlink -f -- "$lane")" = "$lane" ] || exit 74
free=$(df -PB1 -- "$lane" | awk 'NR==2{{print $4}}')
[ "$free" -ge "$((floor*1024*1024*1024))" ] || exit 74
n=0
for ((s=1; s<=slots; s++)); do
  exec {{fd}}>"$lane/slot-$s.lock"
  if flock -n "$fd"; then n=$((n+1)); fi
  exec {{fd}}>&-
done
printf '%s\n' "$n"
"#,
            quote(&self.grant.lane_root),
            self.grant.slots,
            self.grant.disk_floor_gib
        );
        let output = self.execute_generated("bash -s", script.as_bytes())?;
        ensure!(output.exit_code == 0, "build capacity probe rejected");
        let slots: u16 = std::str::from_utf8(&output.stdout)?
            .trim()
            .parse()
            .context("invalid remote slot observation")?;
        ensure!(
            slots <= self.grant.slots,
            "remote slot observation exceeds grant"
        );
        Ok(BuildAvailability {
            computer_id: self.request.computer_id.clone(),
            free_slots: slots,
            observed_at_ms: super::store::now_ms() as i64,
        })
    }
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business_os::store::{BusinessCommand, CommandOrigin};
    use crate::business_os::store_workjet_computers::handle_workjet_computer_store_command;
    use serde_json::{json, Value};
    use std::cell::Cell;

    fn dispatch(root: &Path, kind: &str, payload: Value) -> Result<Value> {
        handle_workjet_computer_store_command(
            root,
            &BusinessCommand {
                id: None,
                module: "ctox".into(),
                command_type: kind.into(),
                record_id: None,
                payload,
                client_context: json!({}),
                origin: CommandOrigin::TrustedLocal,
            },
            "owner-1",
            None,
            "chef",
        )
    }

    fn request() -> ComputerEndpointRequest {
        ComputerEndpointRequest {
            owner_user_id: "owner-1".into(),
            computer_id: "build-computer".into(),
            endpoint_ref: "build-endpoint".into(),
            usage: EndpointUse::Build,
        }
    }

    fn fixture(root: &Path) -> Result<()> {
        dispatch(
            root,
            "ctox.workjet.computer.assign",
            json!({
                "computer_id":"build-computer", "display_name":"Build",
                "hosting_mode":"self_hosted", "agentless":false, "capabilities":[],
                "capability_config":[{
                    "kind":"build", "ssh_endpoint_ref":"build-endpoint", "slots":3,
                    "jobs":6, "lane_root":"/lane", "disk_floor_gib":1, "toolchains":["rust-stable"]
                }]
            }),
        )?;
        dispatch(
            root,
            "ctox.workjet.computer.endpoint.upsert",
            json!({
                "endpoint_ref":"build-endpoint", "computer_id":"build-computer",
                "connection":{
                    "protocol":"ssh", "host":"127.0.0.1", "port":22, "username":"fixture",
                    "root":"/lane", "host_key_sha256":"SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                    "host_key_algorithm":"ssh-ed25519", "private_key":{"scope":"build-fixture","name":"key"},
                    "passphrase":null
                }
            }),
        )?;
        crate::secrets::write_secret_record(
            root,
            "build-fixture",
            "key",
            "first-test-key",
            None,
            json!({}),
        )?;
        Ok(())
    }

    #[test]
    fn rotated_credentials_and_disabled_endpoint_deny_the_next_operation() -> Result<()> {
        let root = tempfile::tempdir()?;
        fixture(root.path())?;
        let admitted = NativeBuildSsh::bind(root.path(), request())?;
        let bound = NativeBuildSsh::resume(root.path(), request(), admitted.fingerprint())?;
        let calls = Cell::new(0);
        bound.with_current(|options| {
            calls.set(calls.get() + 1);
            assert_eq!(options.private_key, "first-test-key");
            Ok(())
        })?;
        assert_eq!(calls.get(), 1);
        crate::secrets::write_secret_record(
            root.path(),
            "build-fixture",
            "key",
            "second-test-key",
            None,
            json!({}),
        )?;
        assert!(bound
            .with_current(|_| {
                calls.set(calls.get() + 1);
                Ok(())
            })
            .is_err());
        assert_eq!(calls.get(), 1, "stale job invoked a protocol operation");
        assert!(NativeBuildSsh::resume(root.path(), request(), bound.fingerprint()).is_err());
        let rebound = NativeBuildSsh::bind(root.path(), request())?;
        assert_ne!(bound.fingerprint(), rebound.fingerprint());
        dispatch(
            root.path(),
            "ctox.workjet.computer.endpoint.disable",
            json!({"endpoint_ref":"build-endpoint"}),
        )?;
        assert!(rebound
            .with_current(|_| {
                calls.set(calls.get() + 1);
                Ok(())
            })
            .is_err());
        assert_eq!(
            calls.get(),
            1,
            "disabled endpoint invoked a protocol operation"
        );
        Ok(())
    }

    #[test]
    fn wrong_owner_and_storage_use_cannot_bind_a_build_transport() -> Result<()> {
        let root = tempfile::tempdir()?;
        fixture(root.path())?;
        let mut forged = request();
        forged.owner_user_id = "owner-2".into();
        assert!(NativeBuildSsh::bind(root.path(), forged).is_err());
        let mut storage = request();
        storage.usage = EndpointUse::Storage {
            purpose: super::super::computer_capabilities::StoragePurpose::Artifacts,
        };
        assert!(NativeBuildSsh::bind(root.path(), storage).is_err());
        Ok(())
    }
}
