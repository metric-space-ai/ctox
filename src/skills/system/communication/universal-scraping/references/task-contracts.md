# Task Contracts

## Target Upsert Payload

```json
{
  "target_key": "acme-jobs",
  "display_name": "Acme Jobs",
  "start_url": "https://example.com/careers",
  "target_kind": "jobs",
  "status": "active",
  "schedule_hint": "0 */6 * * *",
  "config": {
    "expected_provider": "successfactors",
    "sources": [
      {
        "source_key": "board-a",
        "display_name": "Board A",
        "start_url": "https://jobs.example.com/feed.xml",
        "source_kind": "rss",
        "extraction_module": "sources/board-a/extractor.js"
      },
      {
        "source_key": "board-b",
        "display_name": "Board B",
        "start_url": "https://company.example/careers",
        "source_kind": "html",
        "extraction_module": "sources/board-b/extractor.js"
      }
    ]
  },
  "output_schema": {
    "schema_key": "jobs.v1",
    "primary_artifact_kind": "jobs_json"
  }
}
```

## Run Record Payload

```json
{
  "target_key": "acme-jobs",
  "trigger_kind": "scheduled",
  "scheduled_for": "2026-03-27T06:00:00+00:00",
  "status": "succeeded",
  "script_revision_no": 3,
  "run_context": {
    "reason": "nightly refresh"
  },
  "result": {
    "records_found": 214
  },
  "artifacts": [
    {
      "artifact_kind": "jobs_json",
      "path": "outputs/jobs.json",
      "schema_key": "jobs.v1",
      "record_count": 214
    }
  ]
}
```

## Script Contract

Registered scripts should emit JSON to stdout.

Accepted shapes:

- a top-level array of records
- an object with `records`
- an object with `jobs`
- an object with `items`
- an object with `result` that itself contains one of the shapes above

The native runner `ctox scrape execute --target-key <key>` injects these environment variables:

- `CTOX_SCRAPE_TARGET_KEY`
- `CTOX_SCRAPE_TARGET_DIR`
- `CTOX_SCRAPE_MANIFEST_PATH`
- `CTOX_SCRAPE_RUN_DIR`
- `CTOX_SCRAPE_OUTPUT_DIR`
- `CTOX_SCRAPE_START_URL`
- `CTOX_SCRAPE_SOURCES_JSON`
- `CTOX_SCRAPE_SOURCES_MANIFEST_PATH`
- `CTOX_SCRAPE_SOURCES_DIR`

The runner records `result.json` and, when records are detectable, `records.json`.

If the target defines multiple sources, the root script should treat those values as the normalized source graph and delegate source-specific extraction into `sources/<source_key>/` modules where appropriate.

## Source Module Revision Contract

Per-source modules are first-class runtime artifacts for multi-source targets.

Register them with:

```sh
ctox scrape register-source-module \
  --target-key acme-jobs \
  --source-key board-a \
  --module-file /path/to/board-a.js \
  --change-reason initial_source_import
```

CTOX then writes:

- `runtime/scraping/targets/<target_key>/sources/<source_key>/current.<ext>`
- `runtime/scraping/targets/<target_key>/sources/<source_key>/revisions/revNNNN_<sha8>.<ext>`

And persists the metadata in `scrape_source_revision`.

For successful runs, CTOX also materializes a target-local latest state:

- `runtime/scraping/targets/<target_key>/state/latest_records.json`
- `runtime/scraping/targets/<target_key>/state/latest_summary.json`
- per-run delta artifact at `runs/<run_id>/outputs/delta.json`

Identity is resolved from `config.record_key_fields` or `output_schema.record_key_fields` when present, otherwise CTOX falls back to common fields like `id` or `url`.

## Default API Contract

Each target gets a default runtime API scaffold:

- `runtime/scraping/targets/<target_key>/api/api_contract.json`
- `runtime/scraping/targets/<target_key>/api/README.md`
- `runtime/scraping/targets/<target_key>/api/semantic_template.json`
- `runtime/scraping/targets/<target_key>/api/llm_enrichment_template.json`

Native HTTP endpoints:

- `GET /ctox/scrape/targets/<target_key>/api`
- `GET /ctox/scrape/targets/<target_key>/latest`
- `GET /ctox/scrape/targets/<target_key>/records?<field>=<value>&limit=<n>`
- `GET /ctox/scrape/targets/<target_key>/semantic?q=<text>&limit=<n>`

Native CLI equivalents:

- `ctox scrape show-api --target-key <key>`
- `ctox scrape query-records --target-key <key> --where field=value`
- `ctox scrape semantic-search --target-key <key> --query <text>`

`records` uses exact-match scalar filters on dot-path fields such as `classification.category`.

When LLM enrichment is enabled for the target, those filterable fields should be materialized into the canonical latest-record state before the API reads from it.

`semantic` uses the configured embedding service and target-specific `source_fields`.

## Semantic Indexing Contract

Semantic retrieval is target-specific, but standardized:

- source fields come from `config.api.semantic.source_fields` when set
- otherwise CTOX uses common descriptive fields such as `title`, `summary`, `description`, or `content`
- embeddings are requested from the configured CTOX embedding service via `/v1/embeddings`
- the semantic cache is stored in `scrape_semantic_record`

The default model is `Qwen/Qwen3-Embedding-0.6B` unless the runtime config overrides it.

## LLM Enrichment Contract

Per-target postprocessing should be template-driven, not hardcoded.

The default enrichment template should define optional tasks like:

- classify records into exact-filter API fields
- extract structured fields into a stable shape
- write a semantic summary used for retrieval

Execution contract:

- `ctox scrape execute` should read `runtime/scraping/targets/<target_key>/api/llm_enrichment_template.json`
- if `enabled=false`, the raw extractor output becomes the canonical latest state unchanged
- if `enabled=true`, CTOX should call the configured responses model, merge the returned updates into each record, and materialize that enriched record set as the canonical latest state
- enrichment failures should not discard an otherwise successful scrape; CTOX should keep the raw records and record the enrichment failure as an artifact/report instead
- once an operator or agent edits the target-local template, future manifest writes must preserve that file instead of overwriting it with defaults

The agent may adapt or replace this template when the target needs something more specific.

## Drift And Repair Contract

`ctox scrape execute --allow-heal` does not maintain a second agent runtime.

Instead:

- transient transport or upstream availability problems are recorded as `temporary_unreachable`
- reachable portals that now return empty or materially partial output are classified as `portal_drift` or `partial_output`
- only those drift-like outcomes create a repair bundle and enqueue a CTOX queue repair task

The repair task should:

1. read `repair_request.json`
2. revise the target script in `runtime/scraping/targets/<target_key>/scripts/`
3. revise any affected source-local module under `runtime/scraping/targets/<target_key>/sources/<source_key>/`
4. register new revisions with `ctox scrape register-script` and/or `ctox scrape register-source-module`
5. rerun `ctox scrape execute --target-key <target_key> --trigger-kind repair --allow-heal`

## Template Example Recording

Use template examples when a target-specific solution looks reusable for a provider family:

```sh
ctox scrape record-template-example \
  --target-key acme-jobs \
  --template-key successfactors \
  --script-file runtime/scraping/targets/acme-jobs/scripts/current.js \
  --result-count 214 \
  --challenge-score 2 \
  --reason "handled embedded pagination and locale path normalization"
```

Promotion can happen automatically once the threshold is met, or manually:

```sh
ctox scrape promote-template \
  --template-key successfactors \
  --script-file runtime/scraping/targets/acme-jobs/scripts/current.js \
  --reason "manual promotion after repeated verification"
```

## Suggested Failure Language

Use compact failure classes in run metadata:

- `blocked`
- `temporary_unreachable`
- `portal_drift`
- `schema_change`
- `partial_output`

Keep the detailed notes in `result.detail` or in a nearby run artifact, not in the schedule itself.
