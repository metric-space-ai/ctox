# Workjet Owner confirmation bridge

The paired Shell accepts `project.jour_fixe.todos.confirm` with `commandId`, `projectId`, `operationId`, `meetingId`, `expectedRevision`, `proposalRevision` and `expectedGoalRevision`. The command is `ctox.workjet.jour_fixe.todos.confirm`; `record_id` is the project. Payload uses the native snake_case fields in `ConfirmTodosRequest` and contains no caller-supplied Owner, goal or replacement todo list.

The Owner edits proposed items through the existing `todos.revise` control, then confirms that exact proposal. The native handler checks current project/Supervisor authority and all three revisions. An initial project goal uses expectedGoalRevision 0; a later meeting uses the current native goal revision. Read or command conflicts require refreshed native state, never a guessed revision.

The Shell waits for the normal authenticated command-plane terminal receipt and preserves the current session and database. It validates operation/project/meeting intent, confirmed state, next meeting revision, proposal revision, a valid GoalRef, next goal revision and equality between GoalRef.goal_id and mutation.changed_id. It returns `{action, commandId, projectId, contract, mutation, goal}`. Failure does not dispatch a replacement or report success.

The meeting UI owns the real confirmation click, persistence/readback, and installed acceptance. The separate native plan-worker tool-session continuity connection is not established by this Shell bridge.
