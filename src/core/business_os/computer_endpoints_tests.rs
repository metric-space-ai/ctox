// Origin: CTOX
// License: AGPL-3.0-only

use super::*;
use crate::business_os::store::{load_rxdb_collection_record, CommandOrigin};
use crate::business_os::store_workjet_computers::handle_workjet_computer_store_command;
use tempfile::tempdir;

fn command(kind: &str, payload: Value) -> BusinessCommand {
    BusinessCommand {
        id: None,
        module: "ctox".into(),
        command_type: kind.into(),
        record_id: None,
        payload,
        client_context: json!({}),
        origin: CommandOrigin::TrustedLocal,
    }
}

fn dispatch(root: &Path, kind: &str, payload: Value) -> Result<Value> {
    handle_workjet_computer_store_command(root, &command(kind, payload), "owner-1", None, "chef")
}

fn storage_config(protocol: &str, remote_root: &str) -> Value {
    json!([{"kind":"storage", "endpoint_ref":"endpoint-1", "protocol":protocol,
        "root":remote_root, "quota_gib":10, "purposes":["artifacts","exchange"]}])
}

fn assign(root: &Path, config: Value, agentless: bool) -> Result<Value> {
    dispatch(
        root,
        "ctox.workjet.computer.assign",
        json!({
            "computer_id":"computer-1", "display_name":"NAS", "hosting_mode":"self_hosted",
            "capabilities":[], "agentless":agentless, "capability_config":config,
        }),
    )
}

fn ssh() -> Value {
    json!({"protocol":"ssh", "host":"nas.example.test", "port":22, "username":"admin",
        "root":"/volume1", "host_key_sha256":format!("SHA256:{}", STANDARD_NO_PAD.encode([7u8;32])),
        "private_key":{"scope":"computer-fixture","name":"ssh-key"}, "passphrase":null})
}

fn smb() -> Value {
    json!({"protocol":"smb", "host":"10.0.0.28", "port":445, "username":"admin",
        "share":"artifacts", "root":"/", "password":{"scope":"computer-fixture","name":"smb-password"}})
}

fn upsert(root: &Path, connection: Value) -> Result<Value> {
    dispatch(
        root,
        "ctox.workjet.computer.endpoint.upsert",
        json!({"endpoint_ref":"endpoint-1","computer_id":"computer-1","connection":connection}),
    )
}

fn request() -> ComputerEndpointRequest {
    ComputerEndpointRequest {
        owner_user_id: "owner-1".into(),
        computer_id: "computer-1".into(),
        endpoint_ref: "endpoint-1".into(),
        usage: EndpointUse::Storage {
            purpose: StoragePurpose::Artifacts,
        },
    }
}

fn secret(root: &Path, name: &str, value: &str) -> Result<()> {
    crate::secrets::write_secret_record(root, "computer-fixture", name, value, None, json!({}))?;
    Ok(())
}

fn fixture(root: &Path) -> Result<()> {
    assign(root, storage_config("ssh", "/volume1/artifacts"), true)?;
    upsert(root, ssh())?;
    secret(root, "ssh-key", "synthetic-private-key")?;
    Ok(())
}

#[test]
fn endpoint_commands_require_verified_owner_admin_and_immutable_computer_binding() -> Result<()> {
    let root = tempdir()?;
    assign(
        root.path(),
        storage_config("ssh", "/volume1/artifacts"),
        true,
    )?;
    let payload =
        json!({"endpoint_ref":"endpoint-1","computer_id":"computer-1","connection":ssh()});
    for role in ["user", "founder", "team", ""] {
        assert!(handle_workjet_computer_store_command(
            root.path(),
            &command("ctox.workjet.computer.endpoint.upsert", payload.clone()),
            "owner-1",
            None,
            role
        )
        .is_err());
    }
    let first = upsert(root.path(), ssh())?;
    assert!(handle_workjet_computer_store_command(
        root.path(),
        &command("ctox.workjet.computer.endpoint.upsert", payload),
        "owner-2",
        None,
        "admin"
    )
    .is_err());
    assert_eq!(
        first["endpoint"]["_rev"],
        upsert(root.path(), ssh())?["endpoint"]["_rev"]
    );
    dispatch(
        root.path(),
        "ctox.workjet.computer.assign",
        json!({
            "computer_id":"computer-2","display_name":"Other","hosting_mode":"workstation","capabilities":[]
        }),
    )?;
    assert!(dispatch(
        root.path(),
        "ctox.workjet.computer.endpoint.upsert",
        json!({
            "endpoint_ref":"endpoint-1","computer_id":"computer-2","connection":ssh()
        })
    )
    .is_err());
    let mut forged =
        json!({"endpoint_ref":"endpoint-2","computer_id":"computer-1","connection":ssh()});
    forged["owner_user_id"] = json!("owner-2");
    assert!(dispatch(root.path(), "ctox.workjet.computer.endpoint.upsert", forged).is_err());
    Ok(())
}

#[test]
fn endpoint_validation_rejects_unpinned_ssh_unsafe_hosts_roots_and_inline_secrets() -> Result<()> {
    let root = tempdir()?;
    assign(
        root.path(),
        storage_config("ssh", "/volume1/artifacts"),
        true,
    )?;
    for (field, value) in [
        ("host", json!("ssh://user@host")),
        ("host", json!("-oProxyCommand=bad")),
        ("host_key_sha256", json!("SHA256:short")),
        ("port", json!(0)),
        ("username", json!("-bad")),
        ("root", json!("/volume1/../other")),
        ("root", json!("/volume1//artifacts")),
        ("root", json!("/volume1/")),
        (
            "private_key",
            json!({"scope":"fixture","name":"key","value":"inline"}),
        ),
        ("password", json!("inline")),
    ] {
        let mut connection = ssh();
        connection[field] = value;
        assert!(upsert(root.path(), connection).is_err(), "{field}");
    }
    let mut nfs = smb();
    nfs["protocol"] = json!("nfs");
    assert!(upsert(root.path(), nfs).is_err());
    Ok(())
}

#[test]
fn storage_resolution_allows_agentless_nas_but_rechecks_protocol_root_purpose_and_owner(
) -> Result<()> {
    let root = tempdir()?;
    fixture(root.path())?;
    let resolved = resolve_computer_endpoint(root.path(), &request())?;
    assert!(matches!(resolved.grant, ComputerCapability::Storage(_)));
    assert_eq!(resolved.connection.protocol(), StorageProtocol::Ssh);
    let serialized = serde_json::to_string(&resolved)?;
    assert!(!serialized.contains("synthetic-private-key"));
    assert!(serialized.contains("ssh-key"));
    let mut invalid = request();
    invalid.owner_user_id = "owner-2".into();
    assert!(resolve_computer_endpoint(root.path(), &invalid).is_err());
    invalid = request();
    invalid.usage = EndpointUse::Storage {
        purpose: StoragePurpose::Backups,
    };
    assert!(resolve_computer_endpoint(root.path(), &invalid).is_err());
    for (protocol, remote_root) in [
        ("smb", "/volume1/artifacts"),
        ("ssh", "/volume10"),
        ("nfs", "/volume1/artifacts"),
    ] {
        assign(root.path(), storage_config(protocol, remote_root), true)?;
        assert!(resolve_computer_endpoint(root.path(), &request()).is_err());
    }
    assign(
        root.path(),
        storage_config("ssh", "/volume1/artifacts"),
        true,
    )?;
    let mut build = request();
    build.usage = EndpointUse::Build;
    assert!(resolve_computer_endpoint(root.path(), &build).is_err());
    Ok(())
}

#[test]
fn missing_deleted_or_rotated_credentials_never_enter_resume_callback() -> Result<()> {
    let root = tempdir()?;
    assign(
        root.path(),
        storage_config("ssh", "/volume1/artifacts"),
        true,
    )?;
    upsert(root.path(), ssh())?;
    assert!(resolve_computer_endpoint(root.path(), &request()).is_err());
    secret(root.path(), "ssh-key", "synthetic-key-1")?;
    let first = resolve_computer_endpoint(root.path(), &request())?;
    with_current_computer_endpoint(
        root.path(),
        &request(),
        &first.fingerprint,
        |_, credentials| {
            assert_eq!(credentials, &[b"synthetic-key-1".as_slice()]);
            Ok(())
        },
    )?;
    // Even rewriting the same secret value produces a distinct encrypted revision.
    secret(root.path(), "ssh-key", "synthetic-key-1")?;
    let second = resolve_computer_endpoint(root.path(), &request())?;
    assert_ne!(first.fingerprint, second.fingerprint);
    let entered = std::cell::Cell::new(false);
    assert!(
        with_current_computer_endpoint(root.path(), &request(), &first.fingerprint, |_, _| {
            entered.set(true);
            Ok(())
        })
        .is_err()
    );
    assert!(!entered.get());
    crate::secrets::delete_secret_records(root.path(), &[("computer-fixture", "ssh-key")])?;
    assert!(with_current_computer_endpoint(
        root.path(),
        &request(),
        &second.fingerprint,
        |_, _| {
            entered.set(true);
            Ok(())
        }
    )
    .is_err());
    assert!(!entered.get());
    Ok(())
}

#[test]
fn endpoint_change_disable_and_reenable_reject_old_resume_identity() -> Result<()> {
    let root = tempdir()?;
    fixture(root.path())?;
    let first = resolve_computer_endpoint(root.path(), &request())?;
    let mut changed = ssh();
    changed["host"] = json!("replacement.example.test");
    upsert(root.path(), changed.clone())?;
    let second = resolve_computer_endpoint(root.path(), &request())?;
    assert_ne!(first.fingerprint, second.fingerprint);
    assert!(with_current_computer_endpoint(
        root.path(),
        &request(),
        &first.fingerprint,
        |_, _| Ok(())
    )
    .is_err());
    let disabled = dispatch(
        root.path(),
        "ctox.workjet.computer.endpoint.disable",
        json!({"endpoint_ref":"endpoint-1"}),
    )?;
    assert!(resolve_computer_endpoint(root.path(), &request()).is_err());
    let repeated = dispatch(
        root.path(),
        "ctox.workjet.computer.endpoint.disable",
        json!({"endpoint_ref":"endpoint-1"}),
    )?;
    assert_eq!(disabled["endpoint"]["_rev"], repeated["endpoint"]["_rev"]);
    upsert(root.path(), changed)?;
    assert!(with_current_computer_endpoint(
        root.path(),
        &request(),
        &second.fingerprint,
        |_, _| Ok(())
    )
    .is_err());
    Ok(())
}

#[test]
fn capability_revocation_regrant_and_unassignment_fence_old_jobs_without_cosmetic_churn(
) -> Result<()> {
    let root = tempdir()?;
    fixture(root.path())?;
    let first = resolve_computer_endpoint(root.path(), &request())?;
    let refreshed = dispatch(
        root.path(),
        "ctox.workjet.computer.assign",
        json!({
            "computer_id":"computer-1","display_name":"Renamed NAS","hosting_mode":"self_hosted","capabilities":[]
        }),
    )?;
    assert_eq!(
        resolve_computer_endpoint(root.path(), &request())?.fingerprint,
        first.fingerprint
    );
    assert!(refreshed["computer"]["capability_epoch"].as_u64().unwrap() > 0);
    let original = storage_config("ssh", "/volume1/artifacts");
    let mut changed = original.clone();
    changed[0]["quota_gib"] = json!(20);
    assign(root.path(), changed, true)?;
    assign(root.path(), original.clone(), true)?;
    assert!(with_current_computer_endpoint(
        root.path(),
        &request(),
        &first.fingerprint,
        |_, _| Ok(())
    )
    .is_err());
    let regranted = resolve_computer_endpoint(root.path(), &request())?;
    dispatch(
        root.path(),
        "ctox.workjet.computer.unassign",
        json!({"computer_id":"computer-1"}),
    )?;
    assert!(resolve_computer_endpoint(root.path(), &request()).is_err());
    assign(root.path(), original, true)?;
    assert!(with_current_computer_endpoint(
        root.path(),
        &request(),
        &regranted.fingerprint,
        |_, _| Ok(())
    )
    .is_err());
    Ok(())
}

#[test]
fn native_endpoint_and_grant_writes_are_fenced_during_one_bounded_io_poll() -> Result<()> {
    let root = tempdir()?;
    fixture(root.path())?;
    let endpoint = resolve_computer_endpoint(root.path(), &request())?;
    let other = open_store(root.path())?;
    other.busy_timeout(std::time::Duration::ZERO)?;
    with_current_computer_endpoint(root.path(), &request(), &endpoint.fingerprint, |_, _| {
        assert!(other
            .execute(
                "UPDATE business_records SET deleted = 1 WHERE collection IN (?1,?2)",
                [
                    ENDPOINTS,
                    super::super::store_workjet_computers::COMPUTERS_COLLECTION
                ]
            )
            .is_err());
        Ok(())
    })?;
    assert!(
        other.execute(
            "UPDATE business_records SET deleted = 1 WHERE collection = ?1",
            [ENDPOINTS]
        )? > 0
    );
    assert!(resolve_computer_endpoint(root.path(), &request()).is_err());
    Ok(())
}

#[test]
fn smb_and_ssh_passphrase_tuples_have_explicit_order_and_native_only_metadata() -> Result<()> {
    let root = tempdir()?;
    assign(root.path(), storage_config("smb", "/artifacts"), true)?;
    upsert(root.path(), smb())?;
    secret(root.path(), "smb-password", "synthetic-password")?;
    let endpoint = resolve_computer_endpoint(root.path(), &request())?;
    with_current_computer_endpoint(
        root.path(),
        &request(),
        &endpoint.fingerprint,
        |_, values| {
            assert_eq!(values, &[b"synthetic-password".as_slice()]);
            Ok(())
        },
    )?;
    assign(
        root.path(),
        storage_config("ssh", "/volume1/artifacts"),
        true,
    )?;
    let mut connection = ssh();
    connection["passphrase"] = json!({"scope":"computer-fixture","name":"ssh-passphrase"});
    upsert(root.path(), connection)?;
    secret(root.path(), "ssh-key", "synthetic-key")?;
    assert!(resolve_computer_endpoint(root.path(), &request()).is_err());
    secret(root.path(), "ssh-passphrase", "synthetic-passphrase")?;
    let endpoint = resolve_computer_endpoint(root.path(), &request())?;
    with_current_computer_endpoint(
        root.path(),
        &request(),
        &endpoint.fingerprint,
        |_, values| {
            assert_eq!(
                values,
                &[
                    b"synthetic-key".as_slice(),
                    b"synthetic-passphrase".as_slice()
                ]
            );
            Ok(())
        },
    )?;
    let listed = dispatch(
        root.path(),
        "ctox.workjet.computer.endpoint.list",
        json!({}),
    )?;
    assert_eq!(listed["endpoints"].as_array().unwrap().len(), 1);
    assert!(!serde_json::to_string(&listed)?.contains("synthetic-"));
    assert!(load_rxdb_collection_record(root.path(), ENDPOINTS, "endpoint-1")?.is_none());
    Ok(())
}

#[test]
fn build_resolution_uses_registered_ssh_and_current_build_grant() -> Result<()> {
    let root = tempdir()?;
    let build = json!([{"kind":"build","ssh_endpoint_ref":"endpoint-1",
        "slots":2,"jobs":5,"lane_root":"/volume1/build-lane","disk_floor_gib":30,
        "toolchains":["rust-1.93"]}]);
    assign(root.path(), build, false)?;
    upsert(root.path(), ssh())?;
    secret(root.path(), "ssh-key", "synthetic-key")?;
    let mut request = request();
    request.usage = EndpointUse::Build;
    let endpoint = resolve_computer_endpoint(root.path(), &request)?;
    assert!(matches!(endpoint.grant, ComputerCapability::Build(_)));
    assert!(resolve_computer_endpoint(root.path(), &super::tests::request()).is_err());
    Ok(())
}

#[test]
fn native_epoch_stays_outside_the_v1_computer_projection() -> Result<()> {
    let root = tempdir()?;
    std::fs::create_dir_all(root.path().join("runtime"))?;
    let projection_db = Connection::open(super::super::store::rxdb_store_path(root.path()))?;
    projection_db.execute(
        "CREATE TABLE ctox_business_os__workjet_computers__v0 (
        id TEXT PRIMARY KEY, revision TEXT, deleted INTEGER NOT NULL DEFAULT 0,
        lastWriteTime REAL NOT NULL DEFAULT 0, data TEXT NOT NULL)",
        [],
    )?;
    fixture(root.path())?;
    let projection =
        load_rxdb_collection_record(root.path(), "workjet_computers", "computer-1")?.unwrap();
    assert!(projection.get("capability_epoch").is_none());
    assert!(projection.get("capability_config").is_none());
    assert_eq!(projection["capabilities"], json!(["storage"]));
    Ok(())
}

#[test]
fn legacy_ssh_algorithm_omission_preserves_connection_and_job_authority() -> Result<()> {
    let root = tempdir()?;
    fixture(root.path())?;
    let original = ssh();
    let decoded: ComputerEndpoint = serde_json::from_value(original.clone())?;
    assert!(matches!(
        decoded,
        ComputerEndpoint::Ssh {
            host_key_algorithm: None,
            ..
        }
    ));
    assert_eq!(serde_json::to_value(decoded)?, original);
    let before = resolve_computer_endpoint(root.path(), &request())?;
    let mut explicit_none = original;
    explicit_none["host_key_algorithm"] = Value::Null;
    upsert(root.path(), explicit_none)?;
    let after = resolve_computer_endpoint(root.path(), &request())?;
    assert_eq!(after.endpoint_revision, before.endpoint_revision);
    assert_eq!(after.fingerprint, before.fingerprint);
    with_current_computer_endpoint(root.path(), &request(), &before.fingerprint, |_, _| Ok(()))?;
    Ok(())
}

#[test]
fn ssh_algorithm_constraint_is_strict_persisted_and_invalidates_old_jobs() -> Result<()> {
    let root = tempdir()?;
    fixture(root.path())?;
    for algorithm in [
        SshHostKeyAlgorithm::Ed25519,
        SshHostKeyAlgorithm::EcdsaSha2Nistp256,
        SshHostKeyAlgorithm::EcdsaSha2Nistp384,
        SshHostKeyAlgorithm::EcdsaSha2Nistp521,
        SshHostKeyAlgorithm::RsaSha2_256,
        SshHostKeyAlgorithm::RsaSha2_512,
    ] {
        let before = resolve_computer_endpoint(root.path(), &request())?;
        let mut connection = ssh();
        connection["host_key_algorithm"] = json!(algorithm.as_str());
        upsert(root.path(), connection.clone())?;
        let after = resolve_computer_endpoint(root.path(), &request())?;
        assert_eq!(serde_json::to_value(&after.connection)?, connection);
        assert_ne!(after.fingerprint, before.fingerprint);
        let mut entered = false;
        assert!(with_current_computer_endpoint(
            root.path(),
            &request(),
            &before.fingerprint,
            |_, _| {
                entered = true;
                Ok(())
            }
        )
        .is_err());
        assert!(!entered);
        with_current_computer_endpoint(
            root.path(),
            &request(),
            &after.fingerprint,
            |resolved, _| {
                assert!(matches!(&resolved.connection, ComputerEndpoint::Ssh {
                host_key_algorithm: Some(value), ..
            } if *value == algorithm));
                Ok(())
            },
        )?;
    }
    let last = resolve_computer_endpoint(root.path(), &request())?;
    for unsupported in [
        "ssh-rsa",
        "ssh-dss",
        "ed25519",
        "ecdsa",
        "ssh-ed25519-cert-v01@openssh.com",
        "",
    ] {
        let mut connection = ssh();
        connection["host_key_algorithm"] = json!(unsupported);
        assert!(upsert(root.path(), connection).is_err());
    }
    assert_eq!(
        resolve_computer_endpoint(root.path(), &request())?.fingerprint,
        last.fingerprint
    );
    Ok(())
}

#[test]
fn native_ssh_key_setup_returns_only_public_material_and_reuses_the_stored_key() -> Result<()> {
    let root = tempdir()?;
    assign(
        root.path(),
        storage_config("ssh", "/volume1/artifacts"),
        true,
    )?;
    let payload = json!({"computer_id": "computer-1"});
    let first = dispatch(
        root.path(),
        "ctox.workjet.computer.ssh_key.ensure",
        payload.clone(),
    )?;
    let repeated = dispatch(root.path(), "ctox.workjet.computer.ssh_key.ensure", payload)?;
    assert_eq!(first, repeated);
    assert_eq!(first["contract"], "ctox.workjet.computer-ssh-key.v1");
    assert_eq!(first["computer_id"], "computer-1");
    assert!(first["public_key"]
        .as_str()
        .unwrap()
        .starts_with("ssh-ed25519 "));
    assert!(first["public_key_sha256"]
        .as_str()
        .unwrap()
        .starts_with("SHA256:"));
    let serialized = serde_json::to_string(&first)?;
    assert!(!serialized.contains("PRIVATE KEY"));
    assert_eq!(
        crate::secrets::list_secret_records(root.path(), Some("computer-access"))?.len(),
        1
    );
    let reference: SecretReference = serde_json::from_value(first["private_key"].clone())?;
    crate::secrets::with_current_secret_value(
        root.path(),
        &reference.scope,
        &reference.name,
        |bytes| {
            let key = PrivateKey::from_openssh(bytes)?;
            assert_eq!(
                key.public_key().to_openssh()?,
                first["public_key"].as_str().unwrap()
            );
            assert_eq!(
                key.public_key().fingerprint(HashAlg::Sha256).to_string(),
                first["public_key_sha256"]
            );
            Ok(())
        },
    )?;
    let mut connection = ssh();
    connection["private_key"] = first["private_key"].clone();
    upsert(root.path(), connection)?;
    assert!(!resolve_computer_endpoint(root.path(), &request())?
        .fingerprint
        .is_empty());
    Ok(())
}

#[test]
fn native_ssh_key_setup_denies_unassigned_foreign_and_non_owner_computers() -> Result<()> {
    let root = tempdir()?;
    let payload = json!({"computer_id": "computer-1"});
    let kind = "ctox.workjet.computer.ssh_key.ensure";
    assert!(dispatch(root.path(), kind, payload.clone()).is_err());
    assert!(!crate::secrets::secret_store_path(root.path()).exists());
    assign(
        root.path(),
        storage_config("ssh", "/volume1/artifacts"),
        true,
    )?;
    for role in ["user", "founder", "team", ""] {
        assert!(handle_workjet_computer_store_command(
            root.path(),
            &command(kind, payload.clone()),
            "owner-1",
            None,
            role
        )
        .is_err());
    }
    assert!(handle_workjet_computer_store_command(
        root.path(),
        &command(kind, payload.clone()),
        "owner-2",
        None,
        "admin"
    )
    .is_err());
    assert!(dispatch(
        root.path(),
        kind,
        json!({"computer_id":"computer-1", "owner_user_id":"owner-2"})
    )
    .is_err());
    assert!(dispatch(
        root.path(),
        kind,
        json!({"computer_id":"computer-1", "private_key":"not-accepted"})
    )
    .is_err());
    assert!(!crate::secrets::secret_store_path(root.path()).exists());
    dispatch(
        root.path(),
        "ctox.workjet.computer.unassign",
        payload.clone(),
    )?;
    assert!(dispatch(root.path(), kind, payload).is_err());
    assert!(!crate::secrets::secret_store_path(root.path()).exists());
    Ok(())
}

#[test]
fn native_ssh_key_setup_does_not_adopt_or_overwrite_an_unissued_existing_record() -> Result<()> {
    let root = tempdir()?;
    assign(
        root.path(),
        storage_config("ssh", "/volume1/artifacts"),
        true,
    )?;
    let name = format!(
        "workjet-ssh-{:x}",
        Sha256::digest(serde_json::to_vec(&("owner-1", "computer-1"))?)
    );
    crate::secrets::write_secret_record(
        root.path(),
        "computer-access",
        &name,
        "existing-unissued-value",
        None,
        json!({}),
    )?;
    assert!(dispatch(
        root.path(),
        "ctox.workjet.computer.ssh_key.ensure",
        json!({"computer_id":"computer-1"})
    )
    .is_err());
    assert_eq!(
        crate::secrets::read_secret_value(root.path(), "computer-access", &name)?,
        "existing-unissued-value"
    );
    Ok(())
}
