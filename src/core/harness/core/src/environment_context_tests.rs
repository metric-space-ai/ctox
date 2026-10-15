use crate::shell::ShellType;

use super::*;
use core_test_support::test_path_buf;
use pretty_assertions::assert_eq;

fn fake_shell() -> Shell {
    Shell {
        shell_type: ShellType::Bash,
        shell_path: PathBuf::from("/bin/bash"),
        shell_snapshot: crate::shell::empty_shell_snapshot_receiver(),
    }
}







#[test]
fn equals_except_shell_compares_cwd() {
    let context1 = EnvironmentContext::new(
        Some(PathBuf::from("/repo")),
        fake_shell(),
        None,
        None,
        None,
        None,
    );
    let context2 = EnvironmentContext::new(
        Some(PathBuf::from("/repo")),
        fake_shell(),
        None,
        None,
        None,
        None,
    );
    assert!(context1.equals_except_shell(&context2));
}

#[test]
fn equals_except_shell_ignores_sandbox_policy() {
    let context1 = EnvironmentContext::new(
        Some(PathBuf::from("/repo")),
        fake_shell(),
        None,
        None,
        None,
        None,
    );
    let context2 = EnvironmentContext::new(
        Some(PathBuf::from("/repo")),
        fake_shell(),
        None,
        None,
        None,
        None,
    );

    assert!(context1.equals_except_shell(&context2));
}

#[test]
fn equals_except_shell_compares_cwd_differences() {
    let context1 = EnvironmentContext::new(
        Some(PathBuf::from("/repo1")),
        fake_shell(),
        None,
        None,
        None,
        None,
    );
    let context2 = EnvironmentContext::new(
        Some(PathBuf::from("/repo2")),
        fake_shell(),
        None,
        None,
        None,
        None,
    );

    assert!(!context1.equals_except_shell(&context2));
}

#[test]
fn equals_except_shell_ignores_shell() {
    let context1 = EnvironmentContext::new(
        Some(PathBuf::from("/repo")),
        Shell {
            shell_type: ShellType::Bash,
            shell_path: "/bin/bash".into(),
            shell_snapshot: crate::shell::empty_shell_snapshot_receiver(),
        },
        None,
        None,
        None,
        None,
    );
    let context2 = EnvironmentContext::new(
        Some(PathBuf::from("/repo")),
        Shell {
            shell_type: ShellType::Zsh,
            shell_path: "/bin/zsh".into(),
            shell_snapshot: crate::shell::empty_shell_snapshot_receiver(),
        },
        None,
        None,
        None,
        None,
    );

    assert!(context1.equals_except_shell(&context2));
}
