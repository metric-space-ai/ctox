//! Restore an admitted attempt's two crew context lanes through the existing
//! command-session authority. This is not an external execution admission API.
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct SessionBinding {
    pub(super) attempt_id: String,
    task_id: String,
    member_id: String,
    lease_owner: String,
    leased_at: String,
}

pub(super) fn open_read_connection(root: &Path) -> anyhow::Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open_with_flags(
        crate::paths::core_db(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    Ok(conn)
}

pub(super) fn live_binding(
    conn: &rusqlite::Connection,
    command_id: &str,
    payload_hash: &str,
    attempt_id: &str,
) -> anyhow::Result<(SessionBinding, String)> {
    conn.query_row(
        "SELECT a.task_id,a.member_id,r.lease_owner,r.leased_at,c.module FROM crew_attempts a
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
        |row| {
            Ok((
                SessionBinding {
                    attempt_id: attempt_id.to_owned(),
                    task_id: row.get(0)?,
                    member_id: row.get(1)?,
                    lease_owner: row.get(2)?,
                    leased_at: row.get(3)?,
                },
                row.get(4)?,
            ))
        },
    )
    .optional()?
    .context("crew attempt is not active in this command session")
}

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
    let expected: SessionBinding = serde_json::from_value(
        trusted
            .get("crew_binding")
            .cloned()
            .context("crew session has no attempt binding")?,
    )
    .context("crew session has no valid attempt binding")?;
    anyhow::ensure!(
        expected.attempt_id == attempt_id,
        "crew session belongs to another attempt"
    );
    let mut conn = open_read_connection(root)?;
    let tx = conn.transaction()?;
    let (current, module) = live_binding(&tx, &command_id, &payload_hash, &attempt_id)?;
    anyhow::ensure!(
        current == expected,
        "crew session lease or identity changed"
    );
    let task_id = current.task_id;
    let member_id = current.member_id;
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
        "execution_plan": read_execution_plan(&tx, &task_id, &command_id, &attempt_id)?,
    });
    response["context_version"] = Value::String(
        URL_SAFE_NO_PAD
            .encode(digest::digest(&digest::SHA256, &serde_json::to_vec(&response)?).as_ref()),
    );
    tx.commit()?;
    Ok(response)
}

fn read_execution_plan(
    conn: &rusqlite::Connection,
    task_id: &str,
    command_id: &str,
    attempt_id: &str,
) -> anyhow::Result<Option<Value>> {
    let row: Option<(i64, String, i64, String, Option<String>)> = conn
        .query_row(
            "SELECT revision,phase,percent,review_status,
         CASE WHEN length(CAST(steps_json AS BLOB))<=65536 THEN steps_json ELSE NULL END
         FROM task_execution_plan_revisions
         WHERE task_id=?1 AND command_id=?2 AND attempt_id=?3
         ORDER BY revision DESC LIMIT 1",
            params![task_id, command_id, attempt_id],
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
    row.map(|(revision, phase, percent, review_status, steps)| {
        let steps: Value =
            serde_json::from_str(&steps.context("crew execution plan exceeds the context limit")?)?;
        anyhow::ensure!(steps.is_array(), "crew execution plan steps are invalid");
        Ok(serde_json::json!({
            "revision": revision, "phase": phase, "percent": percent,
            "review_status": review_status, "steps": steps,
        }))
    })
    .transpose()
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
                    "payload": {"instruction": "Inspect the assigned task", "mode": "data",
                        "external_executor": {"executor_id":"test-codex", "harness":"codex", "timeout_seconds":5}},
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
        let write_memory = |member: &str, id: &str, statement: &str| -> anyhow::Result<()> {
            let engine = crate::crew::open_engine(root)?;
            engine.continuity_apply_diff(
                crate::crew::member_conversation_id(member),
                crate::lcm::ContinuityKind::Anchors,
                &format!("## Entries\n+ anchor_id: {id}\n+ anchor_type: hypothesis\n+ statement: {statement}\n+ learning_kind: insight\n+ source_class: crew_retrospective\n+ source_ref: context-test\n"),
            )?;
            Ok(())
        };
        write_memory(
            "crew-pico",
            "pico-first",
            "Shared knowledge from the native LCM",
        )?;
        write_memory(
            "crew-nori",
            "nori-only",
            "Foreign member knowledge must stay excluded",
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
            let token = if command == "crew-parent" {
                bind_internal_command_session_to_crew_attempt(
                    root,
                    &token,
                    "crew-attempt",
                    "crew-context-plan",
                )?
            } else {
                token
            };
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
        assert!(call(args.clone(), Some(&trusted))?["execution_plan"].is_null());
        let update = |arguments: Value, authority: Option<&Value>| {
            call_tool_with_trusted_gateway_context(
                root,
                "business_os.update_crew_plan",
                arguments,
                authority,
            )
        };
        let plan_args = serde_json::json!({"steps":[{"label":"Inspect shared knowledge","status":"in_progress"}]});
        assert!(update(plan_args.clone(), None).is_err());
        let mut override_args = plan_args.clone();
        override_args["work_key"] = serde_json::json!("foreign-plan");
        assert!(update(override_args, Some(&trusted)).is_err());
        assert!(update(serde_json::json!({"steps":[]}), Some(&trusted)).is_err());
        assert!(update(
            serde_json::json!({"steps":[{"label":"x", "status":"approved"}]}),
            Some(&trusted)
        )
        .is_err());
        assert!(update(
            serde_json::json!({"steps":[{"label":"x".repeat(70_000), "status":"pending"}]}),
            Some(&trusted)
        )
        .is_err());
        update(plan_args.clone(), Some(&trusted))?;
        assert_eq!(
            call(args.clone(), Some(&trusted))?["execution_plan"]["percent"],
            0
        );
        update(
            serde_json::json!({"steps":[{"label":"Inspect shared knowledge", "status":"completed"}]}),
            Some(&trusted),
        )?;
        let updated_plan = call(args.clone(), Some(&trusted))?;
        assert_eq!(updated_plan["execution_plan"]["percent"], 90);
        assert_eq!(updated_plan["execution_plan"]["review_status"], "pending");
        let rejected = crate::crew::open_engine(root)?.record_task_execution_plan_guarded(
            crate::lcm::TaskExecutionPlanUpdate {
                work_key: "crew-context-plan", task_id: &task_id, command_id: "crew-parent",
                attempt_id: "crew-attempt", explanation: None,
                steps: &[crate::lcm::TaskExecutionPlanStepInput {label: "Must not persist".into(), status: "pending".into()}],
            },
            |transaction| {
                transaction.execute("UPDATE communication_routing_state SET lease_owner='uncommitted-owner' WHERE message_key=?1", [&task_id])?;
                anyhow::bail!("authority changed before write")
            },
        );
        assert!(rejected.is_err());
        assert_eq!(call(args.clone(), Some(&trusted))?, updated_plan);
        let mut policy = mcp_policy(root);
        policy.allow_writes = false;
        save_mcp_policy(root, &policy)?;
        assert!(update(plan_args.clone(), Some(&trusted)).is_err());
        policy.allow_writes = true;
        save_mcp_policy(root, &policy)?;

        let set_plan = |status: &str| -> anyhow::Result<()> {
            crate::crew::open_engine(root)?.record_task_execution_plan(
                crate::lcm::TaskExecutionPlanUpdate {
                    work_key: "crew-context-plan",
                    task_id: &task_id,
                    command_id: "crew-parent",
                    attempt_id: "crew-attempt",
                    explanation: None,
                    steps: &[crate::lcm::TaskExecutionPlanStepInput {
                        label: "Inspect shared knowledge".into(),
                        status: status.into(),
                    }],
                },
            )?;
            Ok(())
        };
        set_plan("in_progress")?;
        crate::crew::open_engine(root)?.record_task_execution_plan(
            crate::lcm::TaskExecutionPlanUpdate {
                work_key: "foreign-plan",
                task_id: "foreign-task",
                command_id: "foreign-parent",
                attempt_id: "crew-attempt",
                explanation: None,
                steps: &[crate::lcm::TaskExecutionPlanStepInput {
                    label: "Foreign plan must stay excluded".into(),
                    status: "in_progress".into(),
                }],
            },
        )?;
        let first = call(args.clone(), Some(&trusted))?;
        assert_eq!(first["execution_plan"]["percent"], 0);
        assert_eq!(
            first["execution_plan"]["steps"][0]["label"],
            "Inspect shared knowledge"
        );
        assert!(!first
            .to_string()
            .contains("Foreign plan must stay excluded"));

        assert_eq!(first["member_id"], native.member_id);
        assert_eq!(first["persona"], native.persona);
        assert_eq!(
            first["memory_block"],
            serde_json::to_value(native.memory_block)?
        );
        assert_eq!(first, call(args.clone(), Some(&trusted))?);
        let initial_memory = first["memory_block"]
            .as_str()
            .context("seeded memory missing")?;
        assert!(initial_memory.contains("Shared knowledge from the native LCM"));
        assert!(!initial_memory.contains("Foreign member knowledge must stay excluded"));
        write_memory(
            "crew-pico",
            "pico-second",
            "Fresh knowledge after context restoration",
        )?;
        let refreshed = call(args.clone(), Some(&trusted))?;
        assert_eq!(first["persona"], refreshed["persona"]);
        assert_eq!(first["member_id"], refreshed["member_id"]);
        assert_ne!(first["context_version"], refreshed["context_version"]);
        assert!(refreshed["memory_block"]
            .as_str()
            .context("refreshed memory missing")?
            .contains("Fresh knowledge after context restoration"));
        assert!(!refreshed
            .to_string()
            .contains("Foreign member knowledge must stay excluded"));

        set_plan("completed")?;
        let planned = call(args.clone(), Some(&trusted))?;
        assert_eq!(planned["execution_plan"]["percent"], 90);
        assert_eq!(planned["execution_plan"]["review_status"], "pending");
        assert_ne!(planned["context_version"], refreshed["context_version"]);
        assert_eq!(planned["memory_block"], refreshed["memory_block"]);
        conn.execute(
            "UPDATE task_execution_plan_revisions SET steps_json=?1 WHERE work_key='crew-context-plan'",
            [serde_json::json!([{"label": "x".repeat(70_000), "status": "completed"}]).to_string()],
        )?;
        let oversized =
            call(args.clone(), Some(&trusted)).expect_err("oversized plan must fail closed");
        assert!(oversized.to_string().contains("context limit"));
        set_plan("completed")?;

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
        let execution_token = issue_internal_command_session_token(
            root,
            "crew-parent",
            trusted["payload_hash"].as_str().unwrap(),
            "crew-operator",
            "admin",
            "crew-workspace",
            &serde_json::json!({}),
        )?;
        let execution_token = bind_internal_command_session_to_crew_attempt(
            root,
            &execution_token,
            "crew-attempt",
            "crew-context-plan",
        )?;
        let worker_root = root.to_path_buf();
        let worker = std::thread::spawn(move || {
            crew_execution::run(
                &worker_root,
                "crew-parent",
                "Inspect the assigned task",
                Some(&execution_token),
            )
        });
        let claim_args = serde_json::json!({"command_id":"crew-parent","executor_id":"test-codex","attempt_id":"crew-attempt"});
        let mut offer = Err(anyhow::anyhow!("external Crew offer was not published"));
        for _ in 0..20 {
            offer = call_tool_with_trusted_gateway_context(
                root,
                "business_os.claim_crew_execution",
                claim_args.clone(),
                Some(&trusted),
            );
            if offer.is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let offer = match offer {
            Ok(offer) => offer,
            Err(error) => {
                let _ = worker.join();
                return Err(error);
            }
        };
        assert_eq!(offer["harness"], "codex");
        assert_eq!(offer["crew_context"]["member_id"], "crew-pico");
        let reclaimed = call_tool_with_trusted_gateway_context(
            root,
            "business_os.claim_crew_execution",
            claim_args,
            Some(&trusted),
        )?;
        assert_eq!(reclaimed["command_session"], offer["command_session"]);
        assert!(call_tool_with_trusted_gateway_context(
            root,
            "business_os.claim_crew_execution",
            serde_json::json!({"command_id":"crew-parent","executor_id":"wrong-executor","attempt_id":"crew-attempt"}),
            Some(&trusted)
        )
        .is_err());
        let execution_authority = verify_internal_command_session_token(
            root,
            offer["command_session"]
                .as_str()
                .context("claim did not return execution authority")?,
        )?;
        let candidate = serde_json::json!({"reply":"External candidate for native review."});
        for _ in 0..2 {
            let reported = call_tool_with_trusted_gateway_context(
                root,
                "business_os.report_crew_execution",
                candidate.clone(),
                Some(&execution_authority),
            )?;
            assert_eq!(reported["review_status"], "pending");
        }
        assert!(call_tool_with_trusted_gateway_context(
            root,
            "business_os.report_crew_execution",
            serde_json::json!({"reply":"Conflicting candidate"}),
            Some(&execution_authority)
        )
        .is_err());
        let reply = worker
            .join()
            .map_err(|_| anyhow::anyhow!("external Crew test worker panicked"))??;
        assert_eq!(
            reply.as_deref(),
            Some("External candidate for native review.")
        );
        let finalized: Option<String> = conn.query_row(
            "SELECT finalized_at FROM crew_attempts WHERE attempt_id='crew-attempt'",
            [],
            |r| r.get(0),
        )?;
        assert!(
            finalized.is_none(),
            "external result must leave native review/finalization to the worker"
        );

        conn.execute("UPDATE communication_routing_state SET lease_expires_at='2000-01-01T00:00:00Z' WHERE message_key=?1", [&task_id])?;

        assert!(call(args.clone(), Some(&trusted)).is_err());
        conn.execute(
            "UPDATE communication_routing_state SET lease_expires_at=?2 WHERE message_key=?1",
            params![
                task_id,
                (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339()
            ],
        )?;
        // Renewal changes only expiry and must preserve the session. Re-leasing
        // with even the same owner changes the generation and invalidates it.
        assert!(call(args.clone(), Some(&trusted)).is_ok());
        let mut claims = BusinessOsMcpInternalSessionClaims {
            schema: "ctox.business_os.mcp_command_session.v1".to_owned(),
            actor: "crew-operator".to_owned(),
            role: "admin".to_owned(),
            workspace: "crew-workspace".to_owned(),
            command_id: "crew-parent".to_owned(),
            payload_hash: trusted["payload_hash"].as_str().unwrap().to_owned(),
            allowed_actions: vec![],
            allowed_collections: vec![],
            crew_binding: Some(serde_json::from_value(trusted["crew_binding"].clone())?),
            crew_work_key: Some("crew-context-plan".to_owned()),
            issued_at_ms: now_ms(),
            expires_at_ms: now_ms() + MCP_INTERNAL_SESSION_TTL_MS,
        };
        let old_token = sign_internal_command_session_claims(root, &claims)?;
        assert!(verify_internal_command_session_token(root, &old_token).is_ok());
        conn.execute("UPDATE communication_routing_state SET leased_at='2001-01-01T00:00:00Z' WHERE message_key=?1", [&task_id])?;
        assert!(verify_internal_command_session_token(root, &old_token).is_err());
        assert!(update(plan_args.clone(), Some(&trusted)).is_err());
        assert!(call(args.clone(), Some(&trusted)).is_err());
        let renewed = session("crew-parent")?;
        assert!(call(args.clone(), Some(&renewed)).is_ok());
        claims.crew_binding = None;
        let generic_token = sign_internal_command_session_claims(root, &claims)?;
        let generic = verify_internal_command_session_token(root, &generic_token)?;
        assert!(call(args.clone(), Some(&generic)).is_err());
        assert!(bind_internal_command_session_to_crew_attempt(
            root,
            &generic_token,
            "unknown-attempt",
            "crew-context-plan"
        )
        .is_err());
        conn.execute("UPDATE communication_routing_state SET lease_owner='another-worker' WHERE message_key=?1", [&task_id])?;
        assert!(call(args.clone(), Some(&renewed)).is_err());
        let final_authority = session("crew-parent")?;
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
        assert!(call(args, Some(&final_authority)).is_err());
        assert!(update(plan_args, Some(&final_authority)).is_err());
        assert!(tool_descriptors()
            .iter()
            .any(|tool| tool.name == "business_os.get_crew_context"));
        Ok(())
    }
}
