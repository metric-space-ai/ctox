//! Session-bound execution-plan updates; identity is never caller-selected.
use super::*;

pub(super) fn update(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
    trusted: Option<&Value>,
) -> anyhow::Result<Value> {
    let object = arguments
        .as_object()
        .context("crew plan arguments must be an object")?;
    anyhow::ensure!(
        object
            .keys()
            .all(|key| matches!(key.as_str(), "steps" | "explanation" | "_context")),
        "crew plan accepts only steps and explanation; execution identity is server-bound"
    );
    anyhow::ensure!(
        serde_json::to_vec(arguments)?.len() <= 64 * 1024,
        "crew plan exceeds context limit"
    );
    let trusted = trusted.context("crew plan requires a signed command session")?;
    let expected: crew_context::SessionBinding = serde_json::from_value(
        trusted
            .get("crew_binding")
            .cloned()
            .context("crew session has no attempt binding")?,
    )?;
    let snapshot = crew_context::read(
        root,
        context,
        &serde_json::json!({"attempt_id": expected.attempt_id}),
        Some(trusted),
    )?;
    let work_key = required_arg(trusted, "crew_work_key")?;
    let command_id = required_arg(trusted, "command_id")?;
    let payload_hash = required_arg(trusted, "payload_hash")?;
    let task_id = required_arg(&snapshot, "task_id")?;
    let items = arguments
        .get("steps")
        .and_then(Value::as_array)
        .context("crew plan steps must be an array")?;
    anyhow::ensure!(
        !items.is_empty() && items.len() <= 100,
        "crew plan requires 1 to 100 steps"
    );
    let steps = items
        .iter()
        .map(
            |item| -> anyhow::Result<crate::lcm::TaskExecutionPlanStepInput> {
                let item_object = item
                    .as_object()
                    .context("crew plan step must be an object")?;
                anyhow::ensure!(
                    item_object
                        .keys()
                        .all(|key| matches!(key.as_str(), "label" | "status")),
                    "crew plan step accepts only label and status"
                );
                Ok(crate::lcm::TaskExecutionPlanStepInput {
                    label: required_arg(item, "label")?,
                    status: required_arg(item, "status")?,
                })
            },
        )
        .collect::<anyhow::Result<Vec<_>>>()?;
    let explanation = arguments
        .get("explanation")
        .map(|value| {
            value
                .as_str()
                .context("crew plan explanation must be a string")
        })
        .transpose()?;
    crate::crew::open_engine(root)?.record_task_execution_plan_guarded(
        crate::lcm::TaskExecutionPlanUpdate {
            work_key: &work_key,
            task_id: &task_id,
            command_id: &command_id,
            attempt_id: &expected.attempt_id,
            explanation,
            steps: &steps,
        },
        |conn| {
            let (current, _) =
                crew_context::live_binding(conn, &command_id, &payload_hash, &expected.attempt_id)?;
            anyhow::ensure!(
                current == expected,
                "crew session lease or identity changed"
            );
            Ok(())
        },
    )
}
