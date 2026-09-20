// Origin: CTOX
// License: Apache-2.0
//! Whole-directory writeback for bounded edits of operator-owned local apps.
use super::*;

pub(crate) fn apply_local_coding_changes(
    root: &Path,
    module_id: &str,
    baseline: &serde_json::Map<String, Value>,
    changes: &[(&str, &str, Option<&str>)],
    revalidate: &dyn Fn() -> anyhow::Result<()>,
) -> anyhow::Result<Option<Vec<String>>> {
    apply_local_coding_changes_with_validator(
        root,
        module_id,
        baseline,
        changes,
        revalidate,
        &validate_local_coding_stage,
    )
}

fn apply_local_coding_changes_with_validator(
    root: &Path,
    module_id: &str,
    baseline: &serde_json::Map<String, Value>,
    changes: &[(&str, &str, Option<&str>)],
    revalidate: &dyn Fn() -> anyhow::Result<()>,
    validate: &dyn Fn(&Path, &str, &Path) -> anyhow::Result<()>,
) -> anyhow::Result<Option<Vec<String>>> {
    let app_root = resolve_business_os_app_root(root)?;
    let (live, source_app_root) = resolve_module_source_root_for_root(root, &app_root, module_id)?;
    if live
        .parent()
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        != Some("local-modules")
    {
        return Ok(None);
    }
    if changes.is_empty() {
        revalidate()?;
        return Ok(Some(Vec::new()));
    }
    let _lease = local_source_write_lease(&live)?;
    for (path, content) in baseline {
        ensure_module_source_record_current(root, module_id, path, content.as_str())?;
    }
    let parent = live.parent().context("local source parent")?;
    let release_id = Uuid::new_v4().to_string();
    let workspace = parent.join(format!(".coding-stage-{release_id}"));
    let staged = workspace
        .join("runtime/business-os/local-modules")
        .join(module_id);
    let backup = parent.join(format!(".coding-backup-{module_id}-{release_id}"));
    let original = source_tree_fingerprint(&live)?;
    let result = (|| -> anyhow::Result<Vec<String>> {
        copy_dir_recursive(&live, &staged)?;
        let mut applied = Vec::new();
        for (path, content, before) in changes {
            let rel = normalize_source_relative_path(path)?;
            anyhow::ensure!(
                is_allowed_source_path(&rel),
                "disallowed local coding source path"
            );
            // This bounded path edits app behavior, never ownership, schema or lifecycle.
            anyhow::ensure!(
                !matches!(
                    rel.to_str(),
                    Some("module.json" | "collections.schema.json")
                ),
                "local coding turns cannot change the manifest or collection schema"
            );
            ensure_module_source_record_current(root, module_id, path, *before)?;
            let target = staged.join(&rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(target, content)?;
            applied.push((*path).to_owned());
        }
        validate(root, module_id, &workspace)?;
        // The model and validator are less privileged and may take time. Refresh
        // authority and the complete live tree immediately before publication.
        revalidate()?;
        anyhow::ensure!(
            source_tree_fingerprint(&live)? == original,
            "local source changed during coding validation"
        );
        let (selected, _) = resolve_module_source_root_for_root(root, &app_root, module_id)?;
        anyhow::ensure!(
            selected == live,
            "local source provenance changed during coding turn"
        );
        let baseline_version = record_module_version(
            root,
            &source_app_root,
            module_id,
            "coding_baseline",
            "Before bounded local coding turn",
            "",
        )?
        .context("local coding rollback version was not recorded")?;
        let receipt_path = parent.join(format!(".coding-receipt-{release_id}.json"));
        let mut receipt = serde_json::json!({
            "contract": "ctox-local-coding-writeback-v1", "module_id": module_id,
            "state": "prepared", "live": live, "backup": backup,
            "baseline_version": baseline_version, "applied_files": applied
        });
        fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt)?)?;
        fs::rename(&live, &backup)?;
        if let Err(error) = fs::rename(&staged, &live) {
            fs::rename(&backup, &live).with_context(|| {
                format!(
                    "activation failed ({error}); restore failed; retained backup {}",
                    backup.display()
                )
            })?;
            return Err(error.into());
        }
        // Retain the complete original directory even if a subsequent metadata
        // or projection write fails. Recovery evidence must outlive this call.
        receipt["state"] = Value::String("activated".into());
        fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt)?)?;
        record_module_version(
            root,
            &source_app_root,
            module_id,
            "coding_turn",
            "Validated bounded local coding turn",
            "",
        )?;
        load_module_source_records(
            root,
            &ModuleSourceLoadMutation {
                module_id: module_id.to_owned(),
            },
        )?;
        receipt["state"] = Value::String("complete".into());
        fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt)?)?;
        Ok(applied)
    })();
    let _ = fs::remove_dir_all(&workspace);
    result.map(Some).with_context(|| {
        format!(
            "local coding release {release_id}; any activation backup is retained at {}",
            backup.display()
        )
    })
}

pub(crate) struct LocalSourceWriteLease(PathBuf);
impl Drop for LocalSourceWriteLease {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(crate) fn local_source_write_lease(
    module_root: &Path,
) -> anyhow::Result<Option<LocalSourceWriteLease>> {
    let parent = module_root.parent().context("module source parent")?;
    if parent.file_name().and_then(|s| s.to_str()) != Some("local-modules") {
        return Ok(None);
    }
    let module_id = module_root
        .file_name()
        .context("module source name")?
        .to_string_lossy();
    let lock = parent.join(format!(".coding-write-{module_id}.lock"));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .with_context(|| {
            format!(
                "local source writer is active or requires recovery: {}",
                lock.display()
            )
        })?;
    Ok(Some(LocalSourceWriteLease(lock)))
}

fn source_tree_fingerprint(root: &Path) -> anyhow::Result<BTreeMap<PathBuf, String>> {
    fn walk(
        root: &Path,
        dir: &Path,
        result: &mut BTreeMap<PathBuf, String>,
        bytes: &mut u64,
    ) -> anyhow::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            anyhow::ensure!(
                !kind.is_symlink(),
                "local coding does not accept symlinked source assets"
            );
            if kind.is_dir() {
                walk(root, &entry.path(), result, bytes)?;
            } else {
                anyhow::ensure!(kind.is_file(), "unsupported local source entry");
                *bytes += entry.metadata()?.len();
                anyhow::ensure!(
                    *bytes <= 64 * 1024 * 1024 && result.len() < 4096,
                    "local coding source exceeds bounded staging limits"
                );
                result.insert(
                    entry.path().strip_prefix(root)?.to_owned(),
                    hex_sha256(&fs::read(entry.path())?),
                );
            }
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result, &mut 0)?;
    Ok(result)
}

#[cfg(not(unix))]
fn validate_local_coding_stage(
    _root: &Path,
    _module_id: &str,
    _workspace: &Path,
) -> anyhow::Result<()> {
    anyhow::bail!("bounded local coding validation requires Unix process supervision")
}

#[cfg(unix)]
fn validate_local_coding_stage(
    root: &Path,
    module_id: &str,
    workspace: &Path,
) -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};
    struct Validator(Child);
    impl Drop for Validator {
        fn drop(&mut self) {
            // This process group was created exclusively for this validation.
            unsafe {
                libc::kill(-(self.0.id() as i32), libc::SIGKILL);
            }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let script = root.join("src/apps/business-os/scripts/validate-app-module.mjs");
    anyhow::ensure!(script.is_file(), "local coding validator unavailable");
    let mut child = Validator(
        Command::new(crate::service::business_os::resolve_business_os_validator_node(root))
            .arg(script)
            .arg(module_id)
            .arg("--local")
            .arg("--skip-tests")
            .arg("--workspace")
            .arg(workspace)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.0.try_wait()? {
            anyhow::ensure!(
                status.success(),
                "local coding staged validator rejected candidate ({status})"
            );
            return Ok(());
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "local coding staged validator exceeded 30 seconds"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path) -> anyhow::Result<(PathBuf, serde_json::Map<String, Value>)> {
        let shell = root.join("src/apps/business-os");
        fs::create_dir_all(&shell)?;
        fs::write(shell.join("index.html"), "")?;
        let live = root.join("runtime/business-os/local-modules/widget");
        fs::create_dir_all(&live)?;
        fs::write(
            live.join("module.json"),
            r#"{"id":"widget","title":"Widget","version":"1.0.0","install_scope":"local","entry":"local-modules/widget/index.html","collections":[]}"#,
        )?;
        fs::write(live.join("index.js"), "old-js")?;
        fs::write(live.join("style.css"), "old-css")?;
        Ok((
            live,
            serde_json::json!({"index.js":"old-js","style.css":"old-css"})
                .as_object()
                .unwrap()
                .clone(),
        ))
    }
    #[test]
    fn local_coding_validation_and_revocation_leave_complete_live_source_unchanged(
    ) -> anyhow::Result<()> {
        for revoked in [false, true] {
            let temp = tempfile::tempdir()?;
            let (live, baseline) = fixture(temp.path())?;
            let before = source_tree_fingerprint(&live)?;
            let result = apply_local_coding_changes_with_validator(
                temp.path(),
                "widget",
                &baseline,
                &[
                    ("index.js", "new-js", Some("old-js")),
                    ("style.css", "new-css", Some("old-css")),
                ],
                &|| {
                    anyhow::ensure!(!revoked, "revoked");
                    Ok(())
                },
                &|_, _, workspace| {
                    let staged = workspace.join("runtime/business-os/local-modules/widget");
                    assert_eq!(fs::read_to_string(staged.join("index.js"))?, "new-js");
                    assert_eq!(fs::read_to_string(staged.join("style.css"))?, "new-css");
                    anyhow::ensure!(revoked, "validator rejected candidate");
                    Ok(())
                },
            );
            assert!(result.is_err());
            assert_eq!(source_tree_fingerprint(&live)?, before);
            assert!(
                local_source_write_lease(&live)?.is_some(),
                "lease released on rejection"
            );
        }
        Ok(())
    }
    #[test]
    fn local_coding_keeps_complete_backup_and_local_provenance() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let (live, baseline) = fixture(temp.path())?;
        let applied = apply_local_coding_changes_with_validator(
            temp.path(),
            "widget",
            &baseline,
            &[
                ("index.js", "new-js", Some("old-js")),
                ("style.css", "new-css", Some("old-css")),
            ],
            &|| Ok(()),
            &|_, _, _| Ok(()),
        )?
        .context("local result")?;
        assert_eq!(applied, vec!["index.js", "style.css"]);
        assert_eq!(fs::read_to_string(live.join("index.js"))?, "new-js");
        assert_eq!(fs::read_to_string(live.join("style.css"))?, "new-css");
        let backups: Vec<_> = fs::read_dir(live.parent().unwrap())?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".coding-backup-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            fs::read_to_string(backups[0].path().join("index.js"))?,
            "old-js"
        );
        assert_eq!(
            fs::read_to_string(backups[0].path().join("style.css"))?,
            "old-css"
        );
        assert!(!temp
            .path()
            .join("runtime/business-os/installed-modules/widget")
            .exists());
        Ok(())
    }
    #[test]
    fn local_coding_rejects_stale_source_schema_changes_and_competing_writer() -> anyhow::Result<()>
    {
        let temp = tempfile::tempdir()?;
        let (live, baseline) = fixture(temp.path())?;
        let lease = local_source_write_lease(&live)?;
        assert!(local_source_write_lease(&live).is_err());
        drop(lease);
        assert!(apply_local_coding_changes_with_validator(
            temp.path(),
            "widget",
            &baseline,
            &[("module.json", "{}", None)],
            &|| Ok(()),
            &|_, _, _| Ok(())
        )
        .is_err());
        fs::write(live.join("style.css"), "newer-source")?;
        assert!(apply_local_coding_changes_with_validator(
            temp.path(),
            "widget",
            &baseline,
            &[("index.js", "new-js", Some("old-js"))],
            &|| Ok(()),
            &|_, _, _| Ok(())
        )
        .is_err());
        assert_eq!(fs::read_to_string(live.join("index.js"))?, "old-js");
        assert_eq!(fs::read_to_string(live.join("style.css"))?, "newer-source");
        Ok(())
    }
}
