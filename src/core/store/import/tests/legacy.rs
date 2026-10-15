use super::*;
#[test]
fn conversations_take_existing_key_and_keep_deletions_and_source_cursor() -> Result<()> {
    let mut source = rusqlite::Connection::open_in_memory()?;
    source.execute_batch("CREATE TABLE legacy_accounts(id TEXT,data TEXT,deleted INTEGER,revision TEXT); INSERT INTO legacy_accounts VALUES ('a','{\"account_key\":\"account-a\"}',0,'4-original'),('z','{}',1,'5-original');")?;
    let tx = source.transaction()?;
    let plan = ReleasePlan {
        version: "v4".into(),
        manifests: vec![],
        collections: BTreeMap::from([(
            "conversations".into(),
            vec![definition("communication_accounts", "v4")],
        )]),
        migrations: BTreeMap::from([(
            "communication_accounts".into(),
            json!({"from_ver":"main@4794af03d"}),
        )]),
        removed_sync_settings: vec![],
        field_merge_core_rule: vec![],
    };
    let page = base_release::legacy_page(
        &tx,
        &plan,
        &ReleaseMigration,
        "communication_accounts",
        "legacy_accounts",
        "",
        2,
    )?;
    assert_eq!(page.documents[0].id, "account-a");
    assert_eq!(page.documents[0].body["id"], "account-a");
    assert_eq!(page.documents[0].body["account_key"], "account-a");
    assert_eq!(page.documents[0].revision.as_deref(), Some("4-original"));
    assert_eq!(page.documents[1].body["_deleted"], true);
    assert_eq!(page.cursor, "z");
    assert!(!page.complete);
    Ok(())
}
