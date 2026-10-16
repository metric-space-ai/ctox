use super::*;
use crate::config_loader::ConfigLayerEntry;
use crate::config_loader::ConfigLayerStack;
use crate::config_loader::ConfigRequirements;
use crate::config_loader::ConfigRequirementsToml;
use ctox_app_server_protocol::ConfigLayerSource;
use ctox_protocol::permissions::FileSystemAccessMode;
use ctox_protocol::permissions::FileSystemPath;
use ctox_protocol::permissions::FileSystemSandboxEntry;
use ctox_protocol::permissions::FileSystemSpecialPath;
use ctox_protocol::protocol::AskForApproval;
use ctox_protocol::protocol::SandboxPolicy;
use ctox_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;
use toml::Value as TomlValue;

fn config_stack_for_dot_codex_folder(dot_codex_folder: &Path) -> ConfigLayerStack {
    let dot_codex_folder =
        AbsolutePathBuf::from_absolute_path(dot_codex_folder).expect("absolute dot_codex_folder");
    let layer = ConfigLayerEntry::new(
        ConfigLayerSource::Project { dot_codex_folder },
        TomlValue::Table(Default::default()),
    );
    ConfigLayerStack::new(
        vec![layer],
        ConfigRequirements::default(),
        ConfigRequirementsToml::default(),
    )
    .expect("ConfigLayerStack")
}

fn host_absolute_path(segments: &[&str]) -> String {
    let mut path = if cfg!(windows) {
        PathBuf::from(r"C:\")
    } else {
        PathBuf::from("/")
    };
    for segment in segments {
        path.push(segment);
    }
    path.to_string_lossy().into_owned()
}

fn host_program_path(name: &str) -> String {
    let executable_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    host_absolute_path(&["usr", "bin", &executable_name])
}

fn starlark_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn read_only_file_system_sandbox_policy() -> FileSystemSandboxPolicy {
    FileSystemSandboxPolicy::restricted(vec![FileSystemSandboxEntry {
        path: FileSystemPath::Special {
            value: FileSystemSpecialPath::Root,
        },
        access: FileSystemAccessMode::Read,
    }])
}

fn unrestricted_file_system_sandbox_policy() -> FileSystemSandboxPolicy {
    FileSystemSandboxPolicy::unrestricted()
}

#[tokio::test]
async fn returns_empty_policy_when_no_policy_files_exist() {
    let temp_dir = tempdir().expect("create temp dir");
    let config_stack = config_stack_for_dot_codex_folder(temp_dir.path());

    let manager = ExecPolicyManager::load(&config_stack)
        .await
        .expect("manager result");
    let policy = manager.current();

    let commands = [vec!["rm".to_string()]];
    assert_eq!(
        Evaluation {
            decision: Decision::Allow,
            matched_rules: vec![RuleMatch::HeuristicsRuleMatch {
                command: vec!["rm".to_string()],
                decision: Decision::Allow
            }],
        },
        policy.check_multiple(commands.iter(), &|_| Decision::Allow)
    );
    assert!(!temp_dir.path().join(RULES_DIR_NAME).exists());
}

#[tokio::test]
async fn collect_policy_files_returns_empty_when_dir_missing() {
    let temp_dir = tempdir().expect("create temp dir");

    let policy_dir = temp_dir.path().join(RULES_DIR_NAME);
    let files = collect_policy_files(&policy_dir)
        .await
        .expect("collect policy files");

    assert!(files.is_empty());
}




#[tokio::test]
async fn loads_policies_from_policy_subdirectory() {
    let temp_dir = tempdir().expect("create temp dir");
    let config_stack = config_stack_for_dot_codex_folder(temp_dir.path());
    let policy_dir = temp_dir.path().join(RULES_DIR_NAME);
    fs::create_dir_all(&policy_dir).expect("create policy dir");
    fs::write(
        policy_dir.join("deny.rules"),
        r#"prefix_rule(pattern=["rm"], decision="forbidden")"#,
    )
    .expect("write policy file");

    let policy = load_exec_policy(&config_stack)
        .await
        .expect("policy result");
    let command = [vec!["rm".to_string()]];
    assert_eq!(
        Evaluation {
            decision: Decision::Forbidden,
            matched_rules: vec![RuleMatch::PrefixRuleMatch {
                matched_prefix: vec!["rm".to_string()],
                decision: Decision::Forbidden,
                resolved_program: None,
                justification: None,
            }],
        },
        policy.check_multiple(command.iter(), &|_| Decision::Allow)
    );
}



#[tokio::test]
async fn ignores_policies_outside_policy_dir() {
    let temp_dir = tempdir().expect("create temp dir");
    let config_stack = config_stack_for_dot_codex_folder(temp_dir.path());
    fs::write(
        temp_dir.path().join("root.rules"),
        r#"prefix_rule(pattern=["ls"], decision="prompt")"#,
    )
    .expect("write policy file");

    let policy = load_exec_policy(&config_stack)
        .await
        .expect("policy result");
    let command = [vec!["ls".to_string()]];
    assert_eq!(
        Evaluation {
            decision: Decision::Allow,
            matched_rules: vec![RuleMatch::HeuristicsRuleMatch {
                command: vec!["ls".to_string()],
                decision: Decision::Allow
            }],
        },
        policy.check_multiple(command.iter(), &|_| Decision::Allow)
    );
}

#[tokio::test]
async fn ignores_rules_from_untrusted_project_layers() -> anyhow::Result<()> {
    let project_dir = tempdir()?;
    let policy_dir = project_dir.path().join(RULES_DIR_NAME);
    fs::create_dir_all(&policy_dir)?;
    fs::write(
        policy_dir.join("untrusted.rules"),
        r#"prefix_rule(pattern=["ls"], decision="forbidden")"#,
    )?;

    let project_dot_codex_folder = AbsolutePathBuf::from_absolute_path(project_dir.path())?;
    let layers = vec![ConfigLayerEntry::new_disabled(
        ConfigLayerSource::Project {
            dot_codex_folder: project_dot_codex_folder,
        },
        TomlValue::Table(Default::default()),
        "marked untrusted",
    )];
    let config_stack = ConfigLayerStack::new(
        layers,
        ConfigRequirements::default(),
        ConfigRequirementsToml::default(),
    )?;

    let policy = load_exec_policy(&config_stack).await?;

    assert_eq!(
        Evaluation {
            decision: Decision::Allow,
            matched_rules: vec![RuleMatch::HeuristicsRuleMatch {
                command: vec!["ls".to_string()],
                decision: Decision::Allow,
            }],
        },
        policy.check_multiple([vec!["ls".to_string()]].iter(), &|_| Decision::Allow)
    );
    Ok(())
}

#[tokio::test]
async fn loads_policies_from_multiple_config_layers() -> anyhow::Result<()> {
    let user_dir = tempdir()?;
    let project_dir = tempdir()?;

    let user_policy_dir = user_dir.path().join(RULES_DIR_NAME);
    fs::create_dir_all(&user_policy_dir)?;
    fs::write(
        user_policy_dir.join("user.rules"),
        r#"prefix_rule(pattern=["rm"], decision="forbidden")"#,
    )?;

    let project_policy_dir = project_dir.path().join(RULES_DIR_NAME);
    fs::create_dir_all(&project_policy_dir)?;
    fs::write(
        project_policy_dir.join("project.rules"),
        r#"prefix_rule(pattern=["ls"], decision="prompt")"#,
    )?;

    let user_config_toml =
        AbsolutePathBuf::from_absolute_path(user_dir.path().join("config.toml"))?;
    let project_dot_codex_folder = AbsolutePathBuf::from_absolute_path(project_dir.path())?;
    let layers = vec![
        ConfigLayerEntry::new(
            ConfigLayerSource::User {
                file: user_config_toml,
            },
            TomlValue::Table(Default::default()),
        ),
        ConfigLayerEntry::new(
            ConfigLayerSource::Project {
                dot_codex_folder: project_dot_codex_folder,
            },
            TomlValue::Table(Default::default()),
        ),
    ];
    let config_stack = ConfigLayerStack::new(
        layers,
        ConfigRequirements::default(),
        ConfigRequirementsToml::default(),
    )?;

    let policy = load_exec_policy(&config_stack).await?;

    assert_eq!(
        Evaluation {
            decision: Decision::Forbidden,
            matched_rules: vec![RuleMatch::PrefixRuleMatch {
                matched_prefix: vec!["rm".to_string()],
                decision: Decision::Forbidden,
                resolved_program: None,
                justification: None,
            }],
        },
        policy.check_multiple([vec!["rm".to_string()]].iter(), &|_| Decision::Allow)
    );
    assert_eq!(
        Evaluation {
            decision: Decision::Prompt,
            matched_rules: vec![RuleMatch::PrefixRuleMatch {
                matched_prefix: vec!["ls".to_string()],
                decision: Decision::Prompt,
                resolved_program: None,
                justification: None,
            }],
        },
        policy.check_multiple([vec!["ls".to_string()]].iter(), &|_| Decision::Allow)
    );
    Ok(())
}

#[tokio::test]
async fn evaluates_bash_lc_inner_commands() {
    let policy_src = r#"
prefix_rule(pattern=["rm"], decision="forbidden")
"#;
    let mut parser = PolicyParser::new();
    parser
        .parse("test.rules", policy_src)
        .expect("parse policy");
    let policy = Arc::new(parser.build());

    let forbidden_script = vec![
        "bash".to_string(),
        "-lc".to_string(),
        "rm -rf /some/important/folder".to_string(),
    ];

    let manager = ExecPolicyManager::new(policy);
    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &forbidden_script,
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: &SandboxPolicy::DangerFullAccess,
            file_system_sandbox_policy: &unrestricted_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        })
        .await;

    assert_eq!(
            requirement,
            ExecApprovalRequirement::Forbidden {
                reason: "`bash -lc 'rm -rf /some/important/folder'` rejected: policy forbids commands starting with `rm`".to_string()
            }
        );
}

#[test]
fn commands_for_exec_policy_falls_back_for_empty_shell_script() {
    let command = vec!["bash".to_string(), "-lc".to_string(), "".to_string()];

    assert_eq!(commands_for_exec_policy(&command), (vec![command], false));
}

#[test]
fn commands_for_exec_policy_falls_back_for_whitespace_shell_script() {
    let command = vec![
        "bash".to_string(),
        "-lc".to_string(),
        "  \n\t  ".to_string(),
    ];

    assert_eq!(commands_for_exec_policy(&command), (vec![command], false));
}

#[tokio::test]
async fn evaluates_heredoc_script_against_prefix_rules() {
    let policy_src = r#"prefix_rule(pattern=["python3"], decision="allow")"#;
    let mut parser = PolicyParser::new();
    parser
        .parse("test.rules", policy_src)
        .expect("parse policy");
    let policy = Arc::new(parser.build());
    let command = vec![
        "bash".to_string(),
        "-lc".to_string(),
        "python3 <<'PY'\nprint('hello')\nPY".to_string(),
    ];

    let requirement = ExecPolicyManager::new(policy)
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &command,
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: &SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: &read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        })
        .await;

    assert_eq!(
        requirement,
        ExecApprovalRequirement::Skip {
            bypass_sandbox: true,
            proposed_execpolicy_amendment: None,
        }
    );
}



#[tokio::test]
async fn justification_is_included_in_forbidden_exec_approval_requirement() {
    let policy_src = r#"
prefix_rule(
    pattern=["rm"],
    decision="forbidden",
    justification="destructive command",
)
"#;
    let mut parser = PolicyParser::new();
    parser
        .parse("test.rules", policy_src)
        .expect("parse policy");
    let policy = Arc::new(parser.build());

    let manager = ExecPolicyManager::new(policy);
    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &[
                "rm".to_string(),
                "-rf".to_string(),
                "/some/important/folder".to_string(),
            ],
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: &SandboxPolicy::DangerFullAccess,
            file_system_sandbox_policy: &unrestricted_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        })
        .await;

    assert_eq!(
        requirement,
        ExecApprovalRequirement::Forbidden {
            reason: "`rm -rf /some/important/folder` rejected: destructive command".to_string()
        }
    );
}

#[tokio::test]
async fn exec_approval_requirement_prefers_execpolicy_match() {
    let policy_src = r#"prefix_rule(pattern=["rm"], decision="prompt")"#;
    let mut parser = PolicyParser::new();
    parser
        .parse("test.rules", policy_src)
        .expect("parse policy");
    let policy = Arc::new(parser.build());
    let command = vec!["rm".to_string()];

    let manager = ExecPolicyManager::new(policy);
    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &command,
            approval_policy: AskForApproval::OnRequest,
            sandbox_policy: &SandboxPolicy::DangerFullAccess,
            file_system_sandbox_policy: &unrestricted_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        })
        .await;

    assert_eq!(
        requirement,
        ExecApprovalRequirement::NeedsApproval {
            reason: Some("`rm` requires approval by policy".to_string()),
            proposed_execpolicy_amendment: None,
        }
    );
}

#[tokio::test]
async fn absolute_path_exec_approval_requirement_matches_host_executable_rules() {
    let git_path = host_program_path("git");
    let git_path_literal = starlark_string(&git_path);
    let policy_src = format!(
        r#"
host_executable(name = "git", paths = ["{git_path_literal}"])
prefix_rule(pattern=["git"], decision="allow")
"#
    );
    let mut parser = PolicyParser::new();
    parser
        .parse("test.rules", &policy_src)
        .expect("parse policy");
    let manager = ExecPolicyManager::new(Arc::new(parser.build()));
    let command = vec![git_path, "status".to_string()];

    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &command,
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: &SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: &read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        })
        .await;

    assert_eq!(
        requirement,
        ExecApprovalRequirement::Skip {
            bypass_sandbox: true,
            proposed_execpolicy_amendment: None,
        }
    );
}

#[tokio::test]
async fn absolute_path_exec_approval_requirement_ignores_disallowed_host_executable_paths() {
    let allowed_git_path = host_program_path("git");
    let disallowed_git_path = host_absolute_path(&[
        "opt",
        "homebrew",
        "bin",
        if cfg!(windows) { "git.exe" } else { "git" },
    ]);
    let allowed_git_path_literal = starlark_string(&allowed_git_path);
    let policy_src = format!(
        r#"
host_executable(name = "git", paths = ["{allowed_git_path_literal}"])
prefix_rule(pattern=["git"], decision="prompt")
"#
    );
    let mut parser = PolicyParser::new();
    parser
        .parse("test.rules", &policy_src)
        .expect("parse policy");
    let manager = ExecPolicyManager::new(Arc::new(parser.build()));
    let command = vec![disallowed_git_path, "status".to_string()];

    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &command,
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: &SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: &read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        })
        .await;

    assert_eq!(
        requirement,
        ExecApprovalRequirement::Skip {
            bypass_sandbox: false,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(command)),
        }
    );
}









#[tokio::test]
async fn empty_bash_lc_script_falls_back_to_original_command() {
    let command = vec!["bash".to_string(), "-lc".to_string(), "".to_string()];

    let manager = ExecPolicyManager::default();
    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &command,
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: &SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: &read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        })
        .await;

    assert_eq!(
        requirement,
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(command)),
        }
    );
}

#[tokio::test]
async fn whitespace_bash_lc_script_falls_back_to_original_command() {
    let command = vec![
        "bash".to_string(),
        "-lc".to_string(),
        "  \n\t  ".to_string(),
    ];

    let manager = ExecPolicyManager::default();
    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &command,
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: &SandboxPolicy::new_read_only_policy(),
            file_system_sandbox_policy: &read_only_file_system_sandbox_policy(),
            sandbox_permissions: SandboxPermissions::UseDefault,
            prefix_rule: None,
        })
        .await;

    assert_eq!(
        requirement,
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: Some(ExecPolicyAmendment::new(command)),
        }
    );
}
