// Origin: CTOX
// License: AGPL-3.0-only
use super::super::super::jour_fixe_repo_digest::{parse, RepoDigest, MAX_AGE_MS};
use super::*;

fn sample() -> Value {
    json!({"repository":"acme/private","since":"2026-10-05T00:00:00Z","collected_at":"2026-10-12T20:00:00Z",
        "commits":[{"sha":"1234567890123456789012345678901234567890","author":"Test author","date":"2026-10-12T19:00:00Z","message":"Verified repository work"}],
        "pulls":[{"number":1,"title":"Merged work","state":"closed","merged_at":"2026-10-12T19:00:00Z","author":"Test author","updated_at":"2026-10-12T19:00:00Z"},
            {"number":2,"title":"Open work","state":"open","merged_at":null,"author":"Test author","updated_at":"2026-10-12T19:00:00Z"}],
        "issues":[{"number":3,"title":"Decision needed","state":"open","updated_at":"2026-10-12T19:00:00Z"}]})
}
fn digest() -> RepoDigest {
    parse(sample().to_string().as_bytes()).unwrap()
}
fn at(value: &str) -> i64 {
    super::super::super::jour_fixe_repo_digest::timestamp(value).unwrap()
}

#[test]
fn jour_fixe_operator_repo_digest_supplies_no_token_private_and_unbound_sources() {
    let missing = github_with(None, 0, |_| panic!("no repository, no HTTP"));
    let now = at("2026-10-12T21:00:00Z");
    let source = github_with_operator_digest(missing, Ok(Some(digest())), 0, now);
    assert_eq!(source["status"], "collected");
    assert_eq!(source["source"], "operator_digest");
    assert_eq!(source["collected_at"], "2026-10-12T20:00:00Z");
    assert_eq!(source["repository"], "acme/private");
    assert_eq!(
        source["data"]["commits"]["data"]["items"][0]["message"],
        "Verified repository work"
    );
    assert_eq!(
        source["data"]["merged_prs"]["data"]["items"][0]["number"],
        1
    );
    assert_eq!(source["data"]["open_prs"]["data"]["items"][0]["number"], 2);
    assert_eq!(source["data"]["issues"]["data"]["items"][0]["number"], 3);
    assert_eq!(source["data"]["releases"]["status"], "missing_source");
    // Private unauthenticated API failures are nested beneath "collected".
    let denied = json!({"status":"collected","data":{"merged_prs":{"status":"unavailable","reason_code":"repository_not_visible","data":null}}});
    assert_eq!(
        github_with_operator_digest(denied, Ok(Some(digest())), 0, now)["source"],
        "operator_digest"
    );
}

#[test]
fn jour_fixe_operator_repo_digest_authenticated_api_wins_without_network_or_credentials() {
    // The injected fetch represents a successful configured-token transport.
    let api = github_with(Some("https://github.com/acme/private"), 0, |path| {
        if path.contains("state=closed") {
            Ok(json!([{"number":9,"title":"API result","merged_at":"2026-10-12T19:00:00Z"}]))
        } else {
            Ok(json!([]))
        }
    });
    let selected = github_with_operator_digest(
        api.clone(),
        Ok(Some(digest())),
        0,
        at("2026-10-12T21:00:00Z"),
    );
    assert_eq!(selected, api);
    assert_eq!(
        selected["data"]["merged_prs"]["data"]["items"][0]["number"],
        9
    );
}

#[test]
fn jour_fixe_operator_repo_digest_stale_and_absent_sources_preserve_truthful_status() {
    let missing =
        json!({"status":"missing_source","reason_code":"repository_not_bound","data":null});
    let collected = at("2026-10-12T20:00:00Z");
    for now in [
        collected + MAX_AGE_MS,
        collected + MAX_AGE_MS + 1,
        collected - 1,
    ] {
        let result = github_with_operator_digest(missing.clone(), Ok(Some(digest())), 0, now);
        assert_eq!(result["status"], "missing_source");
        assert_eq!(result["reason_code"], "operator_digest_stale");
        assert!(result["data"].is_null());
    }
    assert_eq!(
        github_with_operator_digest(missing.clone(), Ok(None), 0, collected),
        missing
    );
    assert_eq!(
        github_with_operator_digest(
            json!({"status":"unavailable"}),
            Ok(Some(digest())),
            0,
            collected + MAX_AGE_MS - 1
        )["source"],
        "operator_digest"
    );
}

#[test]
fn jour_fixe_operator_repo_digest_budgets_and_dates_match_api_section_shape() {
    let mut raw = sample();
    raw["commits"] = json!((0..100).map(|_|json!({"sha":"1234567890123456789012345678901234567890","author":"Test","date":"2026-10-12T19:00:00Z","message":"Ä".repeat(700)})).collect::<Vec<_>>());
    let d = parse(raw.to_string().as_bytes()).unwrap();
    let value = github_with_operator_digest(
        json!({"status":"missing_source"}),
        Ok(Some(d)),
        at("2026-10-06T00:00:00Z"),
        at("2026-10-12T21:00:00Z"),
    );
    assert!(serde_json::to_vec(&value).unwrap().len() < 4000);
    assert_eq!(value["data"]["commits"]["truncated"], true);
    let fresh = github_with_operator_digest(
        json!({"status":"missing_source"}),
        Ok(Some(digest())),
        at("2026-10-12T19:30:00Z"),
        at("2026-10-12T21:00:00Z"),
    );
    assert_eq!(fresh["data"]["commits"]["data"]["items"], json!([]));
    assert_eq!(fresh["data"]["merged_prs"]["data"]["items"], json!([]));
}

#[test]
fn jour_fixe_operator_repo_digest_is_attached_to_the_actual_project_brief_only(
) -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let conn = open_store(root.path())?;
    conn.execute_batch(super::super::jour_fixe_preparation::SCHEMA)?;
    super::super::super::store::upsert_business_record(
        &conn,
        "workjet_projects",
        "project",
        1,
        json!({"id":"project","name":"Test","owner_user_id":"owner"}),
    )?;
    drop(conn);
    let now = super::super::super::store::now_ms() as i64;
    let stamp = chrono::DateTime::from_timestamp_millis(now)
        .unwrap()
        .to_rfc3339();
    let mut raw = sample();
    raw["collected_at"] = json!(stamp);
    let args = [
        "put",
        "--project-id",
        "project",
        "--repository",
        "acme/private",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect::<Vec<_>>();
    super::super::super::jour_fixe_repo_digest::cli(
        root.path(),
        &args,
        &mut std::io::Cursor::new(raw.to_string()),
    )?;
    let mut meeting = Meeting {
        id: "meeting".into(),
        project_id: "project".into(),
        owner_user_id: "owner".into(),
        supervisor: super::super::super::workjet_jour_fixe_contract::SupervisorRef {
            workjet_thread_id: "thread".into(),
            ctox_thread_key: "native-thread".into(),
        },
        scheduled_at_ms: now + 7200000,
        prepare_at_ms: now,
        timezone: "Europe/Berlin".into(),
        state: super::super::super::workjet_jour_fixe_contract::MeetingState::Planned,
        revision: 0,
        deck_revision: 0,
        previous_goal: None,
        slides: vec![],
        comments: vec![],
        transcript: vec![],
        todos: None,
        error: None,
    };
    prepare_with(
        root.path(),
        "prepare",
        &meeting,
        &json!({}),
        |repo, since| {
            assert!(repo.is_none());
            github_with(repo, since, |_| panic!("never call GitHub"))
        },
    )?;
    let conn = open_store(root.path())?;
    let brief: Value =
        serde_json::from_str(&read(&conn, "prepare", "owner", "project", "thread")?.unwrap())?;
    assert_eq!(brief["github"]["source"], "operator_digest");
    assert!(serde_json::to_vec(&brief)?.len() <= MAX_BYTES);
    drop(conn);
    meeting.project_id = "foreign".into();
    prepare_with(
        root.path(),
        "foreign",
        &meeting,
        &json!({}),
        |repo, since| github_with(repo, since, |_| panic!("never call GitHub")),
    )?;
    let conn = open_store(root.path())?;
    let brief: Value =
        serde_json::from_str(&read(&conn, "foreign", "owner", "foreign", "thread")?.unwrap())?;
    assert_eq!(brief["github"]["status"], "missing_source");
    assert!(brief["github"].get("repository").is_none());
    Ok(())
}
