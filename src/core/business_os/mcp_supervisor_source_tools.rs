// Origin: CTOX
// License: AGPL-3.0-only
//! Fixed native Supervisor tools, under the original enrolled controller.
//! Never forward a caller-selected tool, actor, root, token or HTTP endpoint.
use super::*;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DispatchArguments {
    task: String,
    title: Option<String>,
    computer_id: Option<String>,
    worker_profile_id: Option<String>,
}
fn request(id: &str, raw: &str) -> anyhow::Result<Value> {
    uuid::Uuid::parse_str(id)?;
    anyhow::ensure!(
        !raw.is_empty() && raw.len() <= 64 * 1024,
        "native tool arguments exceed budget"
    );
    let arguments: DispatchArguments = serde_json::from_str(raw)?;
    let mut request = serde_json::to_value(json!({
        "action":"dispatch", "dispatch_key":id, "task":arguments.task
    }))?;
    for (key, value) in [
        ("title", arguments.title),
        ("computer_id", arguments.computer_id),
        ("worker_profile_id", arguments.worker_profile_id),
    ] {
        if let Some(value) = value {
            request[key] = json!(value);
        }
    }
    Ok(request)
}

fn goal_request(id: &str, raw: &str) -> anyhow::Result<Value> {
    uuid::Uuid::parse_str(id)?;
    anyhow::ensure!(
        !raw.is_empty() && raw.len() <= 1024,
        "goal read arguments exceed budget"
    );
    let request: crate::business_os::workjet_jour_fixe_contract::ReadConfirmedGoalRequest =
        serde_json::from_str(raw)?;
    Ok(json!({"action":"read_confirmed_goal","request":request}))
}

pub(super) fn descriptors() -> Value {
    json!([{"name":"worker_dispatch",
        "description":"Request one owned worker via the existing registered project Source. Acknowledged startup is not completed work.",
        "inputSchema":{"type":"object","additionalProperties":false,"required":["task"],
            "properties":{"task":{"type":"string","minLength":1,"maxLength":16384},
                "title":{"type":"string","maxLength":200},
                "computer_id":{"type":"string","maxLength":256},
                "worker_profile_id":{"type":"string","maxLength":256}}}},
        {"name":"confirmed_goal_read","description":"Read this project’s actual Owner-confirmed goal, native step status and saved results. Null means no confirmed definition. Read-only; never confirms, replans or completes work.",
        "inputSchema":{"type":"object","additionalProperties":false,"properties":{}}}])
}
pub(super) fn respond(
    host: &NativeSupervisorSourceHost,
    authority: AdmittedConsumerAuthority,
    operation: &wire::SourceOperation,
) -> anyhow::Result<GuardedAuxiliaryResponse> {
    let kind = operation
        .native_tool
        .context("native Supervisor tool missing")?;
    let id = operation
        .operation_id
        .as_deref()
        .context("native tool operation missing")?;
    let raw = operation
        .tool_arguments_json
        .as_deref()
        .context("native tool arguments missing")?;
    let (tool, name, arguments) = match kind {
        wire::SourceNativeTool::WorkerDispatch => (
            workjet_worker_dispatch::TOOL,
            "worker_dispatch",
            request(id, raw)?,
        ),
        wire::SourceNativeTool::ConfirmedGoalRead => (
            workjet_jour_fixe::READ_TOOL,
            "confirmed_goal_read",
            goal_request(id, raw)?,
        ),
    };
    let controller = host.original_controller(&authority, operation)?;
    let trusted = &controller.lease.trusted;
    enforce_internal_command_session_scope(tool, &arguments, Some(trusted))?;
    let context =
        context_from_arguments_with_trusted_gateway_context(tool, &arguments, Some(trusted))?;
    // Coarse existing channel policy before the scope, then all lease/project/
    // role/epoch checks share the actual Core/Policy mutation reservation.
    enforce_business_os_mcp_policy(&host.root, &context, tool, &arguments)?;
    let publication = controller.publication_for(&authority, Arc::new(ControllerOnly))?;
    let result = controller.with_current(|facts, core, policy| {
        let row = read_offer(
            core,
            operation
                .offer_id
                .as_deref()
                .context("native offer missing")?,
            facts,
        )?;
        anyhow::ensure!(
            row.state == "claimed"
                && row.controller_id.as_deref() == Some(controller.controller_id())
                && row.deadline_ms > now_ms(),
            "native Source tool offer retired"
        );
        match kind {
            wire::SourceNativeTool::WorkerDispatch => {
                workjet_worker_dispatch::dispatch_in_native_scope(
                    core, policy, &context, trusted, &arguments,
                )
            }
            wire::SourceNativeTool::ConfirmedGoalRead => {
                workjet_jour_fixe::read_confirmed_goal_in_native_scope(
                    core, policy, &context, trusted,
                )
            }
        }
    })?;
    anyhow::ensure!(
        serde_json::to_vec(&result)?.len() <= 64 * 1024,
        "native tool reply exceeds budget"
    );
    Ok(GuardedAuxiliaryResponse {
        result: json!({"version":1,"state":"tool_result",
        "operation_id":id,"native_tool":name,"result":result,"execution_ready":false}),
        publication,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_dispatch_uses_the_existing_transaction_and_lost_ack_replays_one_intent(
    ) -> anyhow::Result<()> {
        let (root, token) = super::super::super::super::tests::fixture(true)?;
        let lease = NativeSupervisorExecutionLease::capture(root.path(), &token)?;
        let gateway = json!({"auth_source":"ctox_dev_managed_mcp_token","channel":"ctox_dev_managed_mcp",
            "surface":"workjet","actor":"owner","role":"chef","workspace":"tenant:source-owner","instance_id":"source-instance"});
        call_tool_audited_with_trusted_gateway_context(
            root.path(),
            workjet_worker_dispatch::TOOL,
            json!({"action":"register_source","source_environment_id":"source-env",
                "source_supervisor_thread_id":"cc6cfe73-2824-4360-9daf-3b3efb079931","project_id":"project"}),
            Some(&gateway),
        )?;
        let id = uuid::Uuid::new_v4().to_string();
        let arguments = request(
            &id,
            &json!({"task":"Make an owned tested change"}).to_string(),
        )?;
        let context = context_from_arguments_with_trusted_gateway_context(
            workjet_worker_dispatch::TOOL,
            &arguments,
            Some(&lease.trusted),
        )?;
        lease.verify_session()?;
        let mut core = core(root.path())?;
        let mut policy = store::open_store(root.path())?;
        let first = {
            let core = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let policy = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
            lease.current_native(&core, &policy)?;
            let result = workjet_worker_dispatch::dispatch_in_native_scope(
                &core,
                &policy,
                &context,
                &lease.trusted,
                &arguments,
            )?;
            // Helper must not independently commit the existing native reservation.
            core.rollback()?;
            policy.rollback()?;
            result
        };
        assert_eq!(
            core.query_row(
                "SELECT count(*) FROM workjet_worker_dispatch_intents",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        let accepted = {
            let core = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let policy = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
            lease.current_native(&core, &policy)?;
            let result = workjet_worker_dispatch::dispatch_in_native_scope(
                &core,
                &policy,
                &context,
                &lease.trusted,
                &arguments,
            )?;
            core.commit()?;
            policy.commit()?;
            result
        };
        assert_ne!(first["intent"]["intentId"], accepted["intent"]["intentId"]);
        {
            let core = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let policy = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
            lease.current_native(&core, &policy)?;
            assert_eq!(
                workjet_worker_dispatch::dispatch_in_native_scope(
                    &core,
                    &policy,
                    &context,
                    &lease.trusted,
                    &arguments
                )?,
                accepted
            );
            let mut different = arguments.clone();
            different["task"] = json!("Different work");
            assert!(workjet_worker_dispatch::dispatch_in_native_scope(
                &core,
                &policy,
                &context,
                &lease.trusted,
                &different
            )
            .is_err());
            core.commit()?;
            policy.commit()?;
        }
        assert_eq!(
            core.query_row(
                "SELECT count(*) FROM workjet_worker_dispatch_intents",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1
        );
        core.execute("UPDATE communication_routing_state SET lease_worker_id='replacement' WHERE route_status='leased'",[])?;
        assert!(lease.current_native(&core, &policy).is_err());
        assert!(workjet_worker_dispatch::dispatch_in_native_scope(
            &core,
            &policy,
            &context,
            &lease.trusted,
            &arguments
        )
        .is_err());
        let actual: Option<String> = core.query_row(
            "SELECT actual_json FROM workjet_supervisor_route_attempts",
            [],
            |r| r.get(0),
        )?;
        assert!(actual.is_none());
        Ok(())
    }

    #[test]
    fn native_goal_tool_has_no_caller_selected_project_or_execution() -> anyhow::Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            goal_request(&id, "{}")?,
            json!({"action":"read_confirmed_goal","request":{}})
        );
        for key in [
            "owner",
            "project_id",
            "goal_id",
            "lease",
            "token",
            "root",
            "action",
            "url",
        ] {
            assert!(
                goal_request(&id, &json!({key:"caller"}).to_string()).is_err(),
                "{key}"
            );
        }
        assert!(goal_request("not-an-operation", "{}").is_err());
        assert!(goal_request(&id, &" ".repeat(1025)).is_err());
        let operation = json!({"version":1,"action":"tool_call","offer_id":uuid::Uuid::new_v4().to_string(),
            "controller_id":uuid::Uuid::new_v4().to_string(),"operation_id":id,
            "native_tool":"confirmed_goal_read","tool_arguments_json":"{}"});
        parse_operation(vec![operation])?;
        Ok(())
    }

    #[test]
    fn sdk_tool_cannot_choose_authority_action_or_dispatch_identity() -> anyhow::Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let raw = json!({"task":"Make an owned change"}).to_string();
        assert_eq!(request(&id, &raw)?["dispatch_key"], id);
        for key in [
            "action",
            "dispatch_key",
            "project_id",
            "owner",
            "_context",
            "token",
            "root",
            "url",
        ] {
            let mut bad = json!({"task":"Make an owned change"});
            bad[key] = json!("caller");
            assert!(request(&id, &bad.to_string()).is_err(), "{key}");
        }
        assert!(request("not-an-operation", &raw).is_err());
        assert!(request(&id, &"x".repeat(65537)).is_err());
        Ok(())
    }
    #[test]
    fn tool_envelope_is_not_a_model_request_or_a_control_action() -> anyhow::Result<()> {
        let operation = json!({"version":1,"action":"tool_call","offer_id":uuid::Uuid::new_v4().to_string(),
            "controller_id":uuid::Uuid::new_v4().to_string(),"operation_id":uuid::Uuid::new_v4().to_string(),
            "native_tool":"worker_dispatch","tool_arguments_json":"{\"task\":\"Make a change\"}"});
        parse_operation(vec![operation.clone()])?;
        for key in [
            "body_json",
            "model_operation",
            "sdk_session_id",
            "sequence",
            "actual",
            "authority",
        ] {
            let mut bad = operation.clone();
            bad[key] = if key == "sequence" {
                json!(0)
            } else {
                json!("caller")
            };
            assert!(parse_operation(vec![bad]).is_err(), "{key}");
        }
        let mut bad = operation;
        bad["native_tool"] = json!("execute_action");
        assert!(parse_operation(vec![bad]).is_err());
        assert!(parse_operation(vec![
            json!({"version":1,"action":"poll","native_tool":"worker_dispatch"})
        ])
        .is_err());
        assert_eq!(descriptors().as_array().unwrap().len(), 2);
        Ok(())
    }
}
