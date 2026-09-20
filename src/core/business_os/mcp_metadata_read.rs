//! Exact-selector metadata access for an admitted delegated command.
//! The worker receives row presence only, never values or arbitrary metadata.
use super::*;

const SCHEMA: &str = "ctox.worker.credential-presence.v1";
pub(super) const TOOL: &str = "business_os.read_credential_presence";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct CredentialSelector {
    scope: String,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadContract {
    schema: String,
    credentials: Vec<CredentialSelector>,
}

impl ReadContract {
    pub(super) fn parse(value: &Value) -> anyhow::Result<Self> {
        let contract: Self = serde_json::from_value(value.clone())
            .context("invalid credential presence read contract")?;
        anyhow::ensure!(
            contract.schema == SCHEMA,
            "unsupported metadata read schema"
        );
        anyhow::ensure!(
            (1..=8).contains(&contract.credentials.len()),
            "metadata read requires 1 to 8 exact selectors"
        );
        let mut seen = BTreeSet::new();
        for selector in &contract.credentials {
            for value in [&selector.scope, &selector.name] {
                anyhow::ensure!(
                    !value.is_empty()
                        && value.len() <= 256
                        && value.trim() == value
                        && !value
                            .chars()
                            .any(|c| c.is_control() || matches!(c, '*' | '?')),
                    "metadata selector must be an exact non-empty scope/name"
                );
            }
            anyhow::ensure!(
                seen.insert((&selector.scope, &selector.name)),
                "duplicate metadata selector"
            );
        }
        Ok(contract)
    }
}

pub(super) fn authorize(root: &Path, actor: &str, role: &str) -> anyhow::Result<()> {
    let decision = store::trusted_mcp_actor_policy_decision_with_role(
        root,
        actor,
        role,
        BusinessOsPermission::SecretsManage,
        BusinessOsScopeType::Workspace,
        None,
    )?;
    anyhow::ensure!(
        decision.allowed,
        "credential metadata access is denied by native policy"
    );
    Ok(())
}

pub(super) fn validate_admission(
    root: &Path,
    command_id: &str,
    payload_hash: &str,
    actor: &str,
    role: &str,
    contract: &ReadContract,
) -> anyhow::Result<()> {
    let command = crate::mission::channels::business_command_projection(root, command_id)?;
    anyhow::ensure!(
        command.get("module").and_then(Value::as_str) == Some("credentials"),
        "credential presence requires credentials-module admission"
    );
    anyhow::ensure!(
        command.get("execution_phase").and_then(Value::as_str) != Some("terminal"),
        "metadata command is already terminal"
    );
    anyhow::ensure!(
        command.get("command_type").and_then(Value::as_str) == Some("ctox.delegate_task"),
        "metadata reads require a delegated command"
    );
    anyhow::ensure!(
        command.get("payload_hash").and_then(Value::as_str) == Some(payload_hash),
        "metadata command payload changed"
    );
    let declared = ReadContract::parse(
        command
            .pointer("/payload/input/metadata_read_contract")
            .context("delegated command has no metadata read contract")?,
    )?;
    anyhow::ensure!(
        &declared == contract,
        "metadata selectors differ from admitted command"
    );
    let authorization =
        store::revalidate_business_command_execution_authorization(root, command_id)?;
    anyhow::ensure!(
        authorization.pointer("/actor/id").and_then(Value::as_str) == Some(actor)
            && authorization
                .pointer("/actor/role")
                .and_then(Value::as_str)
                .map(normalize_role)
                .as_deref()
                == Some(role),
        "metadata command actor changed"
    );
    authorize(root, actor, role)
}

pub(super) fn read(
    root: &Path,
    trusted: Option<&Value>,
    arguments: &Value,
) -> anyhow::Result<Value> {
    let context = trusted
        .filter(|value| {
            string_field(value, "auth_source").as_deref() == Some(MCP_INTERNAL_SESSION_AUTH_SOURCE)
        })
        .context("credential presence requires an authenticated command session")?;
    anyhow::ensure!(
        arguments.as_object().is_some_and(|args| args.is_empty()),
        "credential presence takes no caller-selected arguments"
    );
    let contract = ReadContract::parse(
        context
            .get("metadata_read_contract")
            .context("command has no credential presence grant")?,
    )?;
    let command_id = required_arg(context, "command_id")?;
    let payload_hash = required_arg(context, "payload_hash")?;
    let actor = required_arg(context, "actor")?;
    let role = normalize_role(&required_arg(context, "role")?);
    let expires_at = context
        .get("expires_at_ms")
        .and_then(Value::as_i64)
        .context("metadata command session has no expiry")?;
    anyhow::ensure!(now_ms() < expires_at, "metadata command session expired");
    validate_admission(root, &command_id, &payload_hash, &actor, &role, &contract)?;
    let mut entries = Vec::with_capacity(contract.credentials.len());
    for selector in &contract.credentials {
        let present = crate::secrets::secret_exists(root, &selector.scope, &selector.name)?;
        entries.push(
            serde_json::json!({"scope": selector.scope, "name": selector.name, "present": present}),
        );
    }
    // Revalidate after store access. Store failure remains an error, never an
    // absent/readiness result. A present row does not prove provider readiness.
    validate_admission(root, &command_id, &payload_hash, &actor, &role, &contract)?;
    anyhow::ensure!(now_ms() < expires_at, "metadata command session expired");
    Ok(
        serde_json::json!({"schema": SCHEMA, "entries": entries, "provider_readiness": "not_checked"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admitted_metadata_session(root: &Path, role: &str) -> anyhow::Result<(String, Value)> {
        let contract = serde_json::json!({
            "schema": SCHEMA,
            "credentials": [
                {"scope": "credentials", "name": "TEST_API_KEY"},
                {"scope": "account:other", "name": "TEST_API_KEY"}
            ]
        });
        let (capability, _) = store::issue_business_os_capability_token_for_managed_user(
            root,
            "metadata-operator",
            "Metadata operator",
            role,
            now_ms(),
        )?;
        let accepted = store::accept_rxdb_business_command_with_origin(
            root,
            serde_json::json!({
                "id": "metadata-command", "command_id": "metadata-command", "module": "credentials",
                "command_type": "ctox.delegate_task",
                "payload": {"title": "Credential presence", "objective": "Report only row presence", "input": {"metadata_read_contract": contract}},
                "client_context": {"capability_token": capability, "actor": {"id": "forged", "role": "chef"}}
            }),
            store::CommandOrigin::ReplicatedPeer,
        )?;
        anyhow::ensure!(
            accepted["status"] == "accepted",
            "metadata command was not admitted"
        );
        let canonical =
            crate::mission::channels::business_command_projection(root, "metadata-command")?;
        let token = issue_internal_command_session_token(
            root,
            "metadata-command",
            canonical["payload_hash"].as_str().context("payload hash")?,
            "metadata-operator",
            role,
            "workspace:metadata",
            &serde_json::json!({"metadata_read_contract": contract}),
        )?;
        let trusted = verify_internal_command_session_token(root, &token)?;
        Ok((token, trusted))
    }

    #[test]
    fn metadata_session_reads_exact_presence_and_rejects_widening_and_terminal_reuse(
    ) -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        crate::secrets::write_secret_record(
            root,
            "credentials",
            "TEST_API_KEY",
            "PRIVATE_CANARY_VALUE",
            Some("PRIVATE_CANARY_DESCRIPTION".to_string()),
            serde_json::json!({"private": "PRIVATE_CANARY_METADATA"}),
        )?;
        let (token, trusted) = admitted_metadata_session(root, "admin")?;
        let result = read(root, Some(&trusted), &serde_json::json!({}))?;
        assert_eq!(result["entries"][0]["present"], true);
        assert_eq!(result["entries"][1]["present"], false);
        assert_eq!(result["provider_readiness"], "not_checked");
        assert!(!result.to_string().contains("PRIVATE_CANARY"));
        assert!(read(root, Some(&trusted), &serde_json::json!({"scope": "*"})).is_err());
        for tool in [
            "business_os.query_records",
            "business_os.execute_action",
            "business_os.list_modules",
        ] {
            assert!(enforce_internal_command_session_scope(
                tool,
                &serde_json::json!({}),
                Some(&trusted)
            )
            .is_err());
        }
        let mut expired = trusted.clone();
        expired["expires_at_ms"] = Value::from(0);
        assert!(read(root, Some(&expired), &serde_json::json!({})).is_err());
        let mut widened = trusted.clone();
        widened["metadata_read_contract"]["credentials"][0]["name"] = Value::from("UNGRANTED_KEY");
        assert!(read(root, Some(&widened), &serde_json::json!({})).is_err());
        let mut changed_actor = trusted.clone();
        changed_actor["actor"] = Value::from("another-actor");
        assert!(read(root, Some(&changed_actor), &serde_json::json!({})).is_err());
        store::mark_business_command_failed(
            root,
            "metadata-command",
            "test terminal transition",
            now_ms(),
        )?;
        assert!(verify_internal_command_session_token(root, &token).is_err());
        assert!(read(root, Some(&trusted), &serde_json::json!({})).is_err());
        Ok(())
    }

    #[test]
    fn metadata_session_requires_native_credential_permission() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        assert!(admitted_metadata_session(temp.path(), "user").is_err());
        Ok(())
    }

    #[test]
    fn metadata_session_rejects_actor_role_revocation() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let (token, trusted) = admitted_metadata_session(root, "admin")?;
        store::issue_business_os_capability_token_for_managed_user(
            root,
            "owner",
            "Owner",
            "admin",
            now_ms(),
        )?;
        let owner = store::BusinessOsSession {
            ok: true,
            authenticated: true,
            auth_required: false,
            user: Some(store::BusinessOsSessionUser {
                id: "owner".to_string(),
                display_name: "Owner".to_string(),
                role: "admin".to_string(),
                is_admin: true,
            }),
            login_url: None,
            reason: None,
        };
        store::upsert_user(
            root,
            &owner,
            store::BusinessOsUserMutation {
                id: "metadata-operator".to_string(),
                display_name: "Metadata operator".to_string(),
                role: "user".to_string(),
                active: true,
                profile: None,
                accept_recovery_responsibility: false,
            },
        )?;
        assert!(verify_internal_command_session_token(root, &token).is_err());
        assert!(read(root, Some(&trusted), &serde_json::json!({})).is_err());
        Ok(())
    }

    #[test]
    fn metadata_contract_rejects_broad_or_malformed_selectors() {
        for value in [
            serde_json::json!({"schema": SCHEMA, "credentials": []}),
            serde_json::json!({"schema": SCHEMA, "credentials": [{"scope": "*", "name": "KEY"}]}),
            serde_json::json!({"schema": SCHEMA, "credentials": [{"scope": "credentials", "name": " "}]}),
            serde_json::json!({"schema": SCHEMA, "credentials": [{"scope": "credentials", "name": "KEY", "value": "forbidden"}]}),
            serde_json::json!({"schema": SCHEMA, "credentials": [{"scope": "credentials", "name": "KEY"}], "allowed_actions": []}),
        ] {
            assert!(ReadContract::parse(&value).is_err());
        }
    }

    #[test]
    fn metadata_tool_rejects_untrusted_call_before_store_access() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("absent");
        assert!(read(&root, None, &serde_json::json!({})).is_err());
        assert!(!root.exists());
    }
}
