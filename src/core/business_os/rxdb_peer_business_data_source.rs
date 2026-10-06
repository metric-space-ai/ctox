//! Business OS policy and durable command intake for native BusinessData.
//! Every request is revalidated against the current signed WebRTC capability.
//! Scope is narrowed server-side, and commands reuse ReplicatedPeer intake.

use super::store;
use ctox_sync::business_data_contract::{
    NativeBusinessDataCommand as Command, NativeBusinessDataCommandState as CommandState,
    NativeBusinessDataCommandStatus, NativeBusinessDataScope as Scope,
};
use ctox_sync::business_data_remote::{Access, BusinessDataAccessPolicy, RemoteIdentity};

use serde_json::{json, Value};
use std::{io, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

pub(super) const READABLE_COLLECTIONS: &[&str] = &[
    "business_records",
    "business_commands",
    "ctox_queue_tasks",
    "ctox_runs",
    "project_chats",
    "project_workers",
    "profile_bindings",
    "ctox_crew_members",
    "ctox_harness_status",
    "workjet_computers",
    "workjet_projects",
    "workjet_sessions",
    "workjet_transfers",
];

#[derive(Clone)]
pub struct NativeBusinessDataPolicy {
    root: PathBuf,

    command_lock: Arc<Mutex<()>>,
}

impl NativeBusinessDataPolicy {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,

            command_lock: Arc::default(),
        }
    }

    fn allowed(&self, collection: &str, scope: &Scope) -> io::Result<()> {
        if !READABLE_COLLECTIONS.contains(&collection) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "collection is not eligible",
            ));
        }
        if matches!(scope, Scope::Instance {})
            || matches!(collection, "business_records" | "project_chats")
        {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "scope is not eligible",
            ))
        }
    }
}

/// These readers exist only inside one synchronous current-authority callback.
/// All policy/lineage/projection methods borrow them and cannot open/migrate state.
struct NativeReadAuthority<'a> {
    core: &'a rusqlite::Connection,
    policy: &'a rusqlite::Connection,
    projection: &'a rusqlite::Connection,
    collection: &'a str,
    claims: super::capability::CapabilityClaims,
}

impl NativeReadAuthority<'_> {
    fn document_view(&self, collection: &str, document: &Value) -> anyhow::Result<Option<Value>> {
        anyhow::ensure!(
            collection == self.collection,
            "read authority belongs to another collection"
        );
        if !super::threads::native_business_data_document_visible_from_connections(
            self.core,
            self.policy,
            self.projection,
            collection,
            document,
            super::threads::ReplicationActor {
                user_id: &self.claims.user_id,
                role: &self.claims.role,
                collection_read_allowed: true,
            },
        )? {
            return Ok(None);
        }
        let mut visible = document.clone();
        if collection == "ctox_crew_members" {
            if let Some(fields) = super::policy::crew_fields_for_role(&self.claims.role) {
                rxdb::plugins::replication_webrtc::webrtc_types::retain_readable_fields(
                    &mut visible,
                    &fields,
                );
            }
        }
        Ok(Some(visible))
    }
}

impl NativeBusinessDataPolicy {
    fn with_current_read_authority<T>(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        collection: &str,
        scope: &Scope,
        apply: impl FnOnce(&NativeReadAuthority<'_>) -> anyhow::Result<T>,
    ) -> io::Result<T> {
        self.allowed(collection, scope)?;
        store::with_current_webrtc_capability_signer(&self.root, |signer| {
            // No CREATE flag, schema preparation, cached credentials or waits.
            // Enter issuer -> Core -> policy -> projection, then borrow all
            // readers for one bounded synchronous check/publication callback.
            let open = |path: PathBuf| -> anyhow::Result<rusqlite::Connection> {
                let conn = rusqlite::Connection::open_with_flags(
                    path,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
                )?;
                conn.busy_timeout(std::time::Duration::ZERO)?;
                Ok(conn)
            };
            let mut core = open(crate::paths::core_db(&self.root))?;
            let core = core.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let mut policy = open(store::business_os_store_path(&self.root))?;
            let policy =
                policy.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let mut projection = open(store::rxdb_store_path(&self.root))?;
            let projection =
                projection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let at_ms = chrono::Utc::now().timestamp_millis();
            let claims = store::verified_webrtc_capability_claims_from_connection(
                &policy,
                capability_token,
                signer,
                at_ms,
            )
            .ok_or_else(|| anyhow::anyhow!("current native actor is unavailable"))?;
            anyhow::ensure!(
                claims.user_id == identity.user_id
                    && u64::try_from(claims.actor_epoch).ok() == Some(identity.authorization_epoch)
                    && store::existing_instance_id(&self.root)? == identity.instance_id,
                "native actor or instance changed"
            );
            anyhow::ensure!(
                store::webrtc_capability_allows_collection_permission_from_connection(
                    &policy,
                    capability_token,
                    signer,
                    collection,
                    store::BusinessOsPermission::DataRead,
                    at_ms,
                ),
                "current native collection access denied"
            );
            let result = apply(&NativeReadAuthority {
                core: &core,
                policy: &policy,
                projection: &projection,
                collection,
                claims,
            })?;
            anyhow::ensure!(
                store::existing_instance_id(&self.root)? == identity.instance_id,
                "native instance changed during callback"
            );
            // Read-only publication: dropping the immediate transactions
            // releases the mutation fences, without writing or committing data.
            Ok(result)
        })
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "native read authority unavailable",
            )
        })
    }
}

fn command_projection(document: &Value) -> CommandState {
    let status = match document.get("status").and_then(Value::as_str) {
        Some("completed") => NativeBusinessDataCommandStatus::Completed,
        Some("failed") => NativeBusinessDataCommandStatus::Failed,
        Some("unknown") => NativeBusinessDataCommandStatus::Unknown,
        _ => NativeBusinessDataCommandStatus::Pending,
    };
    CommandState {
        command_id: document
            .get("command_id")
            .or_else(|| document.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        status,
        result: document
            .get("result")
            .cloned()
            .filter(|value| !value.is_null()),
        error: document
            .get("last_retry_error")
            .or_else(|| document.get("error"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

// Only call with a projection read from the canonical native command store.
fn owned_command_projection(
    stored: &Value,
    command_id: &str,
    user_id: &str,
) -> io::Result<CommandState> {
    let receipt = &stored["native_authorization"];
    let matches_owner = if let Some(owner) = stored.get("native_owner") {
        owner["contract"].as_str() == Some("ctox-business-command-owner-v1")
            && owner["user_id"].as_str() == Some(user_id)
    } else {
        receipt["contract"].as_str() == Some("ctox-business-command-authorization-v1")
            && receipt["allowed"].as_bool() == Some(true)
            && receipt["actor"]["trusted"].as_bool() == Some(true)
            && receipt["actor"]["id"].as_str() == Some(user_id)
    };
    if user_id.is_empty() || !matches_owner {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "command has no matching trusted owner receipt",
        ));
    }
    let state = command_projection(stored);
    if state.command_id != command_id || command_id.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "command identity changed",
        ));
    }
    Ok(state)
}

#[cfg(test)]
mod owner_receipt_tests {
    use super::*;

    struct ReadFixture {
        root: tempfile::TempDir,
        policy: NativeBusinessDataPolicy,
        token: String,
        identity: RemoteIdentity,
        table: String,
    }

    async fn read_fixture(role: &str) -> anyhow::Result<ReadFixture> {
        let root = tempfile::tempdir()?;
        crate::mission::channels::open_channel_db(&crate::paths::core_db(root.path()))?;
        store::tests::seed_business_user(root.path(), "alice", role)?;
        let (token, _) = store::issue_business_os_capability_token_for_managed_user(
            root.path(),
            "alice",
            "Alice",
            role,
            chrono::Utc::now().timestamp_millis(),
        )?;
        let projection = rusqlite::Connection::open(store::rxdb_store_path(root.path()))?;
        let schemas: Value =
            serde_json::from_str(include_str!("business_os_schema_contract.json"))?;
        let version = schemas["business_commands"]["version"]
            .as_u64()
            .expect("canonical command schema");
        let table = format!("ctox_business_os__business_commands__v{version}");
        projection.execute_batch(&format!(
            "CREATE TABLE {table} (id TEXT PRIMARY KEY, data TEXT NOT NULL)"
        ))?;
        let policy = NativeBusinessDataPolicy::new(root.path().to_path_buf());
        let identity = policy
            .identity(&token)
            .await
            .expect("current signed actor and instance");
        Ok(ReadFixture {
            root,
            policy,
            token,
            identity,
            table,
        })
    }

    fn grant_read(root: &std::path::Path, user: &str, collection: &str) -> anyhow::Result<()> {
        store::open_store(root)?.execute(
            "INSERT INTO business_permission_grants
             (grant_id, subject_type, subject_id, permission, scope_type, scope_id,
              active, reason, created_by, created_at_ms, updated_at_ms)
             VALUES (?1, 'user', ?2, ?4, 'collection', ?3, 1, 'fixture', 'fixture', 1, 1)",
            rusqlite::params![
                format!("read-{user}-{collection}"),
                user,
                collection,
                store::BusinessOsPermission::DataRead.as_str()
            ],
        )?;
        Ok(())
    }

    #[tokio::test]
    async fn native_document_view_preserves_domain_parent_then_projection_fallback(
    ) -> anyhow::Result<()> {
        let fixture = read_fixture("user").await?;
        grant_read(fixture.root.path(), "alice", "business_commands")?;
        let domain = store::open_store(fixture.root.path())?;
        store::upsert_business_record(
            &domain,
            "business_commands",
            "parent",
            1,
            json!({"id":"parent", "client_context":{"actor":{"id":"alice"}}}),
        )?;
        let projection = rusqlite::Connection::open(store::rxdb_store_path(fixture.root.path()))?;
        projection.execute(
            &format!(
                "INSERT INTO {} (id,data) VALUES ('parent',?1)",
                fixture.table
            ),
            [json!({"id":"parent", "client_context":{"actor":{"id":"foreign"}}}).to_string()],
        )?;
        let child = json!({"id":"child", "command_id":"child", "payload":{"workflow_id":"parent"}});
        let legacy = super::super::threads::replication_document_filter(
            fixture.root.path(),
            &fixture.token,
            "business_commands",
        );
        assert!(legacy(&child));
        assert_eq!(
            fixture
                .policy
                .document_view(
                    &fixture.identity,
                    &fixture.token,
                    "business_commands",
                    &child,
                )
                .await?,
            Some(child.clone())
        );

        // A deleted domain record falls back exactly as the ordinary reader.
        domain.execute(
            "UPDATE business_records SET deleted=1 WHERE collection='business_commands' AND record_id='parent'",
            [],
        )?;
        assert!(!legacy(&child));
        assert_eq!(
            fixture
                .policy
                .document_view(
                    &fixture.identity,
                    &fixture.token,
                    "business_commands",
                    &child,
                )
                .await?,
            None
        );
        projection.execute(
            &format!("UPDATE {} SET data=?1 WHERE id='parent'", fixture.table),
            [json!({"id":"parent", "client_context":{"actor":{"id":"alice"}}}).to_string()],
        )?;
        assert!(legacy(&child));
        assert_eq!(
            fixture
                .policy
                .document_view(
                    &fixture.identity,
                    &fixture.token,
                    "business_commands",
                    &child,
                )
                .await?,
            Some(child)
        );
        Ok(())
    }

    #[tokio::test]
    async fn current_native_read_authority_holds_all_readers_and_rejects_role_change(
    ) -> anyhow::Result<()> {
        let fixture = read_fixture("chef").await?;
        let writers = [
            crate::paths::core_db(fixture.root.path()),
            store::business_os_store_path(fixture.root.path()),
            store::rxdb_store_path(fixture.root.path()),
        ]
        .into_iter()
        .map(|path| {
            let conn = rusqlite::Connection::open(path)?;
            conn.busy_timeout(std::time::Duration::ZERO)?;
            Ok(conn)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
        let core_path = crate::paths::core_db(fixture.root.path());
        crate::mission::channels::reset_channel_db_open_count_for_tests(&core_path);
        fixture.policy.with_current_read_authority(
            &fixture.identity, &fixture.token, "ctox_crew_members", &Scope::Instance {},
            |authority| {
                let crew = json!({"id":"crew","name":"Crew","soul":{"private":"fixture"}});
                assert_eq!(authority.document_view("ctox_crew_members", &crew)?, Some(crew));
                for writer in &writers {
                    let error = writer.execute_batch("BEGIN IMMEDIATE")
                        .expect_err("native mutation cannot cross the current read callback");
                    assert!(matches!(error, rusqlite::Error::SqliteFailure(code, _)
                        if matches!(code.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)));
                }
                assert_eq!(authority.core.total_changes(), 0);
                assert_eq!(authority.policy.total_changes(), 0);
                assert_eq!(authority.projection.total_changes(), 0);
                assert!(authority.document_view("business_records", &json!({"id":"other"})).is_err());
                Ok(())
            },
        )?;
        assert_eq!(
            crate::mission::channels::channel_db_open_count_for_tests(&core_path),
            0
        );
        for writer in &writers {
            writer.execute_batch("BEGIN IMMEDIATE; ROLLBACK")?;
        }
        writers[1].execute(
            "UPDATE business_users SET role='user' WHERE user_id='alice'",
            [],
        )?;
        let mut called = false;
        assert!(fixture
            .policy
            .with_current_read_authority(
                &fixture.identity,
                &fixture.token,
                "ctox_crew_members",
                &Scope::Instance {},
                |_| {
                    called = true;
                    Ok(())
                },
            )
            .is_err());
        assert!(
            !called,
            "an old signed actor cannot enter the callback after native role change"
        );

        let (current_token, _) = store::issue_business_os_capability_token_for_managed_user(
            fixture.root.path(),
            "alice",
            "Alice",
            "user",
            chrono::Utc::now().timestamp_millis(),
        )?;
        let identity = fixture
            .policy
            .identity(&current_token)
            .await
            .expect("fresh native user");
        let crew = json!({"id":"crew","name":"Crew","soul":{"private":"fixture"}});
        let projected = fixture
            .policy
            .document_view(&identity, &current_token, "ctox_crew_members", &crew)
            .await?
            .expect("public crew remains readable");
        assert_eq!(projected["id"], "crew");
        assert_eq!(projected["name"], "Crew");
        assert!(
            projected.get("soul").is_none(),
            "current role's field projection must remain authoritative"
        );
        std::fs::write(
            fixture.root.path().join("runtime/business-os-instance-id"),
            "another-native-instance\n",
        )?;
        assert!(
            fixture
                .policy
                .document_view(&identity, &current_token, "ctox_crew_members", &crew)
                .await
                .is_err(),
            "cached connection identity cannot authorize a changed native instance"
        );
        Ok(())
    }

    #[test]
    fn native_instance_read_never_initializes_missing_authority() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        assert!(store::existing_instance_id(root.path()).is_err());
        assert!(!root.path().join("runtime").exists());
        let id = store::stable_instance_id(root.path())?;
        assert_eq!(store::existing_instance_id(root.path())?, id);
        std::fs::write(
            root.path().join("runtime/business-os-instance-id"),
            "changed-native-instance\n",
        )?;
        assert_eq!(
            store::existing_instance_id(root.path())?,
            "changed-native-instance"
        );
        Ok(())
    }

    #[tokio::test]
    async fn signed_queue_admission_preserves_exact_observation_owner() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        store::tests::seed_business_user(root.path(), "alice", "chef")?;
        store::tests::seed_business_user(root.path(), "bob", "chef")?;
        let issue = |user| {
            store::issue_business_os_capability_token_for_managed_user(
                root.path(),
                user,
                user,
                "chef",
                chrono::Utc::now().timestamp_millis(),
            )
        };
        let (alice_token, _) = issue("alice")?;
        let (bob_token, _) = issue("bob")?;
        let policy = NativeBusinessDataPolicy::new(root.path().to_path_buf());
        let alice = policy
            .identity(&alice_token)
            .await
            .expect("current signed Alice identity");
        let bob = policy
            .identity(&bob_token)
            .await
            .expect("current signed Bob identity");
        let command = Command {
            command_id: "native-owned-queue".into(),
            command_type: "business_os.command".into(),
            payload: json!({"instruction":"Record a bounded fixture task"}),
        };
        let admitted = policy
            .submit_command(&alice, &alice_token, &command)
            .await?;
        assert_eq!(admitted.command_id, command.command_id);
        let before = crate::mission::channels::business_command_projection(
            root.path(),
            &command.command_id,
        )?;
        assert_eq!(before["native_authorization"]["actor"]["id"], "alice");
        assert_eq!(before["native_authorization"]["actor"]["trusted"], true);

        // Even a forged replicated owner/result must not replace native state.
        let forged = json!({"id":command.command_id, "native_owner":{
            "contract":"ctox-business-command-owner-v1", "user_id":"bob"
        }, "status":"completed", "result":{"forged":true}});
        assert_eq!(policy.command_state(&alice, &forged).await?, admitted);
        assert!(policy.command_state(&bob, &forged).await.is_err());
        assert!(policy
            .command_state(&alice, &json!({"id":"missing-command"}))
            .await
            .is_err());
        assert!(policy
            .submit_command(&bob, &bob_token, &command)
            .await
            .is_err());
        let after = crate::mission::channels::business_command_projection(
            root.path(),
            &command.command_id,
        )?;
        assert_eq!(
            after["native_authorization"],
            before["native_authorization"]
        );
        assert_eq!(policy.command_state(&alice, &forged).await?, admitted);
        assert!(policy.command_state(&bob, &forged).await.is_err());
        Ok(())
    }

    #[test]
    fn canonical_owner_binding_requires_exact_owner() {
        let mut stored = json!({"id":"command-a", "native_owner": {
            "contract":"ctox-business-command-owner-v1", "user_id":"alice"
        }});
        assert!(owned_command_projection(&stored, "command-a", "alice").is_ok());
        assert!(owned_command_projection(&stored, "command-a", "bob").is_err());
        stored["native_owner"]["contract"] = json!("untrusted");
        assert!(owned_command_projection(&stored, "command-a", "alice").is_err());
    }

    #[test]
    fn claimed_owner_and_client_context_cannot_replace_native_receipt() {
        let forged = json!({"id":"command-a", "owner_user_id":"alice",
            "client_context":{"actor":{"id":"alice","trusted":true}}});
        assert!(owned_command_projection(&forged, "command-a", "alice").is_err());
    }

    #[test]
    fn native_receipt_binds_exact_command_and_owner() {
        let valid = json!({"id":"command-a", "status":"completed", "result":{"ok":true},
            "native_authorization":{"contract":"ctox-business-command-authorization-v1",
                "allowed":true,"actor":{"id":"alice","trusted":true}}});
        let state = owned_command_projection(&valid, "command-a", "alice").unwrap();
        assert_eq!(state.command_id, "command-a");
        assert_eq!(state.result, Some(json!({"ok":true})));
        assert!(owned_command_projection(&valid, "command-a", "bob").is_err());
        assert!(owned_command_projection(&valid, "command-b", "alice").is_err());
        for pointer in [
            "/native_authorization/allowed",
            "/native_authorization/actor/trusted",
        ] {
            let mut denied = valid.clone();
            *denied.pointer_mut(pointer).unwrap() = json!(false);
            assert!(owned_command_projection(&denied, "command-a", "alice").is_err());
        }
        let mut conflicting = valid;
        conflicting["command_id"] = json!("command-b");
        assert!(owned_command_projection(&conflicting, "command-a", "alice").is_err());
    }
}

#[async_trait::async_trait]
impl BusinessDataAccessPolicy for NativeBusinessDataPolicy {
    async fn identity(&self, capability_token: &str) -> Option<RemoteIdentity> {
        let root = self.root.clone();
        let token = capability_token.to_owned();
        tokio::task::spawn_blocking(move || {
            let claims = store::verified_webrtc_capability_claims(&root, &token)?;
            let instance = store::sync_connection_config(&root).ok()?.instance_id;
            Some(RemoteIdentity {
                user_id: claims.user_id,
                authorization_epoch: u64::try_from(claims.actor_epoch).ok()?,
                instance_id: instance,
            })
        })
        .await
        .ok()
        .flatten()
    }

    async fn authorize(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        collection: &str,
        access: Access,
        scope: &Scope,
    ) -> io::Result<()> {
        self.allowed(collection, scope)?;
        let root = self.root.clone();
        let token = capability_token.to_owned();
        let collection = collection.to_owned();
        let user_id = identity.user_id.clone();
        let authorization_epoch = identity.authorization_epoch;
        let read = access == Access::Read;
        tokio::task::spawn_blocking(move || {
            let claims =
                store::verified_webrtc_capability_claims(&root, &token).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::PermissionDenied, "capability expired")
                })?;
            if claims.user_id != user_id
                || u64::try_from(claims.actor_epoch).ok() != Some(authorization_epoch)
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "actor changed",
                ));
            }
            let permission = if read {
                store::BusinessOsPermission::DataRead
            } else {
                store::BusinessOsPermission::DataWrite
            };
            if store::webrtc_capability_allows_collection_permission(
                &root,
                &token,
                &collection,
                permission,
            ) {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "BusinessData access denied",
                ))
            }
        })
        .await
        .map_err(|_| io::Error::other("BusinessData policy task failed"))?
    }

    async fn submit_command(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        command: &Command,
    ) -> io::Result<CommandState> {
        let document = json!({
            "command_id": command.command_id,
            "command_type": command.command_type,
            "payload": command.payload,
            "inbound_channel": "native_business_data",
            "client_context": {
                "actor": { "id": identity.user_id },
                "capability_token": capability_token,
            }
        });
        let _guard = self.command_lock.lock().await;
        // Canonical admission must authorize and check idempotency before any
        // replicated projection can be changed. Never stage a caller's command
        // over an existing RxDB command while admission may still reject it.
        let root = self.root.clone();
        let accepted = tokio::task::spawn_blocking(move || {
            store::accept_rxdb_business_command_with_origin(
                &root,
                document,
                store::CommandOrigin::ReplicatedPeer,
            )
        })
        .await
        .map_err(|_| io::Error::other("BusinessData command task failed"))?
        .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error.to_string()))?;
        let command_id = accepted
            .get("command_id")
            .or_else(|| accepted.get("id"))
            .and_then(Value::as_str)
            .unwrap_or(command.command_id.as_str());
        if command_id != command.command_id {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "command admission returned a different identity",
            ));
        }
        // Admission results must come from the canonical owned command, not
        // from a potentially delayed or stale replicated projection.
        self.command_state(identity, &json!({"id": command_id}))
            .await
    }

    async fn command_state(
        &self,
        identity: &RemoteIdentity,
        document: &Value,
    ) -> io::Result<CommandState> {
        let command_id = document
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "command ID is required"))?;
        let root = self.root.clone();
        let command_id = command_id.to_owned();
        let user_id = identity.user_id.clone();
        tokio::task::spawn_blocking(move || {
            // Ownership comes from the server-persisted admission receipt,
            // never from a peer-writable RxDB owner or client_context field.
            let stored = crate::mission::channels::business_command_projection(&root, &command_id)
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::PermissionDenied, "command is unavailable")
                })?;
            owned_command_projection(&stored, &command_id, &user_id)
        })
        .await
        .map_err(|_| io::Error::other("BusinessData command owner policy task failed"))?
    }

    async fn authorize_query(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        collection: &str,
        scope: &Scope,
        query: &Value,
    ) -> io::Result<()> {
        self.authorize(identity, capability_token, collection, Access::Read, scope)
            .await?;
        if collection != "ctox_crew_members" {
            return Ok(());
        }
        let root = self.root.clone();
        let token = capability_token.to_owned();
        let query = query.clone();
        tokio::task::spawn_blocking(move || {
            let (_, role) =
                store::verify_webrtc_capability_actor(&root, &token).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::PermissionDenied, "capability expired")
                })?;
            if let Some(fields) = super::policy::crew_fields_for_role(&role) {
                if !rxdb::plugins::replication_webrtc::webrtc_types::readable_query_fields(
                    &query, &fields,
                ) {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "BusinessData query references unreadable fields",
                    ));
                }
            }
            Ok(())
        })
        .await
        .map_err(|_| io::Error::other("BusinessData query policy task failed"))?
    }

    async fn document_view(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        collection: &str,
        document: &Value,
    ) -> io::Result<Option<Value>> {
        self.authorize(
            identity,
            capability_token,
            collection,
            Access::Read,
            &Scope::Instance {},
        )
        .await?;
        let policy = self.clone();
        let token = capability_token.to_owned();
        let identity = identity.clone();
        let requested_collection = collection.to_owned();
        let document = document.clone();
        tokio::task::spawn_blocking(move || {
            policy.with_current_read_authority(
                &identity,
                &token,
                &requested_collection,
                &Scope::Instance {},
                |authority| authority.document_view(&requested_collection, &document),
            )
        })
        .await
        .map_err(|_| io::Error::other("BusinessData document policy task failed"))?
    }

    async fn command_event(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        expected_command_id: &str,
        document: &Value,
    ) -> io::Result<Option<CommandState>> {
        let actual = document
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if actual != expected_command_id {
            return Ok(None);
        }
        self.authorize(
            identity,
            capability_token,
            "business_commands",
            Access::Read,
            &Scope::Instance {},
        )
        .await?;
        self.command_state(identity, &json!({"id": expected_command_id}))
            .await
            .map(Some)
    }
}
