// Origin: CTOX
// License: AGPL-3.0-only
//! Computation evidence derived inside the retained original Source controller.
//! This does not complete a goal, review, command or queue lease.
use super::*;
use sha2::Digest;

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_supervisor_native_completions (
 controller_id TEXT PRIMARY KEY, offer_id TEXT NOT NULL,
 execution_key TEXT NOT NULL, lease_hash TEXT NOT NULL, owner_user_id TEXT NOT NULL,
 evidence_json TEXT NOT NULL, evidence_sha256 TEXT NOT NULL,
 reply_text TEXT NOT NULL, reply_sha256 TEXT NOT NULL, recorded_at_ms INTEGER NOT NULL,
 published_at_ms INTEGER,
 UNIQUE(execution_key,lease_hash));";

fn digest_text(text: &str) -> String {
    format!("{:x}", sha2::Sha256::digest(text.as_bytes()))
}

/// Caller holds this exact original controller's issuer/Core/Policy reservation.
/// Strings from the SDK select data; they never construct this controller.
/// A private cleanup/publication marker, never an execution authority.
pub(super) struct StoredComputation {
    controller_id: String,
}

pub(super) fn record(
    core: &Connection,
    controller: &NativeSupervisorHoldingController,
    facts: &ConsumerFacts,
) -> anyhow::Result<Option<StoredComputation>> {
    let Some(parent) = sdk::joined_in_current(
        core,
        controller.controller_id(),
        controller.execution_key(),
        &controller.lease.lease_hash,
    )?
    else {
        return Ok(None);
    };
    let lease = &controller.lease;
    let offer: String = core.query_row(
        "SELECT offer_id FROM workjet_supervisor_source_offers
        WHERE controller_id=?1 AND execution_key=?2 AND lease_hash=?3 AND state='claimed'
          AND owner_user_id=?4 AND computer_id=?5 AND project_id=?6 AND supervisor_thread_id=?7",
        params![
            controller.controller_id(),
            lease.execution_key,
            lease.lease_hash,
            facts.owner_user_id,
            facts.computer_id,
            lease.requested.project_id,
            lease.requested.supervisor_thread_id
        ],
        |row| row.get(0),
    )?;
    let evidence = json!({
        "schema":"ctox.workjet.supervisor_computed.v1",
        "offer_id":offer,"controller_id":controller.controller_id(),
        "execution_key":lease.execution_key,"lease_hash":lease.lease_hash,
        "project_id":lease.requested.project_id,"supervisor_thread_id":lease.requested.supervisor_thread_id,
        "owner_user_id":facts.owner_user_id,
        "configured":{"luma_id":lease.requested.luma_id,"route_id":lease.requested.route_id,
            "revision":lease.requested.configuration_revision},
        "observed":{"harness":"claude-code","computer_id":facts.computer_id,
            "account_id":controller.selection().account().account_id,
            "model":parent.model,"model_operation_id":parent.operation_id,
            "native_message_id":parent.message_id,"upstream_request_id":parent.request_id,
            "model_finished_at_ms":parent.finished_at_ms,
            "sdk_session_id":parent.session_id,"sdk_turn_id":parent.turn_id,
            "sdk_assistant_id":parent.assistant_id,"sdk_result_id":parent.result_id,
            "closed_child_pids":parent.child_pids},
        "reply_sha256":digest_text(&parent.text),
        "consumer":facts
    });
    let raw = serde_json::to_string(&evidence)?;
    anyhow::ensure!(
        raw.len() <= 16 * 1024,
        "native computation evidence exceeds budget"
    );
    store_computed(
        core,
        controller.controller_id(),
        &offer,
        &lease.execution_key,
        &lease.lease_hash,
        &facts.owner_user_id,
        &raw,
        &parent.text,
    )?;
    Ok(Some(StoredComputation {
        controller_id: controller.controller_id().to_owned(),
    }))
}

/// A completed journal cannot open another native model session. This is a
/// negative restriction inside the original reservation, never a Boolean permit.
pub(super) fn ensure_not_computed(core: &Connection, controller_id: &str) -> anyhow::Result<()> {
    let exists: bool = core.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='workjet_supervisor_native_completions')",
        [], |row| row.get(0))?;
    if exists {
        let completed: bool = core.query_row(
            "SELECT EXISTS(SELECT 1 FROM workjet_supervisor_native_completions WHERE controller_id=?1)",
            [controller_id], |row| row.get(0))?;
        anyhow::ensure!(
            !completed,
            unavailable(
                "supervisor_execution_fenced",
                "original computation already recorded"
            )
        );
    }
    Ok(())
}

/// Called only after a successful guarded send, under the same retained native
/// controller. Failed/Pending sends cannot expose a result to the service.
pub(super) fn mark_published(
    core: &Connection,
    controller: &NativeSupervisorHoldingController,
    marker: &StoredComputation,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        marker.controller_id == controller.controller_id(),
        "computation publication controller differs"
    );
    let changed = core.execute(
        "UPDATE workjet_supervisor_native_completions SET published_at_ms=COALESCE(published_at_ms,?1)
         WHERE controller_id=?2 AND execution_key=?3 AND lease_hash=?4",
        params![now_ms(),controller.controller_id(),controller.execution_key(),controller.lease.lease_hash])?;
    anyhow::ensure!(changed == 1, "native computation publication missing");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn store_computed(
    core: &Connection,
    controller: &str,
    offer: &str,
    execution: &str,
    lease: &str,
    owner: &str,
    evidence: &str,
    reply: &str,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        !reply.trim().is_empty() && reply.len() <= 64 * 1024,
        "native computation reply exceeds budget"
    );
    core.execute_batch(SCHEMA)?;
    let evidence_hash = digest_text(evidence);
    let reply_hash = digest_text(reply);
    core.execute("INSERT INTO workjet_supervisor_native_completions
        (controller_id,offer_id,execution_key,lease_hash,owner_user_id,evidence_json,evidence_sha256,
         reply_text,reply_sha256,recorded_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(controller_id) DO NOTHING",
        params![controller,offer,execution,lease,owner,evidence,evidence_hash,reply,reply_hash,now_ms()])?;
    let same:bool=core.query_row("SELECT EXISTS(SELECT 1 FROM workjet_supervisor_native_completions
        WHERE controller_id=?1 AND offer_id=?2 AND execution_key=?3 AND lease_hash=?4 AND owner_user_id=?5
          AND evidence_json=?6 AND evidence_sha256=?7 AND reply_text=?8 AND reply_sha256=?9)",
        params![controller,offer,execution,lease,owner,evidence,evidence_hash,reply,reply_hash],|row|row.get(0))?;
    anyhow::ensure!(same, "original native computation changed");
    Ok(())
}

/// Data-only read under the service's existing native lease. It does not create
/// a controller/consumer, invoke a model, change a result or grant Source rights.
pub(super) fn read_in_current(
    core: &Connection,
    lease: &NativeSupervisorExecutionLease,
    offer_id: &str,
) -> anyhow::Result<Option<String>> {
    let exists: bool = core.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master
        WHERE type='table' AND name='workjet_supervisor_native_completions')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    let row: Option<(String, String, String, String, String)> = core
        .query_row(
            "SELECT r.evidence_json,r.evidence_sha256,r.reply_text,r.reply_sha256,r.controller_id
         FROM workjet_supervisor_native_completions r JOIN workjet_supervisor_source_offers o
           ON o.offer_id=r.offer_id AND o.controller_id=r.controller_id
         WHERE r.offer_id=?1 AND r.execution_key=?2 AND r.lease_hash=?3 AND r.owner_user_id=?4
           AND o.execution_key=r.execution_key AND o.lease_hash=r.lease_hash AND o.state='claimed'
           AND o.owner_user_id=r.owner_user_id
           AND r.published_at_ms IS NOT NULL
           AND o.project_id=?5 AND o.supervisor_thread_id=?6 AND o.computer_id=?7",
            params![
                offer_id,
                lease.execution_key,
                lease.lease_hash,
                lease.trusted["actor"].as_str(),
                lease.requested.project_id,
                lease.requested.supervisor_thread_id,
                lease.requested.computer_id
            ],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;
    let Some((evidence, hash, reply, reply_hash, controller_id)) = row else {
        return Ok(None);
    };
    anyhow::ensure!(
        evidence.len() <= 16 * 1024
            && digest_text(&evidence) == hash
            && !reply.trim().is_empty()
            && reply.len() <= 64 * 1024
            && digest_text(&reply) == reply_hash,
        "native computation receipt changed"
    );
    let value: Value = serde_json::from_str(&evidence)?;
    anyhow::ensure!(
        value["schema"] == "ctox.workjet.supervisor_computed.v1"
            && value["controller_id"] == controller_id
            && value["offer_id"] == offer_id
            && value["execution_key"] == lease.execution_key
            && value["lease_hash"] == lease.lease_hash
            && value["project_id"] == lease.requested.project_id
            && value["supervisor_thread_id"] == lease.requested.supervisor_thread_id
            && value["owner_user_id"] == lease.trusted["actor"]
            && value["reply_sha256"] == reply_hash,
        "native computation receipt binding differs"
    );
    anyhow::ensure!(
        value["configured"]["luma_id"] == lease.requested.luma_id
            && value["configured"]["route_id"] == lease.requested.route_id
            && value["configured"]["revision"] == lease.requested.configuration_revision
            && value["observed"]["harness"] == lease.harness()
            && value["observed"]["computer_id"] == lease.requested.computer_id
            && value["observed"]["account_id"] == lease.selection().account().account_id,
        "native computation selection differs"
    );
    let parent = sdk::joined_in_current(
        core,
        &controller_id,
        &lease.execution_key,
        &lease.lease_hash,
    )?
    .context("native computation SDK/model join missing")?;
    let observed = &value["observed"];
    anyhow::ensure!(
        observed["sdk_session_id"] == parent.session_id
            && observed["sdk_turn_id"] == parent.turn_id
            && observed["sdk_assistant_id"] == parent.assistant_id
            && observed["sdk_result_id"] == parent.result_id
            && observed["closed_child_pids"] == json!(parent.child_pids)
            && observed["model_operation_id"] == parent.operation_id
            && observed["model"] == parent.model
            && observed["native_message_id"] == parent.message_id
            && observed["upstream_request_id"] == parent.request_id
            && observed["model_finished_at_ms"] == parent.finished_at_ms
            && parent.text == reply,
        "native computation model/SDK witness differs"
    );
    Ok(Some(reply))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn computation_replay_is_one_row_changed_reply_or_scope_is_rejected() -> anyhow::Result<()> {
        let mut core = Connection::open_in_memory()?;
        store_computed(
            &core,
            "controller",
            "offer",
            "execution",
            "lease",
            "owner",
            "metadata",
            "native reply",
        )?;
        store_computed(
            &core,
            "controller",
            "offer",
            "execution",
            "lease",
            "owner",
            "metadata",
            "native reply",
        )?;
        assert_eq!(
            core.query_row(
                "SELECT count(*) FROM workjet_supervisor_native_completions",
                [],
                |row| row.get::<_, i64>(0)
            )?,
            1
        );
        for (owner, lease, evidence, reply) in [
            ("other", "lease", "metadata", "native reply"),
            ("owner", "stale", "metadata", "native reply"),
            ("owner", "lease", "changed metadata", "native reply"),
            ("owner", "lease", "metadata", "changed reply"),
        ] {
            let tx = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
            assert!(store_computed(
                &tx,
                "controller",
                "offer",
                "execution",
                lease,
                owner,
                evidence,
                reply
            )
            .is_err());
            tx.rollback()?;
        }
        assert!(store_computed(
            &core,
            "replacement",
            "offer",
            "execution",
            "lease",
            "owner",
            "metadata",
            "native reply"
        )
        .is_err());
        Ok(())
    }

    // Isolated Core data fixtures, not an HTTP/SDK/installed acceptance claim.
    #[test]
    fn service_read_rejects_a_completion_blob_without_the_native_sdk_model_join(
    ) -> anyhow::Result<()> {
        let (root, token) = super::super::super::super::tests::fixture(true)?;
        let lease = NativeSupervisorExecutionLease::capture(root.path(), &token)?;
        let offer = NativeSupervisorSourceOffer::open(lease, "fixture prompt")?;
        let core = super::super::core(root.path())?;
        core.execute("UPDATE workjet_supervisor_source_offers SET state='claimed',controller_id='fixture-controller'",[])?;
        let receipt = json!({"schema":"ctox.workjet.supervisor_computed.v1",
            "offer_id":offer.id,"controller_id":"fixture-controller",
            "execution_key":offer.lease.execution_key,"lease_hash":offer.lease.lease_hash,
            "project_id":offer.lease.requested.project_id,
            "supervisor_thread_id":offer.lease.requested.supervisor_thread_id,
            "owner_user_id":offer.lease.trusted["actor"],
            "configured":{"luma_id":offer.lease.requested.luma_id,"route_id":offer.lease.requested.route_id,
                "revision":offer.lease.requested.configuration_revision},
            "observed":{"harness":offer.lease.harness(),"computer_id":offer.lease.requested.computer_id,
                "account_id":offer.lease.selection().account().account_id},
            "reply_sha256":digest_text("unverified reply")});
        let raw = serde_json::to_string(&receipt)?;
        store_computed(
            &core,
            "fixture-controller",
            &offer.id,
            &offer.lease.execution_key,
            &offer.lease.lease_hash,
            offer.lease.trusted["actor"].as_str().unwrap(),
            &raw,
            "unverified reply",
        )?;
        assert!(read_in_current(&core, &offer.lease, &offer.id)?.is_none());
        core.execute(
            "UPDATE workjet_supervisor_native_completions SET published_at_ms=1",
            [],
        )?;
        assert!(read_in_current(&core, &offer.lease, &offer.id).is_err());
        assert!(read_in_current(&core, &offer.lease, "foreign-offer")?.is_none());
        core.execute(
            "UPDATE workjet_supervisor_native_completions SET owner_user_id='foreign'",
            [],
        )?;
        assert!(read_in_current(&core, &offer.lease, &offer.id)?.is_none());
        core.execute("UPDATE workjet_supervisor_native_completions SET owner_user_id=?1,evidence_json='tampered'",[offer.lease.trusted["actor"].as_str().unwrap()])?;
        assert!(read_in_current(&core, &offer.lease, &offer.id).is_err());
        Ok(())
    }

    #[test]
    fn service_returns_only_published_native_reply_and_rejects_a_replaced_lease(
    ) -> anyhow::Result<()> {
        let (root, token) = super::super::super::super::tests::fixture(true)?;
        let lease = NativeSupervisorExecutionLease::capture(root.path(), &token)?;
        let offer = NativeSupervisorSourceOffer::open(lease, "isolated fixture prompt")?;
        let core = super::super::core(root.path())?;
        let controller = "fixture-controller";
        core.execute(
            "UPDATE workjet_supervisor_source_offers SET state='claimed',controller_id=?1",
            [controller],
        )?;
        model::ensure_model_schema(&core)?;
        core.execute("INSERT INTO workjet_supervisor_native_model_requests
            (operation_id,execution_key,lease_hash,controller_id,sdk_correlation,body_hash,state,
             operation_kind,response_model,response_message_id,response_text,response_stop_reason,
             response_complete,upstream_request_id,http_status,created_at_ms,finished_at_ms)
            VALUES ('fixture-model-op',?1,?2,?3,'fixture-sdk-session','fixture-body','observed',
             'messages',?4,'fixture-message','native fixture reply','end_turn',1,'fixture-request',200,1,2)",
            params![offer.lease.execution_key,offer.lease.lease_hash,controller,offer.lease.requested.model])?;
        let events = [
            json!({"kind":"child-spawned","pid":123}),
            json!({"kind":"sdk-init","session_id":"fixture-sdk-session","init_id":"fixture-init"}),
            json!({"kind":"turn-submitted","turn_id":"fixture-turn"}),
            json!({"kind":"parent-assistant","session_id":"fixture-sdk-session","turn_id":"fixture-turn",
                "message_id":"fixture-message","message_model":offer.lease.requested.model,"assistant_id":"fixture-assistant"}),
            json!({"kind":"sdk-result","session_id":"fixture-sdk-session","turn_id":"fixture-turn",
                "result_id":"fixture-result","subtype":"success","is_error":false}),
            json!({"kind":"child-closed","pid":123,"exit_code":0}),
            json!({"kind":"sdk-stream-joined"}),
            json!({"kind":"sdk-query-close-returned"}),
        ];
        for (sequence, mut event) in events.into_iter().enumerate() {
            event["version"] = json!(1);
            event["sequence"] = json!(sequence);
            let event = serde_json::from_value(event)?;
            let ack = sdk::append_fixture(
                &core,
                controller,
                &offer.lease.execution_key,
                &offer.lease.lease_hash,
                &event,
            )?;
            assert_eq!(ack["execution_ready"], false);
        }
        let parent = sdk::joined_in_current(
            &core,
            controller,
            &offer.lease.execution_key,
            &offer.lease.lease_hash,
        )?
        .unwrap();
        let evidence = json!({"schema":"ctox.workjet.supervisor_computed.v1","controller_id":controller,
            "offer_id":offer.id,"execution_key":offer.lease.execution_key,"lease_hash":offer.lease.lease_hash,
            "project_id":offer.lease.requested.project_id,"supervisor_thread_id":offer.lease.requested.supervisor_thread_id,
            "owner_user_id":offer.lease.trusted["actor"],
            "configured":{"luma_id":offer.lease.requested.luma_id,"route_id":offer.lease.requested.route_id,
                "revision":offer.lease.requested.configuration_revision},
            "observed":{"harness":offer.lease.harness(),"computer_id":offer.lease.requested.computer_id,
                "account_id":offer.lease.selection().account().account_id,
                "model":parent.model,"model_operation_id":parent.operation_id,
                "native_message_id":parent.message_id,"upstream_request_id":parent.request_id,
                "model_finished_at_ms":parent.finished_at_ms,
                "sdk_session_id":parent.session_id,"sdk_turn_id":parent.turn_id,
                "sdk_assistant_id":parent.assistant_id,"sdk_result_id":parent.result_id,
                "closed_child_pids":parent.child_pids},"reply_sha256":digest_text(&parent.text)});
        store_computed(
            &core,
            controller,
            &offer.id,
            &offer.lease.execution_key,
            &offer.lease.lease_hash,
            offer.lease.trusted["actor"].as_str().unwrap(),
            &serde_json::to_string(&evidence)?,
            &parent.text,
        )?;
        assert!(
            read_in_current(&core, &offer.lease, &offer.id)?.is_none(),
            "unpublished SDK ack cannot release the service"
        );
        assert!(
            ensure_not_computed(&core, controller).is_err(),
            "completed turn cannot invoke a fresh model"
        );
        ensure_not_computed(&core, "foreign-controller")?;
        core.execute(
            "UPDATE workjet_supervisor_native_completions SET published_at_ms=3",
            [],
        )?;
        assert_eq!(offer.wait_for_native_result()?, "native fixture reply");
        core.execute("UPDATE communication_routing_state SET lease_worker_id='replacement' WHERE route_status='leased'",[])?;
        assert!(
            offer.wait_for_native_result().is_err(),
            "replaced native lease must never reuse an old reply"
        );
        Ok(())
    }

    #[test]
    fn computation_and_evidence_are_rolled_back_with_the_original_reservation() -> anyhow::Result<()>
    {
        let mut core = Connection::open_in_memory()?;
        let tx = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
        store_computed(
            &tx,
            "controller",
            "offer",
            "execution",
            "lease",
            "owner",
            "metadata",
            "native reply",
        )?;
        tx.rollback()?;
        assert!(!core.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master
            WHERE name='workjet_supervisor_native_completions')",
            [],
            |row| row.get::<_, bool>(0)
        )?);
        assert!(store_computed(
            &core,
            "controller",
            "offer",
            "execution",
            "lease",
            "owner",
            "metadata",
            ""
        )
        .is_err());
        assert!(store_computed(
            &core,
            "controller",
            "offer",
            "execution",
            "lease",
            "owner",
            "metadata",
            &"x".repeat(65537)
        )
        .is_err());
        Ok(())
    }
}
