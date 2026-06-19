use super::*;
use crate::shell::default_user_shell;
use crate::tools::handlers::parse_arguments_with_base_path;
use crate::tools::handlers::resolve_workdir_base_path;
use crate::tools::spec::ZshForkConfig;
use ctox_protocol::models::FileSystemPermissions;
use ctox_protocol::models::PermissionProfile;
use ctox_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_get_command_uses_default_shell_when_unspecified() -> anyhow::Result<()> {
    let json = r#"{"cmd": "echo hello"}"#;

    let args: ExecCommandArgs = parse_arguments(json)?;

    assert!(args.shell.is_none());

    let command = get_command(
        &args,
        Arc::new(default_user_shell()),
        &UnifiedExecShellMode::Direct,
        true,
    )
    .map_err(anyhow::Error::msg)?;

    assert_eq!(command.len(), 3);
    assert_eq!(command[2], "echo hello");
    Ok(())
}

#[test]
fn test_get_command_respects_explicit_bash_shell() -> anyhow::Result<()> {
    let json = r#"{"cmd": "echo hello", "shell": "/bin/bash"}"#;

    let args: ExecCommandArgs = parse_arguments(json)?;

    assert_eq!(args.shell.as_deref(), Some("/bin/bash"));

    let command = get_command(
        &args,
        Arc::new(default_user_shell()),
        &UnifiedExecShellMode::Direct,
        true,
    )
    .map_err(anyhow::Error::msg)?;

    assert_eq!(command.last(), Some(&"echo hello".to_string()));
    if command
        .iter()
        .any(|arg| arg.eq_ignore_ascii_case("-Command"))
    {
        assert!(command.contains(&"-NoProfile".to_string()));
    }
    Ok(())
}

#[test]
fn test_get_command_respects_explicit_powershell_shell() -> anyhow::Result<()> {
    let json = r#"{"cmd": "echo hello", "shell": "powershell"}"#;

    let args: ExecCommandArgs = parse_arguments(json)?;

    assert_eq!(args.shell.as_deref(), Some("powershell"));

    let command = get_command(
        &args,
        Arc::new(default_user_shell()),
        &UnifiedExecShellMode::Direct,
        true,
    )
    .map_err(anyhow::Error::msg)?;

    assert_eq!(command[2], "echo hello");
    Ok(())
}

#[test]
fn test_get_command_respects_explicit_cmd_shell() -> anyhow::Result<()> {
    let json = r#"{"cmd": "echo hello", "shell": "cmd"}"#;

    let args: ExecCommandArgs = parse_arguments(json)?;

    assert_eq!(args.shell.as_deref(), Some("cmd"));

    let command = get_command(
        &args,
        Arc::new(default_user_shell()),
        &UnifiedExecShellMode::Direct,
        true,
    )
    .map_err(anyhow::Error::msg)?;

    assert_eq!(command[2], "echo hello");
    Ok(())
}

#[test]
fn test_get_command_rejects_explicit_login_when_disallowed() -> anyhow::Result<()> {
    let json = r#"{"cmd": "echo hello", "login": true}"#;

    let args: ExecCommandArgs = parse_arguments(json)?;
    let err = get_command(
        &args,
        Arc::new(default_user_shell()),
        &UnifiedExecShellMode::Direct,
        false,
    )
    .expect_err("explicit login should be rejected");

    assert!(
        err.contains("login shell is disabled by config"),
        "unexpected error: {err}"
    );
    Ok(())
}

#[test]
fn test_get_command_ignores_explicit_shell_in_zsh_fork_mode() -> anyhow::Result<()> {
    let json = r#"{"cmd": "echo hello", "shell": "/bin/bash"}"#;
    let args: ExecCommandArgs = parse_arguments(json)?;
    let shell_zsh_path = AbsolutePathBuf::from_absolute_path(if cfg!(windows) {
        r"C:\opt\codex\zsh"
    } else {
        "/opt/codex/zsh"
    })?;
    let shell_mode = UnifiedExecShellMode::ZshFork(ZshForkConfig {
        shell_zsh_path: shell_zsh_path.clone(),
        main_execve_wrapper_exe: AbsolutePathBuf::from_absolute_path(if cfg!(windows) {
            r"C:\opt\codex\ctox-execve-wrapper"
        } else {
            "/opt/codex/ctox-execve-wrapper"
        })?,
    });

    let command = get_command(&args, Arc::new(default_user_shell()), &shell_mode, true)
        .map_err(anyhow::Error::msg)?;

    assert_eq!(
        command,
        vec![
            shell_zsh_path.to_string_lossy().to_string(),
            "-lc".to_string(),
            "echo hello".to_string()
        ]
    );
    Ok(())
}

#[test]
fn exec_command_args_resolve_relative_additional_permissions_against_workdir() -> anyhow::Result<()>
{
    let cwd = tempdir()?;
    let workdir = cwd.path().join("nested");
    fs::create_dir_all(&workdir)?;
    let expected_write = workdir.join("relative-write.txt");
    let json = r#"{
            "cmd": "echo hello",
            "workdir": "nested",
            "additional_permissions": {
                "file_system": {
                    "write": ["./relative-write.txt"]
                }
            }
        }"#;

    let base_path = resolve_workdir_base_path(json, cwd.path())?;
    let args: ExecCommandArgs = parse_arguments_with_base_path(json, base_path.as_path())?;

    assert_eq!(
        args.additional_permissions,
        Some(PermissionProfile {
            file_system: Some(FileSystemPermissions {
                read: None,
                write: Some(vec![AbsolutePathBuf::try_from(expected_write)?]),
            }),
            ..Default::default()
        })
    );
    Ok(())
}

#[test]
fn business_os_guard_blocks_root_module_json_write() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "cd . && cat > module.json <<'EOF'\n{}\nEOF",
        root.path(),
    )
    .expect("root module write should be blocked");

    assert!(err.contains("root-level app artifact write to `module.json`"));
    assert!(err.contains("installed-modules/<module_id>/"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_root_collections_schema_write() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "printf '{}' > collections.schema.json",
        root.path(),
    )
    .expect("root collections schema write should be blocked");

    assert!(err.contains("root-level app artifact write to `collections.schema.json`"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_python_root_artifact_write() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "python3 -c \"import pathlib; root=pathlib.Path('/tmp/app'); (root/'module.json').write_text('{}')\"",
        root.path(),
    )
    .expect("python root module write should be blocked");

    assert!(err.contains("root-level app artifact write to `module.json`"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_root_manifest_alias_write() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "cp runtime/business-os/installed-modules/subscriptions/module.json harness-module.json",
        root.path(),
    )
    .expect("root manifest alias write should be blocked");

    assert!(err.contains("root-level app artifact write to `harness-module.json`"));
    assert!(err.contains("harness aliases"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_root_schema_alias_move() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "echo '{}' > /tmp/probe.json && mv /tmp/probe.json harness-collections.schema.json",
        root.path(),
    )
    .expect("root schema alias move should be blocked");

    assert!(err.contains("root-level app artifact write to `harness-collections.schema.json`"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_root_artifact_status_note() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "cat > harness-artifact-status.md <<'EOF'\nblocked\nEOF",
        root.path(),
    )
    .expect("root artifact status note should be blocked");

    assert!(err.contains("root-level app artifact write to `harness-artifact-status.md`"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_root_probe_file() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "cat > _test_guard.txt <<'EOF'\nprobe\nEOF",
        root.path(),
    )
    .expect("root probe file should be blocked");

    assert!(err.contains("root-level app artifact write to `_test_guard.txt`"));
    assert!(err.contains("probe files"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_generic_root_test_probe_file() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "echo '{\"test\":1}' > test-file.json",
        root.path(),
    )
    .expect("generic root test probe should be blocked");

    assert!(err.contains("root-level app artifact write to `test-file.json`"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_variable_root_manifest_write() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "MODROOT=/tmp/app && cat > \"$MODROOT/module.json\" <<'JSON'\n{}\nJSON",
        root.path(),
    )
    .expect("variable root module write should be blocked");

    assert!(err.contains("root-level app artifact write to `module.json`"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_root_manifest_symlink() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let target = root.path().join("module.json");
    let command = format!(
        "ln -sf runtime/business-os/installed-modules/inventory/module.json {}",
        target.display()
    );

    let err = business_os_app_root_artifact_write_guard(&command, root.path())
        .expect("root module symlink should be blocked");

    assert!(err.contains("root-level app artifact write to `module.json`"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_module_package_json_write() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "cat > runtime/business-os/installed-modules/inventory/package.json <<'EOF'\n{\"type\":\"module\"}\nEOF",
        root.path(),
    )
    .expect("module package.json write should be blocked");

    assert!(err.contains("forbidden generated-module side effect"));
    assert!(err.contains("package.json"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_module_package_json_write_from_module_cwd() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let module_dir = root
        .path()
        .join("runtime/business-os/installed-modules/inventory");
    fs::create_dir_all(&module_dir)?;

    let err = business_os_app_root_artifact_write_guard(
        "cat > package.json <<'EOF'\n{\"type\":\"module\"}\nEOF",
        &module_dir,
    )
    .expect("module package.json write from module cwd should be blocked");

    assert!(err.contains("forbidden generated-module side effect"));
    assert!(err.contains("package.json"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_short_module_root_alias_write() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "MD=runtime/business-os/installed-modules/inventory\ncat > \"$MD/m.json\" <<'JSON'\n{}\nJSON",
        root.path(),
    )
    .expect("short module root alias should be blocked");

    assert!(err.contains("forbidden generated-module side effect"));
    assert!(err.contains("$MD/m.json"));
    Ok(())
}

#[test]
fn business_os_guard_allows_installed_module_write_and_reads() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    assert!(business_os_app_root_artifact_write_guard(
        "cat module.json && cat > runtime/business-os/installed-modules/contracts/module.json <<'EOF'\n{}\nEOF",
        root.path(),
    )
    .is_none());
    Ok(())
}

#[test]
fn business_os_guard_blocks_installed_module_whole_file_cat() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "cat runtime/business-os/installed-modules/inventory/module.json runtime/business-os/installed-modules/inventory/index.js",
        root.path(),
    )
    .expect("whole-file installed module cat should be blocked");

    assert!(err.contains("whole-file dump"));
    assert!(err.contains("runtime/business-os/installed-modules/inventory/module.json"));
    assert!(err.contains("sed -n"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_installed_module_whole_file_cat_with_stderr_redirect(
) -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let artifact = root
        .path()
        .join("runtime/business-os/installed-modules/inventory/module.json");
    let command = format!("cat {} 2>&1", artifact.display());

    let err = business_os_app_root_artifact_write_guard(&command, root.path())
        .expect("stderr redirect must not bypass whole-file cat guard");

    assert!(err.contains("whole-file dump"));
    assert!(err.contains("runtime/business-os/installed-modules/inventory/module.json"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_module_cwd_whole_file_cat() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let module_dir = root
        .path()
        .join("runtime/business-os/installed-modules/inventory");
    fs::create_dir_all(&module_dir)?;

    let err = business_os_app_root_artifact_write_guard("cat module.json index.js", &module_dir)
        .expect("module cwd whole-file cat should be blocked");

    assert!(err.contains("whole-file dump"));
    assert!(err.contains("module.json"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_module_cd_variable_loop_whole_file_cat() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let module_dir = root
        .path()
        .join("runtime/business-os/installed-modules/inventory");
    fs::create_dir_all(&module_dir)?;
    let command = format!(
        "cd {} && for f in module.json collections.schema.json schema.js index.html index.css index.js icon.svg core/automation.mjs core/records.mjs locales/de.json locales/en.json tests/inventory.test.mjs; do echo \"===== $f =====\"; cat \"$f\"; echo; done",
        module_dir.display()
    );

    let err = business_os_app_root_artifact_write_guard(&command, root.path())
        .expect("module-local variable cat loop should be blocked");

    assert!(err.contains("whole-file dump"));
    assert!(err.contains("runtime/business-os/installed-modules/inventory"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_relative_module_cd_variable_whole_file_cat() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "cd runtime/business-os/installed-modules/inventory && f=index.js && cat \"$f\"",
        root.path(),
    )
    .expect("relative module cd variable cat should be blocked");

    assert!(err.contains("whole-file dump"));
    assert!(err.contains("runtime/business-os/installed-modules/inventory"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_module_cd_direct_artifact_whole_file_cat() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let module_dir = root
        .path()
        .join("runtime/business-os/installed-modules/inventory");
    fs::create_dir_all(&module_dir)?;
    let command = format!(
        "cd {} && cat module.json collections.schema.json schema.js icon.svg",
        module_dir.display()
    );

    let err = business_os_app_root_artifact_write_guard(&command, root.path())
        .expect("module-local direct artifact cat should be blocked");

    assert!(err.contains("whole-file dump"));
    assert!(err.contains("runtime/business-os/installed-modules/inventory/module.json"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_heredoc_then_trailing_whole_file_cat() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let module_dir = root
        .path()
        .join("runtime/business-os/installed-modules/subscriptions");
    fs::create_dir_all(module_dir.join("locales"))?;
    let locale_path = module_dir.join("locales/en.json");
    let command = format!(
        "cat > {} <<'JSON'\n{{\"title\":\"Subscriptions\"}}\nJSON\ncat {}",
        locale_path.display(),
        locale_path.display()
    );

    let err = business_os_app_root_artifact_write_guard(&command, root.path())
        .expect("trailing cat after heredoc should be blocked");

    assert!(err.contains("whole-file dump"));
    assert!(err.contains("runtime/business-os/installed-modules/subscriptions/locales/en.json"));
    Ok(())
}

#[test]
fn business_os_guard_allows_targeted_installed_module_reads() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    assert!(business_os_app_root_artifact_write_guard(
        "sed -n '1,40p' runtime/business-os/installed-modules/inventory/index.js",
        root.path(),
    )
    .is_none());
    assert!(business_os_app_root_artifact_write_guard(
        "cat runtime/business-os/installed-modules/inventory/index.css | head -30",
        root.path(),
    )
    .is_none());
    Ok(())
}

#[test]
fn business_os_guard_blocks_runtime_module_wc_readback_audit() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "wc -l runtime/business-os/installed-modules/inventory/*.json runtime/business-os/installed-modules/inventory/*.js runtime/business-os/installed-modules/inventory/*.html runtime/business-os/installed-modules/inventory/core/*.mjs",
        root.path(),
    )
    .expect("broad wc readback audits against generated runtime modules should be blocked");

    assert!(err.contains("self-audit readback"));
    assert!(err.contains("runtime-installed"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_runtime_module_listing_readback() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "ls runtime/business-os/installed-modules/bench_inventory_r101/core/ runtime/business-os/installed-modules/bench_inventory_r101/locales/ runtime/business-os/installed-modules/bench_inventory_r101/tests/",
        root.path(),
    )
    .expect("listing generated runtime module scaffold should be blocked");

    assert!(err.contains("self-audit readback"));
    assert!(err.contains("runtime/business-os/installed-modules/bench_inventory_r101"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_runtime_module_stat_readback() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "stat runtime/business-os/installed-modules/inventory/index.js",
        root.path(),
    )
    .expect("stat readback of generated runtime module artifact should be blocked");

    assert!(err.contains("self-audit readback"));
    assert!(err.contains("index.js"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_runtime_module_multi_sed_readback() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "sed -n '1,33p' runtime/business-os/installed-modules/inventory/module.json 2>&1; echo ----; sed -n '1,59p' runtime/business-os/installed-modules/inventory/schema.js 2>&1",
        root.path(),
    )
    .expect("multi-file sed readback audits against generated runtime modules should be blocked");

    assert!(err.contains("self-audit readback"));
    assert!(err.contains("module.json"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_runtime_module_large_sed_range() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "sed -n '1,120p' runtime/business-os/installed-modules/inventory/index.js 2>&1",
        root.path(),
    )
    .expect("large generated-file sed ranges should be blocked");

    assert!(err.contains("self-audit readback"));
    assert!(err.contains("index.js"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_runtime_module_large_head_readback() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let module_dir = root
        .path()
        .join("runtime/business-os/installed-modules/subscriptions");
    fs::create_dir_all(&module_dir)?;

    let err = business_os_app_root_artifact_write_guard(
        "head -60 tests/subscriptions.test.mjs 2>&1",
        &module_dir,
    )
    .expect("large generated-file head snippets should be blocked");

    assert!(err.contains("self-audit readback"));
    assert!(err.contains("subscriptions.test.mjs"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_runtime_module_large_tail_readback() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let module_dir = root
        .path()
        .join("runtime/business-os/installed-modules/subscriptions");
    fs::create_dir_all(&module_dir)?;

    let err = business_os_app_root_artifact_write_guard(
        "tail -60 tests/subscriptions.test.mjs 2>&1",
        &module_dir,
    )
    .expect("large generated-file tail snippets should be blocked");

    assert!(err.contains("self-audit readback"));
    assert!(err.contains("subscriptions.test.mjs"));
    Ok(())
}

#[test]
fn business_os_guard_allows_small_runtime_module_head_snippet() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let module_dir = root
        .path()
        .join("runtime/business-os/installed-modules/subscriptions");
    fs::create_dir_all(&module_dir)?;

    assert!(business_os_app_root_artifact_write_guard(
        "head -30 tests/subscriptions.test.mjs 2>&1",
        &module_dir
    )
    .is_none());
    Ok(())
}

#[test]
fn business_os_guard_blocks_node_whole_file_dump() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "node -e \"const fs=require('fs'); const code=fs.readFileSync('runtime/business-os/installed-modules/subscriptions/index.js','utf8'); console.log(code);\"",
        root.path(),
    )
    .expect("node readFileSync plus console.log(code) should be blocked");

    assert!(err.contains("whole-file dump"));
    assert!(err.contains("runtime/business-os/installed-modules/subscriptions/index.js"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_node_json_slice_dump() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "node -e \"const fs=require('fs'); const c=JSON.parse(fs.readFileSync('runtime/business-os/installed-modules/subscriptions/collections.schema.json','utf8')); console.log(JSON.stringify(c, null, 2).slice(0, 3000));\"",
        root.path(),
    )
    .expect("node JSON stringify slice dumps should be blocked");

    assert!(err.contains("whole-file dump"));
    assert!(err.contains("collections.schema.json"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_node_line_split_audit() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "node -e \"const fs=require('fs'); const code=fs.readFileSync('runtime/business-os/installed-modules/subscriptions/index.js','utf8'); const lines=code.split('\\n'); console.log('total lines:', lines.length); console.log(lines.slice(0, 80).join('\\n'));\"",
        root.path(),
    )
    .expect("node line-split audits of generated files should be blocked");

    assert!(err.contains("whole-file dump"));
    assert!(err.contains("index.js"));
    Ok(())
}

#[test]
fn business_os_guard_allows_node_json_parse_smoke() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    assert!(
        business_os_app_root_artifact_write_guard(
            "node -e \"JSON.parse(require('fs').readFileSync('runtime/business-os/installed-modules/subscriptions/module.json','utf8')); console.log('module JSON OK')\"",
            root.path()
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn business_os_guard_blocks_python_installed_module_writer() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "python3 -c \"path='runtime/business-os/installed-modules/inventory/index.js'; src=open(path).read(); open(path, 'w').write(src)\"",
        root.path(),
    )
    .expect("python writer against installed module should be blocked");

    assert!(err.contains("programmatic writer"));
    assert!(err.contains("runtime/business-os/installed-modules/inventory/index.js"));
    assert!(err.contains("Python"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_module_sed_in_place_line_surgery() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let module_dir = root
        .path()
        .join("runtime/business-os/installed-modules/inventory");
    fs::create_dir_all(&module_dir)?;

    let err = business_os_app_root_artifact_write_guard(
        "sed -i '' '97,102d' index.js && sed -n '93,105p' index.js",
        &module_dir,
    )
    .expect("sed in-place line surgery inside module cwd should be blocked");

    assert!(err.contains("fragile in-place writer"));
    assert!(err.contains("sed/perl in-place line surgery"));
    assert!(err.contains("index.js"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_tmp_file_copy_wrapper_to_module_artifact() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "cat > /tmp/index.html <<'HTML'\n<main></main>\nHTML\ncp /tmp/index.html runtime/business-os/installed-modules/inventory/index.html",
        root.path(),
    )
    .expect("/tmp copy wrapper to module artifact should be blocked");

    assert!(err.contains("temporary"));
    assert!(err.contains("file-copy wrappers"));
    assert!(err.contains("runtime/business-os/installed-modules/inventory/index.html"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_shell_apply_patch_probe() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "which apply_patch 2>/dev/null; apply_patch --help 2>&1 | head -20",
        root.path(),
    )
    .expect("shell apply_patch probing should be blocked for Business OS app work");

    assert!(err.contains("patch-tool probing"));
    assert!(err.contains("apply_patch"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_validator_internals_read() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "sed -n '1,80p' src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs 2>&1",
        root.path(),
    )
    .expect("reading checker internals should be blocked for App Creator work");

    assert!(err.contains("validator/checker internals"));
    assert!(err.contains("module_static_check.mjs"));
    Ok(())
}

#[test]
fn business_os_guard_allows_running_validator_script() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    assert!(
        business_os_app_root_artifact_write_guard(
            "node src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs bench_projects --installed",
            root.path(),
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn business_os_guard_blocks_broad_bench_find() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "find . -name \"bench_projects_r29*\" 2>&1 | head -10",
        root.path(),
    )
    .expect("broad bench discovery should be blocked");

    assert!(err.contains("broad App Creator discovery"));
    assert!(err.contains("source/runtime/install tree"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_source_modules_root_listing() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os/modules"))?;

    let command = format!(
        "ls {}/src/apps/business-os/modules/ 2>&1",
        root.path().display()
    );
    let err = business_os_app_root_artifact_write_guard(&command, root.path())
        .expect("source modules root listing should be blocked");

    assert!(err.contains("broad App Creator discovery"));
    assert!(err.contains("src/apps/business-os/modules/"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_source_modules_root_find() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os/modules"))?;

    let err = business_os_app_root_artifact_write_guard(
        "find src/apps/business-os/modules -maxdepth 3 -name \"automation.mjs\" -o -name \"records.mjs\" 2>&1 | head -30",
        root.path(),
    )
    .expect("source modules root find should be blocked for App Creator work");

    assert!(err.contains("broad App Creator discovery"));
    assert!(err.contains("src/apps/business-os/modules/"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_source_modules_directory_rg() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os/modules"))?;

    let err = business_os_app_root_artifact_write_guard(
        "rg -n \"buildFollowUpCommand|summarizeRecords|visibleRecords\" src/apps/business-os/modules/ -g '*.{js,mjs}' 2>&1 | head -20",
        root.path(),
    )
    .expect("source modules directory rg should be blocked for App Creator work");

    assert!(err.contains("broad App Creator discovery"));
    assert!(err.contains("src/apps/business-os/modules/"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_source_module_wc_line_count_sweep() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os/modules/customers"))?;
    fs::create_dir_all(root.path().join("src/apps/business-os/modules/shiftflow"))?;
    fs::create_dir_all(root.path().join("src/apps/business-os/modules/outbound"))?;

    let err = business_os_app_root_artifact_write_guard(
        "wc -l src/apps/business-os/modules/customers/index.js src/apps/business-os/modules/shiftflow/index.js src/apps/business-os/modules/outbound/index.js",
        root.path(),
    )
    .expect("source module line-count sweep should be blocked for App Creator work");

    assert!(err.contains("broad App Creator discovery"));
    assert!(err.contains("src/apps/business-os/modules/"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_source_module_multi_file_loop_dump() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os/modules/customers"))?;

    let err = business_os_app_root_artifact_write_guard(
        "cd src/apps/business-os/modules/customers && for f in module.json collections.schema.json schema.js index.js core/automation.mjs; do echo \"=== $f ===\"; cat \"$f\" 2>&1 | head -120; done",
        root.path(),
    )
    .expect("source module multi-file loop dumps should be blocked for App Creator work");

    assert!(err.contains("broad App Creator discovery"));
    assert!(err.contains("src/apps/business-os/modules/"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_business_os_shell_source_discovery() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os/shared"))?;

    let err = business_os_app_root_artifact_write_guard(
        "rg -n \"installed-modules|app\\.entry|resolveModule\" src/apps/business-os/app.js src/apps/business-os/shared/ -g '*.js' 2>&1 | head -40",
        root.path(),
    )
    .expect("Business OS shell source discovery should be blocked for App Creator work");

    assert!(err.contains("broad App Creator discovery"));
    assert!(err.contains("src/apps/business-os/"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_native_business_os_source_reads() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    fs::create_dir_all(root.path().join("src/core/business_os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "sed -n '170,250p' src/core/business_os/server.rs 2>&1",
        root.path(),
    )
    .expect("native Business OS source reads should be blocked for App Creator work");

    assert!(err.contains("broad App Creator discovery"));
    assert!(err.contains("src/core/business_os/"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_installed_modules_root_listing() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "ls runtime/business-os/installed-modules/ 2>&1 | head -40",
        root.path(),
    )
    .expect("installed modules root listing should be blocked");

    assert!(err.contains("broad App Creator discovery"));
    assert!(err.contains("runtime/business-os/installed-modules/"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_tmp_patch_staging_for_module_artifact() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "cat > /tmp/p1.patch <<'PATCH'\n*** Begin Patch\n*** Update File: runtime/business-os/installed-modules/inventory/index.js\n@@\n-console.log('old');\n+console.log('new');\n*** End Patch\nPATCH",
        root.path(),
    )
    .expect("/tmp .patch staging for module artifact should be blocked");

    assert!(err.contains("temporary patch staging"));
    assert!(err.contains("runtime/business-os/installed-modules/inventory/index.js"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_large_shell_payload_module_rewrite() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let payload = (0..140)
        .map(|idx| format!("'export const field{idx} = {idx};'"))
        .collect::<Vec<_>>()
        .join(" \\\n");
    let command = format!(
        "printf '%s\\n' {payload} > runtime/business-os/installed-modules/inventory/core/records.mjs"
    );

    let err = business_os_app_root_artifact_write_guard(&command, root.path())
        .expect("large shell payload rewrite should be blocked");

    assert!(err.contains("oversized shell payload"));
    assert!(err.contains("runtime/business-os/installed-modules/inventory/core/records.mjs"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_state_root_installed_modules_access() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "ls -la /Users/example/.local/state/ctox/business-os/installed-modules/inventory/",
        root.path(),
    )
    .expect("direct state-root installed-modules access should be blocked");

    assert!(err.contains("state-root installed-modules"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_module_scratch_probe_file() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let module_dir = root
        .path()
        .join("runtime/business-os/installed-modules/inventory");
    fs::create_dir_all(&module_dir)?;

    let err = business_os_app_root_artifact_write_guard(
        "echo patched > _scratch_test && rm _scratch_test",
        &module_dir,
    )
    .expect("module scratch probe files should be blocked");

    assert!(err.contains("probe/repair artifacts"));
    assert!(err.contains("_scratch_test"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_module_artifact_append_chunks() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let module_dir = root
        .path()
        .join("runtime/business-os/installed-modules/inventory");
    fs::create_dir_all(&module_dir)?;

    let err = business_os_app_root_artifact_write_guard(
        "cat >> index.js <<'JS'\nexport function mount() {}\nJS",
        &module_dir,
    )
    .expect("append chunks against module artifacts should be blocked");

    assert!(err.contains("append-chunk rewrites"));
    assert!(err.contains("index.js"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_noncanonical_runtime_core_helper() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "cat > runtime/business-os/installed-modules/inventory/core/render.mjs <<'EOF'\nexport function render() {}\nEOF",
        root.path(),
    )
    .expect("extra runtime App Creator helper layers should be blocked");

    assert!(err.contains("noncanonical runtime App Creator helper"));
    assert!(err.contains("core/render.mjs"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_noncanonical_module_dir_core_helper() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "MODULE_DIR=runtime/business-os/installed-modules/inventory && cat > \"$MODULE_DIR/core/render.mjs\" <<'EOF'\nexport function render() {}\nEOF",
        root.path(),
    )
    .expect("extra $MODULE_DIR core helper layers should be blocked");

    assert!(err.contains("noncanonical runtime App Creator helper"));
    assert!(err.contains("$MODULE_DIR/core/render.mjs"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_required_artifact_removal() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let module_dir = root
        .path()
        .join("runtime/business-os/installed-modules/inventory");
    fs::create_dir_all(&module_dir)?;

    let err = business_os_app_root_artifact_write_guard("rm -f index.js", &module_dir)
        .expect("removing required App Creator artifacts should be blocked");

    assert!(err.contains("destructive or replacement operation"));
    assert!(err.contains("index.js"));
    assert!(err.contains("required"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_required_artifact_move() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "mv runtime/business-os/installed-modules/inventory/index.js runtime/business-os/installed-modules/inventory/index.mjs",
        root.path(),
    )
    .expect("moving required App Creator artifacts should be blocked");

    assert!(err.contains("destructive or replacement operation"));
    assert!(err.contains("index.js"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_required_artifact_copy_replacement() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "cp runtime/business-os/installed-modules/inventory/core/records.mjs runtime/business-os/installed-modules/inventory/index.js",
        root.path(),
    )
    .expect("copy replacements for required App Creator artifacts should be blocked");

    assert!(err.contains("destructive or replacement operation"));
    assert!(err.contains("records.mjs"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_module_directory_removal() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "rm -rf runtime/business-os/installed-modules/inventory",
        root.path(),
    )
    .expect("removing generated App Creator module directories should be blocked");

    assert!(err.contains("destructive or replacement operation"));
    assert!(err.contains("runtime/business-os/installed-modules/inventory"));
    Ok(())
}

#[test]
fn business_os_guard_allows_scaffold_runtime_core_helpers() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    assert!(
        business_os_app_root_artifact_write_guard(
            "cat > runtime/business-os/installed-modules/inventory/core/records.mjs <<'EOF'\nexport const records = [];\nEOF",
            root.path(),
        )
        .is_none()
    );
    assert!(
        business_os_app_root_artifact_write_guard(
            "cat > runtime/business-os/installed-modules/inventory/core/automation.mjs <<'EOF'\nexport function buildFollowUpCommand() { return {}; }\nEOF",
            root.path(),
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn business_os_guard_allows_source_module_core_helpers() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    assert!(
        business_os_app_root_artifact_write_guard(
            "cat > src/apps/business-os/modules/inventory/core/render.mjs <<'EOF'\nexport function render() {}\nEOF",
            root.path(),
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn business_os_guard_allows_node_module_tests() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    assert!(business_os_app_root_artifact_write_guard(
        "node --test runtime/business-os/installed-modules/inventory/tests/inventory.test.mjs",
        root.path(),
    )
    .is_none());
    Ok(())
}

#[test]
fn business_os_guard_blocks_oversized_module_heredoc_rewrite() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let body = (0..190)
        .map(|idx| format!("console.log({idx});"))
        .collect::<Vec<_>>()
        .join("\n");
    let command = format!(
        "cat > runtime/business-os/installed-modules/inventory/index.js <<'EOF'\n{body}\nEOF"
    );

    let err = business_os_app_root_artifact_write_guard(&command, root.path())
        .expect("oversized module heredoc rewrite should be blocked");

    assert!(err.contains("oversized heredoc rewrite"));
    assert!(err.contains("runtime/business-os/installed-modules/inventory/index.js"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_source_tree_installed_module_write() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "MODULE_DIR=src/apps/business-os/installed-modules/inventory && cat > \"$MODULE_DIR/module.json\" <<'JSON'\n{}\nJSON",
        root.path(),
    )
    .expect("source-tree installed module write should be blocked");

    assert!(err.contains("source-tree installed module path"));
    assert!(err.contains("runtime/business-os/installed-modules/<module_id>/"));
    Ok(())
}

#[test]
fn business_os_guard_allows_module_dir_manifest_write() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    assert!(
        business_os_app_root_artifact_write_guard(
            "MODULE_DIR=runtime/business-os/installed-modules/inventory && cat > \"$MODULE_DIR/module.json\" <<'JSON'\n{}\nJSON",
            root.path(),
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn business_os_guard_blocks_runtime_installed_legacy_manifest_contract() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let command = r#"cat > runtime/business-os/installed-modules/inventory/module.json <<'JSON'
{
  "id": "inventory",
  "name": "Inventory",
  "version": "1.0.0",
  "entry": "index.js",
  "layout": { "type": "shell" },
  "store": { "distribution": "installable", "installable": true },
  "collections": [{ "name": "inventory_items" }]
}
JSON"#;

    let err = business_os_app_root_artifact_write_guard(command, root.path())
        .expect("legacy runtime manifest contract should be blocked");

    assert!(err.contains("legacy or source-style runtime module contract"));
    assert!(err.contains("module.json"));
    assert!(err.contains("ctox-runtime-installed-module"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_runtime_legacy_collections_schema_contract() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let command = r#"cat > runtime/business-os/installed-modules/inventory/collections.schema.json <<'JSON'
{
  "format": "es-module",
  "collections": [{ "name": "inventory_items", "primaryKey": "id" }]
}
JSON"#;

    let err = business_os_app_root_artifact_write_guard(command, root.path())
        .expect("legacy runtime collections schema contract should be blocked");

    assert!(err.contains("legacy or source-style runtime module contract"));
    assert!(err.contains("collections.schema.json"));
    assert!(err.contains("schema_format"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_runtime_schema_js_legacy_schema_format() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let command = r#"cat > runtime/business-os/installed-modules/inventory/schema.js <<'EOF'
export const schemaFormat = 'es-module';
export const collections = {};
EOF"#;

    let err = business_os_app_root_artifact_write_guard(command, root.path())
        .expect("legacy schema.js wrapper should be blocked");

    assert!(err.contains("legacy or source-style runtime module contract"));
    assert!(err.contains("schema.js"));
    Ok(())
}

#[test]
fn business_os_guard_blocks_runtime_node_stat_self_audit() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "node -e 'const fs=require(\"fs\"); const p=\"runtime/business-os/installed-modules/inventory/index.js\"; console.log(fs.statSync(p).size)'",
        root.path(),
    )
    .expect("node stat readback of generated runtime module artifact should be blocked");

    assert!(err.contains("generated-module self-audit readback"));
    assert!(err.contains("index.js"));
    Ok(())
}

#[test]
fn business_os_guard_allows_runtime_manifest_json_parse_check() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    assert!(business_os_app_root_artifact_write_guard(
        "node -e \"const fs=require('fs'); for (const f of ['runtime/business-os/installed-modules/inventory/module.json','runtime/business-os/installed-modules/inventory/collections.schema.json']) JSON.parse(fs.readFileSync(f,'utf8')); console.log('module JSON OK')\"",
        root.path(),
    )
    .is_none());
    Ok(())
}

#[test]
fn business_os_guard_blocks_runtime_mkdir_only_scaffold_probe() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    let err = business_os_app_root_artifact_write_guard(
        "mkdir -p runtime/business-os/installed-modules/inventory/core runtime/business-os/installed-modules/inventory/locales runtime/business-os/installed-modules/inventory/tests",
        root.path(),
    )
    .expect("mkdir-only scaffold probe should be blocked");

    assert!(err.contains("scaffold-directory probe"));
    assert!(err.contains("runtime/business-os/installed-modules/inventory"));
    Ok(())
}

#[test]
fn business_os_guard_allows_small_module_heredoc_without_readback() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;

    assert!(
        business_os_app_root_artifact_write_guard(
            "cat > runtime/business-os/installed-modules/subscriptions/locales/en.json <<'JSON'\n{\"title\":\"Subscriptions\"}\nJSON",
            root.path(),
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn business_os_cleanup_removes_new_root_artifacts_after_exec() -> anyhow::Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("src/apps/business-os"))?;
    let snapshot = business_os_app_root_artifact_snapshot(root.path())
        .expect("expected Business OS workspace snapshot");

    fs::write(root.path().join("module.json"), "{}\n")?;
    fs::write(root.path().join("collections.schema.json"), "{}\n")?;
    fs::write(root.path().join("harness-module.json"), "{}\n")?;
    fs::write(root.path().join("harness-collections.schema.json"), "{}\n")?;
    fs::write(root.path().join("harness-artifact-status.md"), "blocked\n")?;
    fs::write(root.path().join("_test_guard.txt"), "probe\n")?;

    let message = cleanup_new_business_os_app_root_artifacts(Some(&snapshot))
        .expect("cleanup should report removed root artifacts");
    assert!(
        message.contains(
            "Removed forbidden root file(s): _test_guard.txt, collections.schema.json, harness-artifact-status.md, harness-collections.schema.json, harness-module.json, module.json"
        )
    );
    assert!(!root.path().join("module.json").exists());
    assert!(!root.path().join("collections.schema.json").exists());
    assert!(!root.path().join("harness-module.json").exists());
    assert!(!root.path().join("harness-collections.schema.json").exists());
    assert!(!root.path().join("harness-artifact-status.md").exists());
    assert!(!root.path().join("_test_guard.txt").exists());
    Ok(())
}
