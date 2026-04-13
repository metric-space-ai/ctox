# Storage Layout

Recommended default layout:

```text
runtime/
  ctox_scraping.db
  scraping/
    targets/
      <target_key>/
        manifest.json
        scripts/
          current.js
          revisions/
            rev0001_<sha8>.js
            rev0002_<sha8>.js
        sources/
          sources_manifest.json
          <source_key>/
            source.json
            current.js
            revisions/
              rev0001_<sha8>.js
        runs/
          <run_id>/
            run.json
            outputs/
```

## Rules

- `manifest.json` is the target-local summary copied from the registry view.
- `scripts/current.*` is the latest executable convenience copy.
- `scripts/revisions/` keeps immutable numbered revisions.
- `sources/sources_manifest.json` is the normalized source graph used by the runtime.
- `sources/<source_key>/current.*` is the current source-local extractor/module for that upstream.
- `sources/<source_key>/revisions/` keeps immutable per-source module revisions.
- `runs/<run_id>/run.json` stores a compact run manifest.
- run outputs belong under `runs/<run_id>/outputs/` unless the target contract explicitly points elsewhere.

## Registry Tables

The helper registry currently manages:

- `scrape_target`
- `scrape_script_revision`
- `scrape_source_revision`
- `scrape_template_example`
- `scrape_template_promoted`
- `scrape_run`
- `scrape_artifact`

## Output Normalization

Each target should define:

- `output_schema`
- canonical artifact kinds
- canonical storage location
- overwrite vs append expectations

Typical schema keys:

- `jobs.v1`
- `articles.v1`
- `catalog_items.v1`
- `documents.v1`
- `raw_http_capture.v1`

## Update Policy

- register a new script revision before switching the current script
- keep old run manifests intact
- record artifact metadata even when the artifact files are rotated later
- if output schema changes materially, update the target manifest and register a new script revision with a clear reason
