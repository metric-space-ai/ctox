// Origin: CTOX
// License: AGPL-3.0-only

//! One original native service lease offered to its selected, enrolled Source.
//! Offers survive separation between the service and peer receiver. The signed
//! restricted session lives only in the encrypted native secret store.
use super::*;
use crate::business_os::{
    consumer_authority::{AdmittedConsumerAuthority, NativeConsumerCorePublication},
    workjet_supervisor_source_contract as wire,
};
use rxdb::plugins::replication_webrtc::{
    connection_handler_rs::{WebRTCRsConnection, WebRTCRsConnectionHandler},
    index_mod::{GuardedAuxiliaryResponse, RxWebRTCReplicationPool},
    WebRTCPublicationGuard,
};
use rxdb::rx_error::{new_rx_error, RxResult};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use wire::WireValidate;

pub(crate) const METHOD: &str = "ctox.workjet.project.supervisor.execution.v1";
const SECRET_SCOPE: &str = "supervisor_execution";
const OFFER_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_supervisor_source_offers (
 offer_id TEXT PRIMARY KEY, execution_key TEXT NOT NULL, lease_hash TEXT NOT NULL,
 owner_user_id TEXT NOT NULL, computer_id TEXT NOT NULL, project_id TEXT NOT NULL,
 supervisor_thread_id TEXT NOT NULL, prompt TEXT NOT NULL, deadline_ms INTEGER NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('offered','claimed','closed')),
 controller_id TEXT, UNIQUE(execution_key,lease_hash));";
const WAIT_LIMIT: Duration = Duration::from_secs(900);
const MAX_PROMPT: usize = 64 * 1024;

pub(crate) struct NativeSupervisorSourceOffer {
    lease: Arc<NativeSupervisorExecutionLease>,
    id: String,
    deadline_ms: i64,
}
fn core(root: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open_with_flags(
        crate::paths::core_db(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    Ok(conn)
}
impl NativeSupervisorSourceOffer {
    /// Called by the original admitted native service only. It retains that
    /// task, lease, heartbeat, review and capacity; no new queue/run is created.
    pub(crate) fn open(
        lease: NativeSupervisorExecutionLease,
        prompt: &str,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            lease.harness() == "claude-code",
            unavailable(
                "project_supervisor_holding_executor_unavailable",
                "selected harness has no native Source adapter"
            )
        );
        anyhow::ensure!(
            !prompt.is_empty() && prompt.len() <= MAX_PROMPT,
            "selected Supervisor prompt exceeds native offer budget"
        );
        lease.verify_session()?;
        let id = uuid::Uuid::new_v4().to_string();
        let deadline_ms = lease.trusted["expires_at_ms"]
            .as_i64()
            .context("native session expiry missing")?
            .min(now_ms().saturating_add(WAIT_LIMIT.as_millis() as i64));
        anyhow::ensure!(deadline_ms > now_ms(), "native Source offer expired");
        crate::secrets::write_secret_record(
            &lease.root,
            SECRET_SCOPE,
            &id,
            &lease.token,
            None,
            json!({"internal":true,"expires_at_ms":deadline_ms}),
        )?;
        let stored = (|| -> anyhow::Result<()> {
            let mut core = core(&lease.root)?;
            let mut policy = store::open_store(&lease.root)?;
            let core = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let policy = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
            lease.current_native(&core, &policy)?;
            core.execute_batch(OFFER_SCHEMA)?;
            let count:i64 = core.query_row("SELECT count(*) FROM workjet_supervisor_source_offers WHERE state IN ('offered','claimed') AND deadline_ms>?1", [now_ms()], |r|r.get(0))?;
            anyhow::ensure!(count < 32, "native Source offer capacity reached");
            core.execute("INSERT INTO workjet_supervisor_source_offers
              (offer_id,execution_key,lease_hash,owner_user_id,computer_id,project_id,supervisor_thread_id,prompt,deadline_ms,state)
              VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'offered')",
              params![id,lease.execution_key,lease.lease_hash,lease.trusted["actor"].as_str(),
                lease.requested.computer_id,lease.requested.project_id,lease.requested.supervisor_thread_id,prompt,deadline_ms])?;
            policy.commit()?;
            core.commit()?;
            Ok(())
        })();
        if let Err(error) = stored {
            let _ = crate::secrets::delete_secret_record(&lease.root, SECRET_SCOPE, &id);
            return Err(error);
        }
        Ok(Self {
            lease: Arc::new(lease),
            id,
            deadline_ms,
        })
    }
    /// Bounded service wait; no default-model invocation or replacement task.
    /// The accepted native SDK/model result path is integrated separately:
    /// this handshake never accepts a caller-reported "actual" or reply.
    pub(crate) fn wait_for_native_result(&self) -> anyhow::Result<String> {
        let until = Instant::now() + WAIT_LIMIT;
        loop {
            self.lease.verify_session()?;
            let mut core = core(&self.lease.root)?;
            let mut policy = store::open_store(&self.lease.root)?;
            let core = core.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let policy = policy.transaction_with_behavior(TransactionBehavior::Deferred)?;
            self.lease.current_native(&core, &policy)?;
            let state:String = core.query_row("SELECT state FROM workjet_supervisor_source_offers WHERE offer_id=?1 AND execution_key=?2 AND lease_hash=?3",
                params![self.id,self.lease.execution_key,self.lease.lease_hash],|r|r.get(0))?;
            anyhow::ensure!(
                state != "closed",
                unavailable("supervisor_execution_fenced", "native Source offer retired")
            );
            policy.commit()?;
            core.commit()?;
            if now_ms() >= self.deadline_ms || Instant::now() >= until {
                return Err(unavailable("project_supervisor_source_wait_timeout", "selected Source produced no accepted native SDK/model result before the original offer deadline"));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
    fn close(&self) -> anyhow::Result<()> {
        let mut core = core(&self.lease.root)?;
        let core = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
        core.execute("UPDATE workjet_supervisor_source_offers SET state='closed' WHERE offer_id=?1 AND execution_key=?2 AND lease_hash=?3",
            params![self.id,self.lease.execution_key,self.lease.lease_hash])?;
        let controllers_exist: bool = core.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_supervisor_execution_controllers')",
            [], |r| r.get(0),
        )?;
        if controllers_exist {
            core.execute(
                "UPDATE workjet_supervisor_execution_controllers SET state='cancelled',retired_at_ms=?1
                 WHERE execution_key=?2 AND lease_hash=?3 AND state='active'",
                params![now_ms(), self.lease.execution_key, self.lease.lease_hash],
            )?;
        }
        core.commit()?;
        crate::secrets::delete_secret_record(&self.lease.root, SECRET_SCOPE, &self.id)?;
        Ok(())
    }
}
impl Drop for NativeSupervisorSourceOffer {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

pub(crate) struct NativeSupervisorSourceHost {
    root: PathBuf,
    // Real non-deserializable controllers only; never reconstruct from rows.
    controllers: Mutex<HashMap<String, Arc<NativeSupervisorHoldingController>>>,
    models: model::ModelRegistry,
}
impl NativeSupervisorSourceHost {
    pub(crate) fn new(root: &Path) -> Arc<Self> {
        Arc::new(Self {
            root: root.to_owned(),
            controllers: Mutex::new(HashMap::new()),
            models: model::ModelRegistry::default(),
        })
    }
    pub(crate) fn register(
        pool: &Arc<RxWebRTCReplicationPool<WebRTCRsConnectionHandler>>,
        root: &Path,
    ) -> rxdb::rx_error::RxResult<()> {
        let host = Self::new(root);
        let transport = Arc::clone(&pool.connection_handler);
        pool.register_guarded_auxiliary_request_handler(
            METHOD,
            Arc::new(move |peer, token, params| {
                let host = Arc::clone(&host);
                let transport = Arc::clone(&transport);
                Box::pin(async move {
                    host.respond_async(transport, peer, token, params)
                        .await
                        .map_err(|_| "native Supervisor Source rejected".to_owned())
                })
            }),
        )
    }
    async fn respond_async(
        self: Arc<Self>,
        transport: Arc<WebRTCRsConnectionHandler>,
        peer: WebRTCRsConnection,
        token: String,
        params: Vec<Value>,
    ) -> anyhow::Result<GuardedAuxiliaryResponse> {
        let operation = parse_operation(params.clone())?;
        if matches!(
            operation.action,
            wire::SourceAction::ModelInvoke | wire::SourceAction::ModelRead
        ) {
            let root = self.root.clone();
            let authority = tokio::task::spawn_blocking(move || {
                AdmittedConsumerAuthority::capture(&root, transport, peer, &token)
            })
            .await
            .context("native Source admission context unavailable")??;
            self.model_respond(authority, operation).await
        } else {
            tokio::task::spawn_blocking(move || self.respond(transport, peer, &token, params))
                .await
                .context("native Source execution context unavailable")?
        }
    }
    fn respond(
        &self,
        transport: Arc<WebRTCRsConnectionHandler>,
        peer: WebRTCRsConnection,
        token: &str,
        params: Vec<Value>,
    ) -> anyhow::Result<GuardedAuxiliaryResponse> {
        let operation = parse_operation(params)?;
        let authority = AdmittedConsumerAuthority::capture(&self.root, transport, peer, token)?;
        self.prune()?;
        match operation.action {
            wire::SourceAction::Poll => self.poll(authority),
            wire::SourceAction::Claim => {
                self.claim(authority, operation.offer_id.as_deref().unwrap())
            }
            wire::SourceAction::ToolCall => tools::respond(self, authority, &operation),
            wire::SourceAction::SdkObserve => sdk::respond(self, authority, &operation),
            wire::SourceAction::Status | wire::SourceAction::Cancel => {
                self.control(authority, &operation)
            }
            wire::SourceAction::ModelInvoke | wire::SourceAction::ModelRead => {
                anyhow::bail!("native model operation needs its managed async responder")
            }
        }
    }
    fn prune(&self) -> anyhow::Result<()> {
        let ids = self
            .controllers
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native Source control busy"))?
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(());
        }
        let core = core(&self.root)?;
        for id in ids {
            let active: bool = core.query_row(
                "SELECT EXISTS(SELECT 1 FROM workjet_supervisor_source_offers WHERE offer_id=?1 AND state IN ('offered','claimed') AND deadline_ms>?2)",
                params![id, now_ms()], |r| r.get(0),
            )?;
            if !active {
                let controller = self
                    .controllers
                    .try_lock()
                    .map_err(|_| anyhow::anyhow!("native Source control busy"))?
                    .remove(&id);
                if let Some(controller) = controller {
                    self.models.retire(controller.controller_id());
                    controller.cancel()?;
                }
            }
        }
        Ok(())
    }
    fn poll(
        &self,
        authority: AdmittedConsumerAuthority,
    ) -> anyhow::Result<GuardedAuxiliaryResponse> {
        let ids = authority.with_current_core(|facts,core,_| {
            let exists:bool = core.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_supervisor_source_offers')",[],|r|r.get(0))?;
            if !exists {return Ok(Vec::<String>::new())}
            let mut stmt=core.prepare("SELECT offer_id FROM workjet_supervisor_source_offers
                WHERE owner_user_id=?1 AND computer_id=?2 AND state='offered' AND deadline_ms>?3 ORDER BY deadline_ms,offer_id LIMIT 8")?;
            let rows = stmt.query_map(params![facts.owner_user_id,facts.computer_id,now_ms()],|r|r.get::<_,String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })?;
        for id in ids {
            // Native encrypted token read/capture OUTSIDE all publication locks.
            let token = Zeroizing::new(crate::secrets::read_secret_value(
                &self.root,
                SECRET_SCOPE,
                &id,
            )?);
            let lease = Arc::new(NativeSupervisorExecutionLease::capture(&self.root, &token)?);
            let offer = lease.with_current(&authority, |facts, core, policy| {
                let row = read_offer(core, &id, facts)?;
                check_offer_lease(&row, &lease)?;
                lease.current(core, policy, facts)?;
                anyhow::ensure!(
                    row.state == "offered" && row.deadline_ms > now_ms(),
                    "native Source offer changed"
                );
                Ok(offer_value(&id, &row, &lease)?)
            })?;
            let origin = authority.prepare_core_publication(&authority)?;
            return Ok(GuardedAuxiliaryResponse {
                result: json!({"version":1,"state":"offered","offer":offer,"execution_ready":false}),
                publication: Arc::new(OfferPublication {
                    origin,
                    lease: Some(lease),
                    id: Some(id),
                    expected_state: Some("offered"),
                }),
            });
        }
        let origin = authority.prepare_core_publication(&authority)?;
        Ok(GuardedAuxiliaryResponse {
            result: json!({"version":1,"state":"waiting","offer":null,"execution_ready":false}),
            publication: Arc::new(OfferPublication {
                origin,
                lease: None,
                id: None,
                expected_state: None,
            }),
        })
    }
    fn claim(
        &self,
        authority: AdmittedConsumerAuthority,
        id: &str,
    ) -> anyhow::Result<GuardedAuxiliaryResponse> {
        authority.with_current_core(|facts, core, _| {
            let row = read_offer(core, id, facts)?;
            anyhow::ensure!(
                row.state == "offered" || row.state == "claimed",
                "native Source offer retired"
            );
            anyhow::ensure!(row.deadline_ms > now_ms(), "native Source offer expired");
            Ok(())
        })?;
        let token = Zeroizing::new(crate::secrets::read_secret_value(
            &self.root,
            SECRET_SCOPE,
            id,
        )?);
        let lease = Arc::new(NativeSupervisorExecutionLease::capture(&self.root, &token)?);
        let mut controllers = self
            .controllers
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native Source claim busy"))?;
        if let Some(controller) = controllers.get(id) {
            let publication = controller.publication_for(&authority, Arc::new(ControllerOnly))?;
            let result = controller.with_current(|facts, core, _| {
                let row = read_offer(core, id, facts)?;
                anyhow::ensure!(
                    row.controller_id.as_deref() == Some(controller.controller_id())
                        && row.state == "claimed"
                        && row.deadline_ms > now_ms(),
                    "native Source claim retired"
                );
                Ok(claim_value(id, &row, controller))
            })?;
            return Ok(GuardedAuxiliaryResponse {
                result,
                publication,
            });
        }
        anyhow::ensure!(
            controllers.len() < 32,
            "native Source controller capacity reached"
        );
        let mut claimed_row = None;
        let controller = Arc::new(NativeSupervisorHoldingController::claim_shared_with(
            Arc::clone(&lease),
            authority,
            |controller_id, facts, core, _| {
                let row = read_offer(core, id, facts)?;
                check_offer_lease(&row, &lease)?;
                anyhow::ensure!(
                    row.state == "offered" && row.deadline_ms > now_ms(),
                    "native Source offer unavailable"
                );
                let changed = core.execute(
                    "UPDATE workjet_supervisor_source_offers SET state='claimed',controller_id=?1
                  WHERE offer_id=?2 AND state='offered' AND deadline_ms>?3",
                    params![controller_id, id, now_ms()],
                )?;
                anyhow::ensure!(changed == 1, "native Source offer already claimed");
                claimed_row = Some(OfferRow {
                    state: "claimed".to_owned(),
                    controller_id: Some(controller_id.to_owned()),
                    ..row
                });
                Ok(())
            },
        )?);
        // The actual request moved into this controller; retain that exact
        // native authority, never rebuild from computer facts or a saved label.
        let publication =
            controller.publication_for(controller.authority(), Arc::new(ControllerOnly))?;
        let result = claim_value(
            id,
            &claimed_row.context("native claim row missing")?,
            &controller,
        );
        controllers.insert(id.to_owned(), controller);
        Ok(GuardedAuxiliaryResponse {
            result,
            publication,
        })
    }
    fn control(
        &self,
        authority: AdmittedConsumerAuthority,
        operation: &wire::SourceOperation,
    ) -> anyhow::Result<GuardedAuxiliaryResponse> {
        let id = operation.offer_id.as_deref().unwrap();
        let controller = self
            .controllers
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native Source control busy"))?
            .get(id)
            .cloned()
            .context("original native Source controller unavailable")?;
        anyhow::ensure!(
            operation.controller_id.as_deref() == Some(controller.controller_id()),
            "foreign Source controller"
        );
        let publication = controller.publication_for(&authority, Arc::new(ControllerOnly))?;
        let result=controller.with_current(|facts,core,_| {
            let row=read_offer(core,id,facts)?;
            anyhow::ensure!(row.controller_id.as_deref()==Some(controller.controller_id()) && row.state=="claimed" && row.deadline_ms>now_ms(),"native Source offer retired");
            Ok(json!({"version":1,"state":"claimed","offer_id":id,"controller_id":controller.controller_id(),"execution_ready":false}))
        })?;
        if operation.action == wire::SourceAction::Cancel {
            self.models.retire(controller.controller_id());
            controller.cancel()?;
            core(&self.root)?.execute(
                "UPDATE workjet_supervisor_source_offers SET state='closed'
                WHERE offer_id=?1 AND controller_id=?2 AND state='claimed'",
                params![id, controller.controller_id()],
            )?;
            // Ack is a static enrollment-scoped control result, never a model
            // result or permit after the original controller has retired.
            let origin = authority.prepare_core_publication(&authority)?;
            return Ok(GuardedAuxiliaryResponse {
                result: json!({"version":1,"state":"closed","offer_id":id}),
                publication: Arc::new(OfferPublication {
                    origin,
                    lease: None,
                    id: Some(id.to_owned()),
                    expected_state: None,
                }),
            });
        }
        Ok(GuardedAuxiliaryResponse {
            result,
            publication,
        })
    }
}
impl Drop for NativeSupervisorSourceHost {
    fn drop(&mut self) {
        for controller in self
            .controllers
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
        {
            self.models.retire(controller.controller_id());
            let _ = controller.cancel();
        }
    }
}
struct ControllerOnly;
impl NativeSupervisorPublicationCheck for ControllerOnly {
    fn with_current(
        &self,
        _: &NativeSupervisorCurrentPublication<'_>,
        publish: &mut dyn FnMut() -> RxResult<()>,
    ) -> RxResult<()> {
        publish()
    }
}
struct OfferRow {
    execution_key: String,
    lease_hash: String,
    prompt: String,
    deadline_ms: i64,
    state: String,
    controller_id: Option<String>,
}
fn read_offer(core: &Connection, id: &str, facts: &ConsumerFacts) -> anyhow::Result<OfferRow> {
    core.query_row("SELECT execution_key,lease_hash,prompt,deadline_ms,state,controller_id FROM workjet_supervisor_source_offers
      WHERE offer_id=?1 AND owner_user_id=?2 AND computer_id=?3",
      params![id,facts.owner_user_id,facts.computer_id],|r|Ok(OfferRow{
        execution_key:r.get(0)?,lease_hash:r.get(1)?,prompt:r.get(2)?,deadline_ms:r.get(3)?,state:r.get(4)?,controller_id:r.get(5)?
      })).context("no owned native Source offer")
}
fn check_offer_lease(row: &OfferRow, lease: &NativeSupervisorExecutionLease) -> anyhow::Result<()> {
    anyhow::ensure!(
        row.execution_key == lease.execution_key && row.lease_hash == lease.lease_hash,
        "native offer is not the captured original lease"
    );
    Ok(())
}
fn offer_value(
    id: &str,
    row: &OfferRow,
    lease: &NativeSupervisorExecutionLease,
) -> anyhow::Result<Value> {
    let offer = wire::SourceOffer {
        offer_id: id.to_owned(),
        execution_key: row.execution_key.clone(),
        deadline_ms: row.deadline_ms,
        state: wire::SourceOfferState::Offered,
        route: wire::SourceRequestedRoute {
            project_id: lease.requested.project_id.clone(),
            supervisor_thread_id: lease.requested.supervisor_thread_id.clone(),
            luma_id: lease.requested.luma_id.clone(),
            configuration_revision: lease.requested.configuration_revision,
            computer_id: lease.requested.computer_id.clone(),
            harness: lease.requested.harness.clone(),
            model: lease.requested.model.clone(),
        },
    };
    offer.validate().map_err(anyhow::Error::msg)?;
    Ok(serde_json::to_value(offer)?)
}
fn claim_value(id: &str, row: &OfferRow, controller: &NativeSupervisorHoldingController) -> Value {
    json!({"version":1,"state":"claimed","offer_id":id,"execution_key":row.execution_key,
        "controller_id":controller.controller_id(),"prompt":row.prompt,"deadline_ms":row.deadline_ms,
        "native_tools":tools::descriptors(),
        "execution_ready":false})
}
fn parse_operation(params: Vec<Value>) -> anyhow::Result<wire::SourceOperation> {
    anyhow::ensure!(
        params.len() == 1 && serde_json::to_vec(&params)?.len() <= 256 * 1024,
        "invalid native Source operation"
    );
    let operation: wire::SourceOperation =
        serde_json::from_value(params.into_iter().next().unwrap())?;
    operation.validate().map_err(anyhow::Error::msg)?;
    for id in [
        &operation.offer_id,
        &operation.controller_id,
        &operation.operation_id,
    ]
    .into_iter()
    .flatten()
    {
        anyhow::ensure!(
            uuid::Uuid::parse_str(id).is_ok(),
            "invalid native Source identifier"
        );
    }
    let shape = match operation.action {
        wire::SourceAction::SdkObserve => {
            operation.offer_id.is_some()
                && operation.controller_id.is_some()
                && operation.sdk_observation.is_some()
                && operation.operation_id.is_none()
                && operation.model_operation.is_none()
                && operation.body_json.is_none()
                && operation.sdk_session_id.is_none()
                && operation.sequence.is_none()
                && operation.native_tool.is_none()
                && operation.tool_arguments_json.is_none()
        }
        wire::SourceAction::Poll => {
            operation.offer_id.is_none() && operation.controller_id.is_none()
        }
        wire::SourceAction::Claim => {
            operation.offer_id.is_some() && operation.controller_id.is_none()
        }
        wire::SourceAction::Status | wire::SourceAction::Cancel => {
            operation.offer_id.is_some() && operation.controller_id.is_some()
        }
        wire::SourceAction::ToolCall => {
            operation.offer_id.is_some()
                && operation.controller_id.is_some()
                && operation.operation_id.is_some()
                && operation.native_tool.is_some()
                && operation.tool_arguments_json.is_some()
                && operation.model_operation.is_none()
                && operation.body_json.is_none()
                && operation.sdk_session_id.is_none()
                && operation.sequence.is_none()
        }
        wire::SourceAction::ModelInvoke => {
            operation.offer_id.is_some()
                && operation.controller_id.is_some()
                && operation.operation_id.is_some()
                && operation.model_operation.is_some()
                && operation.body_json.is_some()
                && operation.sdk_session_id.is_some()
                && operation.sequence.is_none()
        }
        wire::SourceAction::ModelRead => {
            operation.offer_id.is_some()
                && operation.controller_id.is_some()
                && operation.operation_id.is_some()
                && operation.sequence.is_some()
                && operation.model_operation.is_none()
                && operation.body_json.is_none()
                && operation.sdk_session_id.is_none()
        }
    };
    let is_model = matches!(
        operation.action,
        wire::SourceAction::ModelInvoke | wire::SourceAction::ModelRead
    );
    anyhow::ensure!(
        is_model
            || operation.action == wire::SourceAction::ToolCall
            || (operation.operation_id.is_none()
                && operation.model_operation.is_none()
                && operation.body_json.is_none()
                && operation.sdk_session_id.is_none()
                && operation.sequence.is_none()),
        "model fields are not control authority"
    );
    anyhow::ensure!(
        operation.action == wire::SourceAction::ToolCall
            || (operation.native_tool.is_none() && operation.tool_arguments_json.is_none()),
        "native tool fields cannot alter model/control authority"
    );
    anyhow::ensure!(shape, "native Source action fields differ");
    anyhow::ensure!(
        operation.action == wire::SourceAction::SdkObserve || operation.sdk_observation.is_none(),
        "SDK observations cannot alter model/tool/control authority"
    );
    Ok(operation)
}
struct OfferPublication {
    origin: NativeConsumerCorePublication,
    lease: Option<Arc<NativeSupervisorExecutionLease>>,
    id: Option<String>,
    expected_state: Option<&'static str>,
}
impl WebRTCPublicationGuard for OfferPublication {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        self.origin
            .with_current(|facts, core, policy| {
                if let Some(id) = &self.id {
                    let row = read_offer(core, id, facts)?;
                    if let Some(state) = self.expected_state {
                        anyhow::ensure!(
                            row.state == state && row.deadline_ms > now_ms(),
                            "native offer publication retired"
                        );
                    }
                    if let Some(lease) = &self.lease {
                        check_offer_lease(&row, lease)?;
                        lease.current(core, policy, facts)?;
                    }
                }
                publish().map_err(|_| anyhow::anyhow!("native Source publication failed"))
            })
            .map_err(|_| new_rx_error("supervisor_execution_fenced", None))
    }
}
#[path = "mcp_supervisor_source_model.rs"]
mod model;
#[path = "mcp_supervisor_source_sdk.rs"]
mod sdk;
#[cfg(test)]
#[path = "mcp_supervisor_source_tests.rs"]
mod tests;
#[path = "mcp_supervisor_source_tools.rs"]
mod tools;
