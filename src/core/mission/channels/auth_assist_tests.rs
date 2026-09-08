use super::*;

#[test]
fn auth_assist_recovery_uses_open_type_index() {
    let root = tempfile::tempdir().unwrap();
    let mut conn = open_channel_db(&resolve_db_path(root.path(), None)).unwrap();
    ensure_queue_account(&mut conn).unwrap();
    let mut statement = conn
        .prepare(
            "EXPLAIN QUERY PLAN
             SELECT a.command_id, l.task_id
             FROM business_command_aggregates a
             JOIN business_command_task_links l ON l.command_id = a.command_id
             JOIN communication_routing_state r ON r.message_key = l.task_id
             WHERE a.command_type = ?1 AND a.execution_phase != 'terminal'
               AND a.command_id > ?2
               AND r.route_status IN ('pending', 'leased', 'review_rework', 'blocked')
             ORDER BY a.command_id LIMIT 32",
        )
        .unwrap();
    let plan = statement
        .query_map(params![REQUEST_TYPE, ""], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
        .join("\n");
    assert!(plan.contains("idx_business_command_open_type_id"), "{plan}");
    assert!(!plan.contains("USE TEMP B-TREE"), "{plan}");
}
