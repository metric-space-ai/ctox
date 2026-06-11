//! Skill catalog loading for the Skills page.
use super::*;

pub(super) fn load_skill_catalog(root: &Path) -> Vec<SkillCatalogEntry> {
    let _ = crate::skill_store::bootstrap_embedded_system_skills(root);
    let _ = crate::skill_store::bootstrap_from_roots(root);
    let mut catalog = crate::skill_store::list_skill_bundles(root)
        .unwrap_or_default()
        .into_iter()
        .map(|bundle| {
            let files =
                crate::skill_store::list_skill_files(root, &bundle.skill_id).unwrap_or_default();
            let helper_tools = files
                .iter()
                .filter_map(|file| file.relative_path.strip_prefix("scripts/"))
                .filter(|value| !value.contains('/'))
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            let resources = summarize_skill_resources(&files);
            SkillCatalogEntry {
                name: bundle.skill_name,
                class: skill_class_from_store(&bundle.class),
                state: skill_state_from_store(&bundle.state),
                cluster: bundle.cluster,
                skill_path: bundle
                    .source_path
                    .as_deref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(format!("sqlite://{}", bundle.skill_id))),
                description: bundle.description,
                helper_tools,
                resources,
            }
        })
        .collect::<Vec<_>>();
    catalog.sort_by(|left, right| {
        left.class
            .rank()
            .cmp(&right.class.rank())
            .then(left.cluster.cmp(&right.cluster))
            .then(left.name.cmp(&right.name))
            .then(left.skill_path.cmp(&right.skill_path))
    });
    catalog
}

pub(super) fn skill_class_from_store(value: &str) -> SkillClass {
    match value.trim() {
        "codex_core" => SkillClass::CodexCore,
        "installed_packs" => SkillClass::InstalledPacks,
        "personal" => SkillClass::Personal,
        _ => SkillClass::CtoxCore,
    }
}

pub(super) fn skill_state_from_store(value: &str) -> SkillState {
    match value.trim() {
        "authored" => SkillState::Authored,
        "generated" => SkillState::Generated,
        "draft" => SkillState::Draft,
        _ => SkillState::Stable,
    }
}

pub(super) fn summarize_skill_resources(
    files: &[crate::skill_store::SkillFileView],
) -> Vec<String> {
    let mut groups: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for prefix in ["references/", "assets/", "templates/", "agents/"] {
        groups.insert(prefix, Vec::new());
    }
    for file in files {
        for prefix in ["references/", "assets/", "templates/", "agents/"] {
            if let Some(stripped) = file.relative_path.strip_prefix(prefix) {
                let name = stripped.split('/').next().unwrap_or(stripped).to_string();
                let group = groups.get_mut(prefix).expect("group inserted");
                if !group.contains(&name) {
                    group.push(name);
                }
            }
        }
    }
    let mut out = Vec::new();
    for (prefix, mut entries) in groups {
        if entries.is_empty() {
            continue;
        }
        entries.sort();
        let label = prefix.trim_end_matches('/');
        let preview = entries
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if entries.len() > 5 {
            format!(" (+{} more)", entries.len() - 5)
        } else {
            String::new()
        };
        out.push(format!("{label}: {preview}{suffix}"));
    }
    out
}
