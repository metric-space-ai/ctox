// Origin: CTOX
// License: AGPL-3.0-only
use super::*;

fn fixture_posts() -> Vec<FeedPost> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../rxdb/tests/fixtures/workjet-project-feed-v1.json"
    ))
    .expect("feed fixture");
    serde_json::from_value(fixture["read_response"]["posts"].clone()).expect("fixture posts")
}

fn conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    ensure_schema(&conn).unwrap();
    conn
}

#[test]
fn fixture_posts_validate() {
    for post in fixture_posts() {
        post.validate(post.occurred_at_ms).unwrap();
    }
}

#[test]
fn absent_table_reads_as_empty_feed() {
    let conn = Connection::open_in_memory().unwrap();
    let (posts, next) = read_posts(&conn, "p-1", "owner", None, 30).unwrap();
    assert!(posts.is_empty());
    assert_eq!(next, None);
}

#[test]
fn append_is_idempotent_and_reads_newest_first_with_paging() {
    let conn = conn();
    let posts = fixture_posts();
    for post in &posts {
        assert!(append_post(&conn, "owner", post, post.occurred_at_ms).unwrap());
        assert!(!append_post(&conn, "owner", post, post.occurred_at_ms).unwrap());
    }
    let (page, next) = read_posts(&conn, "p-1", "owner", None, 1).unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].kind, "disruption");
    let (rest, done) = read_posts(&conn, "p-1", "owner", next, 30).unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].kind, "pr_merged");
    assert_eq!(done, None);
}

#[test]
fn reused_post_id_with_other_payload_fails() {
    let conn = conn();
    let mut post = fixture_posts().remove(0);
    append_post(&conn, "owner", &post, post.occurred_at_ms).unwrap();
    post.body = "Something else.".into();
    assert!(append_post(&conn, "owner", &post, post.occurred_at_ms).is_err());
}

#[test]
fn foreign_owner_is_rejected_on_read() {
    let conn = conn();
    let post = fixture_posts().remove(0);
    append_post(&conn, "owner", &post, post.occurred_at_ms).unwrap();
    assert!(read_posts(&conn, "p-1", "someone-else", None, 30).is_err());
}

#[test]
fn validation_rejects_unsafe_or_oversized_content() {
    let base = fixture_posts().remove(0);
    let now = base.occurred_at_ms;
    let mut long = base.clone();
    long.body = "x".repeat(wire::MAX_BODY + 1);
    assert!(long.validate(now).is_err());
    let mut kind = base.clone();
    kind.kind = "rumour".into();
    assert!(kind.validate(now).is_err());
    let mut path = base.clone();
    path.sources[0].reference = "/home/user/secret.txt".into();
    assert!(path.validate(now).is_err());
    let mut token = base.clone();
    token.sources[0].reference = "https://x.test/?token=abc".into();
    assert!(token.validate(now).is_err());
    let mut future = base.clone();
    future.occurred_at_ms = now + wire::MAX_FUTURE_MS + 1;
    assert!(future.validate(now).is_err());
}

#[test]
fn retention_keeps_newest_posts() {
    let conn = conn();
    let template = fixture_posts().remove(0);
    for index in 0..(wire::RETENTION + 3) {
        let mut post = template.clone();
        post.post_id = format!("post-{index:04}");
        post.occurred_at_ms = 1_000 + index as i64;
        append_post(&conn, "owner", &post, 10_000).unwrap();
    }
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM workjet_project_feed_posts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count as usize, wire::RETENTION);
}

#[test]
fn read_request_limits() {
    let ok = ReadFeedRequest { project_id: "p-1".into(), before_ms: None, limit: Some(50) };
    ok.validate().unwrap();
    let bad = ReadFeedRequest { project_id: "p-1".into(), before_ms: None, limit: Some(51) };
    assert!(bad.validate().is_err());
}
