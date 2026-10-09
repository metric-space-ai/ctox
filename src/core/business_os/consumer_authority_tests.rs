// Origin: CTOX
// License: AGPL-3.0-only
use super::super::{capability::CapabilityDeviceBinding, mobile_invites, store_workjet_computers};
use super::*;
use store::{BusinessCommand, CommandOrigin};

struct Fixture {
    root: tempfile::TempDir,
    token: String,
    pairing: String,
}

impl Fixture {
    fn new(owner: Option<&str>) -> Result<Self> {
        let root = tempfile::tempdir()?;
        store::issue_business_os_capability_token_for_managed_user(
            root.path(),
            "owner",
            "Owner",
            "chef",
            store::now_ms() as i64,
        )?;
        let binding = CapabilityDeviceBinding {
            device_pairing_id: "pairing-fixture".into(),
            device_id: "device-not-a-computer-id".into(),
            proof_key_thumbprint: "A".repeat(43),
        };
        let invite =
            mobile_invites::create_for_owner(root.path(), 300, None, Some(&binding), owner)?;
        // Real native command/projection path; no production state or identities.
        let conn = Connection::open(store::rxdb_store_path(root.path()))?;
        conn.execute_batch(
            "CREATE TABLE ctox_business_os__workjet_computers__v0 (
            id TEXT PRIMARY KEY NOT NULL,revision TEXT,deleted INTEGER NOT NULL DEFAULT 0,
            lastWriteTime REAL NOT NULL DEFAULT 0,data TEXT NOT NULL)",
        )?;
        Ok(Self {
            root,
            token: invite["inviteId"].as_str().unwrap().into(),
            pairing: binding.device_pairing_id,
        })
    }

    fn assign(&self, owner: &str, id: &str, binding: Option<&str>) -> Result<Value> {
        let mut payload = json!({"computer_id":id,"display_name":"Fixture workstation",
            "hosting_mode":"workstation","capabilities":[]});
        if let Some(binding) = binding {
            payload["device_binding_id"] = Value::from(binding);
        }
        store_workjet_computers::handle_workjet_computer_store_command(
            self.root.path(),
            &BusinessCommand {
                id: None,
                module: "ctox".into(),
                command_type: "ctox.workjet.computer.assign".into(),
                record_id: None,
                payload,
                client_context: json!({}),
                origin: CommandOrigin::TrustedLocal,
            },
            owner,
            None,
            "chef",
        )
    }

    fn resolve(&self) -> Result<ConsumerFacts> {
        current_policy(self.root.path(), &self.token, |conn, claims| {
            resolve(conn, claims)
        })
    }
}

#[test]
fn consumer_requires_explicit_owned_computer_association_and_preserves_it_on_refresh() -> Result<()>
{
    let f = Fixture::new(Some("owner"))?;
    assert!(f.resolve().is_err());
    f.assign("owner", "opaque-computer", Some(&f.pairing))?;
    let first = f.resolve()?;
    assert_eq!(first.computer_id, "opaque-computer");
    assert_eq!(first.device_id, "device-not-a-computer-id");
    assert_eq!(first.owner_user_id, "owner");
    assert_ne!(first.actor_user_id, first.owner_user_id);
    f.assign("owner", "opaque-computer", None)?;
    assert_eq!(f.resolve()?, first);
    let projected =
        store::load_rxdb_collection_record(f.root.path(), "workjet_computers", "opaque-computer")?
            .unwrap();
    assert!(projected.get("native_device_binding").is_none());
    Ok(())
}

#[test]
fn foreign_owner_legacy_invite_duplicate_enrollment_and_unbind_fail_closed() -> Result<()> {
    let legacy = Fixture::new(None)?;
    assert!(legacy
        .assign("owner", "opaque-computer", Some(&legacy.pairing))
        .is_err());
    let f = Fixture::new(Some("owner"))?;
    assert!(f
        .assign("foreign", "foreign-computer", Some(&f.pairing))
        .is_err());
    f.assign("owner", "opaque-computer", Some(&f.pairing))?;
    let old = f.resolve()?;
    f.assign("owner", "second-computer", Some(&f.pairing))?;
    assert!(
        f.resolve().is_err(),
        "an ambiguous binding is not an identity"
    );
    f.assign("owner", "second-computer", Some(""))?;
    assert_eq!(f.resolve()?, old);
    f.assign("owner", "opaque-computer", Some(""))?;
    assert!(f.resolve().is_err());
    f.assign("owner", "opaque-computer", Some(&f.pairing))?;
    assert_ne!(
        f.resolve()?.computer_revision,
        old.computer_revision,
        "unbind/rebind cannot reuse old authority"
    );
    Ok(())
}

#[test]
fn current_resolver_rejects_actor_pairing_owner_and_membership_revocation() -> Result<()> {
    for revoke in ["actor", "pairing", "owner", "membership"] {
        let f = Fixture::new(Some("owner"))?;
        f.assign("owner", "opaque-computer", Some(&f.pairing))?;
        let current = f.resolve()?;
        let conn = store::open_store(f.root.path())?;
        match revoke {
            "actor" => {
                conn.execute("UPDATE business_users SET active=0,capability_epoch=capability_epoch+1 WHERE user_id=?1", [&current.actor_user_id])?;
            }
            "pairing" => {
                mobile_invites::revoke_by_device_pairing_id(f.root.path(), &f.pairing)?;
            }
            "owner" => {
                conn.execute(
                    "UPDATE business_users SET active=0 WHERE user_id='owner'",
                    [],
                )?;
            }
            "membership" => {
                let mut record =
                    store::outbound_load_record(&conn, "workjet_computers", "opaque-computer")?
                        .unwrap();
                record["status"] = Value::from("unassigned");
                store::upsert_business_record(
                    &conn,
                    "workjet_computers",
                    "opaque-computer",
                    store::now_ms() as i64,
                    record,
                )?;
            }
            _ => unreachable!(),
        }
        assert!(
            f.resolve().is_err(),
            "{revoke} must retire consumer authority"
        );
    }
    Ok(())
}

#[test]
fn publication_fences_mutations_and_rejects_epoch_changes_without_relabeling() -> Result<()> {
    use rxdb::plugins::replication_webrtc::WebRTCPublicationGuard;
    let f = Fixture::new(Some("owner"))?;
    f.assign("owner", "opaque-computer", Some(&f.pairing))?;
    let expected = f.resolve()?;
    let conn = store::open_store(f.root.path())?;
    conn.busy_timeout(std::time::Duration::ZERO)?;
    let guard = ConsumerPublication {
        root: f.root.path().to_owned(),
        token: f.token.clone(),
        facts: expected.clone(),
        conn: std::sync::Mutex::new(conn),
    };
    let other = store::open_store(f.root.path())?;
    other.busy_timeout(std::time::Duration::ZERO)?;
    let mut called = false;
    guard
        .with_current(&mut || {
            assert!(other
                .execute(
                    "UPDATE business_users SET active=0 WHERE user_id='owner'",
                    []
                )
                .is_err());
            called = true;
            Ok(())
        })
        .map_err(|_| anyhow::anyhow!("fixture publication failed"))?;
    assert!(called);
    other.execute(
        "UPDATE business_users SET capability_epoch=capability_epoch+1 WHERE user_id=?1",
        [&expected.actor_user_id],
    )?;
    // Invite-secret lookup reports the current actor epoch. Old context must
    // reject rather than relabel a pending response with that newer authority.
    assert_ne!(f.resolve()?.actor_epoch, expected.actor_epoch);
    called = false;
    assert!(guard
        .with_current(&mut || {
            called = true;
            Ok(())
        })
        .is_err());
    assert!(!called);
    Ok(())
}

#[test]
fn consumer_wire_request_cannot_supply_a_computer_or_forwarded_owner() {
    assert!(serde_json::from_value::<ConsumerRequest>(json!({"version":1})).is_ok());
    for field in [
        "computerId",
        "ownerUserId",
        "actorUserId",
        "deviceId",
        "consumer",
        "forwardedOrigin",
    ] {
        let mut request = json!({"version":1});
        request[field] = Value::from("untrusted");
        assert!(
            serde_json::from_value::<ConsumerRequest>(request).is_err(),
            "{field}"
        );
    }
}
