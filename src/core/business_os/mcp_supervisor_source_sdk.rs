// Origin: CTOX
// License: AGPL-3.0-only
//! Ordered observations from the original enrolled Source's private SDK callbacks.
//! These records are not execution authority or a caller-reported model result.
use super::*;
use sha2::Digest;
use std::collections::{BTreeMap, BTreeSet};

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_supervisor_sdk_observations (
 controller_id TEXT NOT NULL, execution_key TEXT NOT NULL, lease_hash TEXT NOT NULL,
 sequence INTEGER NOT NULL, observation_json TEXT NOT NULL, recorded_at_ms INTEGER NOT NULL,
 PRIMARY KEY(controller_id,sequence));";

// Constructed only from the native Messages row while the original controller
// remains fenced. SDK strings select candidates; they cannot supply a reply.
struct NativeParentReply {
    operation_id: String,
    text: String,
}
fn join_native_parent(
    core: &Connection,
    controller_id: &str,
    execution_key: &str,
    lease_hash: &str,
    state: &State,
) -> anyhow::Result<Option<NativeParentReply>> {
    if !state.drained_success() {
        return Ok(None);
    }
    let exists: bool = core.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master
        WHERE type='table' AND name='workjet_supervisor_native_model_requests')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    let (message_id, message_model, _) = state
        .parent
        .as_ref()
        .context("original SDK parent missing")?;
    let rows = core
        .prepare(
            "SELECT operation_id,response_text FROM workjet_supervisor_native_model_requests
        WHERE controller_id=?1 AND execution_key=?2 AND lease_hash=?3 AND sdk_correlation=?4
          AND operation_kind='messages' AND state='observed' AND http_status BETWEEN 200 AND 299
          AND response_message_id=?5 AND response_model=?6 AND response_complete=1
          AND response_stop_reason IN ('end_turn','stop_sequence') AND response_text IS NOT NULL
          AND finished_at_ms IS NOT NULL LIMIT 2",
        )?
        .query_map(
            params![
                controller_id,
                execution_key,
                lease_hash,
                state.session,
                message_id,
                message_model
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    anyhow::ensure!(
        rows.len() <= 1,
        "native SDK parent message has multiple model witnesses"
    );
    let Some((operation_id, text)) = rows.into_iter().next() else {
        return Ok(None);
    };
    anyhow::ensure!(
        !text.trim().is_empty() && text.len() <= 64 * 1024,
        "native parent reply exceeds budget"
    );
    Ok(Some(NativeParentReply { operation_id, text }))
}

#[derive(Default)]
struct State {
    children: BTreeMap<u64, bool>,
    session: Option<String>,
    turn: Option<String>,
    parent: Option<(String, String, String)>,
    result: Option<(String, bool, String)>,
    stream_joined: bool,
    query_closed: bool,
}
fn text(value: &Option<String>) -> anyhow::Result<&str> {
    value
        .as_deref()
        .filter(|s| !s.trim().is_empty() && !s.chars().any(char::is_control))
        .context("invalid native SDK callback identity")
}
fn shape(observation: &wire::SourceSdkObservation) -> anyhow::Result<()> {
    observation.validate().map_err(anyhow::Error::msg)?;
    let value = serde_json::to_value(observation)?;
    anyhow::ensure!(
        serde_json::to_vec(&value)?.len() <= 4096,
        "SDK observation budget exceeded"
    );
    use wire::SourceSdkObservationKind as K;
    let fields: &[&str] = match observation.kind {
        K::ChildSpawned => &["pid"],
        K::ChildClosed => &["pid", "exit_code", "signal"],
        K::SdkInit => &["session_id", "init_id"],
        K::TurnSubmitted => &["turn_id"],
        K::ParentAssistant => &[
            "session_id",
            "turn_id",
            "message_id",
            "message_model",
            "assistant_id",
        ],
        K::SdkResult => &["session_id", "turn_id", "result_id", "subtype", "is_error"],
        K::SdkStreamJoined | K::SdkQueryCloseReturned => &[],
    };
    let allowed: BTreeSet<_> = fields
        .iter()
        .copied()
        .chain(["version", "sequence", "kind"])
        .collect();
    anyhow::ensure!(
        value
            .as_object()
            .context("SDK observation object required")?
            .keys()
            .all(|key| allowed.contains(key.as_str())),
        "SDK callback fields differ"
    );
    for key in fields {
        if matches!(*key, "exit_code" | "signal") {
            continue;
        }
        anyhow::ensure!(
            value.get(*key).is_some(),
            "SDK callback field missing: {key}"
        );
    }
    for key in [
        "session_id",
        "init_id",
        "turn_id",
        "message_id",
        "message_model",
        "assistant_id",
        "result_id",
        "subtype",
        "signal",
    ] {
        if let Some(value) = value.get(key) {
            let s = value.as_str().context("SDK identity must be text")?;
            anyhow::ensure!(
                !s.trim().is_empty() && !s.chars().any(char::is_control),
                "invalid SDK identity"
            );
        }
    }
    Ok(())
}
impl State {
    fn apply(&mut self, observation: &wire::SourceSdkObservation) -> anyhow::Result<()> {
        shape(observation)?;
        use wire::SourceSdkObservationKind as K;
        match observation.kind {
            K::ChildSpawned => {
                let pid = observation.pid.context("SDK child missing")?;
                anyhow::ensure!(
                    self.session.is_none()
                        && self.result.is_none()
                        && self.children.len() < 8
                        && !self.children.contains_key(&pid),
                    "original SDK child differs"
                );
                self.children.insert(pid, false);
            }
            K::ChildClosed => {
                let pid = observation.pid.context("SDK child missing")?;
                let closed = self
                    .children
                    .get_mut(&pid)
                    .context("SDK child was not captured")?;
                anyhow::ensure!(!*closed, "SDK child already closed");
                *closed = true;
            }
            K::SdkInit => {
                anyhow::ensure!(
                    !self.children.is_empty()
                        && self.children.values().any(|closed| !closed)
                        && self.session.is_none()
                        && self.result.is_none(),
                    "original SDK init unavailable or replaced"
                );
                self.session = Some(text(&observation.session_id)?.to_owned());
                text(&observation.init_id)?;
            }
            K::TurnSubmitted => {
                anyhow::ensure!(
                    self.turn.is_none()
                        && self.result.is_none()
                        && !self.children.is_empty()
                        && self.children.values().any(|closed| !closed),
                    "original SDK turn unavailable or replaced"
                );
                self.turn = Some(text(&observation.turn_id)?.to_owned());
            }
            K::ParentAssistant | K::SdkResult => {
                anyhow::ensure!(
                    self.session.as_deref() == Some(text(&observation.session_id)?)
                        && self.turn.as_deref() == Some(text(&observation.turn_id)?)
                        && self.result.is_none()
                        && !self.stream_joined,
                    "SDK session/turn/result differs"
                );
                if observation.kind == K::ParentAssistant {
                    self.parent = Some((
                        text(&observation.message_id)?.to_owned(),
                        text(&observation.message_model)?.to_owned(),
                        text(&observation.assistant_id)?.to_owned(),
                    ));
                } else {
                    anyhow::ensure!(
                        self.parent.is_some(),
                        "SDK result has no original parent message"
                    );
                    self.result = Some((
                        text(&observation.result_id)?.to_owned(),
                        observation
                            .is_error
                            .context("SDK result error flag missing")?,
                        text(&observation.subtype)?.to_owned(),
                    ));
                }
            }
            K::SdkStreamJoined => {
                anyhow::ensure!(
                    !self.stream_joined && !self.children.is_empty(),
                    "SDK stream join unavailable"
                );
                self.stream_joined = true;
            }
            K::SdkQueryCloseReturned => {
                anyhow::ensure!(
                    !self.query_closed && !self.children.is_empty(),
                    "SDK query close unavailable"
                );
                self.query_closed = true;
            }
        }
        Ok(())
    }
    fn drained_success(&self) -> bool {
        !self.children.is_empty()
            && self.children.values().all(|closed| *closed)
            && self.stream_joined
            && self.query_closed
            && self
                .result
                .as_ref()
                .is_some_and(|(_, error, subtype)| !error && subtype == "success")
            && self.parent.is_some()
    }
}
fn append(
    core: &Connection,
    controller: &NativeSupervisorHoldingController,
    observation: &wire::SourceSdkObservation,
) -> anyhow::Result<Value> {
    append_in_current(
        core,
        controller.controller_id(),
        controller.execution_key(),
        &controller.lease.lease_hash,
        observation,
    )
}
fn append_in_current(
    core: &Connection,
    controller_id: &str,
    execution_key: &str,
    lease_hash: &str,
    observation: &wire::SourceSdkObservation,
) -> anyhow::Result<Value> {
    shape(observation)?;
    core.execute_batch(SCHEMA)?;
    let records: Vec<(u64, String)> = core
        .prepare(
            "SELECT sequence,observation_json FROM workjet_supervisor_sdk_observations
         WHERE controller_id=?1 AND execution_key=?2 AND lease_hash=?3 ORDER BY sequence",
        )?
        .query_map(params![controller_id, execution_key, lease_hash], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    anyhow::ensure!(records.len() <= 512, "SDK journal budget exceeded");
    let encoded = serde_json::to_string(observation)?;
    let mut state = State::default();
    for (expected, (sequence, raw)) in records.iter().enumerate() {
        anyhow::ensure!(*sequence == expected as u64, "SDK journal sequence differs");
        let previous: wire::SourceSdkObservation = serde_json::from_str(raw)?;
        anyhow::ensure!(
            previous.sequence == *sequence,
            "SDK journal envelope differs"
        );
        state.apply(&previous)?;
    }
    if let Some((_, previous)) = records.get(observation.sequence as usize) {
        anyhow::ensure!(previous == &encoded, "SDK observation replay differs");
    } else {
        anyhow::ensure!(
            observation.sequence == records.len() as u64 && records.len() < 512,
            "SDK observation is not the next sequence"
        );
        state.apply(observation)?;
        core.execute(
            "INSERT INTO workjet_supervisor_sdk_observations
            (controller_id,execution_key,lease_hash,sequence,observation_json,recorded_at_ms)
            VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                controller_id,
                execution_key,
                lease_hash,
                observation.sequence,
                encoded,
                now_ms()
            ],
        )?;
    }
    let parent = join_native_parent(core, controller_id, execution_key, lease_hash, &state)?;
    if let Some(parent) = parent {
        core.execute_batch("CREATE TABLE IF NOT EXISTS workjet_supervisor_sdk_parent_joins (
            controller_id TEXT PRIMARY KEY, execution_key TEXT NOT NULL, lease_hash TEXT NOT NULL,
            sdk_session_id TEXT NOT NULL, sdk_turn_id TEXT NOT NULL, sdk_result_id TEXT NOT NULL,
            model_operation_id TEXT NOT NULL, reply_sha256 TEXT NOT NULL, joined_at_ms INTEGER NOT NULL);")?;
        let reply_hash = format!("{:x}", sha2::Sha256::digest(parent.text.as_bytes()));
        core.execute(
            "INSERT INTO workjet_supervisor_sdk_parent_joins
            (controller_id,execution_key,lease_hash,sdk_session_id,sdk_turn_id,sdk_result_id,
                model_operation_id,reply_sha256,joined_at_ms)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
            ON CONFLICT(controller_id) DO NOTHING",
            params![
                controller_id,
                execution_key,
                lease_hash,
                state.session,
                state.turn,
                state.result.as_ref().map(|result| result.0.as_str()),
                parent.operation_id,
                reply_hash,
                now_ms()
            ],
        )?;
        let same:bool=core.query_row("SELECT EXISTS(SELECT 1 FROM workjet_supervisor_sdk_parent_joins
            WHERE controller_id=?1 AND execution_key=?2 AND lease_hash=?3 AND sdk_session_id=?4
              AND sdk_turn_id=?5 AND sdk_result_id=?6 AND model_operation_id=?7 AND reply_sha256=?8)",
            params![controller_id,execution_key,lease_hash,state.session,state.turn,
                state.result.as_ref().map(|result|result.0.as_str()),
                parent.operation_id,reply_hash],|row|row.get(0))?;
        anyhow::ensure!(same, "native SDK parent join changed");
    }
    // A drained SDK success is necessary, not sufficient: native upstream model
    // and message anchors must be joined before any actual/result is accepted.
    Ok(
        json!({"version":1,"state":"sdk_observed","sequence":observation.sequence,
        "execution_ready":false}),
    )
}
pub(super) fn respond(
    host: &NativeSupervisorSourceHost,
    authority: AdmittedConsumerAuthority,
    operation: &wire::SourceOperation,
) -> anyhow::Result<GuardedAuxiliaryResponse> {
    let controller = host.original_controller(&authority, operation)?;
    let publication = controller.publication_for(&authority, Arc::new(ControllerOnly))?;
    let observation = operation
        .sdk_observation
        .as_ref()
        .context("SDK observation missing")?;
    let result = controller.with_current(|_, core, _| append(core, &controller, observation))?;
    Ok(GuardedAuxiliaryResponse {
        result,
        publication,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(sequence: u64, kind: &str, fields: Value) -> wire::SourceSdkObservation {
        let mut value = json!({"version":1,"sequence":sequence,"kind":kind});
        for (key, value_field) in fields.as_object().unwrap() {
            value[key] = value_field.clone();
        }
        serde_json::from_value(value).unwrap()
    }
    fn start() -> State {
        let mut state = State::default();
        state
            .apply(&event(0, "child-spawned", json!({"pid":123})))
            .unwrap();
        state
            .apply(&event(
                1,
                "sdk-init",
                json!({"session_id":"sdk-session","init_id":"init-id"}),
            ))
            .unwrap();
        state
            .apply(&event(
                2,
                "turn-submitted",
                json!({"turn_id":"original-turn"}),
            ))
            .unwrap();
        state
    }
    fn result(state: &mut State, failed: bool) {
        state
            .apply(&event(
                3,
                "parent-assistant",
                json!({"session_id":"sdk-session",
            "turn_id":"original-turn","message_id":"msg_observed","message_model":"claude-opus-5-5",
            "assistant_id":"assistant-id"}),
            ))
            .unwrap();
        state.apply(&event(4,"sdk-result",json!({"session_id":"sdk-session",
            "turn_id":"original-turn","result_id":"result-id","subtype":if failed {"error_during_execution"} else {"success"},
            "is_error":failed}))).unwrap();
    }
    #[test]
    fn sdk_result_needs_nonempty_captured_children_actual_close_and_both_drains() {
        let mut state = start();
        result(&mut state, false);
        assert!(!state.drained_success());
        state
            .apply(&event(5, "sdk-query-close-returned", json!({})))
            .unwrap();
        assert!(!state.drained_success());
        state
            .apply(&event(6, "sdk-stream-joined", json!({})))
            .unwrap();
        assert!(!state.drained_success());
        assert!(state
            .apply(&event(7, "child-closed", json!({"pid":999})))
            .is_err());
        state
            .apply(&event(7, "child-closed", json!({"pid":123,"exit_code":0})))
            .unwrap();
        assert!(state.drained_success());
        assert!(!State::default().drained_success());
    }
    #[test]
    fn sdk_error_or_changed_session_turn_cannot_become_a_success() {
        let mut state = start();
        for (session, turn) in [("foreign", "original-turn"), ("sdk-session", "stale")] {
            assert!(state.apply(&event(3,"parent-assistant",json!({"session_id":session,"turn_id":turn,
                "message_id":"msg_observed","message_model":"claude-opus-5-5","assistant_id":"assistant-id"}))).is_err());
        }
        assert!(state
            .apply(&event(
                3,
                "sdk-init",
                json!({"session_id":"other","init_id":"new"})
            ))
            .is_err());
        result(&mut state, true);
        state
            .apply(&event(5, "child-closed", json!({"pid":123,"exit_code":1})))
            .unwrap();
        state
            .apply(&event(6, "sdk-stream-joined", json!({})))
            .unwrap();
        state
            .apply(&event(7, "sdk-query-close-returned", json!({})))
            .unwrap();
        assert!(!state.drained_success());
    }
    #[test]
    fn journal_exact_replay_is_one_row_and_rollback_keeps_the_next_sequence() -> anyhow::Result<()>
    {
        let mut core = Connection::open_in_memory()?;
        let first = event(0, "child-spawned", json!({"pid":123}));
        {
            let tx = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
            assert_eq!(
                append_in_current(&tx, "controller", "execution", "lease", &first)?
                    ["execution_ready"],
                false
            );
            tx.rollback()?;
        }
        assert!(!core.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='workjet_supervisor_sdk_observations')",[],|r|r.get::<_,bool>(0))?);
        let tx = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let ack = append_in_current(&tx, "controller", "execution", "lease", &first)?;
        assert_eq!(
            ack,
            append_in_current(&tx, "controller", "execution", "lease", &first)?
        );
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM workjet_supervisor_sdk_observations",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        assert!(append_in_current(
            &tx,
            "controller",
            "execution",
            "lease",
            &event(0, "child-spawned", json!({"pid":999}))
        )
        .is_err());
        assert!(append_in_current(
            &tx,
            "controller",
            "execution",
            "lease",
            &event(2, "turn-submitted", json!({"turn_id":"turn"}))
        )
        .is_err());
        assert!(append_in_current(
            &tx,
            "controller",
            "other-execution",
            "lease",
            &event(1, "turn-submitted", json!({"turn_id":"turn"}))
        )
        .is_err());
        assert!(append_in_current(
            &tx,
            "controller",
            "execution",
            "other-lease",
            &event(1, "turn-submitted", json!({"turn_id":"turn"}))
        )
        .is_err());
        tx.commit()?;
        Ok(())
    }
    #[test]
    fn sdk_observation_envelope_cannot_select_tool_model_or_control_fields() -> anyhow::Result<()> {
        let operation = json!({"version":1,"action":"sdk_observe","offer_id":uuid::Uuid::new_v4().to_string(),
            "controller_id":uuid::Uuid::new_v4().to_string(),"sdk_observation":{
                "version":1,"sequence":0,"kind":"child-spawned","pid":123}});
        parse_operation(vec![operation.clone()])?;
        for key in [
            "operation_id",
            "native_tool",
            "body_json",
            "model_operation",
            "sdk_session_id",
            "sequence",
            "tool_arguments_json",
        ] {
            let mut bad = operation.clone();
            bad[key] = match key {
                "sequence" => json!(0),
                "native_tool" => json!("worker_dispatch"),
                "model_operation" => json!("messages"),
                "operation_id" => json!(uuid::Uuid::new_v4().to_string()),
                _ => json!("caller"),
            };
            assert!(parse_operation(vec![bad]).is_err(), "{key}");
        }
        let mut bad = operation;
        bad["action"] = json!("status");
        assert!(parse_operation(vec![bad]).is_err());
        Ok(())
    }

    fn drained() -> State {
        let mut state = start();
        result(&mut state, false);
        state
            .apply(&event(5, "sdk-query-close-returned", json!({})))
            .unwrap();
        state
            .apply(&event(6, "sdk-stream-joined", json!({})))
            .unwrap();
        state
            .apply(&event(7, "child-closed", json!({"pid":123,"exit_code":0})))
            .unwrap();
        state
    }
    fn native_message(core: &Connection) -> anyhow::Result<()> {
        model::ensure_model_schema(core)?;
        core.execute(
            "INSERT INTO workjet_supervisor_native_model_requests
            (operation_id,execution_key,lease_hash,controller_id,sdk_correlation,body_hash,
             state,created_at_ms,finished_at_ms,operation_kind,requested_model,response_model,
             response_message_id,response_text,response_stop_reason,response_complete,http_status)
            VALUES ('native-op','execution','lease','controller','sdk-session','hash',
             'observed',1,2,'messages','claude-opus-5-5','claude-opus-5-5',
             'msg_observed','Native upstream reply','end_turn',1,200)",
            [],
        )?;
        Ok(())
    }
    #[test]
    fn sdk_parent_join_requires_the_original_complete_native_messages_witness() -> anyhow::Result<()>
    {
        let core = Connection::open_in_memory()?;
        assert!(
            join_native_parent(&core, "controller", "execution", "lease", &drained())?.is_none()
        );
        native_message(&core)?;
        assert!(join_native_parent(&core, "controller", "execution", "lease", &start())?.is_none());
        let parent =
            join_native_parent(&core, "controller", "execution", "lease", &drained())?.unwrap();
        assert_eq!(parent.operation_id, "native-op");
        assert_eq!(parent.text, "Native upstream reply");
        // Outgoing model labels, count_tokens and SDK text cannot substitute for
        // the original native upstream Messages observation.
        for (column, bad, good) in [
            ("controller_id", "foreign", "controller"),
            ("execution_key", "foreign", "execution"),
            ("lease_hash", "stale", "lease"),
            ("sdk_correlation", "foreign", "sdk-session"),
            ("operation_kind", "count_tokens", "messages"),
            ("state", "accepted", "observed"),
            ("response_message_id", "other-message", "msg_observed"),
            ("response_model", "", "claude-opus-5-5"),
            ("response_stop_reason", "max_tokens", "end_turn"),
        ] {
            core.execute(
                &format!("UPDATE workjet_supervisor_native_model_requests SET {column}=?1"),
                [bad],
            )?;
            assert!(
                join_native_parent(&core, "controller", "execution", "lease", &drained())?
                    .is_none(),
                "{column}"
            );
            core.execute(
                &format!("UPDATE workjet_supervisor_native_model_requests SET {column}=?1"),
                [good],
            )?;
        }
        for (column, bad, good) in [("http_status", 500, 200), ("response_complete", 0, 1)] {
            core.execute(
                &format!("UPDATE workjet_supervisor_native_model_requests SET {column}=?1"),
                [bad],
            )?;
            assert!(
                join_native_parent(&core, "controller", "execution", "lease", &drained())?
                    .is_none(),
                "{column}"
            );
            core.execute(
                &format!("UPDATE workjet_supervisor_native_model_requests SET {column}=?1"),
                [good],
            )?;
        }
        core.execute("INSERT INTO workjet_supervisor_native_model_requests SELECT
            'duplicate',execution_key,lease_hash,controller_id,sdk_correlation,body_hash,state,
            operation_kind,requested_model,response_model,response_message_id,response_text,
            response_stop_reason,response_complete,upstream_request_id,http_status,created_at_ms,finished_at_ms
            FROM workjet_supervisor_native_model_requests WHERE operation_id='native-op'",[])?;
        assert!(join_native_parent(&core, "controller", "execution", "lease", &drained()).is_err());
        Ok(())
    }
    #[test]
    fn journal_parent_join_is_immutable_and_does_not_claim_execution_ready() -> anyhow::Result<()> {
        let mut core = Connection::open_in_memory()?;
        native_message(&core)?;
        let observations = [
            event(0, "child-spawned", json!({"pid":123})),
            event(
                1,
                "sdk-init",
                json!({"session_id":"sdk-session","init_id":"init-id"}),
            ),
            event(2, "turn-submitted", json!({"turn_id":"original-turn"})),
            event(
                3,
                "parent-assistant",
                json!({"session_id":"sdk-session","turn_id":"original-turn",
                "message_id":"msg_observed","message_model":"claude-opus-5-5","assistant_id":"assistant-id"}),
            ),
            event(
                4,
                "sdk-result",
                json!({"session_id":"sdk-session","turn_id":"original-turn",
                "result_id":"result-id","subtype":"success","is_error":false}),
            ),
            event(5, "sdk-stream-joined", json!({})),
            event(6, "sdk-query-close-returned", json!({})),
            event(7, "child-closed", json!({"pid":123,"exit_code":0})),
        ];
        for observation in &observations {
            assert_eq!(
                append_in_current(&core, "controller", "execution", "lease", observation)?
                    ["execution_ready"],
                false
            );
        }
        let saved: (String, String) = core.query_row(
            "SELECT model_operation_id,reply_sha256 FROM workjet_supervisor_sdk_parent_joins",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(saved.0, "native-op");
        assert_eq!(
            saved.1,
            format!("{:x}", sha2::Sha256::digest(b"Native upstream reply"))
        );
        let tx = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE workjet_supervisor_native_model_requests SET response_text='Different native reply'",[])?;
        assert!(
            append_in_current(&tx, "controller", "execution", "lease", &observations[7]).is_err()
        );
        tx.rollback()?;
        append_in_current(&core, "controller", "execution", "lease", &observations[7])?;
        assert_eq!(
            core.query_row(
                "SELECT count(*) FROM workjet_supervisor_sdk_parent_joins",
                [],
                |row| row.get::<_, i64>(0)
            )?,
            1
        );
        Ok(())
    }

    #[test]
    fn sdk_callback_shapes_do_not_accept_authority_or_stop_claims() {
        assert!(shape(&event(
            0,
            "child-spawned",
            json!({"pid":1,"is_error":false})
        ))
        .is_err());
        assert!(shape(&event(
            0,
            "sdk-result",
            json!({"session_id":"s","turn_id":"t","result_id":"r",
            "subtype":"success","is_error":false,"pid":1})
        ))
        .is_err());
        for key in [
            "actual",
            "terminated",
            "root",
            "owner",
            "reply",
            "credential",
        ] {
            let mut value = json!({"version":1,"sequence":0,"kind":"child-spawned","pid":1});
            value[key] = json!(true);
            assert!(serde_json::from_value::<wire::SourceSdkObservation>(value).is_err());
        }
    }
}
