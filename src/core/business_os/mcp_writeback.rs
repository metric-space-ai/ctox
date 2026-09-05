use super::*;

pub(crate) const RESEARCH_WRITEBACK_COMMAND: &str = "outbound.lead.research_writeback";
const RESEARCH_COLLECTION: &str = "outbound_lead_generation_leads";
const RESEARCH_MODULE: &str = "outbound-lead-generation";

pub(crate) fn supports_command_writeback(contract: &Value) -> bool {
    contract.get("mechanism").and_then(Value::as_str) == Some("business_command")
        && contract.get("command_type").and_then(Value::as_str) == Some(RESEARCH_WRITEBACK_COMMAND)
        && contract.get("collection").and_then(Value::as_str) == Some(RESEARCH_COLLECTION)
        && !normalized_string_array(contract.get("record_ids")).is_empty()
}

fn bound_payload(parent_id: &str, contract: &Value, arguments: &Value) -> anyhow::Result<Value> {
    anyhow::ensure!(
        supports_command_writeback(contract),
        "unsupported Business OS writeback contract"
    );
    let record_id = required_arg(arguments, "record_id")?;
    anyhow::ensure!(
        normalized_string_array(contract.get("record_ids")).contains(&record_id),
        "writeback record is outside the originating command contract"
    );
    let mut payload = arguments
        .get("payload")
        .cloned()
        .context("writeback payload is required")?;
    let object = payload
        .as_object_mut()
        .context("writeback payload must be an object")?;
    for (key, expected) in [
        ("record_id", record_id.as_str()),
        ("module", RESEARCH_MODULE),
        ("research_command_id", parent_id),
        ("gap_task_id", ""),
    ] {
        if let Some(value) = object.get(key) {
            anyhow::ensure!(
                value.as_str() == Some(expected),
                "writeback payload.{key} is outside its signed command scope"
            );
        }
        object.insert(key.to_string(), Value::String(expected.to_string()));
    }
    Ok(payload)
}

pub(super) fn execute(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
    trusted: Option<&Value>,
) -> anyhow::Result<Value> {
    let trusted = trusted
        .filter(|value| {
            string_field(value, "auth_source").as_deref() == Some(MCP_INTERNAL_SESSION_AUTH_SOURCE)
        })
        .context("execute_writeback requires a signed internal Business OS command session")?;
    let parent_id = required_arg(trusted, "command_id")?;
    let parent = crate::mission::channels::business_command_projection(root, &parent_id)?;
    anyhow::ensure!(
        parent["payload_hash"] == trusted["payload_hash"],
        "writeback parent payload changed"
    );
    anyhow::ensure!(
        parent["command_type"] == "business_os.chat.task",
        "writeback requires a research chat command"
    );
    let authorization =
        store::revalidate_business_command_execution_authorization(root, &parent_id)?;
    anyhow::ensure!(
        authorization.pointer("/actor/id").and_then(Value::as_str) == Some(context.actor.as_str()),
        "writeback actor does not match originating command"
    );
    let contract = parent
        .pointer("/payload/writeback_contract")
        .context("writeback contract missing")?;
    let payload = bound_payload(&parent_id, contract, arguments)?;
    enforce_module_policy(root, RESEARCH_MODULE)?;
    enforce_collection_policy(root, RESEARCH_COLLECTION)?;
    let decision = trusted_mcp_actor_policy_decision(
        root,
        context,
        BusinessOsPermission::DataWrite,
        BusinessOsScopeType::Collection,
        Some(RESEARCH_COLLECTION),
    )?;
    anyhow::ensure!(
        decision.allowed,
        "writeback denied: {}",
        decision.display_reason
    );
    // The same parent, record and payload replay the same native command.
    // Native command validation remains responsible for field evidence and persistence.
    let command_id = format!(
        "writeback_{}",
        URL_SAFE_NO_PAD.encode(
            digest::digest(
                &digest::SHA256,
                &serde_json::to_vec(&serde_json::json!({
                    "parent": parent_id, "payload": payload
                }))?
            )
            .as_ref()
        )
    );
    anyhow::ensure!(
        crate::business_os::command_plane::EXACT_CONTROL_TYPES
            .contains(&RESEARCH_WRITEBACK_COMMAND),
        "native research writeback handler is unavailable; refusing recursive queue fallback"
    );
    let outcome = store::accept_rxdb_business_command(
        root,
        serde_json::json!({
            "id": command_id,
            "command_id": command_id,
            "module": RESEARCH_MODULE,
            "command_type": RESEARCH_WRITEBACK_COMMAND,
            "record_id": payload["record_id"],
            "payload": payload,
            "client_context": {
                "actor": authorization["actor"],
                "parent_command_id": parent_id,
                "mcp_tool": "business_os.execute_writeback"
            }
        }),
    )
    .map_err(|error| anyhow::anyhow!("Business command writeback failed: {error:#}"))?;
    anyhow::ensure!(
        outcome.get("ok").and_then(Value::as_bool) != Some(false)
            && outcome.get("status").and_then(Value::as_str) != Some("failed"),
        "Business command writeback failed: {}",
        outcome
    );
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incident_scoped_writeback_calls_native_handler_without_queue_recursion() -> anyhow::Result<()>
    {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let contract = serde_json::json!({
            "mechanism": "business_command", "command_type": RESEARCH_WRITEBACK_COMMAND,
            "collection": RESEARCH_COLLECTION, "record_ids": ["lead-a"]
        });
        let (capability, _) = store::issue_business_os_capability_token_for_managed_user(
            root,
            "operator",
            "Operator",
            "admin",
            chrono::Utc::now().timestamp_millis(),
        )?;
        store::accept_rxdb_business_command_with_origin(
            root,
            serde_json::json!({
                "id": "research-a", "module": RESEARCH_MODULE,
                "command_type": "business_os.chat.task", "record_id": "lead-a",
                "payload": {"instruction": "Research the lead", "mode": "data", "writeback_contract": contract},
                "client_context": {"capability_token": capability}
            }),
            store::CommandOrigin::ReplicatedPeer,
        )?;
        let parent = crate::mission::channels::business_command_projection(root, "research-a")?;
        let token = issue_internal_command_session_token(
            root,
            "research-a",
            parent["payload_hash"].as_str().unwrap(),
            "operator",
            "admin",
            "research-workspace",
            &contract,
        )?;
        let trusted = verify_internal_command_session_token(root, &token)?;
        let arguments = serde_json::json!({"record_id": "lead-a", "payload": {}});
        let context = context_from_arguments_with_trusted_gateway_context(
            "business_os.execute_writeback",
            &arguments,
            Some(&trusted),
        )?;
        assert!(matches!(
            tool_policy_class("business_os.execute_writeback"),
            McpToolPolicyClass::Write
        ));
        assert!(execute(root, &context, &arguments, None)
            .unwrap_err()
            .to_string()
            .contains("signed"));
        let error = execute(root, &context, &arguments, Some(&trusted))
            .unwrap_err()
            .to_string();
        if crate::business_os::command_plane::EXACT_CONTROL_TYPES
            .contains(&RESEARCH_WRITEBACK_COMMAND)
        {
            assert!(error.contains("writeback failed"), "{error}");
        } else {
            assert!(
                error.contains("refusing recursive queue fallback"),
                "{error}"
            );
        }
        assert_eq!(
            crate::mission::channels::list_queue_tasks(root, &[], 100)?.len(),
            1
        );
        Ok(())
    }
    #[test]
    fn incident_writeback_scope_rejects_cross_record_parent_and_gap() -> anyhow::Result<()> {
        let contract = serde_json::json!({
            "mechanism": "business_command", "command_type": RESEARCH_WRITEBACK_COMMAND,
            "collection": RESEARCH_COLLECTION, "record_ids": ["lead-a"]
        });
        let arguments = serde_json::json!({"record_id": "lead-a",
            "payload": {"field_status": {}, "result": {"fields": {}}}});
        let bound = bound_payload("research-a", &contract, &arguments)?;
        assert_eq!(bound["research_command_id"], "research-a");
        assert_eq!(bound["module"], RESEARCH_MODULE);
        for forbidden in [
            serde_json::json!({"record_id": "lead-b", "payload": {}}),
            serde_json::json!({"record_id": "lead-a", "payload": {"research_command_id": "research-b"}}),
            serde_json::json!({"record_id": "lead-a", "payload": {"gap_task_id": "other-task"}}),
        ] {
            assert!(bound_payload("research-a", &contract, &forbidden).is_err());
        }
        Ok(())
    }
}
