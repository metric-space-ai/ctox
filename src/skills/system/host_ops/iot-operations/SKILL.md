---
name: iot-operations
description: Use CTOX's native IoT engine from agent work items to inspect assets, read telemetry, interpret alarms and rulesets, and perform explicitly approved device writes through `ctox iot`. Use when a CTOX queue task, alarm, or operator request references IoT assets, attributes, datapoints, protocol agents, or `ctox.iot.*` commands.
cluster: host_ops
---

# IoT Operations

## Runtime Contract

- Treat physical-world writes as controlled actions. Never write a device-backed attribute unless the current task explicitly asks for that write or carries clear operator approval for the exact device/action class.
- Read operations are allowed when they support the current task: list/show assets, read attributes, query datapoints, list alarms, list rulesets, and inspect agent status.
- All IoT actions must go through the native CLI (`ctox iot ...`) or the Business OS command path. Do not call device protocols directly from the agent.
- Do not use HTTP bridges to the browser. The browser reads `iot_*` projections over RxDB/WebRTC and writes `business_commands`; the agent uses the CLI.
- Keep the loop bounded: read current state, reason from concrete evidence, act only if authorized, then verify with a read-back or queue/task outcome.
- Do not persist or print credentials. MQTT/HTTP/WebSocket secrets live in CTOX runtime config/secret store and must stay redacted.

## Command Map

Use these commands as the primary interface:

```sh
ctox iot asset list --realm <realm>
ctox iot asset show --id <asset-id>
ctox iot asset upsert --realm <realm> --type <type> --name <name> [--id <id>] [--parent <id>]
ctox iot asset delete --id <asset-id>

ctox iot attribute read --asset <asset-id> --name <attribute>
ctox iot attribute write --asset <asset-id> --name <attribute> --value '<json-or-string>' [--ts <epoch-ms>]

ctox iot datapoints query --asset <asset-id> --name <attribute> --from <epoch-ms> --to <epoch-ms> --shape all
ctox iot datapoints query --asset <asset-id> --name <attribute> --from <epoch-ms> --to <epoch-ms> --shape interval --interval <ms>
ctox iot datapoints query --asset <asset-id> --name <attribute> --from <epoch-ms> --to <epoch-ms> --shape lttb --threshold <n>

ctox iot alarm list --realm <realm>
ctox iot alarm ack --id <alarm-id>
ctox iot alarm resolve --id <alarm-id>

ctox iot rules list --realm <realm>
ctox iot rules save --realm <realm> --name <name> --data '<json>'
ctox iot rules toggle --id <ruleset-id> --enabled true|false

ctox iot agent list --realm <realm>
ctox iot agent configure --realm <realm> --name <name> --kind mqtt --data '<json>'
ctox iot agent status --id <agent-id>
ctox iot project all
```

## Read Workflow

1. Identify the realm, asset id, attribute name, alarm id, or ruleset id from the task.
2. Read the narrowest relevant state first.
3. If state is ambiguous, inspect adjacent context: parent asset, agent status, alarm lifecycle, and recent datapoints.
4. Base conclusions on CLI output, not stale prose or screenshots.
5. Summarize exact ids, values, timestamps, and status names when reporting back.

## Write Workflow

1. Confirm the task authorizes the write. If authorization is missing or vague, ask before writing.
2. Read the target asset and attribute first.
3. Validate that the attribute name and value type match the intended action.
4. Execute one narrow `ctox iot attribute write`.
5. Verify with `ctox iot attribute read`, datapoint query, device echo, or the resulting queue/task outcome.
6. Report the before value, requested value, after value, and any alarm/task side effects.

## Alarm And Ruleset Workflow

- An IoT alarm is a durable event source. Inspect the linked asset and current attribute before diagnosing.
- A queue task spawned from an IoT alarm is bounded by CTOX's spawn budget. Do not spawn another task for the same condition unless the parent task explicitly requires a distinct bounded follow-up.
- JSON attribute conditions are evaluated by the native IoT condition layer. Firing, dedup, recurrence, and loop bounding are CTOX mission/queue/schedule responsibilities.
- Groovy, JavaScript, Flow rules, forecasting, gateway federation, and non-MQTT production protocol bring-up are deferred scope unless the task explicitly says to work on those capabilities.

## Agent Safety Checks

Before any physical-world write, confirm:

- the asset id and attribute name are exact
- the realm is correct
- the task authorizes the write
- the value is bounded and reversible or otherwise operator-approved
- the expected verification path is known

If any item is missing, do not write. Read state and ask for the missing approval or constraint.
