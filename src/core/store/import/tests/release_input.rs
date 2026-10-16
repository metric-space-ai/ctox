use super::*;

#[tokio::test]
#[ignore = "qualification: requires copied release inputs, run via scripts/sync-v4/qualify-import.sh"]
async fn actual_a9_base_is_activated_through_registry_and_replay_has_no_new_seq() -> Result<()> {
    let inputs = std::env::var("IMPORT_INPUT_ROOT").expect("copied immutable release required");
    let defaults = schema::BaseReleaseDefaults {
        app_ver: "main@4794af03d+v4".into(),
        client_limit: 100000,
        byte_budget: 33554432,
        rights_ver: 1,
    };
    let plan = base_release::load(
        &std::path::Path::new(&inputs).join("v4"),
        &std::path::Path::new(&inputs).join("ctox-platform.manifest.json"),
        &defaults,
    )?;
    assert!(plan.collections["ctox-platform"]
        .iter()
        .any(|d| d.name == "app_drafts"));
    assert!(!plan.removed_sync_settings.is_empty());
    for definitions in plan.collections.values() {
        for definition in definitions {
            assert!(
                definition.schema_json["properties"].is_object(),
                "A9 schema envelope must be unwrapped: {}",
                definition.name
            );
            assert!(
                definition.schema_json.get("primaryKey").is_some(),
                "primary key missing: {}",
                definition.name
            );
        }
    }
    let root = tempfile::tempdir()?;
    let writer = Registry.open(
        DbId::new(root.path().join("all-apps.sqlite3"))?,
        WriterOptions::default(),
    )?;
    writer.ready().await?;
    base_release::publish(&writer, &plan, &ReleaseMigration, 1).await?;
    let collections = plan.collections.clone();
    let head = writer
        .submit(WriteJob::new(JobOptions::default(), move |tx| {
            for definitions in collections.values() {
                for def in definitions {
                    let actual = sql(schema::active_collection(tx, &def.name))?
                        .expect("published collection must be active");
                    assert_eq!(actual.definition, *def);
                }
            }
            sql(crate::change_log::head(tx))
        }))?
        .await?;
    base_release::publish(&writer, &plan, &ReleaseMigration, 2).await?;
    let after = writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            sql(crate::change_log::head(tx))
        }))?
        .await?;
    assert_eq!(head.head_seq, after.head_seq);
    writer.shutdown().await?;
    Ok(())
}

#[test]
#[ignore = "qualification: requires copied release inputs, run via scripts/sync-v4/qualify-import.sh"]
fn frozen_m7_libraries_remain_artifacts_outside_app_schema_publication() -> Result<()> {
    let root = std::env::var("M7_INPUT_ROOT").expect("copied frozen M7 release required");
    let root = std::path::Path::new(&root);
    let defaults = schema::BaseReleaseDefaults {
        app_ver: "main@4794af03d+v4".into(),
        client_limit: 100000,
        byte_budget: 33554432,
        rights_ver: 1,
    };
    let input =
        schema::load_base_release(&root.join("v4"), &root.join("ctox-platform.manifest.json"))?;
    let plan = base_release::load(
        &root.join("v4"),
        &root.join("ctox-platform.manifest.json"),
        &defaults,
    )?;
    let artifacts: Vec<_> = input
        .libs
        .iter()
        .map(|lib| lib.artifact_id.as_str())
        .collect();
    assert_eq!(artifacts, ["lib:ctox-office@1.0", "lib:monaco@0.32"]);
    assert_eq!(
        plan.libs.iter().map(|l| &l.artifact_id).collect::<Vec<_>>(),
        input
            .libs
            .iter()
            .map(|l| &l.artifact_id)
            .collect::<Vec<_>>()
    );
    // The unedited DevOps M7 copy also carries its owned preservation probe.
    let base_apps = input
        .manifests
        .iter()
        .filter(|m| m["app_id"] != "ctox-platform" && m["app_id"] != "e2e-probe")
        .count();
    assert_eq!(
        base_apps, 42,
        "frozen Shell apps must not disappear with libraries"
    );
    assert_eq!(
        input
            .manifests
            .iter()
            .filter(|m| m["app_id"] == "ctox-platform")
            .count(),
        1
    );
    assert!(input.manifests.iter().any(|m| m["app_id"] == "e2e-probe"));
    assert_eq!(plan.manifests.len(), input.manifests.len());
    assert!(input
        .manifests
        .iter()
        .all(|m| !m["app_id"].as_str().unwrap().starts_with("lib:")));
    assert!(input.schemas.keys().all(|id| !id.starts_with("lib:")));
    assert!(plan.collections.keys().all(|id| !id.starts_with("lib:")));
    for lib in &input.libs {
        assert!(lib.manifest["files"].is_object());
    }
    Ok(())
}
