---
name: systematic-research
description: Systematic research with a common discovery phase and two output modes. Library mode (record-shape `ctox knowledge data` table) is for tasks framed as "build a library of X / catalog / dataset / matrix / lookup", spanning research-data libraries (papers, standards, measurement data, load cases, parameter tables), engineering reference libraries, and operational record sets (CRM entities, vendor comparisons, parts catalogs, scraping outputs). For technical, engineering, scientific, or regulatory topics the discovery phase prioritizes primary research data (NASA NTRS, scholar, arXiv, IEEE, agency reports, dataset repositories) and standards before OEM datasheets — generic `web search` ranks marketing pages above measurement data and must not be the first move. Decision-report mode (Word document via `ctox report`) is for feasibility study (Machbarkeitsstudie), market research (Marktanalyse), competitive analysis (Wettbewerbsanalyse), technology screening, whitepaper, source review (Quellenreview), literature review (Stand der Technik), decision brief (Entscheidungsvorlage), project description (Fördervorhabenbeschreibung). Run both modes in the same session when the answer needs both data and synthesis. Trigger whenever the research result is meant to outlast the turn.
class: system
state: active
cluster: research
---

# Systematic Research

You are the harness LLM. This skill is the **single entry point for all
durable research work in CTOX**. It orchestrates a common discovery phase
and one or two output modes depending on what the deliverable actually is.
Workspace markdown, CSV, JSON, and Parquet files may be deterministic build
receipts or import inputs, but they are not the durable research deliverable by
themselves. Register or import accepted outputs into CTOX Knowledge, Documents,
Spreadsheets, or Files before claiming completion.

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission
  progress, external waiting, recovery, or explicit decomposition. Do not
  spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review
  feedback, continue the same main work item whenever possible and
  incorporate the feedback there.
- Every durable follow-up, queue item, plan emission, or internal work item
  must have a clear parent/anchor: message key, work id, thread key,
  ticket/case id, or plan step.
- Before adding follow-up work, check existing internal work, queue, plan, and
  ticket state and consolidate instead of duplicating.
- Knowledge lookup is mandatory before every durable research output:
  run `ctox knowledge search --query "<task/topic>"` first. If the lookup
  exposes relevant data tables, inspect them with `ctox knowledge data
  list/describe/select` and use them as evidence; if nothing relevant is
  found, state that the local CTOX Knowledge lookup returned no applicable
  prior knowledge.

## Evidence boundary (fail closed)

Discovery and Evidence are different states. Search/deep-research results,
source catalogs, titles, snippets, DOI strings, abstracts, resolver metadata,
rankings, and landing pages are **candidates only**. They may locate a source,
but they may never be cited, imported, used in calculations, or promoted into
Knowledge/Reports. Read [evidence_integrity.md](references/evidence_integrity.md)
for the manifest contract and run `scripts/evidence_guard.py` before every
promotion or publication.

Evidence exists only when the original canonical non-metadata URL returned a
current 2xx response and the downloaded original content/data is present in a
SHA-256-verified snapshot. Require actual full text or original data content,
`relevance_score >= 8`, and the immutable
`claim_id -> evidence_id -> snapshot_id -> source_id -> canonical_url` chain.
404, login/cookie walls, JavaScript shells, snippets, aggregators, mirrors,
DOI resolver/landing URLs, and metadata are permanent rejection states. Never
fill them from memory or a second-hand summary.

## Choosing the output mode

Look at the deliverable the operator described, not the verb. There are
two output modes; pick one, the other, or both.

**Library mode** (record-shape `ctox knowledge data` table) — pick when the
deliverable is naturally a collection of records sharing a schema:

- a library, catalog, dataset, comparison matrix, lookup, registry, parts
  list, paper or patent bibliography, vendor matrix, CRM-style entity list,
  load-case table, measurement set, structured interview corpus, scraping
  output, monitoring export
- typical wording: "build a library of …", "compile a catalog of …",
  "collect every paper / standard / part on …", "compare X vendors so we
  can decide later", "list every …", "tabulate …"
- the deliverable's natural shape is a table; rows share columns; later
  turns will want to add rows or columns

**Decision-report mode** (Word document via `ctox report`) — pick when the
deliverable is a decision-grade prose synthesis:

- Machbarkeitsstudie / feasibility study
- Marktanalyse / market research, market study
- Wettbewerbsanalyse / competitive analysis (with scoring matrix)
- Technologie-Screening / technology screening
- Whitepaper
- Quellenreview / Quellenkompendium / source review
- Stand der Technik / literature review
- Entscheidungsvorlage / decision brief / decision memo
- Projektbeschreibung / Fördervorhabenbeschreibung / project description
  for funding (e.g. ZIM, EFRE, Horizon Europe)
- the deliverable is a single multi-section cited written report that
  answers one decision-grade question

**Combined mode** — pick when the deliverable is *both* a durable data
collection *and* a synthesis written on top of it. Run library mode first
so the report's claims cite the table by `domain/table_key`, then run
decision-report mode using the table as evidence.

If you cannot decide between library and decision-report, default to
library mode for "collect" / "compile" / "list" wording and decision-report
mode for "evaluate" / "decide" / "study" wording.

## Phase 1 — Discovery (shared by all modes)

Before producing anything, find out what CTOX already knows on the topic
and what the **best-quality external sources** are. The discovery phase is
the same regardless of which output mode you pick.

1. **Inventory CTOX**: `ctox skill list` and `ctox knowledge data list` to
   find existing research artifacts on this topic. Extend instead of
   duplicating.
2. **External source mining**: do NOT default to plain `ctox web search`.
   That ranks SEO-optimized consumer/marketing pages above primary research
   data. Use the source-class priority below.
3. **Source-catalog table**: open a `source_catalog` table in
   `ctox knowledge data` (one row per candidate source, with provenance,
   source-class tag, verification state, snapshot hash, directness status, and a
   one-line note on what it contributes). Candidate rows are discovery
   receipts, not evidence. Library and decision-report mode may read only
   rows whose `evidence_eligible` field is `true`. Build and verify the catalog
   before drafting the actual library schema or report blueprint.
4. **Schema/blueprint inference**: only after the source set is solid,
   decide what columns the library row needs (or which report-type
   blueprint fits). The schema is shaped by what the sources actually
   carry, not by what you think a priori.

### Source-class priority

For any technical, engineering, scientific, regulatory, or research
topic, work through these classes in order. Each lower class is a fallback
for what the higher classes did not cover, not a starting point.

**Tier 1 — Primary measurements and research data** (start here):

- government technical-report repositories: NASA NTRS (ntrs.nasa.gov),
  DoD Defense Technical Information Center (apps.dtic.mil), DOE OSTI
  (osti.gov), national lab repositories, agency-hosted PDFs
- scholarly literature: Google Scholar (discovery only), Semantic Scholar
  and OpenAlex (metadata discovery), arXiv, IEEE Xplore, SAE Mobilus, ACM
  Digital Library, Elsevier/Springer/Wiley DOIs
- public dataset repositories: Zenodo, Figshare, Dataverse, HuggingFace
  Datasets, Open Science Framework (OSF). Kaggle is discovery-only unless
  the record is demonstrably uploaded by the original data owner and its
  files/checksums match the canonical source.
- domain-specific reference databases (examples — adapt per topic):
  - aerospace/UAV: UIUC Propeller Database (m-selig.ae.illinois.edu),
    NASA MTB2 (rotorcraft.arc.nasa.gov), DARPA briefings, AIAA papers
  - mechanical/bearings: SKF, Schaeffler, NSK whitepapers + ISO 281
  - electronics: arxiv, IEEE TPEL, ASME
  - biomedical: PubMed/PMC, ClinicalTrials.gov, FDA databases
  - climate/earth: NOAA, NASA EOSDIS, Copernicus, ECMWF
  - economic/market: OECD, Eurostat, BLS, Destatis, Census APIs
  - flight logs/UAV telemetry: ardupilot.org logs, PX4-flight-review,
    open flight-data archives

**Tier 2 — Standards and regulatory**:

- ISO, ASTM, IEEE, DIN, EN, VDE standards (often paywalled but titled
  and abstracted on the standard body's site)
- regulatory: FAA AC/TSO, EASA CS/AMC, FDA guidance, FCC/ITU, BSI

**Tier 3 — Industry/OEM material** (use only as context, not as primary
data):

- vendor datasheets (DJI, Skydio, ABB, Siemens, ...) — useful for
  product overview, **not** for measurement data
- application notes, product manuals
- white papers from vendors (always read as marketing-plus-engineering)

Going straight to Tier 3 because the answer "looks like a product
comparison" is a discovery failure when the topic is engineering/research.
Industry datasheets give you MTOW and headline specs; they do not give
you measured rotor loads, fatigue curves, vibration spectra, or
qualification-test reports.

**Discovery-only aggregators — never cite as primary evidence**:

- ResearchGate, Academia.edu, Google Scholar result pages, Semantic Scholar
  and OpenAlex metadata pages, DOI resolver pages without extracted source
  content, Kaggle reuploads, GitHub link/resource lists, and search snippets
- use these only to locate the publisher, institutional repository, original
  dataset owner, DOI, lawful open-access full text, or canonical data archive
- a reachable aggregator does not inherit the authority of the source it
  links to

### Discovery tools — strict ordering

For technical/engineering/scientific topics, use the CTOX web stack in
this **exact order**. The first move is always `ctox web deep-research`;
the lower-level surfaces are only for follow-up extraction once the
catalog has its first entries.

1. **`ctox web deep-research`** — mandatory first move for any technical
   library construction:

   ```
   ctox web deep-research --query "<topic>" --depth standard --max-sources 24
   ```

   This call internally combines scholarly + agency + standards +
   dataset + industry buckets into one ranked envelope. Use `--depth
   exhaustive --max-sources 40` when the catalog needs to be
   near-complete. The output JSON carries one entry per source with
   `url`, `title`, `source_type`, `source_tier`, `verification_status`,
   `canonical_url`, `transport_verified`, `content_extracted`, `evidence_eligible`,
   `evidence_rejection_reason`, `http_status`, and `snapshot_hash`. Feed
   those directly into the candidate source catalog. Do not invent URLs
   from training-data memory; only record what this call (or the follow-up
   reads) returned.

2. **`ctox web scholarly search`** — for second-pass DOI / open-access
   PDF enrichment of specific entries that `deep-research` flagged but
   did not resolve:

   ```
   ctox web scholarly search --query "<refined topic>" [--with-oa-pdf] [--only-doi]
   ```

3. **`ctox web read`** — for fetching the body of a specific landing
   page when you need to extract the actual dataset / file URLs hosted
   on it (e.g. an agency programme page that lists XLSX downloads).

4. **`ctox web search` and `ctox web sources info`** — only as fallback
   for obviously non-technical lookups (CRM entity, vendor matrix,
   regulatory page lookup) where the upper layers returned nothing.

**Forbidden during source discovery:**

- The OpenAI-native `web_search` tool. Its result envelopes are large
  and noisy; multiple calls overflow the context window and the
  payloads get compacted out before synthesis. CTOX provides
  `ctox web deep-research` and `ctox web scholarly search` precisely to
  return compact, structured source lists you can persist row-by-row.
- Piping any `ctox web …` output through `head`, `tail`, `head -N`,
  `tail -N`, or any byte-truncator. The result envelope must reach the
  agent intact; truncate via the command's own `--max-sources` /
  `--max-results` / `--limit` flag instead. Self-truncated discovery
  output loses Tier-1 candidates that ranked just below the cutoff.
- The legacy `ctox ticket source-skill-import-bundle` path during
  fresh discovery — use it only when a real bundle directory already
  exists. For incremental procedural-knowledge writes, use
  `ctox knowledge skill new / add-skillbook / add-runbook / add-item`.

Skipping the deep-research/scholarly surfaces and going straight to
`ctox web search`, the OpenAI native `web_search` tool, or a plain
`cat > workspace_file.md` is a discovery failure: the source ranking
will skew toward Tier 3, the catalog will miss the Tier 1 measurement
data the deliverable actually needs, and the result will not be
durable.

### Evidence promotion - mandatory fail-closed gate

Discovery results are candidates. Promote a candidate to evidence only after
the evidence manifest passes the deterministic guard and all checks below pass:

1. **Transport/freshness**: the canonical non-metadata URL returned 2xx and
   the snapshot is current, downloaded bytes, and SHA-256 verified.
2. **Content**: the snapshot contains actual full text or original data. A
   title, abstract, metadata card, cookie/login wall, empty shell, snippet,
   DOI landing page, mirror, or aggregator is never evidence.
3. **Relevance/directness**: the original owner URL directly supports the
   facet and has an explicit relevance score of at least 8/10.
4. **Data integrity**: every original data file was downloaded and passed the
   deterministic hash/schema/row/unit/null check; failures are quarantined,
   not guessed or imputed.
5. **Claim trace**: every factual or numerical claim records the exact file,
   table/figure, row/range, column, unit, conversion, derivation, and the
   immutable Claim -> Evidence -> Snapshot -> Source lineage.

Retain rejected candidates with their rejection reason for auditability, but
exclude them from knowledge construction, calculations, and report evidence
registers. Never fill a failed or missing read from model memory.

### Breadth before depth — facet the query, never settle for one pull

A single `ctox web deep-research --query "<topic>"` returns one ranked
envelope. Ranking favors well-cited, canonical sources, so a single broad
query converges on the obvious references and leaves the long tail of
niche Tier-1 sources below the `--max-sources` cutoff — they never surface.
One pull is a starting point, not a complete discovery. Treat discovery as
a controlled facet sweep, not a single call:

1. **Facet the topic into orthogonal sub-queries.** Decompose the topic
   into independent angles that each surface a different ranked list —
   e.g. by source-class, by sub-phenomenon, by data type, by methodology,
   or by regime/region. Each facet is its own `ctox web deep-research`
   call. Run dependent facets serially. Independent, orthogonal facets may use
   bounded subagents, but the parent owns deduplication, verification, and
   promotion decisions. If the sweep needs to span turns, persist an internal
   work item and resume. Vary the query string between calls; re-issuing the
   same query just returns the same top hits.

2. **Exclude what you already hold.** The source-catalog table you are
   appending to *is* your exclusion list. Before each new facet, steer the
   wording away from the source-classes and specific sources already
   captured, so ranking is pushed off the canonical hits and into
   unexplored niches. A query that does not steer away from what you
   already have will re-return the same top entries.

3. **Stop on saturation, not on first results.** Discovery is done when
   consecutive new facets return only sources already in the catalog — not
   when the catalog merely has entries. If two or three orthogonal facets
   in a row surface no new Tier-1 source, the space is saturated and you
   can move to schema/blueprint inference. Until then, keep faceting.

This breadth pass is what separates a catalog that captured only the
obvious references from one that is actually near-complete. A catalog built
from a single query will systematically miss the Tier-1 sources that ranked
just below the cutoff.

### Append as you discover — never batch at the end

Write each `ctox web deep-research` / `ctox web scholarly search`
result batch into `ctox knowledge data append --domain <d> --key
source_catalog --rows '[…]'` **before** issuing the next discovery
call. Reasons:

- Native API-tool responses are large; without incremental persistence
  the agent's per-turn context fills with raw search envelopes, those
  get compacted out, and the final synthesis falls back to training-data
  memory of canonical references instead of the actual fresh results.
- The catalog row is the durable record. If the turn times out
  mid-discovery, the rows you have already appended survive and the
  next turn (or queue worker) can resume from where you left off.
- Provenance is preserved at row level: each row records `source_url`,
  `extracted_at`, the discovery query that found it, and the bucket
  the upstream tool assigned (`scholarly`, `agency`, `dataset`, …).

After persisting a candidate batch, read each new canonical source and update
its verification fields before another phase consumes it. A batch is not
complete while any candidate lacks an explicit `eligible` or `rejected`
decision.

Before closing discovery, run three independent reviews over the persisted
receipts. They must be distinct, passing reviews, not three labels on one
agent's unchecked output:

- **Source auditor**: reopens every eligible canonical URL and confirms
  authority, content extraction, topical relevance, and snapshot hash.
- **Data auditor**: reproduces every imported numeric field from the original
  archive/table and verifies units, parsing, conversions, nulls, and row counts.
- **Claim auditor**: checks each knowledge statement and report claim against
  eligible source or data receipts and rejects unsupported strength or scope.

Run `scripts/evidence_guard.py` after these reviews. A failed check blocks
library import, Knowledge promotion, and report publication.

The parent agent resolves disagreements and owns the final promotion decision.

## Phase 2A — Library mode

Run order: **source-catalog first, then the actual library**. The
source-catalog from Phase 1 is what gives you the schema for the library
— do not invert the order. A library row that cannot be traced back to a
source-catalog row by `source_id` is hearsay.

Drive the work through `ctox knowledge data create / add-column / append /
import / export`. CTOX is the system of record:

- The catalog lives in SQLite (`knowledge_data_tables`) so you can
  discover, name, fork, archive, and drop tables across turns.
- The content lives in Parquet so very large collections do not bloat the
  runtime DB and the schema can evolve.
- You decide the domains, table keys, columns, and enrichment passes —
  CTOX is not a framework prescribing schemas.

For technical/research libraries, the typical row pattern is:

- one row per primary measurement record (e.g. a single propeller test
  point from UIUC, a single NASA MTB2 test point, a single instrumented
  flight log segment, one published material-property datapoint)
- include `source_id` linking back to the source-catalog
- include `record_type` distinguishing empirical / derived /
  manufacturer-spec / standard
- include `derivation_method` and `assumption_text` for any
  non-trivial computation done on top of the source value

When the sources include both primary measurements **and** OEM/vendor
specs, keep them in the **same** table with different `record_type`
values rather than splitting into two tables — the curation discipline
is the same, the source-class field just makes it queryable.

Curation discipline (non-negotiable):

- **Provenance**: every row that carries a non-trivial fact records
  `source_url`, `source_id`, `extracted_at`, optionally a verbatim quote.
  A row without provenance is hearsay.
- **No extrapolation**: if a fact requires inference beyond what the
  source actually states, mark it (`derived_from`, `assumption_text`,
  `derivation_method`) or leave the cell `null`. Do not silently impute.
- **Schema evolves additively**: when the third source brings a field the
  first two did not have, `add-column` rather than smuggling it into a
  free-text field. Earlier rows get `NULL` — that is honest.
- **Single source of truth per table**: do not split the same conceptual
  list across two tables. Use `--tag` or a dedicated column for subsets.
- **Cite the table back**: when you report numbers in chat, name `domain`,
  `table_key`, and the snapshot timestamp.
- **Locale-safe numeric storage**: store machine-readable numbers with `.` as
  decimal separator and no thousands separator. Keep units in typed columns or
  column metadata, never in numeric cells. German `,` formatting is a display
  or explicitly declared export presentation only; CSV exports must declare
  delimiter, encoding, decimal convention, and preserve Excel-safe columns.

When the library work crosses into analysis that exceeds counts and simple
group-bys (clustering, modeling, hypothesis tests, complex joins, custom
statistics), use the `clone` → describe → Python via `shell` against the
`parquet_path` → `import --mode replace` pattern. Reusable Python belongs
in `scripts/` here, not as ad-hoc inline code.

### Cross-linking the library to procedural knowledge

When the library is itself the output of a documented procedure (a runbook
item / skillbook in `ctox knowledge skill`), or when later steps will need
to consult a specific runbook item for derivation rules, record the edge
explicitly so future turns find both sides:

```sh
# the library was produced by following this runbook item
ctox knowledge link --from data_table:<domain>/<table_key> --to runbook_item:<item_id> --relation produced_by --note "<one-line reason>"

# a runbook item should pull values from this library
ctox knowledge link --from runbook_item:<item_id> --to data_table:<domain>/<table_key> --relation consult --note "<one-line reason>"
```

Then `ctox knowledge search --query "<topic>" --with-references` reveals
the procedure ↔ data relationship for any hit, instead of leaving it
buried in free-text comments. Use `ctox knowledge kinds` to see the
canonical relation labels.

### Knowledge promotion — required for reusable or living research

When the operator expects later questions, reports, or periodic research
updates, the verified source catalog and data tables are inputs, not the end of
the chain. Promote them into procedural Knowledge before drafting documents:

1. Create or update one topic-scoped main skill with `ctox knowledge skill`.
   Record the source-catalog table, promoted data-table versions, research run,
   schema version, and freshness timestamp.
2. Use skillbooks to organize stable interpretation knowledge and runbooks for
   repeatable procedures, calculations, refreshes, audits, and report creation.
   Do not paste a report into a skillbook and call it knowledge.
3. Keep each knowledge item concise and executable. Link it to exact verified
   source IDs, snapshot IDs/content hashes, data-table rows or ranges, units,
   derivations, and caveats. The source snapshot remains the place to read the
   underlying evidence.
4. Require the trace `knowledge_version -> claim_id -> evidence_id ->
   snapshot_id -> source_id -> canonical_url` for every factual or numerical
   statement, including the claim lineage hash. Missing,
   unreachable, rejected, or hash-invalid evidence invalidates the dependent
   claim; it must not silently fall back to model memory.
5. Preserve conflicts as contested claims with both evidence paths. Do not
   collapse disagreement into a fabricated consensus.
6. Link the main skill, skillbooks, runbooks, source catalog, data tables, and
   research run through `ctox knowledge link`. A UI consumer must be able to
   traverse Research → Knowledge → source snapshot and Knowledge → Documents.

Knowledge is living state. On refresh, use this order as one append-only
promotion workflow: discover candidates -> verify new snapshots -> download
and deterministically check original data -> build a new data/Knowledge version
-> rerun independent Source/Data/Claim reviews -> record invalidations for
dependent claims and reports -> regenerate or explicitly revalidate reports.
Never mutate an old source hash, claim, table, Knowledge version, or report in
place merely to keep it appearing current.

## Phase 2B — Decision-report mode

Drive the work through `ctox report …` CLI subcommands. The full mode
playbook lives in `references/decision-report-mode-full.md` — open it
before starting decision-report work. It covers:

- The nine `report_type_id` values and when each applies (typical_chars,
  min_sections, the type-specific evidence and section conventions).
- The deterministic `ctox report new`, `ctox report status`, `ctox report
  flavor-brief`, and write/publish subcommands.
- Evidence register discipline: every non-trivial claim cites an
  `evidence_id` from the run's register, except for project descriptions
  where the register is a silent drafting ledger.
- Quality gates and release-guard lints (see also
  `references/release_guard_lints.md`).
- Style guides for Fördervorhaben / project description deliverables
  (`references/project_description_style.md`,
  `references/project_description_reference_archetype.md`).
- Sub-skill workflows for writing, revising, and flow review
  (`references/sub_skill_writer.md`, `sub_skill_revisor.md`,
  `sub_skill_flow_reviewer.md`).
- Manager-path orchestration when the run is large enough to warrant
  multi-stage execution (`references/manager_path.md`).
- Troubleshooting (`references/troubleshooting.md`).

When the requested deliverable is Word/DOCX, route the final artifact
through the file-backed `doc` skill after the report content is assembled.
That skill is CTOX's canonical Word production path: apply a design preset,
real Word styles, real numbering, explicit table geometry, captions/figures
where useful, and render-or-structural QA. Do not hand off a Markdown file as
the final deliverable when a Business OS Documents command requests `.docx`.

Decision-report mode produces exactly one `report_type_id` per run. If
the operator asked for multiple report types, open separate runs.

## Phase 2C — Combined mode

For deliverables that are both a durable data collection and a written
synthesis:

1. Run library mode first. Persist the records into `ctox knowledge data`.
2. Run Knowledge promotion. Build or update the topic skill, skillbooks, and
   runbooks, then link them to the verified catalog and versioned tables.
3. Run decision-report mode from the promoted Knowledge version. The report's
   evidence register points at Knowledge claim IDs and the underlying table
   rows/source snapshots. Automatically selected skills must still be recorded
   in the document lineage; a user-selected skill must be validated for topic,
   freshness, and evidence health.
4. Publish the document and export files only after the Knowledge and claim
   audits pass. Return the document/spreadsheet/file IDs and locations so the
   user can open or download every artifact.

Combined mode is the default when the operator's wording uses both data
and judgement verbs ("compare X and recommend which to use", "build a
library of Y and write a feasibility study from it").

## CLI surfaces

Two complementary CLIs back the two output modes. Both are scripted from
this skill via Bash.

**`ctox knowledge data`** (library mode):

- Lifecycle: `create`, `list`, `describe`, `clone`, `rename`, `archive`,
  `restore`, `delete`, `tag`, `untag`
- Read: `head`, `schema`, `stats`, `count`, `select` (with `--where`,
  `--columns`, `--order-by`, `--limit`, `--offset`)
- Row-write: `append --rows <json-array>`, `update --where … --set …`,
  `delete-rows --where …`
- Column-write: `add-column --column N --dtype <i64|f64|bool|str>
  [--default V]`, `drop-column --column N`
- Bridge: `import --from-file <path> [--mode replace|append]`,
  `export --to-file <path>`

`--where` operators: `=`, `!=`, `<`, `<=`, `>`, `>=`, `~` (regex). Repeat
`--where` for AND-chained predicates.

**`ctox knowledge` (discovery + cross-form linkage)**, peer of the data
form — used regardless of whether the run produces a library, a report,
or both:

- `ctox knowledge search --query <text> [--limit <n>] [--form <skills|procedural|data|facts>] [--with-references]`
  — single-call discovery across skill bundles, procedural main skills,
  data tables, and ticket-scoped facts. Always the first move when you
  start work on a topic; tells you what CTOX already owns before you
  open new tables.
- `ctox knowledge link --from <kind>:<id> --to <kind>:<id> --relation <name> [--note <text>]`
  — record a structural cross-reference. Canonical kinds and relations:
  `ctox knowledge kinds`.
- `ctox knowledge references --of <kind>:<id> [--direction <out|in|both>] [--relation <name>] [--limit <n>]`
  — list the edges touching a specific item.
- `ctox knowledge skill <verb>` — procedural-knowledge surface
  (main-skill + skillbooks + runbooks + labeled items). Use this when
  the research deliverable includes a process you want CTOX to remember,
  not only data. See `ctox knowledge skill --help` for the full verb
  list (`new`, `add-skillbook`, `add-runbook`, `add-item`, `query`,
  `import-bundle`, etc.).
- `ctox knowledge facts <verb>` — single ticket-scoped fact entries.

**`ctox report`** (decision-report mode):

- `ctox report blueprints` — list the nine `report_type_id` values and
  their conventions
- `ctox report new <report_type_id> --goal …` — start a run
- `ctox report status <run_id> --json` — read durable state
- `ctox report flavor-brief --run-id <run_id>` — type-specific brief
- `ctox report project-description-agent-brief --run-id <run_id>` for
  Fördervorhaben runs
- Plus deterministic write subcommands documented in
  `references/decision-report-mode-full.md`.

## Persisting open work

If at the end of the turn the library still has known gaps (columns that
should be filled but the source set did not cover them, sources that were
identified but not yet read, derived rows that need replacement with
measurements), persist exactly one internal work item in CTOX state pointing
at the table by `domain/table_key`. The next turn picks it up.

Likewise, if a decision-report run was started but is not yet at
publish-ready quality, persist a internal work item anchored on the
`run_id`. Workspace-only notes about open work do not count.

## Scope reminder

This skill is the right one for any research whose deliverable should
survive the turn. Most of the time the work is in Phase 1 (discovery) and
the chosen Phase 2 (library mode or decision-report mode). The skill is
not for one-shot answers, code explanations, short summaries, or live
debugging — those belong in ad-hoc reply work.
