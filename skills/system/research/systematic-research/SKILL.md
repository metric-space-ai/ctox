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
Never write workspace markdown, CSV, or JSON files as the deliverable for
durable research — those vanish after the turn.

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission
  progress, external waiting, recovery, or explicit decomposition. Do not
  spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review
  feedback, continue the same main work item whenever possible and
  incorporate the feedback there.
- Every durable follow-up, queue item, plan emission, or self-work item
  must have a clear parent/anchor: message key, work id, thread key,
  ticket/case id, or plan step.
- Before adding follow-up work, check existing self-work, queue, plan, and
  ticket state and consolidate instead of duplicating.

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
   source-class tag, and a one-line note on what it contributes). Library
   and decision-report mode both read from it. Build it up before drafting
   the actual library schema or report blueprint.
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
- scholarly literature: Google Scholar (scholar.google.com), Semantic
  Scholar, OpenAlex, arXiv, ResearchGate, IEEE Xplore, SAE Mobilus, ACM
  Digital Library, Elsevier/Springer/Wiley DOIs
- public dataset repositories: Zenodo, Figshare, Dataverse, HuggingFace
  Datasets, Open Science Framework (OSF), Kaggle
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

### Discovery tools

The following CTOX surfaces map onto the tiers above; use the most
specific one available:

- `ctox web scholarly search --query "<topic>" [--ext pdf] [--with-oa-pdf]`
  — for Tier 1 scholarly literature with open-access PDFs and DOI
  filters. Strongly preferred over plain web search for engineering and
  scientific topics.
- `ctox web deep-research --query "<topic>" --depth standard --max-sources <N>`
  — for multi-profile source mining (combines scholarly + agency +
  standards + dataset + industry buckets in one call). Use `--depth
  exhaustive` when the source-catalog needs to be near-complete.
- `ctox web deep-research --query "<topic>" --depth exhaustive --max-sources <N>`
  plus `ctox knowledge data` recording — for fully systematic source review
  with explicit query plan and auditable source catalog. Do not execute embedded
  research scripts from this system skill; if a source-review operation lacks a
  CTOX CLI/API command, add that command first.
- `ctox web search` and `ctox web read` — generic fallback when the
  scholarly/deep-research surfaces returned nothing useful for an
  obviously non-technical topic (a CRM-style entity list, a vendor
  matrix where vendor pages ARE the primary source, …).

Skipping the scholarly/deep-research surfaces and going straight to
`ctox web search` on a technical topic is a discovery failure — the
ranking will skew the source catalog toward Tier 3 and you will miss
the Tier 1 measurement data that the deliverable actually needs.

Going straight to `cat > workspace_file.md` before this discovery pass
is a discipline failure of a different kind — you bypass durable
persistence entirely.

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

When the library work crosses into analysis that exceeds counts and simple
group-bys (clustering, modeling, hypothesis tests, complex joins, custom
statistics), use the `clone` → describe → Python via `shell` against the
`parquet_path` → `import --mode replace` pattern. Reusable Python belongs
in `scripts/` here, not as ad-hoc inline code.

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

Decision-report mode produces exactly one `report_type_id` per run. If
the operator asked for multiple report types, open separate runs.

## Phase 2C — Combined mode

For deliverables that are both a durable data collection and a written
synthesis:

1. Run library mode first. Persist the records into
   `ctox knowledge data`.
2. Run decision-report mode with the library as the primary evidence
   source — the report's evidence-register entries point at the library
   table by `domain/table_key`, and key claims cite specific rows.
3. The report mentions the library by name so future readers can re-open
   the source.

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
measurements), persist exactly one self-work item in CTOX state pointing
at the table by `domain/table_key`. The next turn picks it up.

Likewise, if a decision-report run was started but is not yet at
publish-ready quality, persist a self-work item anchored on the
`run_id`. Workspace-only notes about open work do not count.

## Scope reminder

This skill is the right one for any research whose deliverable should
survive the turn. Most of the time the work is in Phase 1 (discovery) and
the chosen Phase 2 (library mode or decision-report mode). The skill is
not for one-shot answers, code explanations, short summaries, or live
debugging — those belong in ad-hoc reply work.
