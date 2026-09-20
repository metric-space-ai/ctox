//! Business OS policy and durable command intake for native BusinessData.
//! Every request is revalidated against the current signed WebRTC capability.
//! Scope is narrowed server-side, and commands reuse ReplicatedPeer intake.

use ctox_sync::business_data_contract::{
    NativeBusinessDataCommand as Command, NativeBusinessDataCommandState as CommandState,
    NativeBusinessDataCommandStatus, NativeBusinessDataScope as Scope,
};
use ctox_sync::business_data_remote::{Access, BusinessDataAccessPolicy, RemoteIdentity};
use rxdb::rx_database::RxDatabase;
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
    database: Arc<RxDatabase>,
    command_lock: Arc<Mutex<()>>,
}

impl NativeBusinessDataPolicy {
    pub fn new(root: PathBuf, database: Arc<RxDatabase>) -> Self {
        Self {
            root,
            database,
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

    async fn command_document(&self, command_id: &str) -> Option<Value> {
        let commands = self.database.collection("business_commands")?;
        commands
            .find_one(Some(rxdb::types::MangoQuery {
                selector: Some(json!({ "id": { "$eq": command_id } })),
                ..Default::default()
            }))
            .ok()?
            .exec(false)
            .await
            .ok()
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
        let read = access == Access::Read;
        tokio::task::spawn_blocking(move || {
            let claims =
                store::verified_webrtc_capability_claims(&root, &token).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::PermissionDenied, "capability expired")
                })?;
            if claims.user_id != user_id {
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
        let queued = super::rxdb_peer::enqueue_business_command_document_with_database_public(
            &self.database,
            document,
        )
        .await
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
        let root = self.root.clone();
        let accepted = tokio::task::spawn_blocking(move || {
            store::accept_rxdb_business_command_with_origin(
                &root,
                queued,
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
        let stored = self.command_document(command_id).await.unwrap_or_else(|| {
            json!({ "id": command.command_id, "command_id": command.command_id, "status": "unknown" })
        });
        Ok(command_projection(&stored))
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
        let stored = self.command_document(command_id).await.unwrap_or_else(
            || json!({ "id": command_id, "command_id": command_id, "status": "unknown" }),
        );
        if stored
            .get("owner_user_id")
            .and_then(Value::as_str)
            .is_some_and(|owner| owner != identity.user_id)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "command owner changed",
            ));
        }
        Ok(command_projection(&stored))
    }
}
