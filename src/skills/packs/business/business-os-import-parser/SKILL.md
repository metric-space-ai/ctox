# Business OS Import Parser

Use this skill when a CTOX queue task has `suggested_skill: business-os-import-parser` or the prompt contains a `business_os.source.parse` command.

The UI is only a structured task producer. Do not call external CV/job import services from the UI, and do not bypass the CTOX queue. Process the queued command through the native Business OS command boundary:

```bash
ctox business-os commands process <command-id>
```

Workflow:

1. Read the `command_id` from the queued task prompt or metadata.
2. Run `ctox business-os commands process <command-id>` from the CTOX workspace root.
3. Verify the JSON output has `ok: true` and `status: "completed"`.
4. If the command fails, report the command id and error. The command processor records the failure on `business_commands` and the related `ctox_queue_tasks` projection.

The processor writes canonical records to `business_records` and compatibility projections for the requirement-matching UI:

- Requirement URL imports write `companies`, `jobs`, and `postings`.
- Candidate document imports write `candidates`.
- Import artifacts are stored under `runtime/business-os-imports/<command-id>/`.

Do not manually insert partial projection rows unless the native command is unavailable and the user explicitly asks for forensic recovery.
