use crate::features::Feature;
use crate::function_tool::FunctionCallError;
use crate::is_safe_command::is_known_safe_command;
use crate::protocol::EventMsg;
use crate::protocol::TerminalInteractionEvent;
use crate::sandboxing::SandboxPermissions;
use crate::shell::Shell;
use crate::shell::get_shell_by_model_provided_path;
use crate::skills::maybe_emit_implicit_skill_invocation;
use crate::tools::context::ExecCommandToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::apply_granted_turn_permissions;
use crate::tools::handlers::apply_patch::intercept_apply_patch;
use crate::tools::handlers::implicit_granted_permissions;
use crate::tools::handlers::normalize_and_validate_additional_permissions;
use crate::tools::handlers::parse_arguments;
use crate::tools::handlers::parse_arguments_with_base_path;
use crate::tools::handlers::resolve_workdir_base_path;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use crate::tools::spec::UnifiedExecShellMode;
use crate::unified_exec::ExecCommandRequest;
use crate::unified_exec::UnifiedExecContext;
use crate::unified_exec::UnifiedExecProcessManager;
use crate::unified_exec::WriteStdinRequest;
use async_trait::async_trait;
use ctox_protocol::models::PermissionProfile;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct UnifiedExecHandler;

#[derive(Debug, Deserialize)]
pub(crate) struct ExecCommandArgs {
    cmd: String,
    #[serde(default)]
    pub(crate) workdir: Option<String>,
    #[serde(default)]
    shell: Option<String>,
    #[serde(default)]
    login: Option<bool>,
    #[serde(default = "default_tty")]
    tty: bool,
    #[serde(default = "default_exec_yield_time_ms")]
    yield_time_ms: u64,
    #[serde(default)]
    max_output_tokens: Option<usize>,
    #[serde(default)]
    sandbox_permissions: SandboxPermissions,
    #[serde(default)]
    additional_permissions: Option<PermissionProfile>,
    #[serde(default)]
    justification: Option<String>,
    #[serde(default)]
    prefix_rule: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct WriteStdinArgs {
    // The model is trained on `session_id`.
    session_id: i32,
    #[serde(default)]
    chars: String,
    #[serde(default = "default_write_stdin_yield_time_ms")]
    yield_time_ms: u64,
    #[serde(default)]
    max_output_tokens: Option<usize>,
}

fn default_exec_yield_time_ms() -> u64 {
    10_000
}

fn default_write_stdin_yield_time_ms() -> u64 {
    250
}

fn default_tty() -> bool {
    false
}

#[async_trait]
impl ToolHandler for UnifiedExecHandler {
    type Output = ExecCommandToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn is_mutating(&self, invocation: &ToolInvocation) -> bool {
        let ToolPayload::Function { arguments } = &invocation.payload else {
            tracing::error!(
                "This should never happen, invocation payload is wrong: {:?}",
                invocation.payload
            );
            return true;
        };

        let Ok(params) = serde_json::from_str::<ExecCommandArgs>(arguments) else {
            return true;
        };
        let command = match get_command(
            &params,
            invocation.session.user_shell(),
            &invocation.turn.tools_config.unified_exec_shell_mode,
            invocation.turn.tools_config.allow_login_shell,
        ) {
            Ok(command) => command,
            Err(_) => return true,
        };
        !is_known_safe_command(&command)
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            tracker,
            call_id,
            tool_name,
            payload,
            ..
        } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "unified_exec handler received unsupported payload".to_string(),
                ));
            }
        };

        let manager: &UnifiedExecProcessManager = &session.services.unified_exec_manager;
        let context = UnifiedExecContext::new(session.clone(), turn.clone(), call_id.clone());

        let response = match tool_name.as_str() {
            "exec_command" => {
                let cwd = resolve_workdir_base_path(&arguments, context.turn.cwd.as_path())?;
                let args: ExecCommandArgs =
                    parse_arguments_with_base_path(&arguments, cwd.as_path())?;
                let raw_cmd_for_guard = args.cmd.clone();
                maybe_emit_implicit_skill_invocation(
                    session.as_ref(),
                    turn.as_ref(),
                    &args.cmd,
                    args.workdir.as_deref(),
                )
                .await;
                let process_id = manager.allocate_process_id().await;
                let command = get_command(
                    &args,
                    session.user_shell(),
                    &turn.tools_config.unified_exec_shell_mode,
                    turn.tools_config.allow_login_shell,
                )
                .map_err(FunctionCallError::RespondToModel)?;
                let command_for_display = ctox_shell_command::parse_command::shlex_join(&command);
                if let Some(message) =
                    business_os_app_root_artifact_write_guard(&raw_cmd_for_guard, &cwd)
                {
                    manager.release_process_id(process_id).await;
                    return Err(FunctionCallError::RespondToModel(message));
                }

                let ExecCommandArgs {
                    workdir,
                    tty,
                    yield_time_ms,
                    max_output_tokens,
                    sandbox_permissions,
                    additional_permissions,
                    justification,
                    prefix_rule,
                    ..
                } = args;

                let exec_permission_approvals_enabled =
                    session.features().enabled(Feature::ExecPermissionApprovals);
                let requested_additional_permissions = additional_permissions.clone();
                let effective_additional_permissions = apply_granted_turn_permissions(
                    context.session.as_ref(),
                    sandbox_permissions,
                    additional_permissions,
                )
                .await;
                let additional_permissions_allowed = exec_permission_approvals_enabled
                    || (session.features().enabled(Feature::RequestPermissionsTool)
                        && effective_additional_permissions.permissions_preapproved);

                // Sticky turn permissions have already been approved, so they should
                // continue through the normal exec approval flow for the command.
                if effective_additional_permissions
                    .sandbox_permissions
                    .requests_sandbox_override()
                    && !effective_additional_permissions.permissions_preapproved
                    && !matches!(
                        context.turn.approval_policy.value(),
                        ctox_protocol::protocol::AskForApproval::OnRequest
                    )
                {
                    let approval_policy = context.turn.approval_policy.value();
                    manager.release_process_id(process_id).await;
                    return Err(FunctionCallError::RespondToModel(format!(
                        "approval policy is {approval_policy:?}; reject command — you cannot ask for escalated permissions if the approval policy is {approval_policy:?}"
                    )));
                }

                let workdir = workdir.filter(|value| !value.is_empty());

                let workdir = workdir.map(|dir| context.turn.resolve_path(Some(dir)));
                let cwd = workdir.clone().unwrap_or(cwd);
                let root_artifact_snapshot = business_os_app_root_artifact_snapshot(&cwd);
                let normalized_additional_permissions = match implicit_granted_permissions(
                    sandbox_permissions,
                    requested_additional_permissions.as_ref(),
                    &effective_additional_permissions,
                )
                .map_or_else(
                    || {
                        normalize_and_validate_additional_permissions(
                            additional_permissions_allowed,
                            context.turn.approval_policy.value(),
                            effective_additional_permissions.sandbox_permissions,
                            effective_additional_permissions.additional_permissions,
                            effective_additional_permissions.permissions_preapproved,
                            &cwd,
                        )
                    },
                    |permissions| Ok(Some(permissions)),
                ) {
                    Ok(normalized) => normalized,
                    Err(err) => {
                        manager.release_process_id(process_id).await;
                        return Err(FunctionCallError::RespondToModel(err));
                    }
                };

                if let Some(output) = intercept_apply_patch(
                    &command,
                    &cwd,
                    Some(yield_time_ms),
                    context.session.clone(),
                    context.turn.clone(),
                    Some(&tracker),
                    &context.call_id,
                    tool_name.as_str(),
                )
                .await?
                {
                    let cleanup_message =
                        cleanup_new_business_os_app_root_artifacts(root_artifact_snapshot.as_ref());
                    manager.release_process_id(process_id).await;
                    if let Some(message) = cleanup_message {
                        return Err(FunctionCallError::RespondToModel(message));
                    }
                    return Ok(ExecCommandToolOutput {
                        event_call_id: String::new(),
                        chunk_id: String::new(),
                        wall_time: std::time::Duration::ZERO,
                        raw_output: output.into_text().into_bytes(),
                        max_output_tokens: None,
                        process_id: None,
                        exit_code: None,
                        original_token_count: None,
                        session_command: None,
                    });
                }

                let exec_result = manager
                    .exec_command(
                        ExecCommandRequest {
                            command,
                            process_id,
                            yield_time_ms,
                            max_output_tokens,
                            workdir,
                            network: context.turn.network.clone(),
                            tty,
                            sandbox_permissions: effective_additional_permissions
                                .sandbox_permissions,
                            additional_permissions: normalized_additional_permissions,
                            additional_permissions_preapproved: effective_additional_permissions
                                .permissions_preapproved,
                            justification,
                            prefix_rule,
                        },
                        &context,
                    )
                    .await;
                if let Some(message) =
                    cleanup_new_business_os_app_root_artifacts(root_artifact_snapshot.as_ref())
                {
                    return Err(FunctionCallError::RespondToModel(message));
                }
                exec_result.map_err(|err| {
                    FunctionCallError::RespondToModel(format!(
                        "exec_command failed for `{command_for_display}`: {err:?}"
                    ))
                })?
            }
            "write_stdin" => {
                let args: WriteStdinArgs = parse_arguments(&arguments)?;
                let response = manager
                    .write_stdin(WriteStdinRequest {
                        process_id: args.session_id,
                        input: &args.chars,
                        yield_time_ms: args.yield_time_ms,
                        max_output_tokens: args.max_output_tokens,
                    })
                    .await
                    .map_err(|err| {
                        FunctionCallError::RespondToModel(format!("write_stdin failed: {err}"))
                    })?;

                let interaction = TerminalInteractionEvent {
                    call_id: response.event_call_id.clone(),
                    process_id: args.session_id.to_string(),
                    stdin: args.chars.clone(),
                };
                session
                    .send_event(turn.as_ref(), EventMsg::TerminalInteraction(interaction))
                    .await;

                response
            }
            other => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "unsupported unified exec function {other}"
                )));
            }
        };

        Ok(response)
    }
}

pub(crate) fn business_os_app_root_artifact_write_guard(
    command: &str,
    cwd: &Path,
) -> Option<String> {
    let workspace_root = business_os_workspace_root(cwd)?;
    if command_accesses_state_root_installed_modules(command) {
        return Some(state_root_installed_modules_guard_message());
    }
    if command_probes_or_invokes_shell_patch_tool(command) {
        return Some(module_shell_patch_tool_guard_message());
    }
    if let Some(path) =
        command_stages_tmp_patch_for_business_os_module(command, &workspace_root, cwd)
    {
        return Some(module_tmp_patch_guard_message(&path));
    }
    for artifact in ["module.json", "collections.schema.json"] {
        let absolute = workspace_root.join(artifact);
        if command_writes_path(command, artifact)
            || command_writes_path(command, &absolute.to_string_lossy())
        {
            return Some(root_artifact_guard_message(artifact));
        }
    }
    if let Some(artifact) = command_writes_source_tree_installed_module(command) {
        return Some(source_tree_installed_module_guard_message(&artifact));
    }
    if let Some(artifact) = command_writes_forbidden_root_app_artifact(command) {
        return Some(root_artifact_guard_message(&artifact));
    }
    if let Some(artifact) =
        command_writes_forbidden_business_os_module_side_effect(command, &workspace_root, cwd)
    {
        return Some(module_side_effect_guard_message(&artifact));
    }
    if let Some(path) =
        command_writes_noncanonical_runtime_module_helper(command, &workspace_root, cwd)
    {
        return Some(module_noncanonical_helper_guard_message(&path));
    }
    if let Some(path) = command_reads_business_os_module_whole_file(command, &workspace_root, cwd) {
        return Some(module_whole_file_read_guard_message(&path));
    }
    if let Some(path) =
        command_uses_forbidden_business_os_module_writer(command, &workspace_root, cwd)
    {
        return Some(module_writer_guard_message(&path));
    }
    if let Some(path) = command_reads_runtime_module_self_audit(command, &workspace_root, cwd) {
        return Some(module_self_audit_read_guard_message(&path));
    }
    if let Some(path) =
        command_writes_large_business_os_module_payload(command, &workspace_root, cwd)
    {
        return Some(module_large_payload_guard_message(&path));
    }
    if let Some(path) =
        command_writes_large_business_os_module_heredoc(command, &workspace_root, cwd)
    {
        return Some(module_large_heredoc_guard_message(&path));
    }
    None
}

fn root_artifact_guard_message(artifact: &str) -> String {
    format!(
        "Business OS app module guard blocked a root-level app artifact write to `{artifact}`. \
Write runtime-installed app artifacts only under `runtime/business-os/installed-modules/<module_id>/` or \
direct state-root `business-os/installed-modules/<module_id>/`; write source template artifacts under \
`src/apps/business-os/modules/<module_id>/` using MODULE_DIR. Do not create workspace-root \
manifests, schema aliases, blocker/status notes, harness aliases, or probe files for app deliverables."
    )
}

fn source_tree_installed_module_guard_message(artifact: &str) -> String {
    format!(
        "Business OS app module guard blocked a write to the source-tree installed module path `{artifact}`. \
Runtime-installed apps must be written under `runtime/business-os/installed-modules/<module_id>/` \
or direct state-root `business-os/installed-modules/<module_id>/`. `src/apps/business-os/` is the \
release/source/template tree, not the App Creator install target."
    )
}

fn module_side_effect_guard_message(artifact: &str) -> String {
    format!(
        "Business OS app module guard blocked a forbidden generated-module side effect `{artifact}`. \
Business OS apps are no-build browser ESM modules. Do not create package.json, lockfiles, \
node_modules, bundle files, or probe/repair artifacts. Use .mjs tests and local browser-safe ESM \
helpers instead."
    )
}

fn state_root_installed_modules_guard_message() -> String {
    "Business OS app module guard blocked direct access to the state-root installed-modules directory. \
Runtime App Creator work must use the prompted `runtime/business-os/installed-modules/<module_id>/` path only; \
do not inspect or write `$HOME/.local/state/ctox/business-os/installed-modules` directly."
        .to_string()
}

fn module_whole_file_read_guard_message(path: &str) -> String {
    format!(
        "Business OS app module guard blocked a whole-file dump of generated module artifact `{path}`. \
Do not load entire installed app files into model context. Use targeted `sed -n 'start,endp'`, \
exact `rg -n` selectors/imports, or the app validator report instead."
    )
}

fn module_self_audit_read_guard_message(path: &str) -> String {
    format!(
        "Business OS app module guard blocked a generated-module self-audit readback `{path}`. \
Do not inspect runtime-installed App Creator files file-by-file, through broad line ranges, globs, \
multi-file grep/sed/wc commands, or consecutive readback chunks. Use the scaffold inventory, focused \
node checks, tests, and `ctox business-os app validate <id> --installed`; inspect only one exact \
failing selector/import/snippet after a concrete validator or syntax error."
    )
}

fn module_noncanonical_helper_guard_message(path: &str) -> String {
    format!(
        "Business OS app module guard blocked noncanonical runtime App Creator helper `{path}`. \
Keep initial runtime-installed apps bounded: use the scaffold helper files `core/records.mjs` and \
`core/automation.mjs`, with simple DOM wiring in `index.js`. Do not create extra helper layers such \
as ui/render/runtime/panel modules during one-shot app creation."
    )
}

fn module_writer_guard_message(path: &str) -> String {
    format!(
        "Business OS app module guard blocked a programmatic writer or fragile in-place writer against generated module artifact `{path}`. \
Do not use Python, Node writer scripts, base64 blobs, generated writer scripts, data URLs, temporary \
file-copy wrappers, shell patch wrappers, append-chunk rewrites, or sed/perl in-place line surgery for Business OS app files. Use direct bounded \
exact-path shell writes or smaller local ESM helpers under the module directory."
    )
}

fn module_large_heredoc_guard_message(path: &str) -> String {
    format!(
        "Business OS app module guard blocked an oversized heredoc rewrite of generated module artifact `{path}`. \
The App Creator scaffold is already present; make targeted edits, split large behavior into smaller \
module-local ESM helpers, or patch a narrow range instead of rewriting whole generated files."
    )
}

fn module_large_payload_guard_message(path: &str) -> String {
    format!(
        "Business OS app module guard blocked an oversized shell payload rewrite of generated module artifact `{path}`. \
Do not stream large printf/echo/tee/cat payloads through shell for Business OS app files. Keep edits small, \
split behavior into module-local ESM helpers, and rewrite only the bounded helper/file that actually changed."
    )
}

fn module_shell_patch_tool_guard_message() -> String {
    "Business OS app module guard blocked shell patch-tool probing or invocation. \
Do not discover, inspect, or invoke apply_patch from shell while building runtime-installed Business OS apps. \
Use direct bounded exact-path writes at the final module path, or split behavior into smaller module-local ESM helpers."
        .to_string()
}

fn module_tmp_patch_guard_message(path: &str) -> String {
    format!(
        "Business OS app module guard blocked temporary patch staging for generated module artifact `{path}`. \
Do not create .patch files under /tmp or shell wrappers around patch tools for Business OS app files. \
Rewrite the affected bounded file directly at its final module path, or split behavior into a smaller module-local ESM helper."
    )
}

fn command_writes_source_tree_installed_module(command: &str) -> Option<String> {
    let compact = command.replace("\\\n", " ").replace('\n', " ");
    let lower = compact.to_ascii_lowercase();
    let source_path = "src/apps/business-os/installed-modules/";
    if !lower.contains(source_path) {
        return None;
    }
    let write_like = lower.contains('>')
        || lower.contains(" tee ")
        || lower.contains(" tee\t")
        || lower.contains("mkdir ")
        || lower.contains("mkdir\t")
        || lower.contains("cp ")
        || lower.contains("mv ")
        || lower.contains("ln ")
        || lower.contains("install ")
        || lower.contains("writefilesync(")
        || lower.contains("writefile(")
        || lower.contains("fs.writefile")
        || lower.contains(".write_text(")
        || lower.contains(".write_bytes(");
    write_like.then(|| {
        let start = lower.find(source_path).unwrap_or(0);
        let tail = &compact[start..];
        tail.split(|ch: char| {
            ch.is_whitespace() || matches!(ch, '"' | '\'' | '`' | ';' | '&' | '|')
        })
        .next()
        .unwrap_or(source_path)
        .trim_matches(|ch: char| matches!(ch, '"' | '\'' | '`'))
        .to_string()
    })
}

#[derive(Debug)]
struct BusinessOsAppRootArtifactSnapshot {
    workspace_root: PathBuf,
    existing_root_entries: BTreeSet<String>,
}

fn business_os_app_root_artifact_snapshot(cwd: &Path) -> Option<BusinessOsAppRootArtifactSnapshot> {
    let workspace_root = business_os_workspace_root(cwd)?;
    let existing_root_entries = fs::read_dir(&workspace_root)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<BTreeSet<_>>();
    Some(BusinessOsAppRootArtifactSnapshot {
        workspace_root,
        existing_root_entries,
    })
}

fn cleanup_new_business_os_app_root_artifacts(
    snapshot: Option<&BusinessOsAppRootArtifactSnapshot>,
) -> Option<String> {
    let snapshot = snapshot?;
    let mut removed = Vec::new();
    let mut remove_errors = Vec::new();
    let Ok(entries) = fs::read_dir(&snapshot.workspace_root) else {
        return None;
    };
    for entry in entries.filter_map(Result::ok) {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if snapshot.existing_root_entries.contains(&name)
            || !forbidden_business_os_root_app_artifact_name(&name)
        {
            continue;
        }
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        match fs::remove_file(&path) {
            Ok(()) => removed.push(name),
            Err(err) => remove_errors.push(format!("{name}: {err}")),
        }
    }
    if removed.is_empty() && remove_errors.is_empty() {
        return None;
    }
    removed.sort();
    remove_errors.sort();
    let mut message = String::from(
        "Business OS app module guard removed newly created root-level app artifact(s). \
Generated app files must live only under \
`runtime/business-os/installed-modules/<module_id>/`, direct state-root \
`business-os/installed-modules/<module_id>/`, or source template \
`src/apps/business-os/modules/<module_id>/` as specified by the task. ",
    );
    if !removed.is_empty() {
        message.push_str("Removed forbidden root file(s): ");
        message.push_str(&removed.join(", "));
        message.push_str(". ");
    }
    if !remove_errors.is_empty() {
        message.push_str("Removal errors: ");
        message.push_str(&remove_errors.join("; "));
        message.push_str(". ");
    }
    message.push_str(
        "Re-run the write using MODULE_DIR and do not create workspace-root manifests, schema aliases, blocker/status notes, harness aliases, or probe files.",
    );
    Some(message)
}

fn business_os_workspace_root(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors()
        .find(|candidate| candidate.join("src/apps/business-os").is_dir())
        .map(Path::to_path_buf)
}

fn command_writes_path(command: &str, path: &str) -> bool {
    let compact = command.replace("\\\n", " ").replace('\n', " ");
    let path = path.trim();
    if path.is_empty() {
        return false;
    }
    let dot_path = format!("./{path}");
    [
        format!("> {path}"),
        format!(">{path}"),
        format!("> \"{path}\""),
        format!(">\"{path}\""),
        format!("> '{path}'"),
        format!(">'{path}'"),
        format!("> {dot_path}"),
        format!(">{dot_path}"),
        format!("> \"{dot_path}\""),
        format!(">\"{dot_path}\""),
        format!("> '{dot_path}'"),
        format!(">'{dot_path}'"),
        format!("tee {path}"),
        format!("tee \"{path}\""),
        format!("tee '{path}'"),
        format!("tee {dot_path}"),
        format!("tee \"{dot_path}\""),
        format!("tee '{dot_path}'"),
    ]
    .iter()
    .any(|needle| compact.contains(needle))
        || command_programmatically_writes_path(&compact, path)
}

fn command_writes_forbidden_root_app_artifact(command: &str) -> Option<String> {
    let compact = command.replace("\\\n", " ").replace('\n', " ");
    let tokens = shellish_tokens(&compact);
    for (idx, token) in tokens.iter().enumerate() {
        let Some(name) = root_artifact_token_name(token) else {
            continue;
        };
        if !forbidden_business_os_root_app_artifact_name(&name) {
            continue;
        }
        if command_writes_path(&compact, token)
            || command_writes_path(&compact, &name)
            || command_programmatically_writes_path(&compact, &name)
            || token_is_target_of_write_verb(&tokens, idx)
        {
            return Some(name);
        }
    }
    None
}

fn shellish_tokens(command: &str) -> Vec<String> {
    command
        .split(|ch: char| {
            ch.is_whitespace() || matches!(ch, ';' | '&' | '|' | '(' | ')' | '{' | '}')
        })
        .map(|token| {
            token
                .trim_matches(|ch: char| {
                    matches!(ch, '\'' | '"' | '`' | ',' | ':' | '[' | ']' | '<' | '>')
                })
                .to_string()
        })
        .filter(|token| !token.is_empty())
        .collect()
}

fn root_artifact_token_name(token: &str) -> Option<String> {
    let trimmed = token
        .trim()
        .trim_start_matches("./")
        .trim_matches(|ch: char| matches!(ch, '\'' | '"' | '`'));
    let lower_trimmed = trimmed.to_ascii_lowercase();
    let basename = lower_trimmed
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(lower_trimmed.as_str())
        .to_string();
    if trimmed.contains('/') || trimmed.contains('\\') {
        let is_module_dir_target = lower_trimmed.contains("module_dir")
            || lower_trimmed.contains("runtime/business-os/installed-modules/")
            || lower_trimmed.contains("business-os/installed-modules/")
            || lower_trimmed.contains("src/apps/business-os/modules/")
            || lower_trimmed.contains("src/apps/business-os/installed-modules/");
        if !is_module_dir_target
            && (trimmed.contains('$') || Path::new(trimmed).is_absolute())
            && forbidden_business_os_root_app_artifact_name(&basename)
        {
            return Some(basename);
        }
    }
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed == "."
        || trimmed == ".."
    {
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}

fn token_is_target_of_write_verb(tokens: &[String], idx: usize) -> bool {
    let start = idx.saturating_sub(6);
    tokens[start..idx].iter().any(|token| {
        matches!(
            token.as_str(),
            "mv" | "cp" | "ln" | "install" | "tee" | "touch" | "write" | "printf"
        )
    })
}

fn forbidden_business_os_root_app_artifact_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "module.json"
        || lower == "collections.schema.json"
        || lower.starts_with("_test_")
        || lower.starts_with("_probe_")
        || lower.starts_with("test-")
        || lower.starts_with("probe-")
        || lower.contains("-test.")
        || lower.contains("_test.")
        || lower.contains("-probe.")
        || lower.contains("_probe.")
        || lower.ends_with("-module.json")
        || lower.ends_with("_module.json")
        || lower.ends_with(".module.json")
        || lower.ends_with("-collections.schema.json")
        || lower.ends_with("_collections.schema.json")
        || lower.ends_with(".collections.schema.json")
        || lower == "artifact-status.md"
        || lower.ends_with("-artifact-status.md")
        || lower.ends_with("_artifact_status.md")
        || lower.ends_with("-blocker.md")
        || lower.ends_with("_blocker.md")
}

fn command_writes_forbidden_business_os_module_side_effect(
    command: &str,
    workspace_root: &Path,
    cwd: &Path,
) -> Option<String> {
    let compact = command.replace("\\\n", " ").replace('\n', " ");
    let tokens = shellish_tokens(&compact);
    let cwd_is_module_dir = is_business_os_module_dir(workspace_root, cwd);
    for (idx, token) in tokens.iter().enumerate() {
        let normalized = token
            .trim()
            .trim_start_matches("./")
            .trim_matches(|ch: char| matches!(ch, '\'' | '"' | '`'))
            .to_string();
        let lower = normalized.to_ascii_lowercase();
        let basename = lower
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(lower.as_str())
            .to_string();
        let path_is_under_business_os_module = lower.contains("src/apps/business-os/modules/")
            || lower.contains("src/apps/business-os/installed-modules/")
            || lower.contains("runtime/business-os/installed-modules/")
            || lower.contains("business-os/installed-modules/");
        let forbidden = forbidden_business_os_module_side_effect_name(&basename)
            || lower.contains("/node_modules/")
            || lower.ends_with("/node_modules");
        if !forbidden {
            continue;
        }
        let explicitly_targets_module = path_is_under_business_os_module
            && (command_writes_path(&compact, &normalized)
                || command_programmatically_writes_path(&compact, &normalized)
                || token_is_target_of_write_verb(&tokens, idx));
        let targets_module_cwd = cwd_is_module_dir
            && (command_writes_path(&compact, &basename)
                || command_programmatically_writes_path(&compact, &basename)
                || token_is_target_of_write_verb(&tokens, idx));
        let variable_module_target = normalized.contains("MODULE_DIR")
            && (command_writes_path(&compact, &normalized)
                || token_is_target_of_write_verb(&tokens, idx));
        if explicitly_targets_module || targets_module_cwd || variable_module_target {
            return Some(normalized);
        }
    }
    None
}

fn forbidden_business_os_module_side_effect_name(name: &str) -> bool {
    matches!(
        name,
        "package.json"
            | "package-lock.json"
            | "yarn.lock"
            | "pnpm-lock.yaml"
            | "bun.lockb"
            | "node_modules"
    ) || name.starts_with("_probe_")
        || name.starts_with("_test_")
        || name.starts_with("_test")
        || name.starts_with("_scratch")
        || name.starts_with("_size")
        || name.contains("scratch")
        || name.contains("probe")
        || name.ends_with(".bundle.js")
        || name.ends_with(".bundle.mjs")
        || name.ends_with(".bundle.css")
        || name.ends_with(".bak")
        || name.ends_with(".orig")
        || name.ends_with(".rej")
        || name.ends_with(".tmp")
}

fn command_writes_noncanonical_runtime_module_helper(
    command: &str,
    workspace_root: &Path,
    cwd: &Path,
) -> Option<String> {
    let compact = command.replace("\\\n", " ").replace('\n', " ");
    let lower = compact.to_ascii_lowercase();
    if !command_targets_runtime_business_os_module(&lower, workspace_root, cwd) {
        return None;
    }
    let tokens = shellish_tokens(&compact);
    let cwd_is_runtime_core_dir = is_runtime_business_os_module_core_dir(workspace_root, cwd);
    for (idx, token) in tokens.iter().enumerate() {
        let normalized = token
            .trim()
            .trim_start_matches("./")
            .trim_matches(|ch: char| matches!(ch, '\'' | '"' | '`'))
            .to_string();
        let lower = normalized.to_ascii_lowercase();
        let basename = lower
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(lower.as_str())
            .to_string();
        if !basename.ends_with(".mjs") {
            continue;
        }
        if matches!(basename.as_str(), "records.mjs" | "automation.mjs") {
            continue;
        }
        let explicit_runtime_core_helper =
            (lower.contains("runtime/business-os/installed-modules/") && lower.contains("/core/"))
                || lower.contains("$module_dir/core/")
                || lower.contains("${module_dir}/core/");
        let relative_runtime_core_helper = lower.starts_with("core/") || cwd_is_runtime_core_dir;
        if !(explicit_runtime_core_helper || relative_runtime_core_helper) {
            continue;
        }
        if command_writes_path(&compact, &normalized)
            || command_programmatically_writes_path(&compact, &normalized)
            || token_is_target_of_write_verb(&tokens, idx)
        {
            return Some(normalized);
        }
    }
    None
}

fn command_reads_runtime_module_self_audit(
    command: &str,
    workspace_root: &Path,
    cwd: &Path,
) -> Option<String> {
    let compact = command.replace("\\\n", " ").replace('\n', " ");
    let lower = compact.to_ascii_lowercase();
    if !command_targets_runtime_business_os_module(&lower, workspace_root, cwd) {
        return None;
    }
    if lower.contains("ctox business-os app validate")
        || lower_contains_shell_word(&lower, "node")
        || lower_contains_shell_word(&lower, "nodejs")
    {
        return None;
    }
    let read_like = lower_contains_shell_word(&lower, "sed")
        || lower_contains_shell_word(&lower, "grep")
        || lower_contains_shell_word(&lower, "rg")
        || lower_contains_shell_word(&lower, "wc")
        || lower_contains_shell_word(&lower, "awk");
    if !read_like {
        return None;
    }
    let module_path = first_business_os_module_artifact_reference(&compact, workspace_root, cwd)
        .unwrap_or_else(|| "runtime-installed module artifact".to_string());
    let artifact_refs = business_os_module_artifact_reference_count(&compact, workspace_root, cwd);
    let broad_module_glob = lower.contains("runtime/business-os/installed-modules/")
        && (lower.contains("/*.json")
            || lower.contains("/*.js")
            || lower.contains("/*.mjs")
            || lower.contains("/*.html")
            || lower.contains("/*.css")
            || lower.contains("/core/")
            || lower.contains("/locales/")
            || lower.contains("/tests/"));
    let multi_readback = artifact_refs > 1
        || lower.contains(" echo ----")
        || lower.contains(" echo ---")
        || lower.contains("; echo")
        || lower.contains(" && echo")
        || lower.contains("\\necho");
    let wc_readback = lower_contains_shell_word(&lower, "wc") && lower.contains("-l");
    let large_sed_range = command_has_large_sed_read_range(&lower);
    let grep_audit = (lower_contains_shell_word(&lower, "grep")
        || lower_contains_shell_word(&lower, "rg"))
        && (lower.matches('|').count() >= 4 || lower.contains("\\|"));

    if broad_module_glob || multi_readback || wc_readback || large_sed_range || grep_audit {
        return Some(module_path);
    }
    None
}

fn business_os_module_artifact_reference_count(
    command: &str,
    workspace_root: &Path,
    cwd: &Path,
) -> usize {
    let cwd_is_module_dir = is_business_os_module_dir(workspace_root, cwd);
    shellish_tokens(command)
        .iter()
        .filter(|token| business_os_module_artifact_token_name(token, cwd_is_module_dir).is_some())
        .count()
}

fn command_has_large_sed_read_range(command_lower: &str) -> bool {
    let bytes = command_lower.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx].is_ascii_digit() {
            let start_idx = idx;
            while idx < bytes.len() && bytes[idx].is_ascii_digit() {
                idx += 1;
            }
            if idx >= bytes.len() || bytes[idx] != b',' {
                idx += 1;
                continue;
            }
            let start = command_lower[start_idx..idx].parse::<u32>().unwrap_or(0);
            idx += 1;
            let end_idx = idx;
            while idx < bytes.len() && bytes[idx].is_ascii_digit() {
                idx += 1;
            }
            if end_idx == idx || idx >= bytes.len() || bytes[idx] != b'p' {
                idx += 1;
                continue;
            }
            let end = command_lower[end_idx..idx].parse::<u32>().unwrap_or(0);
            if end > start && end - start > 60 {
                return true;
            }
        } else {
            idx += 1;
        }
    }
    false
}

fn command_reads_business_os_module_whole_file(
    command: &str,
    workspace_root: &Path,
    cwd: &Path,
) -> Option<String> {
    let compact = command.replace("\\\n", " ").replace('\n', " ");
    let lower = compact.to_ascii_lowercase();
    if !command_targets_business_os_module(&lower, workspace_root, cwd) {
        return None;
    }
    if !lower_contains_shell_word(&lower, "cat") {
        return None;
    }
    if lower.contains("<<") {
        return command_reads_business_os_module_after_heredoc(command, workspace_root, cwd);
    }
    if lower.contains("| head")
        || lower.contains("| tail")
        || lower.contains("| wc")
        || lower.contains("| sed -n")
        || lower.contains("| rg ")
        || lower.contains("| grep ")
    {
        return None;
    }

    let cwd_is_module_dir = is_business_os_module_dir(workspace_root, cwd);
    let tokens = shellish_tokens(&compact);
    let module_cd_target = command_cd_target_business_os_module_dir(&tokens);
    for (idx, token) in tokens.iter().enumerate() {
        if token != "cat" {
            continue;
        }
        for target in tokens.iter().skip(idx + 1) {
            if target.starts_with('-') {
                continue;
            }
            if let Some(path) = business_os_module_artifact_token_name(target, cwd_is_module_dir) {
                return Some(path);
            }
            if let Some(path) =
                business_os_module_cd_artifact_token_name(target, module_cd_target.as_deref())
            {
                return Some(path);
            }
            if module_cd_target.is_some() && token_is_shell_variable_reference(target) {
                return module_cd_target.clone();
            }
        }
    }
    None
}

fn command_reads_business_os_module_after_heredoc(
    command: &str,
    workspace_root: &Path,
    cwd: &Path,
) -> Option<String> {
    let cwd_is_module_dir = is_business_os_module_dir(workspace_root, cwd);
    let full_tokens = shellish_tokens(command);
    let module_cd_target = command_cd_target_business_os_module_dir(&full_tokens);
    for line in command.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if !lower_contains_shell_word(&lower, "cat")
            || lower.contains("<<")
            || lower.contains('>')
            || lower.contains("| head")
            || lower.contains("| tail")
            || lower.contains("| wc")
            || lower.contains("| sed -n")
            || lower.contains("| rg ")
            || lower.contains("| grep ")
        {
            continue;
        }
        let tokens = shellish_tokens(trimmed);
        for (idx, token) in tokens.iter().enumerate() {
            if token != "cat" {
                continue;
            }
            for target in tokens.iter().skip(idx + 1) {
                if target.starts_with('-') {
                    continue;
                }
                if let Some(path) =
                    business_os_module_artifact_token_name(target, cwd_is_module_dir)
                {
                    return Some(path);
                }
                if let Some(path) =
                    business_os_module_cd_artifact_token_name(target, module_cd_target.as_deref())
                {
                    return Some(path);
                }
                if module_cd_target.is_some() && token_is_shell_variable_reference(target) {
                    return module_cd_target.clone();
                }
            }
        }
    }
    None
}

fn command_uses_forbidden_business_os_module_writer(
    command: &str,
    workspace_root: &Path,
    cwd: &Path,
) -> Option<String> {
    let compact = command.replace("\\\n", " ").replace('\n', " ");
    let lower = compact.to_ascii_lowercase();
    if !command_targets_business_os_module(&lower, workspace_root, cwd) {
        return None;
    }
    let module_path = first_business_os_module_artifact_reference(&compact, workspace_root, cwd)
        .unwrap_or_else(|| "module cwd".to_string());

    let python_writer = (lower_contains_shell_word(&lower, "python")
        || lower_contains_shell_word(&lower, "python3")
        || lower_contains_shell_word(&lower, "python3.11")
        || lower_contains_shell_word(&lower, "python3.12"))
        && (lower.contains("open(")
            || lower.contains(".write(")
            || lower.contains("write_text(")
            || lower.contains("write_bytes(")
            || lower.contains("'w'")
            || lower.contains("\"w\""));
    let node_writer = (lower_contains_shell_word(&lower, "node")
        || lower_contains_shell_word(&lower, "nodejs"))
        && (lower.contains("writefilesync")
            || lower.contains("writefile(")
            || lower.contains("appendfilesync")
            || lower.contains("appendfile(")
            || lower.contains("createwritestream")
            || lower.contains("fs.promises.writefile")
            || lower.contains("fs.writefile"));
    let base64_writer = lower_contains_shell_word(&lower, "base64")
        && (lower.contains('>') || lower.contains(" tee "));

    let fragile_in_place_editor = (lower_contains_shell_word(&lower, "sed")
        || lower_contains_shell_word(&lower, "gsed")
        || lower_contains_shell_word(&lower, "perl"))
        && shellish_tokens(&compact).iter().any(|token| {
            let token = token.trim_matches(|ch: char| matches!(ch, '\'' | '"'));
            token == "-pi" || token == "-pi.bak" || token.starts_with("-i")
        });

    let temp_file_copy_wrapper = (lower_contains_shell_word(&lower, "cp")
        || lower_contains_shell_word(&lower, "mv")
        || lower_contains_shell_word(&lower, "install"))
        && (lower.contains("/tmp/")
            || lower.contains(" /tmp")
            || lower.contains("'/tmp")
            || lower.contains("\"/tmp"))
        && first_business_os_module_artifact_reference(&compact, workspace_root, cwd).is_some();
    let append_chunk_writer = lower.contains(">>")
        && first_business_os_module_artifact_reference(&compact, workspace_root, cwd).is_some();

    if python_writer
        || node_writer
        || base64_writer
        || fragile_in_place_editor
        || temp_file_copy_wrapper
        || append_chunk_writer
    {
        return Some(module_path);
    }
    None
}

fn command_accesses_state_root_installed_modules(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    lower.contains(".local/state/ctox/business-os/installed-modules")
        || lower.contains("$home/.local/state/ctox/business-os/installed-modules")
        || lower.contains("~/.local/state/ctox/business-os/installed-modules")
}

fn command_probes_or_invokes_shell_patch_tool(command: &str) -> bool {
    let compact = command.replace("\\\n", " ").replace('\n', " ");
    let lower = compact.to_ascii_lowercase();
    lower_contains_shell_word(&lower, "apply_patch")
        || lower.contains("/apply_patch")
        || lower.contains("codex-arg0")
}

fn command_stages_tmp_patch_for_business_os_module(
    command: &str,
    workspace_root: &Path,
    cwd: &Path,
) -> Option<String> {
    let compact = command.replace("\\\n", " ");
    let lower = compact.to_ascii_lowercase();
    if !lower.contains("/tmp/")
        || !lower.contains(".patch")
        || !command_targets_business_os_module(&lower, workspace_root, cwd)
    {
        return None;
    }
    let writes_tmp_patch = lower.contains("> /tmp/")
        || lower.contains(">/tmp/")
        || lower.contains("> '/tmp/")
        || lower.contains(">'/tmp/")
        || lower.contains("> \"/tmp/")
        || lower.contains(">\"/tmp/")
        || lower.contains("tee /tmp/")
        || lower.contains("tee '/tmp/")
        || lower.contains("tee \"/tmp/");
    if !writes_tmp_patch {
        return None;
    }
    Some(
        first_business_os_module_artifact_reference(command, workspace_root, cwd)
            .unwrap_or_else(|| "module artifact".to_string()),
    )
}

fn command_writes_large_business_os_module_payload(
    command: &str,
    workspace_root: &Path,
    cwd: &Path,
) -> Option<String> {
    let compact = command.replace("\\\n", " ");
    let lower = compact.to_ascii_lowercase();
    if !command_targets_business_os_module(&lower, workspace_root, cwd) {
        return None;
    }
    if lower.contains("<<") {
        return None;
    }
    if command.len() <= 12_000 && command.lines().count() <= 120 {
        return None;
    }
    let shell_payload_write = (lower_contains_shell_word(&lower, "printf")
        || lower_contains_shell_word(&lower, "echo")
        || lower_contains_shell_word(&lower, "cat")
        || lower_contains_shell_word(&lower, "tee"))
        && (lower.contains('>') || lower.contains(" tee "));
    if !shell_payload_write {
        return None;
    }
    Some(
        first_business_os_module_artifact_reference(command, workspace_root, cwd)
            .unwrap_or_else(|| "module artifact".to_string()),
    )
}

fn command_writes_large_business_os_module_heredoc(
    command: &str,
    workspace_root: &Path,
    cwd: &Path,
) -> Option<String> {
    let compact = command.replace("\\\n", " ");
    let lower = compact.to_ascii_lowercase();
    if !command_targets_business_os_module(&lower, workspace_root, cwd) || !lower.contains("<<") {
        return None;
    }
    if command.lines().count() <= 180 && command.len() <= 24_000 {
        return None;
    }
    let module_path = first_business_os_module_artifact_reference(command, workspace_root, cwd)
        .unwrap_or_else(|| "module cwd".to_string());
    Some(module_path)
}

fn command_targets_business_os_module(
    command_lower: &str,
    workspace_root: &Path,
    cwd: &Path,
) -> bool {
    command_lower.contains("src/apps/business-os/modules/")
        || command_lower.contains("src/apps/business-os/installed-modules/")
        || command_lower.contains("runtime/business-os/installed-modules/")
        || command_lower.contains("business-os/installed-modules/")
        || is_business_os_module_dir(workspace_root, cwd)
}

fn command_targets_runtime_business_os_module(
    command_lower: &str,
    workspace_root: &Path,
    cwd: &Path,
) -> bool {
    command_lower.contains("runtime/business-os/installed-modules/")
        || is_runtime_business_os_module_dir(workspace_root, cwd)
}

fn first_business_os_module_artifact_reference(
    command: &str,
    workspace_root: &Path,
    cwd: &Path,
) -> Option<String> {
    let cwd_is_module_dir = is_business_os_module_dir(workspace_root, cwd);
    shellish_tokens(command)
        .iter()
        .find_map(|token| business_os_module_artifact_token_name(token, cwd_is_module_dir))
}

fn command_cd_target_business_os_module_dir(tokens: &[String]) -> Option<String> {
    tokens.windows(2).find_map(|window| {
        if window.first().map(String::as_str) != Some("cd") {
            return None;
        }
        let target = window.get(1)?;
        business_os_module_dir_token_name(target)
    })
}

fn business_os_module_dir_token_name(token: &str) -> Option<String> {
    let normalized = token
        .trim()
        .trim_start_matches("./")
        .trim_matches(|ch: char| matches!(ch, '\'' | '"' | '`'))
        .to_string();
    let lower = normalized.to_ascii_lowercase();
    let module_path = lower.contains("src/apps/business-os/modules/")
        || lower.contains("src/apps/business-os/installed-modules/")
        || lower.contains("runtime/business-os/installed-modules/")
        || lower.contains("business-os/installed-modules/")
        || lower.contains("$module_dir")
        || lower.contains("${module_dir}");
    module_path.then_some(normalized)
}

fn token_is_shell_variable_reference(token: &str) -> bool {
    let trimmed = token
        .trim()
        .trim_matches(|ch: char| matches!(ch, '\'' | '"' | '`'));
    trimmed.starts_with('$') || trimmed.contains("${") || trimmed.contains("$")
}

fn business_os_module_cd_artifact_token_name(
    token: &str,
    module_cd_target: Option<&str>,
) -> Option<String> {
    let module_dir = module_cd_target?;
    let normalized = token
        .trim()
        .trim_start_matches("./")
        .trim_matches(|ch: char| matches!(ch, '\'' | '"' | '`'))
        .to_string();
    if normalized.contains('$') || normalized.contains('/') || normalized.contains('\\') {
        return None;
    }
    let lower = normalized.to_ascii_lowercase();
    business_os_module_artifact_name(&lower).then(|| format!("{module_dir}/{normalized}"))
}

fn business_os_module_artifact_token_name(token: &str, cwd_is_module_dir: bool) -> Option<String> {
    let normalized = token
        .trim()
        .trim_start_matches("./")
        .trim_matches(|ch: char| matches!(ch, '\'' | '"' | '`'))
        .to_string();
    let lower = normalized.to_ascii_lowercase();
    let module_path = lower.contains("src/apps/business-os/modules/")
        || lower.contains("src/apps/business-os/installed-modules/")
        || lower.contains("runtime/business-os/installed-modules/")
        || lower.contains("business-os/installed-modules/")
        || lower.contains("$module_dir/")
        || lower.contains("${module_dir}/");
    let basename = lower
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(lower.as_str())
        .to_string();
    if module_path && business_os_module_artifact_name(&basename) {
        return Some(normalized);
    }
    if cwd_is_module_dir && business_os_module_artifact_name(&basename) {
        return Some(normalized);
    }
    None
}

fn business_os_module_artifact_name(name: &str) -> bool {
    matches!(
        name,
        "module.json"
            | "collections.schema.json"
            | "schema.js"
            | "index.html"
            | "index.css"
            | "index.js"
            | "icon.svg"
            | "automation.mjs"
            | "records.mjs"
            | "en.json"
            | "de.json"
    ) || name.ends_with(".test.mjs")
        || name.ends_with(".mjs")
}

fn lower_contains_shell_word(command_lower: &str, word: &str) -> bool {
    command_lower
        .split(|ch: char| {
            ch.is_whitespace() || matches!(ch, ';' | '&' | '|' | '(' | ')' | '{' | '}' | '"' | '\'')
        })
        .any(|token| token == word || token.rsplit('/').next() == Some(word))
}

fn is_business_os_module_dir(workspace_root: &Path, cwd: &Path) -> bool {
    let Ok(relative) = cwd.strip_prefix(workspace_root) else {
        return false;
    };
    let segments = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    (segments.len() >= 5
        && (segments[0] == "src"
            && segments[1] == "apps"
            && segments[2] == "business-os"
            && (segments[3] == "modules" || segments[3] == "installed-modules")))
        || (segments.len() >= 4
            && segments[0] == "runtime"
            && segments[1] == "business-os"
            && segments[2] == "installed-modules")
        || (segments.len() >= 3
            && segments[0] == "business-os"
            && segments[1] == "installed-modules")
}

fn is_runtime_business_os_module_dir(workspace_root: &Path, cwd: &Path) -> bool {
    let Ok(relative) = cwd.strip_prefix(workspace_root) else {
        return false;
    };
    let segments = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    segments.len() >= 4
        && segments[0] == "runtime"
        && segments[1] == "business-os"
        && segments[2] == "installed-modules"
}

fn is_runtime_business_os_module_core_dir(workspace_root: &Path, cwd: &Path) -> bool {
    let Ok(relative) = cwd.strip_prefix(workspace_root) else {
        return false;
    };
    let segments = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    segments.len() >= 5
        && segments[0] == "runtime"
        && segments[1] == "business-os"
        && segments[2] == "installed-modules"
        && segments[4] == "core"
}

fn command_programmatically_writes_path(command: &str, path: &str) -> bool {
    let single = format!("'{path}'");
    let double = format!("\"{path}\"");
    let root_single = format!("root/'{path}'");
    let root_single_spaced = format!("root / '{path}'");
    let root_double = format!("root/\"{path}\"");
    let root_double_spaced = format!("root / \"{path}\"");
    let write_marker = command.contains(".write_text(")
        || command.contains(".write_bytes(")
        || command.contains("writeFileSync(")
        || command.contains("writeFile(")
        || command.contains("fs.writeFile")
        || command.contains("open(");
    if !write_marker {
        return false;
    }
    if path.contains('/') {
        return command.contains(&single) || command.contains(&double);
    }
    command.contains(&root_single)
        || command.contains(&root_single_spaced)
        || command.contains(&root_double)
        || command.contains(&root_double_spaced)
        || command.contains(&format!("/'{path}'"))
        || command.contains(&format!("/\"{path}\""))
        || command.contains(&format!("open({single}"))
        || command.contains(&format!("open({double}"))
        || command.contains(&format!("writeFileSync({single}"))
        || command.contains(&format!("writeFileSync({double}"))
        || command.contains(&format!("writeFile({single}"))
        || command.contains(&format!("writeFile({double}"))
}

pub(crate) fn get_command(
    args: &ExecCommandArgs,
    session_shell: Arc<Shell>,
    shell_mode: &UnifiedExecShellMode,
    allow_login_shell: bool,
) -> Result<Vec<String>, String> {
    let use_login_shell = match args.login {
        Some(true) if !allow_login_shell => {
            return Err(
                "login shell is disabled by config; omit `login` or set it to false.".to_string(),
            );
        }
        Some(use_login_shell) => use_login_shell,
        None => allow_login_shell,
    };

    match shell_mode {
        UnifiedExecShellMode::Direct => {
            let model_shell = args.shell.as_ref().map(|shell_str| {
                let mut shell = get_shell_by_model_provided_path(&PathBuf::from(shell_str));
                shell.shell_snapshot = crate::shell::empty_shell_snapshot_receiver();
                shell
            });
            let shell = model_shell.as_ref().unwrap_or(session_shell.as_ref());
            Ok(shell.derive_exec_args(&args.cmd, use_login_shell))
        }
        UnifiedExecShellMode::ZshFork(zsh_fork_config) => Ok(vec![
            zsh_fork_config.shell_zsh_path.to_string_lossy().to_string(),
            if use_login_shell { "-lc" } else { "-c" }.to_string(),
            args.cmd.clone(),
        ]),
    }
}

#[cfg(test)]
#[path = "unified_exec_tests.rs"]
mod tests;
