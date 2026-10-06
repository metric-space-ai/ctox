//! Business OS policy and durable command intake for native BusinessData.
//! Every request is revalidated against the current signed WebRTC capability.
//! Scope is narrowed server-side, and commands reuse ReplicatedPeer intake.

use super::{policy::BusinessOsPermission, store};
use ctox_sync::business_data_contract::{
    NativeBusinessDataCommand as Command, NativeBusinessDataCommandState as CommandState,
    NativeBusinessDataCommandStatus, NativeBusinessDataEvent as Event,
    NativeBusinessDataEventPayload as EventPayload, NativeBusinessDataOperation as Operation,
    NativeBusinessDataQuery as Query, NativeBusinessDataRecord as Record,
    NativeBusinessDataRequest as Request, NativeBusinessDataResponse as Response,
    NativeBusinessDataResult as WireResult, NativeBusinessDataScope as Scope,
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
    access: Access,
    claims: super::capability::CapabilityClaims,
}

impl NativeReadAuthority<'_> {
    fn authorize_query(&self, query: &Query) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.access == Access::Read && query.collection == self.collection,
            "query read authority changed"
        );
        if self.collection == "ctox_crew_members" {
            if let Some(fields) = super::policy::crew_fields_for_role(&self.claims.role) {
                anyhow::ensure!(
                    rxdb::plugins::replication_webrtc::webrtc_types::readable_query_fields(
                        &query.query,
                        &fields,
                    ),
                    "query references unreadable fields"
                );
            }
        }
        Ok(())
    }

    fn prepared_record(
        &self,
        query: &Query,
        selector: &rxdb::util::mango::Query,
        record: &Record,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            !record.document_id.is_empty()
                && record.document.get("id").and_then(Value::as_str)
                    == Some(record.document_id.as_str()),
            "prepared record identity changed"
        );
        // Re-read current ownership/scope; values still come from the snapshot.
        let current = store::load_rxdb_collection_record_from_connection(
            self.projection,
            self.collection,
            &record.document_id,
        )?
        .ok_or_else(|| anyhow::anyhow!("prepared row is no longer available"))?;
        anyhow::ensure!(
            self.document_view(self.collection, &current)?.is_some()
                && scope_contains(&query.scope, &current)
                && scope_contains(&query.scope, &record.document)
                && selector.test(&record.document)
                && self.document_view(self.collection, &record.document)?
                    == Some(record.document.clone()),
            "prepared record visibility or projection changed"
        );
        Ok(())
    }

    fn command_state(&self, command_id: &str) -> anyhow::Result<CommandState> {
        let stored =
            crate::mission::channels::business_command_projection_from_conn(self.core, command_id)?;
        Ok(owned_command_projection(
            &stored,
            command_id,
            &self.claims.user_id,
        )?)
    }

    fn document_view(&self, collection: &str, document: &Value) -> anyhow::Result<Option<Value>> {
        anyhow::ensure!(
            self.access == Access::Read && collection == self.collection,
            "read authority belongs to another collection or access"
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
        self.with_current_authority(
            identity,
            capability_token,
            collection,
            Access::Read,
            scope,
            apply,
        )
    }

    fn with_current_authority<T>(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        collection: &str,
        access: Access,
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
                    if access == Access::Read {
                        BusinessOsPermission::DataRead
                    } else {
                        BusinessOsPermission::DataWrite
                    },
                    at_ms,
                ),
                "current native collection access denied"
            );
            let result = apply(&NativeReadAuthority {
                core: &core,
                policy: &policy,
                projection: &projection,
                collection,
                access,
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

fn scope_contains(scope: &Scope, document: &Value) -> bool {
    if document.get("_deleted").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    match scope {
        Scope::Instance {} => true,
        Scope::Project { project_id } => {
            document.get("project_id").and_then(Value::as_str) == Some(project_id.as_str())
        }
        Scope::Thread {
            project_id,
            thread_id,
        } => {
            document.get("project_id").and_then(Value::as_str) == Some(project_id.as_str())
                && document.get("thread_id").and_then(Value::as_str) == Some(thread_id.as_str())
        }
    }
}

/// Created only by the local policy factory, never from a wire guard flag.
struct NativeResponsePublication {
    policy: NativeBusinessDataPolicy,
    identity: RemoteIdentity,
    capability_token: String,
    request: Request,
    response: Response,
    selector: Option<rxdb::util::mango::Query>,
}

impl rxdb::plugins::replication_webrtc::WebRTCPublicationGuard for NativeResponsePublication {
    fn with_current(
        &self,
        publish: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
    ) -> rxdb::rx_error::RxResult<()> {
        let denied = || rxdb::rx_error::new_rx_error("BUSINESS_DATA_CURRENT_POLICY_DENIED", None);
        let (collection, access, scope) = match &self.request.operation {
            Operation::Query { query, .. } | Operation::Watch { query, .. } => {
                (query.collection.as_str(), Access::Read, query.scope.clone())
            }
            Operation::ObserveCommand { .. } => {
                ("business_commands", Access::Read, Scope::Instance {})
            }
            Operation::SubmitCommand { .. } => {
                ("business_commands", Access::Write, Scope::Instance {})
            }
            _ => return Err(denied()),
        };
        self.policy
            .with_current_authority(
                &self.identity,
                &self.capability_token,
                collection,
                access,
                &scope,
                |authority| {
                    match (&self.request.operation, &self.response.result) {
                        (
                            Operation::Query { session, query, .. },
                            WireResult::Page {
                                session: actual,
                                records,
                                ..
                            },
                        ) if actual == session => {
                            authority.authorize_query(query)?;
                            let selector = self
                                .selector
                                .as_ref()
                                .ok_or_else(|| anyhow::anyhow!("prepared selector unavailable"))?;
                            for record in records {
                                authority.prepared_record(query, selector, record)?;
                            }
                        }
                        (
                            Operation::Watch { session, query, .. },
                            WireResult::Subscribed {
                                session: actual,
                                subscription_id,
                            },
                        ) if actual == session && !subscription_id.is_empty() => {
                            authority.authorize_query(query)?;
                        }
                        (
                            Operation::ObserveCommand {
                                session,
                                command_id,
                            },
                            WireResult::Command {
                                session: actual,
                                state,
                            },
                        ) if actual == session => {
                            anyhow::ensure!(
                                state.command_id == *command_id
                                    && authority.command_state(command_id)? == *state,
                                "prepared command authority or state changed"
                            );
                        }
                        (
                            Operation::SubmitCommand { session, command },
                            WireResult::Command {
                                session: actual,
                                state,
                            },
                        ) if actual == session => {
                            anyhow::ensure!(
                                state.command_id == command.command_id
                                    && authority.command_state(&command.command_id)? == *state,
                                "prepared admitted command authority or state changed"
                            );
                        }
                        _ => anyhow::bail!("response does not belong to this request"),
                    }
                    // The current issuer/Core/policy/projection fences are still
                    // held while the transport performs this single bounded poll.
                    Ok(publish())
                },
            )
            .map_err(|_| denied())?
    }
}

struct NativeEventPublication {
    policy: NativeBusinessDataPolicy,
    identity: RemoteIdentity,
    capability_token: String,
    query: Query,
    command_id: Option<String>,
    event: Event,
    selector: rxdb::util::mango::Query,
}

impl rxdb::plugins::replication_webrtc::WebRTCPublicationGuard for NativeEventPublication {
    fn with_current(
        &self,
        publish: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
    ) -> rxdb::rx_error::RxResult<()> {
        let denied = || rxdb::rx_error::new_rx_error("BUSINESS_DATA_CURRENT_POLICY_DENIED", None);
        self.policy
            .with_current_read_authority(
                &self.identity,
                &self.capability_token,
                &self.query.collection,
                &self.query.scope,
                |authority| {
                    authority.authorize_query(&self.query)?;
                    let owned_command = if let Some(command_id) = &self.command_id {
                        anyhow::ensure!(
                            self.query.collection == "business_commands",
                            "command collection changed"
                        );
                        Some(authority.command_state(command_id)?)
                    } else {
                        None
                    };
                    match &self.event.payload {
                        EventPayload::SnapshotPage { records, .. } if self.command_id.is_none() => {
                            for record in records {
                                authority.prepared_record(&self.query, &self.selector, record)?;
                            }
                        }
                        EventPayload::Upsert { record, .. } if self.command_id.is_none() => {
                            authority.prepared_record(&self.query, &self.selector, record)?
                        }
                        EventPayload::Remove { document_id, .. }
                            if self.command_id.is_none() && !document_id.is_empty() =>
                        {
                            // Only the source's visible-ID set can create this removal.
                            // The queued event also retains that exact subscription.
                        }
                        EventPayload::Command { state } => {
                            anyhow::ensure!(
                                owned_command.as_ref() == Some(state),
                                "command owner or state changed"
                            );
                        }
                        EventPayload::SnapshotStart { .. }
                        | EventPayload::SnapshotEnd { .. }
                        | EventPayload::CaughtUp { .. } => {}
                        // Control-only publication is separately classified and
                        // sanitized by the source; it cannot mint a data guard.
                        _ => anyhow::bail!("event is not authorized data"),
                    }
                    Ok(publish())
                },
            )
            .map_err(|_| denied())?
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
        // Active grant changes revoke earlier capability epochs. Admit the
        // fixture token only after its ordinary collection grant is installed.
        if role == "user" {
            grant_read(root.path(), "alice", "business_commands")?;
        }
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
                BusinessOsPermission::DataRead.as_str()
            ],
        )?;
        Ok(())
    }

    #[tokio::test]
    async fn native_document_view_preserves_domain_parent_then_projection_fallback(
    ) -> anyhow::Result<()> {
        let fixture = read_fixture("user").await?;
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
        let parent =
            store::pull_collection_record(fixture.root.path(), "business_commands", "parent")?
                .expect("domain parent must exist before publication");
        assert_eq!(parent["client_context"]["actor"]["id"], "alice");
        assert!(
            store::webrtc_capability_allows_collection_permission(
                fixture.root.path(),
                &fixture.token,
                "business_commands",
                BusinessOsPermission::DataRead,
            ),
            "fixture must grant current collection read access"
        );
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

    fn prepared_page(query: Query, document: Option<Value>) -> (Request, Response) {
        let session = ctox_sync::business_data_contract::NativeBusinessDataSessionRef {
            handle: "native-publication-fixture".into(),
            generation: 1,
        };
        let request = Request {
            version: ctox_sync::business_data_contract::CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
            request_id: "publication-fixture".into(),
            operation: Operation::Query {
                session: session.clone(),
                query,
                page_cursor: None,
            },
        };
        let response = Response {
            version: request.version,
            request_id: request.request_id.clone(),
            result: WireResult::Page {
                session,
                snapshot_id: "native-snapshot".into(),
                records: document
                    .into_iter()
                    .map(|document| Record {
                        document_id: document["id"].as_str().unwrap().into(),
                        document,
                    })
                    .collect(),
                next_page_cursor: None,
                snapshot_complete: true,
            },
        };
        (request, response)
    }

    #[tokio::test]
    async fn native_response_publication_holds_stores_and_rechecks_parent_visibility(
    ) -> anyhow::Result<()> {
        let fixture = read_fixture("user").await?;
        let domain = store::open_store(fixture.root.path())?;
        store::upsert_business_record(
            &domain,
            "business_commands",
            "parent",
            1,
            json!({"id":"parent", "client_context":{"actor":{"id":"alice"}}}),
        )?;
        let projection = rusqlite::Connection::open(store::rxdb_store_path(fixture.root.path()))?;
        let child = json!({"id":"child", "_deleted":false, "payload":{"workflow_id":"parent"}});
        projection.execute(
            &format!(
                "INSERT INTO {} (id,data) VALUES ('child',?1)",
                fixture.table
            ),
            [child.to_string()],
        )?;
        let (request, response) = prepared_page(
            Query {
                collection: "business_commands".into(),
                scope: Scope::Instance {},
                query: json!({"selector":{"id":"child"}, "sort":[{"id":"asc"}]}),
                page_size: 1,
            },
            Some(child),
        );
        let guard = fixture.policy.response_publication(
            &fixture.identity,
            &fixture.token,
            &request,
            &response,
        )?;
        let paths = [
            crate::paths::core_db(fixture.root.path()),
            store::business_os_store_path(fixture.root.path()),
            store::rxdb_store_path(fixture.root.path()),
        ];
        let writers = paths
            .iter()
            .map(rusqlite::Connection::open)
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for writer in &writers {
            writer.busy_timeout(std::time::Duration::ZERO)?;
        }
        let mut calls = 0;
        guard.with_current(&mut || {
            calls += 1;
            for writer in &writers {
                let error = writer.execute_batch("BEGIN IMMEDIATE").expect_err("publication holds this store");
                assert!(matches!(error, rusqlite::Error::SqliteFailure(code, _)
                    if matches!(code.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)));
            }
            Ok(())
        }).expect("current native page must publish");
        assert_eq!(calls, 1);
        for writer in &writers {
            writer.execute_batch("BEGIN IMMEDIATE; ROLLBACK")?;
        }
        store::upsert_business_record(
            &domain,
            "business_commands",
            "parent",
            2,
            json!({"id":"parent", "client_context":{"actor":{"id":"foreign"}}}),
        )?;
        assert!(guard
            .with_current(&mut || {
                calls += 1;
                Ok(())
            })
            .is_err());
        assert_eq!(
            calls, 1,
            "retained guard must re-read current parent visibility"
        );

        let mut wrong = response.clone();
        wrong.request_id = "other-request".into();
        assert!(fixture
            .policy
            .response_publication(&fixture.identity, &fixture.token, &request, &wrong)
            .is_err());
        Ok(())
    }

    #[tokio::test]
    async fn native_response_and_event_publication_recheck_fields_and_empty_query(
    ) -> anyhow::Result<()> {
        let fixture = read_fixture("chef").await?;
        let projection = rusqlite::Connection::open(store::rxdb_store_path(fixture.root.path()))?;
        let schemas: Value =
            serde_json::from_str(include_str!("business_os_schema_contract.json"))?;
        let version = schemas["ctox_crew_members"]["version"].as_u64().unwrap();
        let table = format!("ctox_business_os__ctox_crew_members__v{version}");
        projection.execute_batch(&format!(
            "CREATE TABLE {table} (id TEXT PRIMARY KEY, data TEXT NOT NULL)"
        ))?;
        let crew =
            json!({"id":"crew", "_deleted":false, "name":"Crew", "soul":{"private":"fixture"}});
        projection.execute(
            &format!("INSERT INTO {table} (id,data) VALUES ('crew',?1)"),
            [crew.to_string()],
        )?;
        let query = Query {
            collection: "ctox_crew_members".into(),
            scope: Scope::Instance {},
            query: json!({"selector":{"id":"crew"}, "sort":[{"id":"asc"}]}),
            page_size: 1,
        };
        let (request, response) = prepared_page(query.clone(), Some(crew.clone()));
        let before = fixture.policy.response_publication(
            &fixture.identity,
            &fixture.token,
            &request,
            &response,
        )?;
        before
            .with_current(&mut || Ok(()))
            .expect("current chef view");
        let session = match &request.operation {
            Operation::Query { session, .. } => session.clone(),
            _ => unreachable!(),
        };
        let event = Event {
            version: request.version,
            session,
            subscription_id: "crew-watch".into(),
            sequence: 1,
            payload: EventPayload::Upsert {
                cursor: "native-cursor".into(),
                record: Record {
                    document_id: "crew".into(),
                    document: crew.clone(),
                },
                recovery: false,
            },
        };
        let old_event = fixture.policy.event_publication(
            &fixture.identity,
            &fixture.token,
            &query,
            None,
            &event,
        )?;
        old_event
            .with_current(&mut || Ok(()))
            .expect("current chef event");
        store::open_store(fixture.root.path())?.execute(
            "UPDATE business_users SET role='user' WHERE user_id='alice'",
            [],
        )?;
        let mut calls = 0;
        assert!(before
            .with_current(&mut || {
                calls += 1;
                Ok(())
            })
            .is_err());
        assert!(old_event
            .with_current(&mut || {
                calls += 1;
                Ok(())
            })
            .is_err());
        let (token, _) = store::issue_business_os_capability_token_for_managed_user(
            fixture.root.path(),
            "alice",
            "Alice",
            "user",
            chrono::Utc::now().timestamp_millis(),
        )?;
        let identity = fixture.policy.identity(&token).await.unwrap();
        let changed = fixture
            .policy
            .response_publication(&identity, &token, &request, &response)?;
        let changed_event = fixture
            .policy
            .event_publication(&identity, &token, &query, None, &event)?;
        assert!(changed
            .with_current(&mut || {
                calls += 1;
                Ok(())
            })
            .is_err());
        assert!(changed_event
            .with_current(&mut || {
                calls += 1;
                Ok(())
            })
            .is_err());
        assert_eq!(
            calls, 0,
            "current projection must reject already-prepared private fields"
        );
        let public = fixture
            .policy
            .document_view(&identity, &token, "ctox_crew_members", &crew)
            .await?
            .unwrap();
        let (public_request, public_response) = prepared_page(query.clone(), Some(public));
        fixture
            .policy
            .response_publication(&identity, &token, &public_request, &public_response)?
            .with_current(&mut || Ok(()))
            .expect("current public field view");

        let (empty_request, empty_response) = prepared_page(
            Query {
                query: json!({"selector":{"soul.private":"fixture"}, "sort":[{"id":"asc"}]}),
                ..query
            },
            None,
        );
        assert!(fixture
            .policy
            .response_publication(&identity, &token, &empty_request, &empty_response)?
            .with_current(&mut || {
                calls += 1;
                Ok(())
            })
            .is_err());
        assert_eq!(
            calls, 0,
            "an empty result cannot bypass current query-field policy"
        );
        Ok(())
    }

    #[tokio::test]
    async fn native_command_publication_uses_current_core_owner_and_exact_state(
    ) -> anyhow::Result<()> {
        let fixture = read_fixture("chef").await?;
        store::tests::seed_business_user(fixture.root.path(), "bob", "chef")?;
        let (bob_token, _) = store::issue_business_os_capability_token_for_managed_user(
            fixture.root.path(),
            "bob",
            "Bob",
            "chef",
            chrono::Utc::now().timestamp_millis(),
        )?;
        let bob = fixture.policy.identity(&bob_token).await.unwrap();
        let command = Command {
            command_id: "native-publication-command".into(),
            command_type: "business_os.command".into(),
            payload: json!({"instruction":"Record a bounded publication fixture task"}),
        };
        let state = fixture
            .policy
            .submit_command(&fixture.identity, &fixture.token, &command)
            .await?;
        let session = ctox_sync::business_data_contract::NativeBusinessDataSessionRef {
            handle: "owned-native-command".into(),
            generation: 1,
        };
        let request = Request {
            version: ctox_sync::business_data_contract::CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
            request_id: "owned-command-response".into(),
            operation: Operation::ObserveCommand {
                session: session.clone(),
                command_id: command.command_id.clone(),
            },
        };
        let response = Response {
            version: request.version,
            request_id: request.request_id.clone(),
            result: WireResult::Command {
                session: session.clone(),
                state: state.clone(),
            },
        };
        let owner = fixture.policy.response_publication(
            &fixture.identity,
            &fixture.token,
            &request,
            &response,
        )?;
        owner
            .with_current(&mut || Ok(()))
            .expect("current canonical owner");
        let foreign = fixture
            .policy
            .response_publication(&bob, &bob_token, &request, &response)?;
        let mut calls = 0;
        assert!(foreign
            .with_current(&mut || {
                calls += 1;
                Ok(())
            })
            .is_err());
        let mut forged_response = response.clone();
        if let WireResult::Command { state, .. } = &mut forged_response.result {
            state.result = Some(json!({"forged":true}));
        }
        assert!(fixture
            .policy
            .response_publication(
                &fixture.identity,
                &fixture.token,
                &request,
                &forged_response
            )?
            .with_current(&mut || {
                calls += 1;
                Ok(())
            })
            .is_err());
        let query = Query {
            collection: "business_commands".into(),
            scope: Scope::Instance {},
            query: json!({"selector":{"id":command.command_id}, "sort":[{"id":"asc"}]}),
            page_size: 1,
        };
        let event = Event {
            version: request.version,
            session,
            subscription_id: format!("command:{}", command.command_id),
            sequence: 1,
            payload: EventPayload::Command { state },
        };
        let event_guard = fixture.policy.event_publication(
            &fixture.identity,
            &fixture.token,
            &query,
            Some(&command.command_id),
            &event,
        )?;
        event_guard
            .with_current(&mut || Ok(()))
            .expect("current owned command event");
        let core = rusqlite::Connection::open(crate::paths::core_db(fixture.root.path()))?;
        let mut intent: Value = serde_json::from_str(&core.query_row(
            "SELECT intent_json FROM business_command_aggregates WHERE command_id=?1",
            [&command.command_id],
            |row| row.get::<_, String>(0),
        )?)?;
        intent["native_owner"] =
            json!({"contract":"ctox-business-command-owner-v1", "user_id":"bob"});
        core.execute(
            "UPDATE business_command_aggregates SET intent_json=?2 WHERE command_id=?1",
            rusqlite::params![command.command_id, intent.to_string()],
        )?;
        assert!(owner
            .with_current(&mut || {
                calls += 1;
                Ok(())
            })
            .is_err());
        assert!(event_guard
            .with_current(&mut || {
                calls += 1;
                Ok(())
            })
            .is_err());
        assert_eq!(
            calls, 0,
            "neither prepared response nor event survives canonical owner change"
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
    fn response_publication(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        request: &Request,
        response: &Response,
    ) -> io::Result<Arc<dyn rxdb::plugins::replication_webrtc::WebRTCPublicationGuard>> {
        if request.version != ctox_sync::business_data_contract::CTOX_BUSINESS_DATA_PROTOCOL_VERSION
            || response.version != request.version
            || response.request_id != request.request_id
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "response binding changed",
            ));
        }
        let selector = match &request.operation {
            Operation::Query { query, .. } => {
                // Source preparation already validated the supported Mango
                // query; this matcher reuses its native predicates.
                let selector = query
                    .query
                    .get("selector")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if !selector.is_object() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "invalid selector",
                    ));
                }
                let selector =
                    ctox_sync::business_data_remote::scope_selector(&query.scope, Some(selector));
                Some(rxdb::util::mango::Query::new(&selector))
            }
            Operation::Watch { .. }
            | Operation::ObserveCommand { .. }
            | Operation::SubmitCommand { .. } => None,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "unsupported data publication",
                ))
            }
        };
        Ok(Arc::new(NativeResponsePublication {
            policy: self.clone(),
            identity: identity.clone(),
            capability_token: capability_token.to_owned(),
            request: request.clone(),
            response: response.clone(),
            selector,
        }))
    }

    fn event_publication(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        query: &Query,
        command_id: Option<&str>,
        event: &Event,
    ) -> io::Result<Arc<dyn rxdb::plugins::replication_webrtc::WebRTCPublicationGuard>> {
        if event.version != ctox_sync::business_data_contract::CTOX_BUSINESS_DATA_PROTOCOL_VERSION
            || event.subscription_id.is_empty()
            || event.sequence == 0
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "event binding changed",
            ));
        }
        let selector = query
            .query
            .get("selector")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !selector.is_object() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid selector",
            ));
        }
        let selector =
            ctox_sync::business_data_remote::scope_selector(&query.scope, Some(selector));
        Ok(Arc::new(NativeEventPublication {
            policy: self.clone(),
            identity: identity.clone(),
            capability_token: capability_token.to_owned(),
            query: query.clone(),
            command_id: command_id.map(str::to_owned),
            event: event.clone(),
            selector: rxdb::util::mango::Query::new(&selector),
        }))
    }

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
                BusinessOsPermission::DataRead
            } else {
                BusinessOsPermission::DataWrite
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
