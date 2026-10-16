#[test]
fn canonical_v4_migrations_match_source_and_preserve_sql_history() -> anyhow::Result<()> {
    let corpus: Vec<Value> = serde_json::from_str(include_str!(
        "../rxdb/tests/fixtures/business-os-v4-migrations.json"
    ))?;
    for case in corpus {
        let mut conn = Connection::open_in_memory()?;
        conn.execute_batch("CREATE TABLE old(id TEXT PRIMARY KEY,revision TEXT,deleted INTEGER,lastWriteTime REAL,data TEXT); CREATE TABLE new(id TEXT PRIMARY KEY,revision TEXT,deleted INTEGER,lastWriteTime REAL,data TEXT);")?;
        let id = case["expected"]["id"].as_str().unwrap_or("retained-id");
        let input = case["input"].to_string();
        conn.execute(
            "INSERT INTO old VALUES(?1,'3-history',1,1234,?2)",
            params![id, input],
        )?;
        let steps = vec![vec![case["operation"].clone()]];
        let name = case["collection"].as_str().unwrap();
        migrate_native_rxdb_version_table(&mut conn, name, 0, "old", 1, "new", &steps)?;
        let stored: (String, i64, f64, String) = conn.query_row(
            "SELECT revision,deleted,lastWriteTime,data FROM new",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        assert_eq!(
            (stored.0, stored.1, stored.2),
            ("3-history".into(), 1, 1234.)
        );
        assert_eq!(
            serde_json::from_str::<Value>(&stored.3)?,
            case["expected"],
            "{name}"
        );
        assert_eq!(
            conn.query_row("SELECT data FROM old", [], |r| r.get::<_, String>(0))?,
            input
        );
        assert_eq!(
            migrate_native_rxdb_version_table(&mut conn, name, 0, "old", 1, "new", &steps)?,
            0
        );
        conn.execute("DELETE FROM new", [])?;
        let mut invalid = steps.clone();
        invalid[0][0]["source_sha256"] = "unreviewed-source".into();
        assert!(
            migrate_native_rxdb_version_table(&mut conn, name, 0, "old", 1, "new", &invalid)
                .is_err()
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM new", [], |r| r.get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM old", [], |r| r.get::<_, i64>(0))?,
            1
        );
    }
    Ok(())
}

#[test]
fn canonical_v4_every_version_has_a_native_chain() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let entries = native_rxdb_version_migration_entries(root.path())?;
    for (name, raw) in business_os_schema_contract() {
        let version = i64::from(schema_from_json(raw.clone()).version);
        if version == 0 {
            continue;
        }
        let entry = entries
            .iter()
            .find(|e| e.name == *name)
            .with_context(|| format!("missing {name}"))?;
        assert_eq!(
            native_rxdb_version_migration_steps(
                name,
                0,
                version,
                Some(&entry.migration_strategies)
            )?
            .len(),
            version as usize,
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn canonical_v4_identity_conflict_retains_both_stores() -> anyhow::Result<()> {
    let corpus: Vec<Value> = serde_json::from_str(include_str!(
        "../rxdb/tests/fixtures/business-os-v4-migrations.json"
    ))?;
    let operation = corpus
        .iter()
        .find(|c| c["collection"] == "communication_accounts")
        .unwrap()["operation"]
        .clone();
    let mut conn = Connection::open_in_memory()?;
    conn.execute_batch("CREATE TABLE old(id TEXT PRIMARY KEY,revision TEXT,deleted INTEGER,lastWriteTime REAL,data TEXT); CREATE TABLE new(id TEXT PRIMARY KEY,revision TEXT,deleted INTEGER,lastWriteTime REAL,data TEXT);")?;
    let body = json!({"account_key":"source-key","id":"other-key"}).to_string();
    conn.execute(
        "INSERT INTO old VALUES('source-key','1-original',0,1234,?1)",
        [body.clone()],
    )?;
    let error = migrate_native_rxdb_version_table(
        &mut conn,
        "communication_accounts",
        0,
        "old",
        1,
        "new",
        &[vec![operation]],
    )
    .unwrap_err();
    assert!(error.to_string().contains("row identity"));
    assert_eq!(
        conn.query_row("SELECT data FROM old", [], |r| r.get::<_, String>(0))?,
        body
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM new", [], |r| r.get::<_, i64>(0))?,
        0
    );
    Ok(())
}
