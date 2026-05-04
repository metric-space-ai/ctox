---
name: universal-scraping
description: Plan, build, revise, schedule, and operate reusable scraping workflows when CTOX must extract structured data from websites, APIs, feeds, documents, or browser-backed portals without reinventing the storage, script, and run-management model each time.
cluster: communication
---

# Universal Scraping

## CTOX Runtime Contract

- Task spawning is allowed only for real execution slices that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct slice with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


For CTOX mission work, scraped findings become durable knowledge only when the relevant facts are persisted into SQLite-backed runtime state. Raw exports or workspace notes do not count as durable knowledge by themselves.

Use this skill when the task is really about recurring extraction:

- scraping a website or portal
- deriving a stable extractor from a browser/API observation
- revising a broken scraper after portal drift
- scheduling repeat scrapes through CTOX
- storing scrape outputs and revisions so later runs can reuse them

Do not store generated target scripts inside the skill folder.

The skill folder is for stable reusable resources:

- workflow rules
- helper tooling
- template logic
- storage contracts

Target-specific generated scripts and run artifacts belong under `runtime/`, with metadata in SQLite.

## Operating Model

This skill uses a hybrid model:

1. repo-managed skill resources under `skills/system/communication/universal-scraping/`
2. mutable target workspaces under `runtime/scraping/targets/<target_key>/`
3. mutable registry state in `runtime/ctox_scraping.db`
4. optional compact evidence in the shared SQLite kernel with `skill_key=universal_scraping`

That split is intentional:

- the skill stays clean and reusable
- generated scripts become inspectable runtime artifacts
- revisions, promotions, runs, and artifact metadata stay queryable
- scheduled work can point to a stable `target_key` instead of embedding ad hoc logic every time

## Preferred Helpers

Use the native CTOX scrape surface when it fits:

- `ctox scrape upsert-target`
  - owns target registration and workspace creation
- `ctox scrape register-script`
  - owns script revisioning
- `ctox scrape register-source-module`
  - owns per-source extractor/module revisioning inside a multi-source target
- `ctox scrape record-template-example`
  - owns reusable template evidence capture
- `ctox scrape promote-template`
  - owns promoted provider-family templates
- `ctox scrape execute`
  - owns run execution, drift classification, artifacts, and repair enqueueing
- `ctox scrape show-latest`
  - exposes the current materialized canonical record set for a target
- `ctox scrape show-api`
  - exposes the default target API contract and documented endpoints
- `ctox scrape query-records`
  - runs exact-match filters against the canonical latest record set
- `ctox scrape semantic-search`
  - runs semantic retrieval backed by the configured embedding service
- `ctox scrape summary`
  - summarizes targets, templates, and recent runs

## Tool Contracts

Think in these capability contracts:

- `scrape.target_upsert`
- `scrape.script_register`
- `scrape.template_record`
- `scrape.template_promote`
- `scrape.run_record`
- `scrape.registry_query`

## Workflow

1. Define the target.
   Identify the real source, extraction goal, cadence, and output contract.
2. Register the target first.
   Use `ctox scrape upsert-target` so the scrape has a stable `target_key`, workspace, and output schema before code generation starts.
   For multi-source scrapers, define `config.sources[]` here so each source gets a stable `source_key`, folder, and module path up front.
3. Prefer the cheapest stable extraction path.
   Use direct API/feed/embedded JSON paths before browser-driven DOM scraping.
4. Use browser work as reviewed capability, not as prompt sludge.
   Keep browser traces compact and store artifacts on disk instead of dumping long traces into the main agent context.
5. Generate or revise the extractor script.
   Materialize target-specific scripts under the target workspace and register them as revisions.
   If the target aggregates multiple websites or feeds, keep source-specific logic in `sources/<source_key>/` modules and register those with `ctox scrape register-source-module` instead of stuffing everything into one root script.
6. Record reusable patterns separately.
   If the script solves a provider family, record it as a template example and promote only after cross-target evidence.
7. Record each real run.
   Store trigger, schedule slot, result summary, and artifact metadata with `record-run`.
8. Keep recurring work in CTOX schedule.
   Use `ctox schedule add --skill "universal-scraping"` for repeat runs, and let the scheduled prompt carry `target_key`, cadence, and desired output contract.
9. Keep outputs normalized.
   Each target should define where canonical outputs land and which schema key they follow.
10. Build the API surface with the scraper, not afterward.
   Each target should carry a default records API, semantic-search config, and editable LLM-enrichment template in its runtime workspace.
11. Treat drift honestly.
   If selectors, URLs, or API paths changed, revise the script and record a new revision instead of overwriting history.

## CTOX Execution Path

Use the native CLI bridge when you want the scrape lifecycle to stay inside CTOX:

```sh
ctox scrape init
ctox scrape upsert-target --input /path/to/target.json
ctox scrape register-script --target-key acme-jobs --script-file /path/to/extractor.js --change-reason initial_import
ctox scrape register-source-module --target-key acme-jobs --source-key board-a --module-file /path/to/board-a.js --change-reason initial_source_import
ctox scrape execute --target-key acme-jobs --allow-heal
ctox scrape show-api --target-key acme-jobs
ctox scrape query-records --target-key acme-jobs --where classification.category=job --limit 20
ctox scrape semantic-search --target-key acme-jobs --query "remote rust jobs"
```

`ctox scrape execute` and the target runtime do six things:

- runs the latest registered script revision
- passes the normalized source graph plus latest source-module revisions into the runtime so one target can aggregate multiple upstream sources
- classifies the outcome as `succeeded`, `temporary_unreachable`, `portal_drift`, `blocked`, or `partial_output`
- records the run and artifacts into the scrape registry
- applies optional target-local LLM enrichment before canonical materialization, so classifications and extracted fields become part of the default API surface
- materializes the current canonical latest dataset plus delta summary for successful runs
- writes a default target API contract plus editable enrichment and semantic templates under `runtime/scraping/targets/<target_key>/api/`
- makes the materialized state available through native records and semantic query surfaces
- if `--allow-heal` is set and the failure looks like drift instead of downtime, creates a CTOX queue repair task instead of silently rewriting on transient outages

Do not overwrite an existing target-local `semantic_template.json` or `llm_enrichment_template.json` just because the manifest is refreshed. Those files are intended to be edited per scraper and then reused by later executions.
Do not patch a source-local module in place and call it done; register a source revision so later repair work can compare what changed per upstream source.

That is the CTOX-native replacement for a second hidden agent loop.

## Scheduling Pattern

Recommended schedule prompts should include:

- target key
- expected trigger type
- expected output schema
- freshness expectation
- failure rule

Example:

```sh
ctox schedule add \
  --name "refresh acme jobs" \
  --cron "0 */6 * * *" \
  --skill "universal-scraping" \
  --prompt "Run target_key=acme-jobs. Expect schema=jobs.v1. Store outputs in the registered runtime workspace. If the portal drifted, revise the script, register a new revision, and summarize the delta."
```

## Completion Gate

Do not report a scrape workflow as prepared until:

- a target exists in the registry
- the target workspace exists under `runtime/scraping/targets/`
- the latest script is versioned, not just pasted in chat
- multi-source targets have named `config.sources[]` definitions and source modules registered where source-specific logic exists
- the output schema and storage path are explicit
- repeat execution can be routed through CTOX schedule or queue state

Do not report a reusable template as promoted until:

- it was recorded as a template example
- it has evidence across more than one target or a strong explicit override reason
- the promoted template metadata exists in the registry

## Guardrails

- Do not mutate the skill folder with target-specific scripts.
- Do not treat raw browser traces as the durable product.
- Do not overwrite working scripts in place without creating a revision.
- Do not hide schedule state in prose; use CTOX queue or schedule explicitly.
- Prefer compact artifacts, typed outputs, and stable schemas over free-form dumps.
- Keep browser-backed repeated work eligible for later specialist-model or deterministic-worker promotion.

## Resources

- [references/architecture.md](references/architecture.md)
- [references/storage-layout.md](references/storage-layout.md)
- [references/task-contracts.md](references/task-contracts.md)
