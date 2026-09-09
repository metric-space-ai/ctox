//! Restore an admitted attempt's two crew context lanes through the existing
//! command-session authority. This is not an external execution admission API.
use super::*;

pub(super) fn read(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
    trusted: Option<&Value>,
) -> anyhow::Result<Value> {
    let trusted = trusted
        .filter(|value| {
            string_field(value, "auth_source").as_deref() == Some(MCP_INTERNAL_SESSION_AUTH_SOURCE)
        })
        .context("crew context requires a signed Business OS command session")?;
    let object = arguments
        .as_object()
        .context("crew context arguments must be an object")?;
    anyhow::ensure!(
        object
            .keys()
            .all(|key| matches!(key.as_str(), "attempt_id" | "_context")),
        "crew context accepts only attempt_id; identity and memory scope are server-bound"
    );
    let attempt_id = required_arg(arguments, "attempt_id")?;
    let command_id = required_arg(trusted, "command_id")?;
    let payload_hash = required_arg(trusted, "payload_hash")?;
    let authorization =
        store::revalidate_business_command_execution_authorization(root, &command_id)?;
    anyhow::ensure!(
        authorization.pointer("/actor/id").and_then(Value::as_str) == Some(context.actor.as_str())
            && authorization
                .pointer("/actor/role")
                .and_then(Value::as_str)
                .map(normalize_role)
                == context.trusted_role,
        "crew context command authorization changed"
    );
    enforce_collection_policy(root, "ctox_crew_members")?;
    anyhow::ensure!(
        !crew_read_is_public(root, context, "ctox_crew_members")?,
        "crew context requires private crew-read permission"
    );
    let collections = normalized_string_array(trusted.get("allowed_collections"));
    anyhow::ensure!(
        collections.is_empty() || collections.iter().any(|value| value == "ctox_crew_members"),
        "crew context is outside the signed collection scope"
    );

    // No schema initialization or writable LCM engine on this read path. Keep
    // attempt, native lease, persona and memory in a single core DB snapshot.
    let mut conn = rusqlite::Connection::open_with_flags(
        crate::paths::core_db(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    let tx = conn.transaction()?;
    let binding: Option<(String, String, String)> = tx
        .query_row(
            "SELECT a.task_id,a.member_id,c.module FROM crew_attempts a
         JOIN business_command_task_links l ON l.task_id=a.task_id
         JOIN business_command_aggregates c ON c.command_id=l.command_id
         JOIN communication_routing_state r ON r.message_key=a.task_id
         WHERE a.attempt_id=?1 AND c.command_id=?2 AND c.payload_hash=?3
           AND a.finalized_at IS NULL AND a.started_at IS NOT NULL
           AND c.execution_phase!='terminal' AND r.route_status='leased'
           AND r.crew_member_id=a.member_id AND length(trim(r.lease_owner))>0
           AND julianday(r.leased_at) IS NOT NULL
           AND julianday(r.lease_expires_at)>julianday('now')",
            params![attempt_id, command_id, payload_hash],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let (task_id, member_id, module) =
        binding.context("crew attempt is not active in this command session")?;
    enforce_module_policy(root, &module)?;
    let member = crate::crew::members(&tx)?
        .into_iter()
        .find(|member| member.id == member_id)
        .context("bound crew member is unavailable")?;
    let memory = crate::crew::load_member_memory_checked_from_conn(&tx, &member_id)?;
    let recent = crate::crew::recent_attempts(&tx, &member_id, 6)?;
    let mut response = serde_json::json!({
        "schema": "ctox.crew_context.v1",
        "command_id": command_id,
        "attempt_id": attempt_id,
        "task_id": task_id,
        "module_id": module,
        "member_id": member_id,
        "member_name": member.name,
        "persona": crate::crew::render_persona(&member),
        "memory_block": crate::crew::render_memory_block(&member, &memory, &recent),
    });
    response["context_version"] = Value::String(
        URL_SAFE_NO_PAD
            .encode(digest::digest(&digest::SHA256, &serde_json::to_vec(&response)?).as_ref()),
    );
    tx.commit()?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_context_restores_native_identity_and_rejects_scope_or_lease_changes(
    ) -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let root = root.path();
        let (capability, _) = store::issue_business_os_capability_token_for_managed_user(
            root,
            "crew-operator",
            "Crew operator",
            "admin",
            now_ms(),
        )?;
        for command in ["crew-parent", "foreign-parent"] {
            store::accept_rxdb_business_command_with_origin(
                root,
                serde_json::json!({
                    "id": command, "module": "outbound-lead-generation",
                    "command_type": "business_os.chat.task", "record_id": "lead-a",
                    "payload": {"instruction": "Inspect the assigned task", "mode": "data"},
                    "client_context": {"capability_token": capability}
                }),
                store::CommandOrigin::ReplicatedPeer,
            )?;
        }
        let conn = rusqlite::Connection::open(crate::paths::core_db(root))?;
        let task_id: String = conn.query_row(
            "SELECT task_id FROM business_command_task_links WHERE command_id='crew-parent'",
            [],
            |r| r.get(0),
        )?;
        conn.execute(
            "UPDATE communication_routing_state SET crew_assigned_member_id='crew-pico' WHERE message_key=?1",
            [&task_id],
        )?;
        crate::mission::channels::lease_queue_task(root, &task_id, "crew-worker")?;
        let native = crate::crew::prepare_attempt(
            root,
            &[task_id.clone()],
            "crew-worker",
            "crew-attempt",
            Some("crew-thread"),
            &serde_json::json!({}),
            None,
            "Inspect the assigned task",
            None,
        )?
        .context("native crew context missing")?;
        let session = |command: &str| -> anyhow::Result<Value> {
            let parent = crate::mission::channels::business_command_projection(root, command)?;
            let token = issue_internal_command_session_token(
                root,
                command,
                parent["payload_hash"]
                    .as_str()
                    .context("payload hash missing")?,
                "crew-operator",
                "admin",
                "crew-workspace",
                &serde_json::json!({}),
            )?;
            verify_internal_command_session_token(root, &token)
        };
        let trusted = session("crew-parent")?;
        let args = serde_json::json!({"attempt_id":"crew-attempt"});
        let call = |arguments: Value, authority: Option<&Value>| {
            call_tool_with_trusted_gateway_context(
                root,
                "business_os.get_crew_context",
                arguments,
                authority,
            )
        };
        let first = call(args.clone(), Some(&trusted))?;
        assert_eq!(first["member_id"], native.member_id);
        assert_eq!(first["persona"], native.persona);
        assert_eq!(
            first["memory_block"],
            serde_json::to_value(native.memory_block)?
        );
        assert_eq!(first, call(args.clone(), Some(&trusted))?);
        assert!(call(args.clone(), None).is_err());
        assert!(call(args.clone(), Some(&session("foreign-parent")?)).is_err());
        assert!(call(
            serde_json::json!({"attempt_id":"crew-attempt", "member_id":"crew-nori"}),
            Some(&trusted)
        )
        .is_err());
        assert!(call(serde_json::json!({"attempt_id":"unknown"}), Some(&trusted)).is_err());
        conn.execute(
            "UPDATE crew_members SET name='Pico updated' WHERE id='crew-pico'",
            [],
        )?;
        let changed = call(args.clone(), Some(&trusted))?;
        assert_ne!(first["context_version"], changed["context_version"]);
        assert_eq!(first["attempt_id"], changed["attempt_id"]);
        let policy = mcp_policy(root);
        let mut denied = policy.clone();
        denied.allowed_collections = vec!["unrelated_collection".into()];
        save_mcp_policy(root, &denied)?;
        assert!(call(args.clone(), Some(&trusted)).is_err());
        save_mcp_policy(root, &policy)?;
        conn.execute(
            "ALTER TABLE continuity_commits RENAME TO unavailable_continuity_commits",
            [],
        )?;
        assert!(
            call(args.clone(), Some(&trusted)).is_err(),
            "broken memory must not become empty memory"
        );
        conn.execute(
            "ALTER TABLE unavailable_continuity_commits RENAME TO continuity_commits",
            [],
        )?;
        assert!(call(args.clone(), Some(&trusted)).is_ok());
        conn.execute("UPDATE communication_routing_state SET lease_expires_at='2000-01-01T00:00:00Z' WHERE message_key=?1", [&task_id])?;

        assert!(call(args.clone(), Some(&trusted)).is_err());
        conn.execute(
            "UPDATE communication_routing_state SET lease_expires_at=?2 WHERE message_key=?1",
            params![
                task_id,
                (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339()
            ],
        )?;
        crate::crew::finalize_attempt(
            &conn,
            "crew-attempt",
            "failed",
            None,
            &chrono::Utc::now().to_rfc3339(),
            None,
            "Failed",
            None,
        )?;
        assert!(call(args, Some(&trusted)).is_err());
        assert!(tool_descriptors()
            .iter()
            .any(|tool| tool.name == "business_os.get_crew_context"));
        Ok(())
    }
}
