// Origin: CTOX
// License: AGPL-3.0-only
//! Ordered observations from the original enrolled Source's private SDK callbacks.
//! These records are not execution authority or a caller-reported model result.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_supervisor_sdk_observations (
 controller_id TEXT NOT NULL, execution_key TEXT NOT NULL, lease_hash TEXT NOT NULL,
 sequence INTEGER NOT NULL, observation_json TEXT NOT NULL, recorded_at_ms INTEGER NOT NULL,
 PRIMARY KEY(controller_id,sequence));";

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
