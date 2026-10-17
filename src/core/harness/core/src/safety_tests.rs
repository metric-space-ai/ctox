use super::*;
use ctox_utils_absolute_path::AbsolutePathBuf;
use tempfile::TempDir;

#[test]
fn test_writable_roots_constraint() {
    // Use a temporary directory as our workspace to avoid touching
    // the real current working directory.
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path().to_path_buf();
    let parent = cwd.parent().unwrap().to_path_buf();

    // Helper to build a single‑entry patch that adds a file at `p`.
    let make_add_change = |p: PathBuf| ApplyPatchAction::new_add_for_test(&p, "".to_string());

    let add_inside = make_add_change(cwd.join("inner.txt"));
    let add_outside = make_add_change(parent.join("outside.txt"));

    // Policy limited to the workspace only; exclude system temp roots so
    // only `cwd` is writable by default.
    let policy_workspace_only = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: Default::default(),
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    };

    assert!(is_write_patch_constrained_to_writable_paths(
        &add_inside,
        &FileSystemSandboxPolicy::from(&policy_workspace_only),
        &cwd,
    ));

    assert!(!is_write_patch_constrained_to_writable_paths(
        &add_outside,
        &FileSystemSandboxPolicy::from(&policy_workspace_only),
        &cwd,
    ));

    // With the parent dir explicitly added as a writable root, the
    // outside write should be permitted.
    let policy_with_parent = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![AbsolutePathBuf::try_from(parent).unwrap()],
        read_only_access: Default::default(),
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    };
    assert!(is_write_patch_constrained_to_writable_paths(
        &add_outside,
        &FileSystemSandboxPolicy::from(&policy_with_parent),
        &cwd,
    ));
}
