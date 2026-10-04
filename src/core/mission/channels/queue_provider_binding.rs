//! Native provider preparation and turn witness. A persisted row alone is not authority.
#[cfg(test)]
use super::resolve_db_path;
use super::QueueExecutionFence;
use anyhow::{ensure, Result};
#[cfg(test)]
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock, Weak};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NativeProviderFacts {
    pub(crate) schema: &'static str,
    pub(crate) binding_id: String,
    pub(crate) worker_id: String,
    pub(crate) attempt_id: String,
    pub(crate) routing_attempts: Vec<(String, i64)>,
    pub(crate) provider_session_id: String,
    pub(crate) model_id: String,
    pub(crate) model_provider_id: Option<String>,
    pub(crate) api_provider_id: Option<String>,
    /// Verified at preparation, not a future permission or account identity.
    pub(crate) command_provenance: Option<Value>,
    pub(crate) checkpoint_contract: Option<super::NativeProviderCheckpointContract>,
}

struct ProviderState {
    live: bool,
    /// Registry insertion precedes commit; a concurrent lookup must not turn
    /// an uncommitted or ambiguously failed preparation into live authority.
    committed: bool,
    turn_id: Option<String>,
}

struct ProviderRecord {
    execution: QueueExecutionFence,
    facts: NativeProviderFacts,
    facts_json: String,
    checkpoint: Option<super::NativeProviderCheckpointBinding>,
    state: Mutex<ProviderState>,
    emitted_commands: Mutex<HashSet<String>>,
}

type Registry = HashMap<(PathBuf, String), Weak<ProviderRecord>>;
fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Only the actual direct-session scope retains this owner. Consumers get a
/// different handle; cloning a consumer cannot prolong the provider's lifetime.
pub(crate) struct NativeProviderTurnOwner {
    record: Arc<ProviderRecord>,
}

#[derive(Clone)]
pub(crate) struct NativeProviderBinding {
    record: Arc<ProviderRecord>,
}

/// Issued only by the actual turn owner to the in-process MCP dispatcher.
/// Retaining this capability does not prolong that owner's live lifetime.
#[derive(Clone)]
pub(crate) struct NativeProviderCommandEmitter {
    provider: NativeProviderBinding,
}

/// A single native guest command admitted by the retained turn owner. It is
/// neither deserializable nor constructible from provider facts/session JSON.
/// Clones share consumption; a failed or uncertain effect cannot be repeated.
#[derive(Clone)]
pub(crate) struct NativeProviderCommand {
    provider: NativeProviderBinding,
    turn_id: String,
    canonical_command: Vec<u8>,
    consumed: Arc<Mutex<bool>>,
}

fn canonical_guest_command(
    command: &crate::business_os::store::BusinessCommand,
) -> Result<Vec<u8>> {
    use crate::business_os::store::CommandOrigin;
    ensure!(
        command.origin == CommandOrigin::TrustedLocal
            && command.id.as_deref().is_some_and(valid_id)
            && matches!(
                command.command_type.as_str(),
                "ctox.guest.observe" | "ctox.guest.input"
            ),
        "native guest emission requires an exact trusted command identity"
    );
    fn sorted(value: Value) -> Value {
        match value {
            Value::Object(object) => {
                let mut entries = object.into_iter().collect::<Vec<_>>();
                entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
                Value::Object(entries.into_iter().map(|(k, v)| (k, sorted(v))).collect())
            }
            Value::Array(array) => Value::Array(array.into_iter().map(sorted).collect()),
            other => other,
        }
    }
    let bytes = serde_json::to_vec(&sorted(serde_json::to_value(command)?))?;
    ensure!(
        bytes.len() <= 32 * 1024,
        "native guest command is oversized"
    );
    Ok(bytes)
}

/// Installed only by the native guest producer after it resolves its actual
/// destination and policy. The future must persist real Raft admission before
/// returning. A model payload or a persisted witness cannot register this hook.
pub(crate) trait NativeProviderAdmission: Send + Sync {
    /// Consume the actual emitted command under its held worker transaction,
    /// then the real policy/controller guard. Never re-enter provider guards.
    /// The default is deliberately closed until the native VM owner supplies
    /// its registered account/controller/effect implementation.
    fn execute_guest_command(
        &self,
        _command: &crate::business_os::store::BusinessCommand,
        _witness: NativeProviderCommand,
    ) -> Result<ctox_protocol::mcp::CallToolResult> {
        anyhow::bail!("native guest command consumer is not registered")
    }

    fn admit<'a>(
        &'a self,
        provider: NativeProviderBinding,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

fn valid_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)
}

impl NativeProviderTurnOwner {
    /// Called after actual thread create/resume and before TurnStart. No guessed
    /// gateway-account/harness-version/instance/project or Raft generation.
    pub(crate) fn prepare(
        execution: &QueueExecutionFence,
        provider_session_id: &str,
        model_id: &str,
        model_provider_id: Option<&str>,
        api_provider_id: Option<&str>,
        verified_command_context: Option<&Value>,
    ) -> Result<Self> {
        Self::prepare_with_checkpoint(
            execution,
            provider_session_id,
            model_id,
            model_provider_id,
            api_provider_id,
            verified_command_context,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_with_checkpoint(
        execution: &QueueExecutionFence,
        provider_session_id: &str,
        model_id: &str,
        model_provider_id: Option<&str>,
        api_provider_id: Option<&str>,
        verified_command_context: Option<&Value>,
        checkpoint: Option<&super::NativeProviderCheckpointBinding>,
    ) -> Result<Self> {
        let _account_guard = checkpoint
            .map(|binding| binding.auth.current_runtime_account_guard())
            .transpose()?;
        if let Some(binding) = checkpoint {
            ensure!(
                binding.auth.runtime_account_binding()
                    == Some(binding.contract.gateway_account_id.as_str())
                    && binding.contract.harness == ctox_core::native_harness_name()
                    && binding.contract.harness_version == ctox_core::native_harness_version()
                    && model_provider_id == Some(binding.contract.model_route_id.as_str()),
                "native checkpoint provider contract no longer matches its actual source"
            );
        }

        ensure!(
            valid_id(provider_session_id) && valid_id(model_id),
            "native provider preparation has no actual session/model identity"
        );
        if let Some(binding) = verified_command_context
            .and_then(|context| context.get("crew_binding"))
            .filter(|binding| !binding.is_null())
        {
            ensure!(
                binding.get("attempt_id").and_then(Value::as_str) == Some(execution.attempt_id()),
                "verified command session belongs to another native worker attempt"
            );
        }
        let command_provenance = verified_command_context.map(|context| {
            let keys = [
                "auth_source",
                "actor",
                "role",
                "workspace",
                "command_id",
                "payload_hash",
                "crew_binding",
                "crew_work_key",
                "expires_at_ms",
            ];
            let mut selected = serde_json::Map::new();
            for key in keys {
                if let Some(value) = context.get(key) {
                    selected.insert(key.to_owned(), value.clone());
                }
            }
            Value::Object(selected)
        });
        let facts = NativeProviderFacts {
            schema: "ctox.native.worker_provider_preparation.v1",
            binding_id: uuid::Uuid::new_v4().to_string(),
            worker_id: execution.worker_id().to_owned(),
            attempt_id: execution.attempt_id().to_owned(),
            routing_attempts: execution.routing_attempts().to_vec(),
            provider_session_id: provider_session_id.to_owned(),
            model_id: model_id.to_owned(),
            model_provider_id: model_provider_id.map(str::to_owned),
            api_provider_id: api_provider_id.map(str::to_owned),
            command_provenance,
            checkpoint_contract: checkpoint.map(|binding| binding.contract.clone()),
        };
        let facts_json = serde_json::to_string(&facts)?;
        ensure!(
            facts_json.len() <= 16 * 1024,
            "native provider witness oversized"
        );
        let key = (execution.root().to_owned(), facts.attempt_id.clone());
        // Lock order: exact native execution -> provider registry/state ->
        // guest controller. Never retain a registry lock while waiting on a
        // different worker's execution/lifetime lock.
        let record = execution.with_current_transaction(|tx| {
            let mut entries = registry()
                .lock()
                .map_err(|_| anyhow::anyhow!("native provider registry poisoned"))?;
            if let Some(existing) = entries.get(&key).and_then(Weak::upgrade) {
                let state = existing
                    .state
                    .try_lock()
                    .map_err(|_| anyhow::anyhow!("native provider binding is in use"))?;
                ensure!(
                    !state.live,
                    "native attempt already owns a provider session"
                );
            }
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS native_worker_provider_bindings (
                binding_id TEXT PRIMARY KEY,
                worker_id TEXT NOT NULL,
                attempt_id TEXT NOT NULL,
                provider_session_id TEXT NOT NULL,
                provider_turn_id TEXT,
                facts_json TEXT NOT NULL,
                prepared_at_ms INTEGER NOT NULL,
                finished_at_ms INTEGER
            )",
            )?;
            tx.execute(
                "INSERT INTO native_worker_provider_bindings
                (binding_id, worker_id, attempt_id, provider_session_id, facts_json, prepared_at_ms)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    facts.binding_id,
                    facts.worker_id,
                    facts.attempt_id,
                    facts.provider_session_id,
                    facts_json,
                    chrono::Utc::now().timestamp_millis()
                ],
            )?;
            let record = Arc::new(ProviderRecord {
                execution: execution.clone(),
                facts,
                facts_json,
                checkpoint: checkpoint.cloned(),
                state: Mutex::new(ProviderState {
                    live: true,
                    committed: false,
                    turn_id: None,
                }),
                emitted_commands: Mutex::new(HashSet::new()),
            });
            entries.insert(key.clone(), Arc::downgrade(&record));
            Ok(record)
        })?;
        // If commit fails, even a registry lookup that raced and upgraded the
        // weak reference sees committed=false and cannot obtain a consumer.
        let owner = Self { record };
        owner
            .record
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("native provider preparation state poisoned"))?
            .committed = true;
        Ok(owner)
    }

    pub(crate) fn binding(&self) -> NativeProviderBinding {
        NativeProviderBinding {
            record: Arc::clone(&self.record),
        }
    }

    pub(crate) fn command_emitter(&self) -> NativeProviderCommandEmitter {
        NativeProviderCommandEmitter {
            provider: self.binding(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn admit_emitted_guest_command(
        &self,
        actual_turn_id: &str,
        command: &crate::business_os::store::BusinessCommand,
        admit: impl FnOnce(
            &rusqlite::Transaction<'_>,
            &NativeProviderFacts,
            &str,
            &crate::business_os::store::BusinessCommand,
        ) -> Result<()>,
    ) -> Result<NativeProviderCommand> {
        self.command_emitter()
            .admit_emitted_guest_command(actual_turn_id, command, admit)
    }

    /// The actual TurnStart result may bind only the prepared provider session.
    pub(crate) fn bind_turn(&self, provider_session_id: &str, turn_id: &str) -> Result<()> {
        ensure!(
            valid_id(turn_id) && provider_session_id == self.record.facts.provider_session_id,
            "provider changed after native preparation/admission"
        );
        self.record.execution.with_current_transaction(|tx| {
            let mut state = self
                .record
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("native provider state poisoned"))?;
            ensure!(
                state.live && state.committed && state.turn_id.is_none(),
                "native provider turn already bound/closed"
            );
            let _account_guard = self
                .record
                .checkpoint
                .as_ref()
                .map(|binding| binding.auth.current_runtime_account_guard())
                .transpose()?;
            let updated = tx.execute(
                "UPDATE native_worker_provider_bindings
                SET provider_turn_id=?2
                WHERE binding_id=?1 AND finished_at_ms IS NULL AND provider_turn_id IS NULL
                AND facts_json=?3",
                rusqlite::params![
                    self.record.facts.binding_id,
                    turn_id,
                    self.record.facts_json
                ],
            )?;
            ensure!(
                updated == 1,
                "native provider preparation no longer matches its record"
            );
            state.turn_id = Some(turn_id.to_owned());
            Ok(())
        })
    }
}

impl NativeProviderBinding {
    /// Actual worker store root, not a value reconstructed from witness JSON.
    pub(crate) fn runtime_root(&self) -> &Path {
        self.record.execution.root()
    }

    pub(crate) async fn admit_before_start(
        &self,
        admission: &dyn NativeProviderAdmission,
    ) -> Result<()> {
        self.with_live_provider(|facts, turn| {
            ensure!(
                facts.checkpoint_contract.is_some(),
                "native guest admission requires actual account and harness binding"
            );
            ensure!(
                turn.is_none(),
                "provider turn already started before admission"
            );
            Ok(())
        })?;
        admission.admit(self.clone()).await?;
        // An await is not a reusable permit. Revalidate the retained provider
        // and exact worker after the native owner's actual admission completes.
        self.with_live_provider(|_, turn| {
            ensure!(turn.is_none(), "provider turn started during admission");
            Ok(())
        })
    }

    /// Current observation only. Raft admission must separately prove native
    /// destination/policy/account/version and persist its actual job binding.
    pub(crate) fn with_live_provider<T>(
        &self,
        publish: impl FnOnce(&NativeProviderFacts, Option<&str>) -> Result<T>,
    ) -> Result<T> {
        self.with_live_provider_transaction(|_, facts, turn| publish(facts, turn))
    }

    /// Native policy reads and frame/import writes may use this exact held
    /// channel transaction. Never reopen the same store, await, retain a
    /// transaction reference, or re-enter worker/provider lifecycle callbacks.
    /// The borrowed transaction cannot be returned as a future permit.
    pub(crate) fn with_live_provider_transaction<T>(
        &self,
        publish: impl FnOnce(
            &rusqlite::Transaction<'_>,
            &NativeProviderFacts,
            Option<&str>,
        ) -> Result<T>,
    ) -> Result<T> {
        self.record.execution.with_current_transaction(|tx| {
            let state = self
                .record
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("native provider state poisoned"))?;
            ensure!(
                state.live && state.committed,
                "native provider lifetime ended or preparation not committed"
            );
            let (json, turn, finished): (String, Option<String>, Option<i64>) = tx.query_row(
                "SELECT facts_json, provider_turn_id, finished_at_ms
                 FROM native_worker_provider_bindings WHERE binding_id=?1",
                [&self.record.facts.binding_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            ensure!(
                json == self.record.facts_json && turn == state.turn_id && finished.is_none(),
                "native provider witness changed, ended or was replayed"
            );
            let _account_guard = self
                .record
                .checkpoint
                .as_ref()
                .map(|binding| binding.auth.current_runtime_account_guard())
                .transpose()?;
            publish(tx, &self.record.facts, state.turn_id.as_deref())
        })
    }
}

impl NativeProviderCommandEmitter {
    /// Producer primitive, called only at actual native command emission.
    /// The callback must admit this exact command and reject a refused/replayed
    /// admission. It runs synchronously inside the live worker/provider guard;
    /// it may not await, reopen this store or re-enter lifecycle callbacks.
    /// The witness is returned only after that transaction commits. An ID is
    /// burned even on admission error, since an external effect may be uncertain.
    /// A session token, model payload or observed asynchronous tool-begin event
    /// is NOT a caller for this primitive. Wiring the real emitter is separate.
    #[allow(dead_code)]
    pub(crate) fn admit_emitted_guest_command(
        &self,
        actual_turn_id: &str,
        command: &crate::business_os::store::BusinessCommand,
        admit: impl FnOnce(
            &rusqlite::Transaction<'_>,
            &NativeProviderFacts,
            &str,
            &crate::business_os::store::BusinessCommand,
        ) -> Result<()>,
    ) -> Result<NativeProviderCommand> {
        let canonical_command = canonical_guest_command(command)?;
        let provider = self.provider.clone();
        provider.with_live_provider_transaction(|tx, facts, turn| {
            ensure!(
                turn == Some(actual_turn_id),
                "command emission does not belong to the actual bound turn"
            );
            let mut emitted = self
                .provider
                .record
                .emitted_commands
                .lock()
                .map_err(|_| anyhow::anyhow!("native command emission state poisoned"))?;
            ensure!(
                emitted.len() < 1024,
                "native guest emission budget exhausted"
            );
            ensure!(
                emitted.insert(command.id.as_ref().expect("validated command ID").clone()),
                "native guest command emission was already attempted"
            );
            admit(tx, facts, actual_turn_id, command)?;
            Ok(())
        })?;
        Ok(NativeProviderCommand {
            provider,
            turn_id: actual_turn_id.to_owned(),
            canonical_command,
            consumed: Arc::new(Mutex::new(false)),
        })
    }
}

impl NativeProviderCommand {
    /// Observation of the actual retained provider root, never a wire claim.
    pub(crate) fn runtime_root(&self) -> &Path {
        self.provider.runtime_root()
    }

    /// Consume at the actual effect boundary. Current policy/controller checks
    /// and the bounded effect belong in this callback, under the same guard.
    /// Returning identity and applying an effect later is not an admission.
    /// The complete admitted envelope is compared, including client_context;
    /// those labels cannot mint or replace this private native witness.
    #[allow(dead_code)]
    pub(crate) fn with_current_command_transaction<T>(
        &self,
        command: &crate::business_os::store::BusinessCommand,
        effect: impl FnOnce(&rusqlite::Transaction<'_>, &NativeProviderFacts, &str) -> Result<T>,
    ) -> Result<T> {
        ensure!(
            canonical_guest_command(command)? == self.canonical_command,
            "native guest command does not match its admitted envelope"
        );
        self.provider
            .with_live_provider_transaction(|tx, facts, turn| {
                ensure!(
                    turn == Some(self.turn_id.as_str()),
                    "native guest command turn changed"
                );
                let mut consumed = self
                    .consumed
                    .lock()
                    .map_err(|_| anyhow::anyhow!("native command consumption state poisoned"))?;
                ensure!(
                    !*consumed,
                    "native guest command witness was already consumed"
                );
                *consumed = true;
                effect(tx, facts, &self.turn_id)
            })
    }
}

pub(crate) fn lookup_native_provider_binding(
    root: &Path,
    attempt_id: &str,
    provider_session_id: &str,
) -> Result<NativeProviderBinding> {
    let record = registry()
        .lock()
        .map_err(|_| anyhow::anyhow!("native provider registry poisoned"))?
        .get(&(root.to_owned(), attempt_id.to_owned()))
        .and_then(Weak::upgrade)
        .ok_or_else(|| anyhow::anyhow!("no retained native provider preparation"))?;
    ensure!(
        record.facts.provider_session_id == provider_session_id,
        "native provider session does not match its admitted attempt"
    );
    let binding = NativeProviderBinding { record };
    binding.with_live_provider(|_, _| Ok(()))?;
    Ok(binding)
}

impl Drop for NativeProviderTurnOwner {
    fn drop(&mut self) {
        if let Ok(mut state) = self.record.state.lock() {
            state.live = false;
        }
        // Marking closed is best-effort evidence; a row is never a stop witness
        // or permission without the retained live native object.
        // The path may now name a replacement store. Closing evidence must
        // obey the same retained store/worker binding as other native writes.
        // Cancellation/expiry may deny this optional marker; live=false above
        // remains the authority revocation even when no marker can be written.
        let _ = self.record.execution.with_current_transaction(|tx| {
            tx.execute(
                "UPDATE native_worker_provider_bindings SET finished_at_ms=?2
                WHERE binding_id=?1 AND facts_json=?3",
                rusqlite::params![
                    self.record.facts.binding_id,
                    chrono::Utc::now().timestamp_millis(),
                    self.record.facts_json
                ],
            )?;
            Ok(())
        });
        if let Ok(mut entries) = registry().lock() {
            let key = (
                self.record.execution.root().to_owned(),
                self.record.facts.attempt_id.clone(),
            );
            if entries
                .get(&key)
                .is_some_and(|entry| entry.ptr_eq(&Arc::downgrade(&self.record)))
            {
                entries.remove(&key);
            }
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    // ctox-allow-direct-state-write: isolated native witness/lease fixtures
    use super::*;
    use crate::channels::{
        create_queue_task, lease_queue_task, record_queue_lease_worker, QueueTaskCreateRequest,
        QueueTurnLeaseFence, QueueWorkerLifetime,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub(crate) fn admitted() -> Result<(
        tempfile::TempDir,
        QueueExecutionFence,
        Arc<QueueWorkerLifetime>,
    )> {
        let root = tempfile::tempdir()?;
        let task = create_queue_task(
            root.path(),
            QueueTaskCreateRequest {
                title: "actual provider fixture".into(),
                prompt: "provider fixture".into(),
                thread_key: "queue/provider-binding".into(),
                workspace_root: None,
                priority: "normal".into(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )?;
        lease_queue_task(root.path(), &task.message_key, "ctox-service")?;
        record_queue_lease_worker(
            root.path(),
            &[task.message_key.clone()],
            "ctox-service",
            "provider-worker",
        )?;
        let fence = QueueTurnLeaseFence {
            root: root.path().to_owned(),
            message_keys: vec![task.message_key],
            worker_id: "provider-worker".into(),
            execution: None,
        };
        let lifetime = Arc::new(QueueWorkerLifetime::for_native_worker(
            root.path(),
            &fence.message_keys,
            Some(&fence.worker_id),
        ));
        let execution = QueueExecutionFence::capture(&fence, "provider-attempt", lifetime.clone())?;
        Ok((root, execution, lifetime))
    }

    fn prepare(execution: &QueueExecutionFence) -> Result<NativeProviderTurnOwner> {
        NativeProviderTurnOwner::prepare(
            execution,
            "actual-thread",
            "actual-model",
            Some("actual-model-route"),
            Some("actual-api-route"),
            None,
        )
    }

    #[test]
    fn native_provider_preparation_precedes_actual_turn_and_selects_verified_provenance(
    ) -> Result<()> {
        let (root, execution, _) = admitted()?;
        let context = serde_json::json!({
            "actor": "native-actor", "workspace": "native-workspace",
            "command_id": "native-command", "payload_hash": "native-payload-hash",
            "crew_binding": {"attempt_id": "provider-attempt"}, "secret_token": "never-persist",
            "allowed_actions": ["not-a-future-grant"],
        });
        let mut foreign_context = context.clone();
        foreign_context["crew_binding"]["attempt_id"] = Value::String("foreign-attempt".into());
        assert!(NativeProviderTurnOwner::prepare(
            &execution,
            "actual-thread",
            "actual-model",
            None,
            None,
            Some(&foreign_context),
        )
        .is_err());
        let owner = NativeProviderTurnOwner::prepare(
            &execution,
            "actual-thread",
            "actual-model",
            None,
            None,
            Some(&context),
        )?;
        let binding =
            lookup_native_provider_binding(root.path(), "provider-attempt", "actual-thread")?;
        binding.with_live_provider(|facts, turn| {
            assert_eq!(facts.worker_id, "provider-worker");
            assert_eq!(facts.attempt_id, "provider-attempt");
            assert_eq!(facts.provider_session_id, "actual-thread");
            assert!(turn.is_none());
            let provenance = facts.command_provenance.as_ref().unwrap();
            assert_eq!(provenance["actor"], "native-actor");
            assert!(provenance.get("secret_token").is_none());
            assert!(provenance.get("allowed_actions").is_none());
            Ok(())
        })?;
        assert!(owner.bind_turn("foreign-thread", "actual-turn").is_err());
        owner.bind_turn("actual-thread", "actual-turn")?;
        binding.with_live_provider(|_, turn| {
            assert_eq!(turn, Some("actual-turn"));
            Ok(())
        })?;
        assert!(owner.bind_turn("actual-thread", "successor-turn").is_err());
        Ok(())
    }

    #[test]
    fn native_provider_owner_drop_and_replayed_row_cannot_revive_consumer() -> Result<()> {
        let (root, execution, _) = admitted()?;
        let owner = prepare(&execution)?;
        let binding = owner.binding();
        drop(owner);
        let conn = Connection::open(resolve_db_path(root.path(), None))?;
        conn.execute(
            "UPDATE native_worker_provider_bindings SET finished_at_ms=NULL",
            [],
        )?;
        assert!(binding
            .with_live_provider::<()>(|_, _| panic!("dead owner callback"))
            .is_err());
        assert!(
            lookup_native_provider_binding(root.path(), "provider-attempt", "actual-thread")
                .is_err()
        );
        // A newly retained native owner is independent of a stale consumer.
        let next = prepare(&execution)?;
        assert!(binding.with_live_provider(|_, _| Ok(())).is_err());
        next.binding().with_live_provider(|_, turn| {
            assert!(turn.is_none());
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn native_provider_drop_does_not_write_a_replaced_store() -> Result<()> {
        let (root, execution, _) = admitted()?;
        let owner = prepare(&execution)?;
        let binding = owner.binding();
        let binding_id = binding.record.facts.binding_id.clone();
        let path = resolve_db_path(root.path(), None);
        let replacement = root.path().join("replacement.sqlite3");
        let conn = Connection::open(&path)?;
        // A real consistent snapshot preserves the witness while changing
        // the store inode; no connection remains open across replacement.
        conn.execute("VACUUM INTO ?1", [replacement.to_string_lossy().as_ref()])?;
        drop(conn);
        std::fs::rename(&replacement, &path)?;
        assert!(binding
            .with_live_provider::<()>(|_, _| panic!("replacement store callback"))
            .is_err());
        drop(owner);
        let conn = Connection::open(&path)?;
        let finished: Option<i64> = conn.query_row(
            "SELECT finished_at_ms FROM native_worker_provider_bindings WHERE binding_id=?1",
            [&binding_id],
            |row| row.get(0),
        )?;
        assert!(finished.is_none(), "Drop wrote into the replacement store");
        assert!(binding
            .with_live_provider::<()>(|_, _| panic!("dead owner after replacement"))
            .is_err());
        Ok(())
    }

    #[test]
    fn native_provider_rejects_duplicate_session_tampering_and_revoked_worker() -> Result<()> {
        let (root, execution, lifetime) = admitted()?;
        let owner = prepare(&execution)?;
        assert!(prepare(&execution).is_err());
        assert!(
            lookup_native_provider_binding(root.path(), "provider-attempt", "foreign-thread")
                .is_err()
        );
        let conn = Connection::open(resolve_db_path(root.path(), None))?;
        conn.execute(
            "UPDATE native_worker_provider_bindings SET provider_turn_id='forged-turn'",
            [],
        )?;
        assert!(owner
            .binding()
            .with_live_provider::<()>(|_, _| panic!("tampered witness"))
            .is_err());
        conn.execute(
            "UPDATE native_worker_provider_bindings SET provider_turn_id=NULL",
            [],
        )?;
        lifetime.revoke();
        assert!(owner
            .binding()
            .with_live_provider::<()>(|_, _| panic!("revoked worker"))
            .is_err());
        assert!(owner.bind_turn("actual-thread", "actual-turn").is_err());
        Ok(())
    }

    fn prepare_guest(
        execution: &QueueExecutionFence,
        root: &Path,
    ) -> Result<(NativeProviderTurnOwner, Arc<ctox_core::AuthManager>)> {
        let auth = ctox_core::AuthManager::from_account_bound_runtime_auth(
            ctox_core::CodexAuth::create_dummy_chatgpt_auth_for_testing(),
            root.to_owned(),
        )?;
        let checkpoint = super::super::NativeProviderCheckpointBinding::from_pinned_auth(
            auth.clone(),
            "actual-model-route",
        )?;
        let owner = NativeProviderTurnOwner::prepare_with_checkpoint(
            execution,
            "actual-thread",
            "actual-model",
            Some("actual-model-route"),
            Some("actual-api-route"),
            None,
            Some(&checkpoint),
        )?;
        Ok((owner, auth))
    }

    fn guest_command(id: &str) -> crate::business_os::store::BusinessCommand {
        crate::business_os::store::BusinessCommand {
            id: Some(id.into()),
            module: "guest".into(),
            command_type: "ctox.guest.observe".into(),
            record_id: Some("guest-1".into()),
            payload: serde_json::json!({"guest_id": "guest-1", "project_id": "project-1"}),
            client_context: serde_json::json!({"actor": {"id": "native-actor"}}),
            origin: crate::business_os::store::CommandOrigin::TrustedLocal,
        }
    }

    #[test]
    fn native_guest_command_binds_exact_envelope_and_consumes_all_clones() -> Result<()> {
        let (_root, execution, _) = admitted()?;
        let owner = prepare(&execution)?;
        owner.bind_turn("actual-thread", "actual-turn")?;
        let command = guest_command("new-command");
        let witness = owner.admit_emitted_guest_command(
            "actual-turn",
            &command,
            |_, facts, turn, actual| {
                assert_eq!(facts.worker_id, "provider-worker");
                assert_eq!(turn, "actual-turn");
                assert_eq!(actual.id.as_deref(), Some("new-command"));
                Ok(())
            },
        )?;
        for field in ["id", "module", "type", "record", "payload", "context"] {
            let mut foreign = command.clone();
            match field {
                "id" => foreign.id = Some("another-command".into()),
                "module" => foreign.module = "another-module".into(),
                "type" => foreign.command_type = "ctox.guest.input".into(),
                "record" => foreign.record_id = Some("another-guest".into()),
                "payload" => foreign.payload["guest_id"] = serde_json::json!("another-guest"),
                "context" => {
                    foreign.client_context["actor"]["id"] = serde_json::json!("another-actor")
                }
                _ => unreachable!(),
            }
            assert!(witness
                .with_current_command_transaction::<()>(&foreign, |_, _, _| panic!(
                    "changed command reached effect"
                ))
                .is_err());
        }
        let mut equivalent = command.clone();
        equivalent.payload =
            serde_json::from_str(r#"{"project_id":"project-1","guest_id":"guest-1"}"#)?;
        let clone = witness.clone();
        assert_eq!(
            witness.with_current_command_transaction(&equivalent, |_, facts, turn| {
                assert_eq!(facts.provider_session_id, "actual-thread");
                assert_eq!(turn, "actual-turn");
                Ok(17)
            })?,
            17
        );
        assert!(clone
            .with_current_command_transaction::<()>(&command, |_, _, _| panic!("replayed effect"))
            .is_err());
        assert!(owner
            .admit_emitted_guest_command("actual-turn", &command, |_, _, _, _| panic!(
                "replayed admission"
            ))
            .is_err());
        Ok(())
    }

    #[test]
    fn native_guest_command_requires_bound_turn_and_trusted_identity() -> Result<()> {
        let (_root, execution, _) = admitted()?;
        let owner = prepare(&execution)?;
        let command = guest_command("new-command");
        assert!(owner
            .admit_emitted_guest_command("actual-turn", &command, |_, _, _, _| panic!(
                "pre-turn command admitted"
            ))
            .is_err());
        owner.bind_turn("actual-thread", "actual-turn")?;
        assert!(owner
            .admit_emitted_guest_command("foreign-turn", &command, |_, _, _, _| panic!(
                "foreign turn admitted"
            ))
            .is_err());
        for invalid in ["peer", "missing-id", "other-type", "oversized"] {
            let mut rejected = command.clone();
            match invalid {
                "peer" => {
                    rejected.origin = crate::business_os::store::CommandOrigin::ReplicatedPeer
                }
                "missing-id" => rejected.id = None,
                "other-type" => rejected.command_type = "ctox.business_os.app.modify".into(),
                "oversized" => rejected.payload = serde_json::json!("x".repeat(33 * 1024)),
                _ => unreachable!(),
            }
            assert!(owner
                .admit_emitted_guest_command("actual-turn", &rejected, |_, _, _, _| panic!(
                    "invalid command admitted"
                ))
                .is_err());
        }
        owner.admit_emitted_guest_command("actual-turn", &command, |_, _, _, _| Ok(()))?;
        Ok(())
    }

    #[test]
    fn native_guest_command_failed_admission_rolls_back_and_cannot_remint() -> Result<()> {
        let (root, execution, _) = admitted()?;
        let owner = prepare(&execution)?;
        owner.bind_turn("actual-thread", "actual-turn")?;
        let conn = Connection::open(resolve_db_path(root.path(), None))?;
        conn.execute("CREATE TABLE test_command_admission (id TEXT)", [])?;
        let command = guest_command("uncertain-command");
        assert!(owner
            .admit_emitted_guest_command("actual-turn", &command, |tx, _, _, actual| {
                tx.execute(
                    "INSERT INTO test_command_admission VALUES (?1)",
                    [actual.id.as_deref()],
                )?;
                anyhow::bail!("native admission refused")
            })
            .is_err());
        let count: i64 =
            conn.query_row("SELECT count(*) FROM test_command_admission", [], |r| {
                r.get(0)
            })?;
        assert_eq!(count, 0);
        assert!(owner
            .admit_emitted_guest_command("actual-turn", &command, |_, _, _, _| panic!(
                "uncertain admission repeated"
            ))
            .is_err());
        Ok(())
    }

    #[test]
    fn native_guest_command_revoked_owner_and_lease_deny_effect() -> Result<()> {
        for mutation in [
            "owner-drop",
            "worker-revoke",
            "route_status='cancelled'",
            "attempt=attempt+1",
            "lease_expires_at='2000-01-01T00:00:00Z'",
            "provider-turn",
        ] {
            let (root, execution, lifetime) = admitted()?;
            let owner = prepare(&execution)?;
            owner.bind_turn("actual-thread", "actual-turn")?;
            let command = guest_command("new-command");
            let witness =
                owner.admit_emitted_guest_command("actual-turn", &command, |_, _, _, _| Ok(()))?;
            let mut owner = Some(owner);
            if mutation == "owner-drop" {
                drop(owner.take());
            } else if mutation == "worker-revoke" {
                lifetime.revoke();
            } else {
                let conn = Connection::open(resolve_db_path(root.path(), None))?;
                if mutation == "provider-turn" {
                    conn.execute(
                        "UPDATE native_worker_provider_bindings SET provider_turn_id='foreign'",
                        [],
                    )?;
                } else {
                    conn.execute(
                        &format!("UPDATE communication_routing_state SET {mutation}"),
                        [],
                    )?;
                }
            }
            assert!(witness
                .with_current_command_transaction::<()>(&command, |_, _, _| panic!(
                    "revoked command reached effect"
                ))
                .is_err());
        }
        Ok(())
    }

    #[test]
    fn native_guest_command_failed_effect_is_consumed_and_rolls_back() -> Result<()> {
        let (root, execution, _) = admitted()?;
        let owner = prepare(&execution)?;
        owner.bind_turn("actual-thread", "actual-turn")?;
        let command = guest_command("new-command");
        let witness =
            owner.admit_emitted_guest_command("actual-turn", &command, |_, _, _, _| Ok(()))?;
        let clone = witness.clone();
        let conn = Connection::open(resolve_db_path(root.path(), None))?;
        conn.execute("CREATE TABLE test_command_effect (id TEXT)", [])?;
        assert!(witness
            .with_current_command_transaction::<()>(&command, |tx, _, _| {
                tx.execute("INSERT INTO test_command_effect VALUES ('new-command')", [])?;
                anyhow::bail!("native effect requires reconciliation")
            })
            .is_err());
        let count: i64 =
            conn.query_row("SELECT count(*) FROM test_command_effect", [], |r| r.get(0))?;
        assert_eq!(count, 0);
        assert!(clone
            .with_current_command_transaction::<()>(&command, |_, _, _| panic!(
                "uncertain effect repeated"
            ))
            .is_err());
        Ok(())
    }

    struct Admission {
        calls: Arc<AtomicUsize>,
        revoke: Option<Arc<QueueWorkerLifetime>>,
        deny: bool,
    }
    impl NativeProviderAdmission for Admission {
        fn admit<'a>(
            &'a self,
            binding: NativeProviderBinding,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
            Box::pin(async move {
                binding.with_live_provider(|_, turn| {
                    assert!(turn.is_none(), "model must not start before admission");
                    Ok(())
                })?;
                self.calls.fetch_add(1, Ordering::SeqCst);
                tokio::task::yield_now().await;
                if let Some(lifetime) = &self.revoke {
                    lifetime.revoke();
                }
                ensure!(!self.deny, "native guest owner denied");
                Ok(())
            })
        }
    }

    #[test]
    fn native_guest_emitter_does_not_prolong_owner_or_supply_a_default_consumer() -> Result<()> {
        let (_root, execution, _) = admitted()?;
        let owner = prepare(&execution)?;
        owner.bind_turn("actual-thread", "actual-turn")?;
        let emitter = owner.command_emitter();
        let command = guest_command("new-command");
        let witness =
            emitter.admit_emitted_guest_command("actual-turn", &command, |_, _, _, _| Ok(()))?;
        let consumer = Admission {
            calls: Arc::new(AtomicUsize::new(0)),
            revoke: None,
            deny: false,
        };
        assert!(
            consumer.execute_guest_command(&command, witness).is_err(),
            "an admission hook alone cannot perform guest effects"
        );
        drop(owner);
        assert!(emitter
            .admit_emitted_guest_command(
                "actual-turn",
                &guest_command("after-owner"),
                |_, _, _, _| panic!("dead owner emitted"),
            )
            .is_err());
        Ok(())
    }

    #[tokio::test]
    async fn native_provider_admission_revalidates_after_await_and_denies_before_start(
    ) -> Result<()> {
        for (revoke, deny) in [(false, false), (true, false), (false, true)] {
            let (root, execution, lifetime) = admitted()?;
            let (owner, _) = prepare_guest(&execution, root.path())?;
            let calls = Arc::new(AtomicUsize::new(0));
            let admission = Admission {
                calls: calls.clone(),
                revoke: revoke.then_some(lifetime),
                deny,
            };
            let result = owner.binding().admit_before_start(&admission).await;
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert_eq!(result.is_ok(), !revoke && !deny);
            if result.is_ok() {
                owner.bind_turn("actual-thread", "actual-turn")?;
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn native_guest_admission_requires_real_account_source_before_owner_callback(
    ) -> Result<()> {
        let (_root, execution, _) = admitted()?;
        let owner = prepare(&execution)?;
        let calls = Arc::new(AtomicUsize::new(0));
        let admission = Admission {
            calls: calls.clone(),
            revoke: None,
            deny: false,
        };
        assert!(owner
            .binding()
            .admit_before_start(&admission)
            .await
            .is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[test]
    fn native_account_removal_fences_provider_publication_and_turn_binding() -> Result<()> {
        let (root, execution, _) = admitted()?;
        let (owner, auth) = prepare_guest(&execution, root.path())?;
        owner.binding().with_live_provider(|facts, _| {
            assert_eq!(
                facts
                    .checkpoint_contract
                    .as_ref()
                    .unwrap()
                    .gateway_account_id,
                "account_id"
            );
            Ok(())
        })?;
        // Real file-backed logout/reload removes the account while the native
        // worker and its committed provider witness remain alive.
        auth.reload();
        assert!(owner
            .binding()
            .with_live_provider(|_, _| panic!("removed account published"))
            .is_err());
        assert!(owner.bind_turn("actual-thread", "actual-turn").is_err());
        Ok(())
    }

    #[test]
    fn native_provider_transaction_keeps_policy_and_publication_under_one_guard() -> Result<()> {
        let (root, execution, _) = admitted()?;
        let owner = prepare(&execution)?;
        let binding = owner.binding();
        let path = resolve_db_path(root.path(), None);
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "CREATE TABLE test_native_policy (allowed INTEGER NOT NULL);
             INSERT INTO test_native_policy VALUES (1);
             CREATE TABLE test_native_publication (binding_id TEXT NOT NULL);",
        )?;
        binding.with_live_provider_transaction(|tx, facts, turn| {
            assert!(turn.is_none());
            let allowed: i64 =
                tx.query_row("SELECT allowed FROM test_native_policy", [], |row| {
                    row.get(0)
                })?;
            assert_eq!(allowed, 1);
            // A competing native policy change cannot interleave after the
            // policy read and before publication in this same transaction.
            let competing = Connection::open(&path)?;
            competing.busy_timeout(std::time::Duration::ZERO)?;
            let error = competing
                .execute("UPDATE test_native_policy SET allowed=0", [])
                .unwrap_err();
            assert_eq!(
                error.sqlite_error_code(),
                Some(rusqlite::ErrorCode::DatabaseBusy)
            );
            tx.execute(
                "INSERT INTO test_native_publication VALUES (?1)",
                [&facts.binding_id],
            )?;
            Ok(())
        })?;
        let count: i64 =
            conn.query_row("SELECT count(*) FROM test_native_publication", [], |row| {
                row.get(0)
            })?;
        assert_eq!(count, 1);

        assert!(binding
            .with_live_provider_transaction::<()>(|tx, facts, _| {
                tx.execute(
                    "INSERT INTO test_native_publication VALUES (?1)",
                    [&facts.binding_id],
                )?;
                anyhow::bail!("native controller refused publication")
            })
            .is_err());
        let count: i64 =
            conn.query_row("SELECT count(*) FROM test_native_publication", [], |row| {
                row.get(0)
            })?;
        assert_eq!(
            count, 1,
            "failed callback must roll back native publication"
        );
        drop(owner);
        let mut invoked = false;
        assert!(binding
            .with_live_provider_transaction(|_, _, _| {
                invoked = true;
                Ok(())
            })
            .is_err());
        assert!(!invoked);
        Ok(())
    }

    #[test]
    fn native_provider_cancellation_and_attempt_replacement_deny_publication() -> Result<()> {
        for mutation in [
            "route_status='cancelled'",
            "attempt=attempt+1",
            "lease_expires_at='2000-01-01T00:00:00Z'",
        ] {
            let (root, execution, _) = admitted()?;
            let owner = prepare(&execution)?;
            let conn = Connection::open(resolve_db_path(root.path(), None))?;
            conn.execute(
                &format!("UPDATE communication_routing_state SET {mutation}"),
                [],
            )?;
            assert!(owner
                .binding()
                .with_live_provider::<()>(|_, _| panic!("replaced lease"))
                .is_err());
            assert!(owner.bind_turn("actual-thread", "actual-turn").is_err());
        }
        Ok(())
    }
}
