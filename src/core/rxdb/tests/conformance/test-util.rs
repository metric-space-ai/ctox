use serde_json::Value;

pub fn repeat_test(times: usize, mut test: impl FnMut(usize)) {
    for run in 0..times {
        test(run);
    }
}

pub fn ensure_json_states_equal(
    left: &[Value],
    right: &[Value],
    log_context: Option<&str>,
) -> Result<(), String> {
    if left == right {
        return Ok(());
    }

    Err(format!(
        "ensure_json_states_equal({}) states not equal: left={left:?}, right={right:?}",
        log_context.unwrap_or("")
    ))
}

#[test]
fn repeat_test_runs_expected_number_of_times() {
    let mut runs = Vec::new();
    repeat_test(3, |run| runs.push(run));
    assert_eq!(runs, vec![0, 1, 2]);
}

#[test]
fn ensure_json_states_equal_reports_context_on_mismatch() {
    let left = vec![serde_json::json!({ "id": "a" })];
    let right = vec![serde_json::json!({ "id": "b" })];

    let err = ensure_json_states_equal(&left, &right, Some("replication-check")).unwrap_err();

    assert!(err.contains("replication-check"));
    assert!(err.contains("left="));
    assert!(err.contains("right="));
}
