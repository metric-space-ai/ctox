# Business OS Requirement Matching

Use this skill when a CTOX queue task has `suggested_skill: business-os-requirement-matching` or the prompt contains a `business_os.match.*` command.

Architectural rule:

- The Requirement Matching UI may create structured commands and display persisted match artifacts.
- Match reasoning and LLM calls must run through the CTOX harness.
- Do not transplant the old extension pattern where the browser directly owns the LLM call and final persistence.

Expected match artifact shape for the UI:

- Store match rows in the `matches` projection collection.
- Keep the canonical command/result trace in `business_commands` or `business_records`.
- Use match items grouped by `priority`: `base`, `performance`, and `enthusiasm`.
- Each item should include requirement title, dimension, match level, score, job snippet, CV snippet, and explanation.
- The UI expects a total `score` from 0 to 100 and item-level `matchScore` from 0.0 to 1.0.

If the queued command payload contains a ready prompt under `payload.request.messages`, answer that prompt with the required JSON only, then persist the normalized match row using the Business OS store/projection contract. If a native `ctox business-os` matching processor is available, prefer it over manual persistence.
