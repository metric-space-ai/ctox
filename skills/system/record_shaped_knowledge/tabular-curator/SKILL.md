---
name: tabular-curator
description: Curate record-shape knowledge in CTOX — collections of structured records such as CRM entities, contact lists, competitor analyses, structured interviews, lookup tables, research libraries of references, and scraping outputs. Use this skill whenever the natural form of the knowledge you are building or maintaining is a table of rows that share a schema, rather than prose, a runbook, or a single fact entry. Covers the full lifecycle: ad-hoc creation, schema evolution as new fields recur, enrichment passes (scores, classifications, embeddings as columns), and durable cross-turn persistence. Light analytical follow-up (descriptive stats, simple aggregations) is in scope; deeper data science is a subordinate sub-role that hands off to Python via the `clone`/`import` pattern documented below.
metadata:
  short-description: Curate record-shape knowledge tables as a CTOX-native form
cluster: record_shaped_knowledge
---

# Record-Shape Knowledge — Tabular Curator

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step.
- Before adding follow-up work, check existing self-work, queue, plan, and ticket state and consolidate instead of duplicating.

## What this Skill is

Record-shape knowledge is a first-class CTOX knowledge form, on par with skillbooks, runbooks, and ticket knowledge entries. It is the right form whenever knowledge is naturally a collection of structured records that share a schema and grow over time.

This skill names the role you play when you build, maintain, and query that form. The role is primarily that of a **structured-knowledge curator** — keeping reference works correct, complete, and useful as they evolve. Light analytical work (counts per category, distributions, simple aggregations) is part of curation. Deeper analytical work (clustering, modeling, custom statistics) is a separate sub-role that hands off to Python via the pattern at the bottom of this skill.

## When to use record-shape knowledge

Use it when the answer to "what shape is this knowledge?" is one of:

- a list of entities with shared attributes (companies, contacts, accounts, candidates, vendors, devices, repositories, drone airframes)
- a library of references (papers, standards, datasets, patents, OEM specs — typical "build a catalog" research output)
- a comparative reference work (competitor analysis, vendor matrix, pricing landscape, feature comparison)
- a corpus of structured observations (interview notes with consistent fields, audit findings, support cases, incident records, lab measurements)
- a lookup or reference table (mappings, codes, statuses, taxonomies, glossaries with structured fields)
- the materialized output of an ongoing collection process (scraping results, monitoring exports, research datasets)

Do **not** use it for:

- single facts → use `ctox ticket knowledge-*` entries
- procedures and how-tos → use runbooks
- heuristics and decision rules → use skills
- narrative state of the current mission → use continuity documents
- anything the CTOX core service loop reads in the hot path (queue, ticket cases, transitions, routing) → stays in SQLite

## How it fits in CTOX

- The catalog lives in SQLite (`knowledge_data_tables`) so CTOX manages the form transactionally and you can discover, name, fork, archive, and drop tables across turns.
- The content lives separately (Parquet) so very large collections do not bloat the runtime DB and schema can evolve.
- CTOX provides the substrate. You decide the domains, table keys, columns, and enrichment passes. CTOX is not a framework prescribing CRM, competitor-analysis, interview, or library schemas — you shape them per use case.
- Embeddings, scores, classifications, derived metrics, and similar enrichments are columns you add when you need them. They are not a separate concept.

## Curation discipline

- **Provenance**: every row that carries a non-trivial fact must record where it came from (`source_url`, `source_id`, `extracted_at`, optionally a verbatim quote). A row without provenance is hearsay and devalues the table.
- **No extrapolation**: if a fact requires inference beyond what the source actually states, mark it (`derived_from`, `assumption_text`) or do not include it. Silent inference is a misrepresentation.
- **Schema evolves additively**: when the third source brings a field the first two did not have, `add-column` rather than smuggling it into a free-text field. The earlier rows get NULL — that is honest.
- **Single source of truth per table**: do not split the same conceptual list across two tables. If a record set has natural subsets, use `--tag` or a dedicated column, not parallel tables. Use `clone` only for snapshots / explorations / forks, not for splits.
- **Cite the table back**: when you report numbers in chat, name `domain`, `table_key`, and the timestamp of the snapshot so a reader can reproduce the count.

## When the work crosses into analysis

If a question requires more than counts and simple group-bys — clustering, modeling, hypothesis tests, complex joins, custom statistics — the in-process CLI is intentionally not enough. The pattern is:

1. `ctox knowledge data clone --from-domain X --from-key Y --to-domain working --to-key X-Y-analysis-<date>` — fork an isolated working copy so the canonical table is unchanged during exploration.
2. `ctox knowledge data describe --domain working --key X-Y-analysis-<date>` — read the `parquet_path` from the JSON.
3. Run Python via the `shell` tool against that parquet path. Polars-Python (or pandas) is available on the host. Write the result to a new parquet under `/tmp` or alongside.
4. `ctox knowledge data import --domain working --key <result-name> --from-file <result.parquet> --mode replace` — bring the result back into the catalog as a new durable table, or replace the working copy.
5. When done, promote (via `clone` or `rename`) or `archive` the working copy.

When you do report statistics, keep them honest: be explicit about what rows represent and how they were sampled (selection bias is the usual source of wrong answers), distinguish observed counts from inferred rates, distinguish correlation from causation, and note missing data rather than silently imputing.

Reusable Python patterns belong as scripts in `scripts/` inside this skill bundle (or in a sibling skill bundle), not as ad-hoc inline code in turn transcripts. Ad-hoc one-shot snippets are fine via `shell` but are not durable knowledge.

## CLI surface

`ctox knowledge data` is the CLI entry point. Two layers:

**Lifecycle (operate on the table as an object — catalog-only, no content access):**
- `create`, `list`, `describe`, `clone`, `rename`, `archive`, `restore`, `delete`, `tag`, `untag`

**Operational (operate on the data inside the table — Polars-backed):**
- Read: `head`, `schema`, `stats`, `count`, `select` (with `--where col=val`, `--columns c1,c2`, `--order-by col[:desc]`, `--limit N`, `--offset N`)
- Row-write: `append --rows <json-array>`, `update --where ... --set "c1=v1,c2=v2"`, `delete-rows --where ...`
- Column-write: `add-column --column N --dtype <i64|f64|bool|str> [--default V]`, `drop-column --column N`
- Bridge: `import --from-file <path> [--mode replace|append]`, `export --to-file <path>` (auto-detects format by extension: .parquet, .csv, .json, .jsonl)

Every write verb returns the updated `row_count`, `bytes`, and `schema_hash` in its JSON so you can verify the catalog is consistent.

`--where` operators: `=`, `!=`, `<`, `<=`, `>`, `>=`, `~` (regex substring match). Repeat `--where` for AND-chained predicates.

## Scope reminder

Most record-shape knowledge work is curation — finding sources, adding rows, fixing fields, refreshing references, evolving the schema as the picture sharpens. Statistical work is in scope but a smaller part. Choose this skill whenever you are building or maintaining a table of records, regardless of whether analysis comes later or not.
