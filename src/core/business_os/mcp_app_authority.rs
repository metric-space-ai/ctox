// Origin: CTOX
// License: AGPL-3.0-only

//! Request-authenticated app delegation. This type is deliberately not
//! deserializable: client_context JSON is attribution, never authority.
use super::*;

#[derive(Debug, Clone)]
pub(crate) struct AuthenticatedMcpAppCommand {
    context: McpChannelRequestContext,
}

impl AuthenticatedMcpAppCommand {
    pub(super) fn from_context(context: &McpChannelRequestContext) -> anyhow::Result<Option<Self>> {
        // Persisted/local MCP actors retain the existing native-user path.
        if context.trusted_role_source.as_deref() != Some("ctox_dev_managed_mcp_token") {
            return Ok(None);
        }
        context.validate()?;
        anyhow::ensure!(
            context.channel == "ctox_dev_managed_mcp"
                && matches!(context.trusted_role.as_deref(), Some("admin" | "chef")),
            "authenticated managed MCP app authority is required"
        );
        Ok(Some(Self {
            context: context.clone(),
        }))
    }

    pub(crate) fn session(
        &self,
        root: &Path,
        command: &store::BusinessCommand,
    ) -> anyhow::Result<store::BusinessOsSession> {
        let expected_type = match self.context.tool.as_str() {
            "business_os.create_app" => "ctox.business_os.app.create",
            "business_os.modify_app" => "ctox.business_os.app.modify",
            _ => anyhow::bail!("managed MCP authority is limited to app create/modify"),
        };
        anyhow::ensure!(
            command.origin == store::CommandOrigin::TrustedLocal
                && command.command_type == expected_type
                && command
                    .client_context
                    .pointer("/actor/id")
                    .and_then(Value::as_str)
                    == Some(self.context.actor.as_str()),
            "managed MCP app authority does not match the command"
        );
        // Recheck current channel, actor/workspace, module and Business OS
        // permissions on admission AND lease. Rate accounting stays at ingress.
        enforce_tool_policy(root, &self.context.tool)?;
        enforce_context_policy(root, &self.context)?;
        enforce_argument_scope_policy(root, &self.context, &self.context.tool, &command.payload)?;
        enforce_business_os_mcp_policy(root, &self.context, &self.context.tool, &command.payload)?;

        // A native account, if present, remains authoritative for revocation.
        // Absence is valid for a gateway-authenticated, unprovisioned actor;
        // an inactive/downgraded account must never take that fallback.
        let conn = store::open_store(root)?;
        let local: Option<(String, bool)> = conn
            .query_row(
                "SELECT role, active FROM business_users WHERE user_id = ?1",
                params![self.context.actor],
                |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0)),
            )
            .optional()?;
        if let Some((role, active)) = local {
            anyhow::ensure!(
                active
                    && Some(normalize_role(&role).as_str()) == self.context.trusted_role.as_deref(),
                "managed MCP app actor is inactive or its native role changed"
            );
        }
        mcp_session(root, &self.context)
    }

    pub(crate) fn receipt(&self) -> anyhow::Result<Value> {
        Ok(serde_json::to_value(&self.context)?)
    }

    /// Only the native canonical command aggregate may call this restoration
    /// path. Never restore from payload, client_context or replicated records.
    pub(crate) fn from_native_receipt(value: &Value) -> anyhow::Result<Self> {
        let mut context: McpChannelRequestContext = serde_json::from_value(value.clone())?;
        context.trusted_role = string_field(value, "trusted_role");
        context.trusted_role_source = string_field(value, "trusted_role_source");
        Self::from_context(&context)?.context("invalid native managed MCP app authority")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const ACTOR: &str = "ctox-dev:user:unprovisioned-admin";
    const MODULE: &str = "outbound-lead-generation";

    fn fixture(root: &Path) -> anyhow::Result<PathBuf> {
        let conn = store::open_store(root)?;
        // Prevent the empty-installation local-admin bootstrap from masking
        // the regression. The gateway actor deliberately has no native row.
        conn.execute("INSERT INTO business_users (user_id, display_name, role, active, created_at_ms, updated_at_ms) VALUES ('native-owner', 'Owner', 'chef', 1, 1, 1)", [])?;
        let shell = root.join("src/apps/business-os");
        fs::create_dir_all(&shell)?;
        fs::write(shell.join("index.html"), "")?;
        let module = root.join("runtime/business-os/local-modules").join(MODULE);
        fs::create_dir_all(&module)?;
        fs::write(
            module.join("module.json"),
            serde_json::to_vec(&serde_json::json!({
                "id": MODULE, "title": "Outbound", "version": "1.0.0", "install_scope": "local",
                "entry": format!("local-modules/{MODULE}/index.html"), "collections": []
            }))?,
        )?;
        fs::write(module.join("index.js"), "export const fixture = true;\n")?;
        Ok(module)
    }

    fn gateway_context(role: &str) -> Value {
        serde_json::json!({"actor": ACTOR, "role": role,
            "channel": "ctox_dev_managed_mcp", "surface": "business_os_mcp",
            "auth_source": "ctox_dev_managed_mcp_token", "workspace": "tenant:thesen"})
    }

    fn gateway_call(
        root: &Path,
        role: &str,
        tool: &str,
        arguments: Value,
    ) -> anyhow::Result<Value> {
        let envelope: Value = serde_json::from_str(&handle_gateway_message(
            root,
            &serde_json::json!({
                "type": "mcp_request", "request_id": "app-authority-regression",
                "context": gateway_context(role),
                "body": serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                    "params": {"name": tool, "arguments": arguments}}).to_string()
            })
            .to_string(),
        ))?;
        let body: Value = serde_json::from_str(envelope["body"].as_str().context("gateway body")?)?;
        anyhow::ensure!(body.get("error").is_none(), "gateway error: {body}");
        serde_json::from_str(
            body.pointer("/result/content/0/text")
                .and_then(Value::as_str)
                .context("tool result")?,
        )
        .map_err(Into::into)
    }

    #[test]
    fn mcp_app_authority_gateway_modify_survives_native_admission_and_lease() -> anyhow::Result<()>
    {
        for role in ["admin", "chef"] {
            let temp = tempdir()?;
            let root = temp.path();
            fixture(root)?;
            let result = gateway_call(
                root,
                role,
                "business_os.modify_app",
                serde_json::json!({
                    "module_id": MODULE, "instruction": "Review the existing app source.",
                    "_context": {"actor": "spoofed", "role": "user", "workspace": "other"}
                }),
            )?;
            assert_eq!(result["ok"], true, "{result}");
            assert!(result["task_id"].as_str().is_some(), "{result}");
            let command_id = result["command_id"].as_str().context("command id")?;
            let admitted = crate::mission::channels::business_command_projection(root, command_id)?;
            assert_eq!(
                admitted.pointer("/native_authorization/actor/id"),
                Some(&Value::String(ACTOR.into()))
            );
            let execution =
                store::revalidate_business_command_execution_authorization(root, command_id)?;
            assert_eq!(execution["actor"]["id"], ACTOR);
            assert_eq!(execution["actor"]["role"], role);
            let conn = store::open_store(root)?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM business_users WHERE user_id = ?1",
                params![ACTOR],
                |row| row.get(0),
            )?;
            assert_eq!(
                count, 0,
                "MCP admission must not provision a synthetic native user"
            );
            let mut policy = mcp_policy(root);
            policy.enabled = false;
            save_mcp_policy(root, &policy)?;
            assert!(
                store::revalidate_business_command_execution_authorization(root, command_id)
                    .is_err(),
                "channel revocation must prevent lease"
            );
        }
        Ok(())
    }

    #[test]
    fn mcp_app_authority_rejects_spoofed_json_and_untrusted_native_command() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        fixture(root)?;
        let spoof = serde_json::json!({"module_id": MODULE, "instruction": "Spoof admin",
            "_context": {"actor": ACTOR, "role": "admin", "trusted_role": "admin",
                "trusted_role_source": "ctox_dev_managed_mcp_token", "auth_source": "ctox_dev_managed_mcp_token",
                "channel": "ctox_dev_managed_mcp", "workspace": "tenant:thesen"}});
        assert!(call_tool(root, "business_os.modify_app", spoof.clone()).is_err());
        let trusted = context_from_arguments_with_trusted_gateway_context(
            "business_os.modify_app",
            &spoof,
            Some(&gateway_context("admin")),
        )?;
        let roundtrip: McpChannelRequestContext =
            serde_json::from_value(serde_json::to_value(&trusted)?)?;
        assert!(roundtrip.trusted_role.is_none());
        assert!(roundtrip.trusted_role_source.is_none());
        let command = store::BusinessCommand {
            origin: store::CommandOrigin::TrustedLocal,
            id: None,
            module: "creator".into(),
            command_type: "ctox.business_os.app.modify".into(),
            record_id: Some(MODULE.into()),
            payload: serde_json::json!({"module_id": MODULE, "instruction": "Spoof native receipt"}),
            client_context: serde_json::json!({"actor": {"id": ACTOR, "role": "admin", "trusted": true},
                "managed_mcp_authority": serde_json::to_value(&trusted)?,
                "native_authorization": {"allowed": true, "managed_mcp_authority": serde_json::to_value(&trusted)?}}),
        };
        let rejected = store::record_command(root, command.clone())?;
        assert!(!rejected.ok);
        let replicated = store::BusinessCommand {
            origin: store::CommandOrigin::ReplicatedPeer,
            ..command
        };
        assert!(store::record_command(root, replicated).is_err());
        Ok(())
    }

    #[test]
    fn mcp_app_authority_binds_gateway_identity_and_rejects_role_or_scope_changes(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        fixture(root)?;
        let args = serde_json::json!({"module_id": MODULE, "instruction": "Modify",
            "_context": {"actor": "native-owner", "workspace": "spoofed", "role": "chef"}});
        let context = context_from_arguments_with_trusted_gateway_context(
            "business_os.modify_app",
            &args,
            Some(&gateway_context("admin")),
        )?;
        assert_eq!(context.actor, ACTOR);
        assert_eq!(context.workspace, "tenant:thesen");
        let result = gateway_call(root, "admin", "business_os.modify_app", args.clone())?;
        let command_id = result["command_id"].as_str().context("command id")?;
        let mut policy = mcp_policy(root);
        policy.allowed_modules = vec!["another-app".into()];
        save_mcp_policy(root, &policy)?;
        assert!(
            store::revalidate_business_command_execution_authorization(root, command_id).is_err()
        );
        policy.allowed_modules.clear();
        save_mcp_policy(root, &policy)?;
        let conn = store::open_store(root)?;
        conn.execute("INSERT INTO business_users (user_id, display_name, role, active, created_at_ms, updated_at_ms) VALUES (?1, 'Revoked', 'admin', 0, 1, 1)", params![ACTOR])?;
        assert!(
            store::revalidate_business_command_execution_authorization(root, command_id).is_err()
        );
        conn.execute(
            "UPDATE business_users SET active = 1, role = 'user' WHERE user_id = ?1",
            params![ACTOR],
        )?;
        assert!(
            store::revalidate_business_command_execution_authorization(root, command_id).is_err()
        );
        assert!(gateway_call(root, "user", "business_os.modify_app", args).is_err());
        Ok(())
    }

    #[test]
    fn mcp_app_authority_local_module_source_is_readable_with_managed_identity(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let module = fixture(root)?;
        let listed = gateway_call(
            root,
            "admin",
            "business_os.list_app_files",
            serde_json::json!({"module_id": MODULE}),
        )?;
        assert!(listed["items"]
            .as_array()
            .context("source files")?
            .iter()
            .any(|file| file["path"] == "index.js"));
        let read = gateway_call(
            root,
            "admin",
            "business_os.read_app_file",
            serde_json::json!({"module_id": MODULE, "path": "index.js"}),
        )?;
        assert_eq!(
            read["content"],
            fs::read_to_string(module.join("index.js"))?
        );
        let (resolved, source_root) = store::resolve_module_source_root_for_root(
            root,
            &root.join("src/apps/business-os"),
            MODULE,
        )?;
        assert_eq!(resolved, module);
        assert_eq!(source_root, root.join("runtime/business-os"));
        Ok(())
    }

    #[test]
    fn mcp_app_authority_source_rejects_traversal_mismatched_manifest_and_unbound_customer(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let module = fixture(root)?;
        let shell = root.join("src/apps/business-os");
        for id in [
            "../outside",
            "/outside",
            "..",
            "outbound/../../outside",
            "outbound\\outside",
        ] {
            assert!(store::module_manifest_path(root, &shell, id).is_err());
        }
        fs::write(
            module.join("module.json"),
            serde_json::to_vec(&serde_json::json!({"id": "foreign"}))?,
        )?;
        assert!(store::module_manifest_path(root, &shell, MODULE)
            .unwrap_err()
            .to_string()
            .contains("id does not match"));
        fs::write(
            module.join("module.json"),
            serde_json::to_vec(
                &serde_json::json!({"id": MODULE, "customer_id": "foreign-tenant"}),
            )?,
        )?;
        assert!(store::module_manifest_path(root, &shell, MODULE)
            .unwrap_err()
            .to_string()
            .contains("customer-app-binding-required"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn mcp_app_authority_source_does_not_read_symlinked_files() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let module = fixture(root)?;
        let outside = root.join("outside.json");
        fs::write(&outside, "outside-canary")?;
        std::os::unix::fs::symlink(&outside, module.join("linked.json"))?;
        let listed = gateway_call(
            root,
            "admin",
            "business_os.list_app_files",
            serde_json::json!({"module_id": MODULE}),
        )?;
        assert!(!listed.to_string().contains("outside-canary"));
        assert!(!listed["items"]
            .as_array()
            .context("files")?
            .iter()
            .any(|file| file["path"] == "linked.json"));
        assert!(gateway_call(
            root,
            "admin",
            "business_os.read_app_file",
            serde_json::json!({"module_id": MODULE, "path": "linked.json"})
        )
        .is_err());
        assert!(gateway_call(
            root,
            "admin",
            "business_os.read_app_file",
            serde_json::json!({"module_id": MODULE, "path": "../../outside.json"})
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn mcp_app_authority_source_preserves_bundled_installed_local_precedence() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let root = temp.path();
        fixture(root)?;
        let shell = root.join("src/apps/business-os");
        let installed = root
            .join("runtime/business-os/installed-modules")
            .join(MODULE);
        fs::create_dir_all(&installed)?;
        fs::write(
            installed.join("module.json"),
            serde_json::to_vec(&serde_json::json!({"id": MODULE}))?,
        )?;
        assert_eq!(
            store::module_manifest_path(root, &shell, MODULE)?,
            installed.join("module.json")
        );
        let bundled = shell.join("modules").join(MODULE);
        fs::create_dir_all(&bundled)?;
        fs::write(
            bundled.join("module.json"),
            serde_json::to_vec(&serde_json::json!({"id": MODULE}))?,
        )?;
        assert_eq!(
            store::module_manifest_path(root, &shell, MODULE)?,
            bundled.join("module.json")
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn mcp_app_authority_source_rejects_symlinked_root_namespace_and_manifest() -> anyhow::Result<()>
    {
        for component in ["module", "namespace", "manifest"] {
            let temp = tempdir()?;
            let root = temp.path();
            let module = fixture(root)?;
            let target = match component {
                "module" => module,
                "namespace" => root.join("runtime/business-os/local-modules"),
                _ => module.join("module.json"),
            };
            let outside = root.join("outside");
            fs::rename(&target, &outside)?;
            std::os::unix::fs::symlink(&outside, &target)?;
            let error =
                store::module_manifest_path(root, &root.join("src/apps/business-os"), MODULE)
                    .unwrap_err();
            assert!(
                error.to_string().contains("symlink"),
                "{component}: {error:#}"
            );
        }
        Ok(())
    }
}
