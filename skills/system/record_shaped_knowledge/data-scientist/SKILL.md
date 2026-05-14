---
name: data-scientist
description: Maintain and work with record-shape knowledge in CTOX — collections of structured records such as CRM entities, competitor analyses, structured interviews, lookup tables, and scraping outputs. Use this skill whenever the right form of knowledge for the work is tabular data rather than prose, a runbook, or a single fact entry.
metadata:
  short-description: Curate record-shape knowledge tables as a CTOX-native form
cluster: record_shaped_knowledge
---

# Record-Shape Knowledge

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step.
- Before adding follow-up work, check existing self-work, queue, plan, and ticket state and consolidate instead of duplicating.

## What this Skill is

Record-shape knowledge is a first-class CTOX knowledge form, on par with skillbooks, runbooks, and ticket knowledge entries. It is the right form whenever knowledge is naturally a collection of structured records that share a schema and grow over time.

This skill names the role you play when you create, maintain, and query that form. The role is mostly that of a structured-knowledge curator — keeping reference works correct, complete, and useful. Statistical or analytical work (distributions, correlations, hypothesis checks) is a minor sub-role; reach for it only when a question actually requires it.

## When to use record-shape knowledge

Use it when the answer to "what shape is this knowledge?" is one of:

- a list of entities with shared attributes (companies, contacts, accounts, candidates, vendors, devices, repositories)
- a comparative reference work (competitor analysis, vendor matrix, pricing landscape, feature comparison)
- a corpus of structured observations (interview notes with consistent fields, audit findings, support cases, incident records)
- a lookup or reference table (mappings, codes, statuses, taxonomies, glossaries with structured fields)
- the materialized output of an ongoing collection process (scraping results, monitoring exports, research datasets)

Do **not** use it for:

- single facts → use `ctox ticket knowledge-*` entries
- procedures and how-tos → use runbooks
- heuristics and decision rules → use skills
- narrative state of the current mission → use continuity documents
- anything the CTOX core service loop reads in the hot path (queue, ticket cases, transitions, routing) → stays in SQLite

## How it fits in CTOX

- The catalog lives in SQLite (`knowledge_data_tables`) so CTOX manages the form transactionally and the agent can discover, name, fork, archive, and drop tables across turns.
- The content lives separately (Parquet) so very large collections do not bloat the runtime DB and schema can evolve.
- CTOX provides the substrate. You decide the domains, table keys, columns, and enrichment passes. CTOX is not a framework prescribing CRM, competitor-analysis, or interview schemas — you shape them per use case.
- Embeddings, scores, classifications, derived metrics, and similar enrichments are columns you add when you need them. They are not a separate concept.

## Methodological discipline

When the work crosses from curation into analysis, the role becomes more like a statistician. Keep these honest:

- Be explicit about what the rows represent and how the sample was drawn. Selection bias is the most common source of wrong answers.
- Distinguish observed counts from inferred rates. Do not report "X percent" when the denominator is unstable or undefined.
- Distinguish correlation from causation. Note confounders before claiming an effect.
- Distinguish statistical significance from practical effect size. A significant result on a small effect is rarely worth acting on.
- Note missing data and how it was handled. Silent imputation is a misrepresentation.
- Cite the data table (`domain`, `table_key`, snapshot timestamp) for any number you report so a reader can reproduce it.

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

## For real data-science work

When your task crosses from CRUD into actual data science (clustering, modeling, complex joins, statistical tests, custom enrichment), the CLI surface above is intentionally not enough. Use this pattern instead:

1. `ctox knowledge data clone --from-domain X --from-key Y --to-domain working --to-key X-Y-analysis-<date>` — fork an isolated working copy so the canonical table is unchanged during exploration.
2. `ctox knowledge data describe --domain working --key X-Y-analysis-<date>` — get the `parquet_path` from the JSON.
3. Run Python via the `shell` tool against that parquet path. Polars-Python (or pandas) is available on the host. Write the result to a new parquet under `/tmp` or alongside.
4. `ctox knowledge data import --domain working --key <result-name> --from-file <result.parquet> --mode replace` — bring the result back into the catalog as a new durable table, or replace the working copy.
5. When done, promote (via `clone` or `rename`) or `archive` the working copy.

Reusable Python patterns belong as scripts in `scripts/` inside this skill bundle (or in a sibling skill bundle), not as ad-hoc inline code in turn transcripts. Ad-hoc one-shot snippets are fine via `shell` but are not durable knowledge.

## Scope reminder

This skill is intentionally small. Most record-shape knowledge work is mundane curation — adding rows, fixing fields, refreshing references. Reach for the data-scientist framing only when the work genuinely is data science. Otherwise the role is simpler: keep the structured knowledge correct and current.
