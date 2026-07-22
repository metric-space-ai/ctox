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

- Treat `web_stack_plan.required_depth` from a typed Business OS research
  command as an immutable execution requirement. Resolve it before the first
  `ctox_deep_research` call and pass that exact value as the tool's `depth`.
  In particular, `required_depth: exhaustive` requires at least one successful,
  persisted typed `ctox_deep_research` call with `depth: exhaustive` in the
  current durable research run. A `standard` call is useful only as an
  orientation round and never satisfies an exhaustive command.
- Never downgrade required research depth because of token pressure, provider
  rate limits, an existing standard-depth workspace, or a retry. Preserve the
  task as pending when the required call cannot complete. Before synthesis,
  validation, or writeback, inspect the successful typed tool receipt and
  verify that its reported `depth` is at least the command's required depth.
- CTOX-managed harness sessions do not expose free child agents or
  `spawn_agent`. Keep discovery, extraction, synthesis, correction, and
  completion ownership in the same parent work item. Use the typed Web Stack
  tools for research work; do not plan around unavailable subagent controls.
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
Copy `relevance_score` exactly from the current typed `ctox_web_read`
`evidence_relevance_score` field. Never estimate, rescale, round, or replace
that machine-computed 0–10 value in the evidence manifest.
For prose, use the server-written `workspace_evidence` / deep-research
snapshot receipt and its full extracted-text artifact. Every claim must carry
a verbatim `evidence_quote` (at least six words and 40 characters) that occurs
in that extracted text; a title, abstract, snippet, model paraphrase, or a
quote copied from another URL is not evidence. Claims over original data have
the same quote minimum and must use a hash-bound `data_excerpt`; for nested
archives, list and hash every ZIP member in order so the guard can read the
final source bytes itself. Do not cite model-generated CSV/XLSX extracts.
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
3. **Candidate and verified-source tables**: open a `source_candidates`
   table in `ctox knowledge data` for every discovered source, including
   rejected and unresolved rows with provenance, source-class tag,
   verification state, snapshot hash, directness status, and a one-line note
   on what it contributes. Candidate rows are discovery receipts, not
   evidence. Open `source_catalog` as a separate verified-source registry and
   copy a source there only after `scripts/evidence_guard.py` passes for its
   current original-content snapshot. Library and decision-report mode may
   read evidence only from `source_catalog`; a row remaining solely in
   `source_candidates` is never citable. Build and verify both tables before
   drafting the actual library schema or report blueprint.
   For a Business OS research command, copy the exact `Research Run ID` and
   `Research Command ID` from the task into `research_run_id` and
   `research_command_id` on every row created or updated in `source_catalog`,
   `evidence_points`, `evaluation_matrix`, and semantic-graph tables. Never
   substitute a previous run's IDs or omit them. Native completion hashes and
   admits only the rows bound to that exact run/command pair.
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

### Discovery tools — adaptive iterative loop

Systematic Research owns the research strategy. No individual Web Stack tool
owns the workflow and no tool has a mandatory fixed position. Select the next
tool from the current evidence gap, exactly as in an agentic benchmark run:

- use `ctox_scholarly_search` for papers, DOI resolution, open-access copies,
  authors and bibliographic seed records
- use `ctox_web_search` for a focused query, Google-style discovery, official
  portals and source-specific lookup
- use `ctox_deep_research` for one broad, multi-provider candidate sweep; its
  result is one discovery round, never the completed research
- use `ctox_web_read` for the canonical original page, paper, PDF or data file
- use browser/scrape tools when a source requires interaction or structured
  extraction

Plan, execute, inspect, persist and then reformulate. Repeat with different
facets and the complete canonical exclusion list until the source space
saturates. A single static envelope is not Systematic Research.

For scientific work, the first scholarly response is a seed ledger, not a
reason to start another broad sweep immediately. Select the relevant DOI/OA
records, call `ctox_web_read` on at least the three strongest canonical
original/full-text URLs, and record accepted or rejected read receipts before
calling `ctox_deep_research`. If a provider reports CAPTCHA or HTTP 429, do not
repeat the same provider/query in the next facet. Continue from admitted seeds,
their references, repository links, and an orthogonal provider after its
cooldown.

In a managed Harness run, invoke the directly exposed typed tools
`ctox_deep_research`, `ctox_scholarly_search`, and `ctox_web_read`. Do not run
their `ctox web ...` CLI equivalents through `exec_command`: the shell sandbox
cannot write the server-owned receipt cache, so shell output is discovery-only
and cannot satisfy the evidence guard. The CLI examples below are for an
operator shell. If the typed tools are absent in a managed run, stop
fail-closed and report the platform defect instead of substituting shell
output, native web search, or memory.

Every typed `ctox_web_read` that may become evidence must include a precise
`query` describing the factual or engineering reading intent. URL-only reads
intentionally receive no server-owned relevance score and must remain
discovery-only. If a typed read is rejected, record that rejection; never
download the same URL with `curl`, Python, shell, browser automation, or another
unbound fallback for evidence.

1. **Broad candidate sweep (`ctox web deep-research`)** — optional when a
   multi-provider orientation round is useful:

   ```
   ctox web deep-research --query "<topic>" --depth <required-depth> --max-sources 24 --workspace "$PWD/research/deep-research-$(date +%s)"
   ```

   For an untyped ad-hoc task, use `standard` as the default value for
   `<required-depth>`. For a typed Business OS command, copy
   `web_stack_plan.required_depth` exactly; do not infer or choose a cheaper
   depth. This call internally combines scholarly + agency + standards +
   dataset + industry buckets into one ranked envelope. Use `--depth
   exhaustive --max-sources 40` when the catalog needs to be
   near-complete. The output JSON carries one entry per source with
   `url`, `title`, `source_type`, `source_tier`, `verification_status`,
   `canonical_url`, `transport_verified`, `content_extracted`,
   `evidence_eligible`, `evidence_rejection_reason`, `http_status`, and
   `snapshot_hash`. Feed `source_candidates` directly into the
   `source_candidates` table. Feed `sources` into `source_catalog` only after
   the full manifest passes `scripts/evidence_guard.py`; the tool-level
   eligibility flag alone does not replace claim, data, and independent-review
   validation. Do not invent URLs from training-data memory; only record what
   this call (or the follow-up reads) returned.

   Always use a unique writable folder inside the current task workspace for
   shell invocations. The typed `ctox_deep_research` tool supplies such a
   call-specific workspace automatically when `workspace` is omitted.

2. **Scientific discovery (`ctox web scholarly search`)** — use directly
   whenever papers are expected; do not wait for another tool to flag them:

   ```
   ctox web scholarly search --query "<refined topic>" [--with-oa-pdf] [--only-doi]
   ```

3. **`ctox web read`** — for fetching the body of a specific landing
   page when you need to extract the actual dataset / file URLs hosted
   on it (e.g. an agency programme page that lists XLSX downloads).

4. **`ctox web search` and `ctox web sources info`** — for focused discovery,
   Google/provider diagnostics, official domains and queries that need a
   smaller result envelope than the broad sweep.

For scientific topics, follow the literature graph rather than stopping at
keyword search:

1. Find relevant seed papers through Scholar/OpenAlex/Crossref/Semantic
   Scholar and resolve DOI records to lawful original or open-access full text.
2. Read the seed paper and collect its cited references, datasets and
   supplementary files.
3. Resolve relevant backward references and forward citations, then read their
   original full text.
4. Persist parent-paper, cited-paper and relation provenance. Metadata records
   remain candidates; only read, hash-bound original content can become
   evidence.
5. Continue breadth-first across relevant references until two consecutive
   citation/facet rounds add no new eligible source.

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

Using only one search surface is a discovery failure. The parent must combine
the adapters required by the evidence space and persist every round before
continuing.

### Evidence promotion - mandatory fail-closed gate

Discovery results are candidates. Promote a candidate to evidence only after
the evidence manifest passes the deterministic guard and all checks below pass:
Before creating or repairing that manifest, read
`references/evidence_integrity.md` and use its exact
`ctox.research.evidence.v2` schema. Do not infer a schema name or invent a
reduced manifest shape from table rows.

1. **Transport/freshness**: the canonical non-metadata URL returned 2xx and
   the snapshot is current, downloaded bytes, and SHA-256 verified.
   HTTP 204 is not content evidence. Every admitted item must include the
   immutable `ctox_web_read` or `ctox_deep_research` retrieval receipt with
   request/final URL, status, `checked_at_epoch`, byte count, content kind,
   body hash, and a hashed v2 receipt artifact inside the current workspace.
   Copy those fields exactly from the artifact: never rewrite redirects,
   interstitial URLs, timestamps, or response metadata. The evidence
   `canonical_url` must equal the persisted final URL.
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

The evidence manifest must copy the task's exact `Research Run ID` and
`Research Command ID` into `research_run_id` and `research_command_id`.
It must also copy the server-injected `Research Attempt ID` into
`research_attempt_id`; `run_id` must equal `research_run_id`. The only accepted
manifest path is `validation/evidence-manifest.json`. Every admitted Web Stack
receipt must belong to the same durable research run, command, and workspace.
Immutable retrieval receipts may be reused across bounded correction attempts
of that run; the manifest itself and the completion result must always carry
the current server attempt ID.
All artifact paths are workspace-relative; absolute paths, `..` escapes, and
symlink escapes are rejected. Free subagents are not part of CTOX research:
never call `spawn_agent`, `spawn_agents_on_csv`, or related collaboration
tools. The deterministic evidence guard verifies every original-content
receipt, content hash, data artifact, row/schema/unit constraint, and
Claim -> Evidence -> Snapshot -> Source lineage. After that gate passes, the
CTOX service runs its independent completion review outside the parent tool
surface. Living Knowledge and Report versions include hashed artifacts plus
the exact claim IDs they consume.

Retain rejected candidates with their rejection reason for auditability, but
exclude them from knowledge construction, calculations, and report evidence
registers. Never fill a failed or missing read from model memory.

The admitted source set contains one row per unique canonical original URL and
every manifest source must have a matching eligible Evidence entry. URL aliases,
duplicate records, Wikipedia or other tertiary encyclopedias remain discovery
candidates only and never increase the verified source count.

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
   call. Run facets serially in the parent. If the sweep needs to span turns,
   persist an internal work item and resume. Vary the query string between
   calls; re-issuing the same query just returns the same top hits.

2. **Exclude what you already hold.** The source-catalog table you are
   appending to *is* your exclusion list. Pass every canonical URL already
   present through `exclude_urls` on each later `ctox_deep_research` call,
   in addition to reformulating the facet. Existing sources must not consume
   discovery or read budget.

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
source_candidates --rows '[…]'` **before** issuing the next discovery call.
Promote only the rows whose original-content receipts pass the evidence guard
into `source_catalog`. Reasons:

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

Before claiming the Business OS task complete, re-read every output table and
verify that all rows attributed to this run carry the exact immutable
`research_run_id` and `research_command_id` from the command. Missing or mixed
lineage is a failed run, not a partial success.

Before closing discovery, the parent must complete all three audit passes over
the persisted material:

- **Source integrity**: reopen every eligible canonical URL and confirm
  authority, full-content extraction, topical relevance, status, and snapshot
  hash through CTOX Web Stack receipts.
- **Data integrity**: reproduce every imported numeric field from the original
  archive/table and verify units, parsing, conversions, nulls, and row counts.
- **Claim integrity**: bind every knowledge statement and report claim to the
  exact eligible source/data receipt and reject unsupported strength or scope.

These are deterministic build checks, not model-authored review labels. Run
`scripts/evidence_guard.py` after the passes. A failed check blocks library
import, Knowledge promotion, and report publication. The independent
service-owned completion review runs only after this guard succeeds.

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
