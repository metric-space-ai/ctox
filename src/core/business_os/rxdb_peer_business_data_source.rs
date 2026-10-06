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

const READABLE_COLLECTIONS: &[&str] = &[
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
        let root = self.root.clone();
        let token = capability_token.to_owned();
        let requested_collection = collection.to_owned();
        let mut visible = document.clone();
        tokio::task::spawn_blocking(move || {
            let filter =
                super::threads::replication_document_filter(&root, &token, &requested_collection);
            if !filter(&visible) {
                return Ok(None);
            }
            let fields = if requested_collection == "ctox_crew_members" {
                store::verify_webrtc_capability_actor(&root, &token)
                    .map(|(_, role)| role)
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "BusinessData capability is no longer valid",
                        )
                    })?
            } else {
                String::new()
            };
            if requested_collection == "ctox_crew_members" {
                if let Some(allowed) = super::policy::crew_fields_for_role(&fields) {
                    rxdb::plugins::replication_webrtc::webrtc_types::retain_readable_fields(
                        &mut visible,
                        &allowed,
                    );
                }
            }
            Ok(Some(visible))
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
