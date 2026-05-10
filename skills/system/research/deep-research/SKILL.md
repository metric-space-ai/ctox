---
name: deep-research
description: Produces decision-grade reports across nine types — Machbarkeitsstudien (feasibility), Projekt-/Fördervorhabenbeschreibungen, Quellenreviews/Quellenkompendien, Marktanalyse (market research), Wettbewerbsanalyse, Technologie-Screening, Whitepaper, Stand-der-Technik (literature review), Entscheidungsvorlage (decision brief). The harness LLM drives the run by calling deterministic `ctox report …` CLI subcommands.
class: system
state: active
cluster: research
---

# Deep Research

You are the harness LLM. This skill instructs you to produce a decision-grade
written report on the topic the operator named. You drive the run yourself by
calling deterministic `ctox report …` CLI subcommands via Bash. There is no
external "manager" — you are the manager. There is no external "writer
sub-skill" — you write the prose yourself, then save it via the CLI.

## Use this skill when

Trigger this skill whenever the operator asks for an evidence-grade,
decision-supporting written deliverable that requires multi-source synthesis.

Trigger phrases (German + English):

- "Mach mir eine Machbarkeitsstudie zu …" / "feasibility study on …"
- "berührungslose Prüfverfahren für …" / "contactless inspection method assessment for …"
- "Materialprüfungs-Methodenvergleich …" / "materials testing method comparison …"
- "Technologie-Screening …" / "technology screening for …"
- "Projektbeschreibung / Fördervorhabenbeschreibung zu …" / "project description / funding project description for …"
- "Quellenreview / Quellenkompendium / möglichst alle Quellen zu …" / "source review / source compendium / find all sources on …"
- "Marktstudie / Marktrecherche zu …" / "market study / market research on …"
- "Wettbewerbsanalyse mit Bewertungsmatrix …" / "competitive analysis with scoring matrix …"
- "Entscheidungsvorlage mit Optionsbewertung …" / "decision memo with option evaluation …"
- "Stand-der-Technik-Übersicht für …" / "state of the art review for …"

Skip this skill for short answers, summaries of a single paper, code
explanations, or anything that does not need a multi-section cited report.

## Report types

Every run is bound to exactly one `report_type_id`. Pick from these nine:

| `report_type_id`         | When                                                                    | Typical chars | Min sections |
| ------------------------ | ----------------------------------------------------------------------- | ------------- | ------------ |
| `feasibility_study`      | "Geht das Verfahren X für Anwendung Y?" — option matrix + verdict       | ~30 000       | 9            |
| `project_description`    | "Beschreibe Förder-/Innovationsvorhaben X" — company, problem, innovation, implementation, budget, benefit | ~22 000 | 8 |
| `source_review`          | "Finde möglichst alle Quellen/Daten zu X" — search log + source catalog + coverage/gaps | ~26 000 | 8 |
| `market_research`        | "Wie groß ist Markt X, wer kauft, was zahlen sie?"                      | ~25 000       | 7            |
| `competitive_analysis`   | "Wer sind die Anbieter, wie schneiden sie ab?"                          | ~20 000       | 8            |
| `technology_screening`   | "Welche Methoden gibt es überhaupt, welche schaffen es in die Shortlist?" | ~22 000     | 8            |
| `whitepaper`             | "Hier ist die These und die Argumente"                                  | ~18 000       | 5            |
| `literature_review`      | "Was ist der Stand der Forschung zu Thema X?"                           | ~28 000       | 6            |
| `decision_brief`         | "Eine-Seite-Vorlage mit Optionen und Empfehlung"                        | ~8 000        | 5            |

Cross-type invariants for every report:

- Length aligned to the type's `typical_chars` (±15% tolerance).
- Every non-trivial claim cites at least one entry from the run's evidence
  register by `evidence_id`, except `project_description`: for
  Fördervorhaben-/project-description deliverables the evidence register is a
  silent drafting ledger. Its facts shape the wording, but the final Word file
  must read like an applicant/project document, not like a scientific report.
- For types with a matrix block (feasibility, competitive, technology screening,
  decision brief): every cell carries a short rationale plus an `evidence_id`.
  The same rationale string MUST NOT appear in two cells of the same option.
- Verdict line per option uses the type's `verdict_line_pattern` from the
  asset pack (e.g. for feasibility: `Erfolgsaussichten (qualitativ): …`).

## How to run a report — the actual commands

Read the run state, do research, draft exactly one durable unit at a time,
stage it immediately, verify the state, then move on. All via Bash.

**Context and checkpoint discipline is mandatory.** Do not hold a whole
study, raw source dumps, or long scratch drafts in your conversation context.
The CTOX report database is the state machine. After every phase below, run
`ctox report status RUN_ID --json` and confirm the durable state changed
before continuing. If the state did not change, fix that transition first.

Never install OS or Python packages during a report run. Use the existing
CLI, Mermaid, structured tables, and built-in renderers. Package setup is not
research and it consumes the same execution budget needed for the deliverable.

### 1. Create the run

```bash
ctox report new <report_type_id> \
    --domain <domain_id_from_blueprints> \
    --depth standard \
    --language de \
    --topic "<konkretes Thema und Rahmenbedingungen>"
```

For a Fördervorhabenbeschreibung use:

```bash
ctox report new project_description \
    --domain innovation_funding_project \
    --depth standard \
    --language de \
    --topic "Fördervorhabenbeschreibung für <Unternehmen> zum Vorhaben <Projektname>; Laufzeit/Budget/Status/Kostenblöcke: <Angaben>"
```

For a Quellenreview / Quellenkompendium use:

```bash
ctox report new source_review \
    --domain technical_data_sources \
    --depth standard \
    --language de \
    --topic "Quellenreview zu <Daten-/Informationsfrage>; Scope: <Objekt/Klassen/Zeitraum/Region>"
```

Capture the printed `run_id` (`run_<hex>`). Use it on every subsequent call,
either as the first positional argument or via `--run-id RUN_ID`.

Available domain profiles, depth profiles, and report types:

```bash
ctox report blueprints
```

### 2. Read the run skeleton

```bash
ctox report status RUN_ID --json
```

Returns the run's `report_type`, `domain_profile`, `depth_profile`,
`character_budget`, the list of pending and committed blocks, the evidence
register size and the depth profile's `min_evidence_count` floor, plus any
operator-supplied seed DOIs / reference documents.

You also have the static knowledge in this skill's `references/` folder —
read those for block scaffolding and writing rubrics:

- `references/asset_pack.json` — every report type's `block_library_keys[]`,
  the document blueprint, the matrix template, the verdict-line pattern, the
  domain and depth profiles, the style profiles. **This is the canonical
  source of truth for what blocks the run needs.**
- `references/sub_skill_writer.md` — drafting rubric. Read it before you
  start writing. (It used to be a separate sub-skill; now it's instructions
  for you.)
- `references/sub_skill_revisor.md` — revision rubric.
- `references/sub_skill_flow_reviewer.md` — narrative flow rubric.
- `references/release_guard_lints.md` — every lint the structural
  release_guard_check enforces; if a draft would trip a lint, fix it before
  staging.
- `references/project_description_style.md` — mandatory for
  `project_description`; describes the Fördervorhaben writing process,
  structure, client-facing style and anti-patterns.

Do not use historical pseudo-tool documentation such as `check_contracts.md`
as an operating manual. The only executable interface in this skill is the
`ctox report ...` and `ctox web ...` CLI shown here.

Do not load or imitate topic-specific historical examples as a template for a
new study. A feasibility study can be about aircraft inspection, baking,
software architecture, chemistry, finance, or any other domain. The process is
generic; only the evidence, options, figures, and recommendations change.

For `source_review`, do not collapse the work into "find 20 citations". A
source review must separate three quantities:

- **Candidate hits screened**: the broad search-result pool, documented in a
  search-log table.
- **Usable sources**: sources that contain relevant information after screening.
- **Cited evidence**: the subset registered with `add-evidence` and cited in
  blocks/tables.

At `standard` depth, document roughly 1000+ screened candidate hits across
multiple search paths before claiming broad coverage. The final deliverable
must not stop at a small citation sample: it needs a large, grouped source
catalog with direct URL/DOI/access links. If the search protocol says hundreds
of sources were relevant or included, the catalog must show those sources in
the same order of magnitude, or the counts must be corrected downward.

When the source domain is international or the operator's prompt is in
English, create the run with `--language en`, write the prompt in English, and
stage English section titles via `--title`. Do not leave German headings in an
English source review.

For `project_description`, research is input, not the visible product. Do not
write a literature review, scientific paper, source review or citation-heavy
market study. Read `references/project_description_style.md` before drafting
and treat it as a release contract:

- final Word text has no bracket citations such as `[1][2]`, no DOI/reference
  list and no "Quellen und Recherchebasis" appendix unless the operator
  explicitly requests an annotated source appendix;
- write from the applicant/project perspective with a clear funding narrative:
  company development -> status quo / bottleneck -> innovation jump -> target
  operating model -> implementation -> costs/timeline -> economic benefit;
- create an internal `fact-transfer-ledger.md` before drafting. Extract
  concrete facts from the evidence register and map them to target chapters:
  company/legal/location/history, products/services, customers/segments,
  technical data, market/competitor baseline, project scope and economic
  mechanisms. At standard depth, transfer at least twelve non-prompt facts
  when available; richer evidence needs a correspondingly richer narrative;
- every major chapter must contain concrete researched facts, not only generic
  funding prose. Use research to make statements specific, then translate it
  into smooth Fördervorhaben prose without exposing the research mechanics;
- when Laufzeit, Status, Budget or Kostenblöcke are known, add a compact
  project-scope table. Prefer the deterministic helper
  `ctox report project-description-sync --run-id RUN_ID`; it extracts
  Laufzeit, Status, Budget and Kostenblöcke from the run topic / committed
  project-scope prose and binds the resulting native table to
  `project_scope_budget_timeline`. Use manual `table-add` only when the helper
  cannot parse the supplied framing;
- if a reference DOCX contains Word comments, treat comments as revision
  criteria for storyline, readability and structure. Pass commented reference
  files as `--review-doc` at `ctox report new` time, or immediately import
  them with `ctox report review-import --run-id RUN_ID --review-doc PATH`.
  Do not rely on a path mentioned only in prose; the comments must be present
  in `review_feedback`. Do not copy the comments into the final document.

### 3. Build the evidence register

You need at least the depth profile's `min_evidence_count` evidences in the
register before you start drafting blocks. **Stub evidences (title only,
no real source content) are rejected by the tool** — every entry must
carry either a resolver-fetched abstract, or a manually-supplied
abstract/snippet of at least 200 characters from `ctox web read`.

#### a) Resolver path (preferred, when a DOI or arXiv id is known)

```bash
# By DOI — resolver pulls Crossref + OpenAlex metadata, including the
# abstract. Use this path whenever a DOI exists.
ctox report add-evidence --run-id RUN_ID --doi "10.1016/j.compstruct.2020.112345"

# By arXiv id — resolver pulls the arXiv summary (often the canonical
# abstract for preprints).
ctox report add-evidence --run-id RUN_ID --arxiv-id "2401.12345"
```

Both paths populate `abstract_md` from the source automatically.

If a web page URL contains a DOI (for example a publisher URL ending in
`/10.xxxx/...`), still prefer registering the DOI with `--doi` as a separate
evidence item. If the DOI resolver fails but the URL page/PDF is readable, add
the URL evidence with real extracted content and do not cite the bare DOI in
the prose unless the release guard recognises it as registered evidence. Never
leave a DOI string in a block that is not backed by the current run's evidence
register.

#### b) Manual path (book, standard, magazine, web page)

For sources without a DOI / arXiv id, you must fetch the content yourself
with `ctox web read` and pass it via `--abstract-file`:

```bash
# 1. fetch the source page
ctox web read --url "https://www.nasa.gov/sti/some-paper" \
    > /tmp/nasa_lightning_raw.json
# 2. extract the abstract / key excerpts into /tmp/nasa_lightning_abs.md
#    (you do this — strip JSON envelope, keep the textual content)
# 3. register
ctox report add-evidence --run-id RUN_ID \
    --title "Lightning protection of CFRP structures" \
    --authors "Smith, A; Doe, J" \
    --year 2024 \
    --url "https://www.nasa.gov/sti/some-paper" \
    --abstract-file /tmp/nasa_lightning_abs.md
```

The CLI **rejects** every manual call where neither `--abstract-file` nor
`--snippet-file` carries at least 200 characters of real source content.
Title-only entries are not citable — that's what produced the previous
fake feasibility study.

#### Available web tools

```bash
# Topical web search — returns URLs + snippets
ctox web search --query "<query>"

# Primary evidence discovery. This combines web search, scholarly metadata
# and readable-source collection, and is more robust than calling a single
# scholarly backend directly.
ctox web deep-research --query "<query>" --depth standard --max-sources 12

# Optional metadata-only scholarly search. Treat empty results as non-fatal:
# continue with ctox web search/deep-research instead of stopping.
ctox web scholarly search --query "<query>" --max-results 12

# Fetch a specific URL and return markdown-extracted content
ctox web read --url "<url>"
```

#### Source-review search ledger

For every `source_review`, the search protocol is not a prose task. It is a
state-machine task. You must persist each search path before drafting and you
may only put counts in the Word table that are backed by these persisted logs.

Use the bundled discovery runner. It creates a broad query plan, saves every
raw JSON payload, deduplicates sources, writes `search_protocol.csv` and
an accepted-only `candidate_sources.csv`, and calls `ctox report
research-log-add` for every executed query. `candidate_sources.csv` is not a
raw metadata dump: every row must pass the topic-specific acceptance gate and
must carry a numeric relevance score. The full audit trail lives in
`screened_sources.csv`; rejected/off-topic hits live in `rejected_sources.csv`.
The paper/citation traversal lives in `discovery_graph.json`. Never hand the
raw screened catalog to the user as the source catalog.

For broad source-discovery tasks where the user expects hundreds or thousands
of screened sources, start with `--discovery-backend open-metadata`. It queries
OpenAlex and Crossref directly and is the correct first pass for corpus-scale
screening. Use `ctox web deep-research` afterwards only for targeted follow-up
reading and evidence extraction, not as the only mechanism for a 1000+ source
ledger.

```bash
python3 skills/system/research/deep-research/scripts/source_review_discovery.py \
  --topic "<source-review topic and scope>" \
  --run-id RUN_ID \
  --out-dir "/tmp/RUN_ID_source_discovery" \
  --max-sources-per-query 80 \
  --target-reviewed 1000 \
  --discovery-backend open-metadata \
  --query-timeout-sec 25 \
  --snowball-rounds 1
```

If the generated query plan is too generic for the topic, create a CSV with
columns `focus,query` and pass it via `--queries-file`. Do not reduce the
query count just to save time. Increase queries or `--snowball-rounds` when
the output reports fewer than the requested reviewed-result target.

For scientific source discovery, citation snowballing is mandatory when
`--snowball-rounds` is greater than zero. Do not skip it just because the first
metadata pass already exceeded `--target-reviewed`; the target is a minimum
screening depth, not a reason to avoid references/cited-by discovery. When
OpenAlex IDs are available, the snowball pass must use OpenAlex
`referenced_works`, `related_works` and `cites:` paths rather than merely
turning a DOI into another broad search string.

Discovery is not the same as reading. After the graph exists, select the
highest-scoring source families and perform a targeted reading/extraction pass
against source pages, PDFs, datasets and tables before writing a client-facing
research report.

Use the bundled reading runner for that pass. It resolves OpenAlex open-access
locations, tries direct PDF/HTML extraction plus `ctox web read`, writes a
readability ledger, and extracts measurement snippets into a separate evidence
table. A client-facing report must not claim full-text review for sources that
are only `metadata_only` or `blocked`.

```bash
python3 skills/system/research/deep-research/scripts/source_review_reading.py \
  --discovery-dir "/tmp/RUN_ID_source_discovery" \
  --out-dir "/tmp/RUN_ID_source_reading" \
  --limit 80 \
  --max-urls-per-source 6 \
  --read-timeout-sec 30
```

Mandatory reading artifacts:

- `reading_status.csv`: one row per selected source with `extracted`,
  `readable_no_measurements`, `metadata_only`, or `blocked`.
- `extracted_measurements.csv`: normalized evidence rows with value, unit,
  measurement family and source snippet.
- `reading_graph.json`: source-to-evidence graph for later report figures and
  audit.

If fewer than 15 selected sources are actually readable, continue source
resolution and targeted follow-up before drafting the report, or state the
access limitation explicitly. Do not turn a metadata-only corpus into a
research report.

Do not invent `sources-count`. The `research-log-add` command now requires a
raw payload file and rejects counts that are not backed by the payload's
source/result records. If the tool returns only 47 reviewed results, log 47.
To reach broad coverage, run more query families; never fill the gap with
rounded estimates. The final search-protocol and source-catalog tables are
generated from CTOX state, not from memory or desired coverage. After all
research logs and evidence sources are persisted, run:

```bash
ctox report source-review-sync --run-id RUN_ID
```

This command rebuilds the search protocol, scoring model and grouped source
tables from `report_research_log` and `report_evidence_register`. If the
generated tables are too small, the answer is more discovery and more
persisted evidence, not hand-written count inflation.

Mandatory source-review discovery passes:

1. Broad web and domain synonyms.
2. Scholarly metadata / OpenAlex / DOI-oriented queries.
3. Agencies and regulators.
4. Standards and public standards metadata.
5. Technical reports, government repositories and institutional libraries.
6. Datasets, repositories and telemetry/data portals.
7. OEM / industry manuals and datasheets.
8. Patents and adjacent technical reports.
9. Citation snowballing: take the strongest papers/reports found so far,
   inspect references/cited-by metadata where available, then run follow-up
   queries for recurring authors, datasets, report numbers, standards and
   terminology. Stop only when the next pass yields no material new source
   families or the depth target is met.

For `technical_data_sources` source reviews, treat the deliverable as a
source-map, not a bibliography. Before running discovery, write a short data
need map with:

- target object/class, operating range and exclusions;
- the exact measured variables sought;
- source families likely to contain those variables;
- synonyms and classification terms that change the search result set;
- "direct data", "proxy data" and "context-only" criteria.

For drone/UAS load-data research, do not assume one meaning of "load". Cover
and label at least these meanings where relevant: payload/cargo mass, takeoff
weight/MTOW/AUW, thrust/force/load-cell measurements, rotor/propeller loads,
airframe/aerodynamic loads, flight-log load proxies such as current draw or
motor output, and regulatory or military class definitions. For Class 1/2 or
up-to-25 kg scope, explicitly include terms such as `sUAS`, `UAS`, `UAV`,
`drone`, `multirotor`, `fixed-wing`, `eVTOL`, `Group 1`, `Group 2`, `DoD UAS
classification`, `MTOW`, `maximum takeoff weight`, `payload capacity`, `load
cell`, `thrust stand`, `force moment`, `flight log`, `PX4`, `ArduPilot`,
`NASA`, `FAA`, `EASA`, `DTIC`, `DoD`, `NATO`, `ASTM`, `manufacturer
datasheet`, `technical report`, `dataset`, and `GitHub`.

The source catalog for technical data-source reviews must expose usefulness, not
just existence. Add columns where the final table format allows it:

- `Data type`: measured data, dataset, specification, regulation, standard,
  technical report, paper, repository, manual, or proxy.
- `Variables available`: e.g. mass, payload, thrust, current, forces/moments,
  speed, endurance, dimensions, test setup.
- `Vehicle scope`: platform type, size/weight class, example vehicles.
- `Access`: public download, public page only, paywalled, restricted, unclear.
- `Extraction effort`: ready dataset, table/PDF extraction, manual reading,
  scraping/API, or not extractable.
- `Best use`: primary quantitative source, triangulation, terminology,
  legal/classification context, or exclusion.

For standard depth, persisted research logs should cover roughly 1000+
reviewed candidate hits unless the operator explicitly requested a narrow
search. If the real reachable corpus is smaller, say that explicitly and let
the counts remain smaller; do not create a fake 1000+ table.

Client-facing wording must be plain: use "search protocol", "search path",
"search term", "reviewed results", "excluded results" and
"included/relevant sources". Do not use internal phrases such as "ledger",
"screened candidate", "usable/cited", `source_review`, raw evidence IDs or
run/workspace terms in the visible report.

After the discovery pass, create source tables before writing synthesis by
running `ctox report source-review-sync --run-id RUN_ID`. The generated tables
must then be cited, summarized and interpreted in the prose:

1. A short coverage summary table by source group.
2. A scoring model table. If the operator did not provide one, define a
   task-specific A-D or 0-5 scheme before scoring sources. Typical criteria:
   directness of the data for the question, public verifiability, data
   granularity, topical fit, recency/currentness, and access friction.
3. A grouped source catalog with at least: `Group`, `Source`,
   `Publisher/author`, `Year`, `Type`, `Data contribution`,
   `Direct URL/DOI`, and `Score`.
4. Group-specific source tables for the main groups. For technical source
   reviews this normally means regulation/agencies, military/DoD/NATO,
   standards, NASA/DTIC/technical reports, academic literature,
   datasets/repositories, OEM/industry/manuals, patents/other.

Every source table row must be traceable with a direct URL, DOI, arXiv link,
repository URL, standards identifier with access page, or a clear "not public /
paywalled" access note. A visible table with 50 example sources is not a
complete source review when the search protocol says hundreds of useful
sources were found.

Every source table row must also be scored. Do not use "registered" or
"included" as a substitute for a score. The score rationale must say what was
actually learned from the source, not only that the source exists. Example:
"A - direct six-axis rotor force/moment data with public XLSX files" is
useful; "A - relevant source" is not.

Each `add-evidence` prints the new `evidence_id` (`ev_<hex>`) plus the
abstract/snippet length actually stored. Keep a notes file mapping
`evidence_id → topic` so you can later cite the right ones.

**Verify before drafting:** after registering N evidences, run
`ctox report status RUN_ID --json` and check that
`evidence_register_size >= depth_profile.min_evidence_count`. If the
register is full of titles but no abstracts, the prose you'll write will
be hallucinated — the lint engine catches that, but it's faster to fix
the evidence step first.

For `source_review`, also verify before drafting:

```bash
ctox report status RUN_ID --json
```

The status must show non-empty `available_research_ids`. When committing the
search-method block, attach the relevant IDs:

```bash
ctox report block-apply --run-id RUN_ID \
  --instance-id doc_source_review__source_review_search_method \
  --used-research-ids "research_..."
```

If the search table claims candidate counts but the block is not linked to
research IDs, the deliverable-quality gate fails. This is intentional: a
search protocol without provenance is fake research.

### 3x. Mandatory small-stage workflow

From this point on, work in small durable stages. Do not draft the whole
report in one hidden scratch buffer and do not wait until the end to persist.

Required loop:

1. Pick one artifact or one report block only.
2. Load only the evidence needed for that unit with `ctox report evidence-show`.
3. Write the artifact/block to a temp file.
4. Immediately persist it with `storyline-set`, `figure-add`, `table-add`, or
   `block-stage`.
5. Immediately verify with `ctox report status RUN_ID --json`, `block-list`,
   `figure-list`, or `table-list`.
6. Only then move to the next unit.

For block prose, stage one block at a time and run `ctox report block-apply`
after every one to three staged blocks. Never carry more than three unstaged
blocks in context. If context grows large, stop adding new source text, write
a short handoff note in `synthesis/context-handoff.md`, and continue from the
durable report state.

The final document is allowed only after durable content exists. Rendering a
DOCX/PDF before at least the required blocks, figures, tables, and checks are
present is a workflow failure.

### 3a. Storyline — write the dramatic spine BEFORE any block

A real feasibility study is not an inventory of facts in a fixed
section order. It is a journey the reader is led through: a tension is
opened, a naïve answer is proposed, the naïve answer fails on a
specific finding, a turning point reframes the problem, and the
recommendation falls out as a consequence. Without this spine, the
output reads as disconnected sentences with citations bolted on — the
exact failure mode this skill exists to prevent.

**Hard rule: before any `block-stage`, you MUST persist a storyline
treatment.**

```bash
cat > /tmp/storyline.md <<'EOF'
Diese Studie folgt zwei Spannungsbögen.

Erstens: die offensichtliche Antwort auf "geht das" ist Methode A — sie
koppelt am direktesten an das Kupfergitter. In §5 zeigt sich aber an
zwei Quellen, dass A am Schichtaufbau-Lift-off scheitert.

Zweitens: die saubere Auflösung ist nicht eine bessere Einzelmethode,
sondern ein gestuftes Konzept aus A + B + C. Diese Architektur taucht
in §1 als These auf, wird in §6 pro Methode mit ihren Stärken und
Lücken belegt, in §7 zur Roadmap (Phase 0/1/2), und in §9 zur
priorisierten Empfehlung.

Block-Bogen-Allokation:
- §1 Management Summary: tension_open + resolution_ratify (nimmt das
  Ergebnis vorweg).
- §2 Ausgangslage: tension_open vertieft.
- §3 Bauteilaufbau: support.
- §4 Anforderungen: support.
- §5 Screening: complication (Matrix zeigt: keine Methode überall stark).
- §6.x Detailbewertungen: tension_deepen pro Methode.
- §7 Roadmap: turning_point + resolution_construct.
- §8 Risiken: support.
- §9 Fazit: resolution_ratify mit explizitem Rückbezug auf §1+§2.
EOF
ctox report storyline-set --run-id RUN_ID --markdown-file /tmp/storyline.md
ctox report storyline-show --run-id RUN_ID
```

The storyline is free-form prose — there is no schema for it. But the
text MUST cover: central tension(s), naïve answer + why it fails, the
turning-point finding (named source), the resolution as architecture,
and a per-block arc-position hint. Minimum 400 characters.

When you `block-stage` later, pass `--arc-position <pos>` so the role
in the bow is recorded. Allowed values: `tension_open` |
`tension_deepen` | `complication` | `turning_point` |
`resolution_construct` | `resolution_ratify` | `support`.

### 3b. Figures — generate or extract, then cite

Use `ctox report figure-add` to register every figure. The renderer
auto-numbers them in document order and resolves the
`{{fig:<figure_id>}}` token in your block markdown to "Abbildung N".

```bash
# Schematic via mermaid (preferred for layered-stack diagrams):
cat > /tmp/stack.mmd <<'EOF'
flowchart TD
  A[Lack/Primer/Surfacer] --> B[Blitzschutz-Kupfergitter (LSP)]
  B --> C[CFK-Lagen]
  C --> D[Innenstruktur]
EOF
ctox report figure-add --run-id RUN_ID \
    --kind schematic --code-mermaid /tmp/stack.mmd \
    --caption "Schematischer Schichtaufbau einer CFK-Tragfläche mit LSP" \
    --source "eigene Darstellung" \
    --instance-id "doc_study__component_layout"
# → prints figure_id: fig_<hex>; cite from a block via {{fig:fig_<hex>}}.

# Schematic via Python/matplotlib:
cat > /tmp/zones.py <<'EOF'
import matplotlib.patches as patches
fig, ax = plt.subplots(figsize=(6, 3))
for i, (label, depth) in enumerate(
    [("Lack", 0.05), ("Surfacer", 0.10), ("LSP-Mesh", 0.20), ("CFK", 1.0)]
):
    ax.add_patch(patches.Rectangle((0, i*0.25), 5, 0.20, label=label))
ax.set_xlim(0, 5); ax.set_ylim(0, 1.5); ax.legend(); ax.axis("off")
EOF
ctox report figure-add --run-id RUN_ID --kind schematic --code-python /tmp/zones.py \
    --caption "Eindringtiefe vs. Verfahren (Schemaillustration)" \
    --source "eigene Darstellung" \
    --instance-id "doc_study__screening_logic"

# Extract a page image from a stored open-access PDF:
ctox report figure-add --run-id RUN_ID --kind extracted \
    --extract-from-evidence ev_abc123 --page 4 \
    --caption "FE-Modell des LSP-Stack (nach Hu/Yu 2019, Abb. 4)" \
    --source "Hu et al. 2019, doi:..."
```

Then in a block markdown body cite as: `Wie in {{fig:fig_abc123}}
gezeigt, ...`. The renderer resolves it to `Abbildung 1` etc.

### 3c. Tables — structured, not Markdown-pseudo

Use `ctox report table-add` for every numeric/comparison table
(Bewertungsmatrix, Szenario-Matrix, Defektkatalog, Risikoregister,
Abkürzungsverzeichnis). The DOCX renderer emits a native Word table
with bold header row; the markdown renderer emits a GFM pipe table
with caption + legend. Cite via `{{tbl:<table_id>}}`.

```bash
cat > /tmp/matrix.csv <<'EOF'
Verfahren,Fläche,Gitterbild,Defekt,Delamination,Reifegrad
Hyperspektral,hoch,niedrig,niedrig,mittel,hoch
THz,mittel,hoch*,mittel,mittel,mittel
ECT/Arrays,mittel,sehr hoch,sehr hoch,mittel,hoch
Induktions-Thermografie,hoch,hoch,hoch,hoch,hoch
EOF
ctox report table-add --run-id RUN_ID \
    --kind matrix \
    --caption "Bewertungsmatrix: kontaktlose NDT-Verfahren (qualitativ)" \
    --legend "Legende: Fläche = Single-Shot-Potenzial; * = nur wenn LSP erste Metallschicht." \
    --csv-file /tmp/matrix.csv \
    --instance-id "doc_study__screening_matrix"
# → prints table_id: tbl_<hex>; cite as {{tbl:tbl_<hex>}}.
```

### 4. Draft block markdown — MUST load abstracts first

**Hard rule: before drafting any block, load the source content for
every evidence_id you intend to cite into your working context.** The
resolver path (`add-evidence --doi`) writes the metadata + abstract +
(when the source is open-access) the full PDF text to SQLite, but
nothing is in your conversation context across turns. If you write
block prose without re-loading the content, you will hallucinate from
priors and the prose will not actually use the sources.

For each evidence_id you plan to cite:

```bash
# Default — abstract + metadata. Always returns whether full_text
# was attached at registration time and how many chars it carries.
ctox report evidence-show --run-id RUN_ID --evidence-id ev_abc123

# Full body (for open-access PDFs / HTML the resolver fetched at
# registration time). Use this whenever full_text was attached, so
# the prose can cite specific results, parameters, methods — not
# just the abstract paraphrase.
ctox report evidence-show --run-id RUN_ID --evidence-id ev_abc123 --full-text

# Whole run at once (JSON, with optional --full-text):
ctox report evidence-show --run-id RUN_ID --all --json > /tmp/evidences.json
ctox report evidence-show --run-id RUN_ID --all --full-text --json > /tmp/evidences_full.json
```

The output carries `title`, `authors`, `year`, `venue`, `url`,
`abstract_md`, and (when attached) `full_text_md` with the OA paper
body in markdown form (PDF text-extracted by ctox-pdf-parse, or HTML
stripped to plain text). Read the relevant content before you write
any sentence that cites that evidence_id.

- If `full_text:   attached` is shown, prefer the full body — the
  abstract alone almost never carries enough specific detail
  (numerical results, experimental conditions, method-step parameters)
  for a feasibility-grade claim. Read it via `--full-text`.
- If only an abstract is present (`full_text: (not attached)`), the
  source still contributes a citation but specific factual claims
  (numbers, named methods, experimental protocols) must be backed by
  a source whose abstract or full text actually states them.
- If `abstract: (none)` AND `full_text: (not attached)`, the source
  has no usable content — drop the citation, pick a different one, or
  fetch the page with `ctox web read` and re-register via
  `add-evidence --abstract-file`.

For each block in the run's `report_type.block_library_keys[]` (read from
`references/asset_pack.json`), draft markdown that:

- Follows the block's `scaffold` and `title` from the asset pack.
- For each cited `evidence_id`: paraphrase or quote at least one
  **specific** finding from that source's abstract — a method name, a
  numeric result, a condition, an experimental setup. Generic-sounding
  prose with citation tags bolted on is the failure mode this skill
  exists to prevent. If you cannot pull a specific finding from the
  abstract, the citation is wrong.
- Cites `evidence_id`s inline as `[ev_abc123]` (single brackets, no
  `{}`) directly after the sentence whose claim came from that source.
  Never write bare `ev...` IDs in prose or tables; bracketed IDs are rendered
  as numbered references, bare IDs are a client-facing defect.
- Stays inside the block's `target_chars` from the depth profile.
  Source-grounded prose is naturally longer than generic prose; if your
  draft is far below `target_chars`, you are summarising priors instead
  of integrating sources — go back and re-read abstracts.
- For matrix blocks: each cell carries a short rationale + at least one
  `evidence_id`. **Never repeat the same rationale string across cells of one
  option** — that's the `release_guard_check` slop signature.
- For the verdict block: use the `verdict_line_pattern` from the report type
  verbatim (e.g. for feasibility: "Erfolgsaussichten (qualitativ): high|medium|low").
- **Topic-match check:** the cited source's title/abstract must
  actually be about the topic of the surrounding paragraph. A
  cold-spray fabrication paper does not belong in an active-thermography
  block. Re-read the abstract; if topic mismatches, drop the citation.

Save each draft to a temp file. The markdown body must not contain a duplicate
heading line; the block title is already passed via `--title`.

```bash
cat > /tmp/block_executive_summary.md <<'EOF'
… your prose, with [ev_abc123] citations …

EOF
```

### 5. Stage and commit blocks

```bash
ctox report block-stage --run-id RUN_ID \
    --instance-id "feasibility_main__exec_summary" \
    --markdown-file /tmp/block_executive_summary.md \
    --title "Executive Summary" \
    --ord 0 \
    --reason "first pass" \
    --used-reference-ids "ev_abc123,ev_def456" \
    --arc-position resolution_ratify
```

The `--arc-position` flag locks in the block's role in the storyline
arc (see step 3a). Allowed values: `tension_open` | `tension_deepen` |
`complication` | `turning_point` | `resolution_construct` |
`resolution_ratify` | `support`. The renderer + check engine use this
later to verify the bow closes.

The `instance_id` shape is `<doc_id>__<block_id>`, where the `block_id` comes
from the report type's `block_library_keys[]` and `doc_id` is the run's
primary document (typically the report_type's id with `_main` suffix; check
`document_blueprint.sequence[]` for the canonical doc_ids).

After staging the current block, inspect pending state. Apply immediately
unless you are intentionally batching at most three related blocks:

```bash
ctox report block-list --run-id RUN_ID --pending --json
ctox report block-apply --run-id RUN_ID
ctox report status RUN_ID --json
```

To inspect:

```bash
ctox report block-list --run-id RUN_ID --pending --json
ctox report block-list --run-id RUN_ID --json
```

If `status` still shows zero completed blocks after you believe you drafted
content, stop writing prose. The transition failed; fix `block-stage` /
`block-apply` usage before doing more research or rendering.

### 6. Run the five deterministic checks

After every block-apply, run each check. All five must report
`ready_to_finish=true` (or `check_applicable=false`) before you can
finalise.

```bash
ctox report check --run-id RUN_ID completeness --json
ctox report check --run-id RUN_ID character_budget --json
ctox report check --run-id RUN_ID release_guard --json
ctox report check --run-id RUN_ID narrative_flow --json
ctox report check --run-id RUN_ID deliverable_quality --json
```

Each prints a `CheckOutcome`:

- `ready_to_finish=true` — pass for that gate.
- `needs_revision=true` — concrete failures, with `candidate_instance_ids`
  and `goals` you should act on.
- `check_applicable=false` — the gate doesn't apply to this run (e.g.
  `narrative_flow` for a 1-block draft).

When a check flags failures, revise the implicated block: re-draft the
markdown, re-`block-stage` (which replaces the prior pending), `block-apply`,
re-run the failed check.

Treat a failed check as a transition to the revision state, not as the end of
the run. In particular:

- `LINT-FAB-DOI`: for every DOI in the goals, first try
  `ctox report add-evidence --run-id RUN_ID --doi "<doi>"`. If it resolves,
  re-run `release_guard`. If it does not resolve, remove the DOI string from
  the implicated block or replace it with the already-registered URL/source
  citation and re-stage that block.
- `LINT-CITED-BUT-MISSING` / `LINT-FAB-AUTHOR`: rewrite the sentence so it
  cites only evidence ids present in `ctox report evidence-show --all`.
- `LINT-MIN-CHARS` / `LINT-MAX-CHARS` / duplicate openings: revise form only;
  do not add facts unless an evidence-integrity lint also requires it.

Do not call final DOCX `render` or `finalise` while any of the five checks has
`ready_to_finish=false`. If you need a visual draft during revision, use
`ctox report render RUN_ID --format docx --allow-draft --out /tmp/draft.docx`
and keep it out of the final output path. If a render/finalise call fails,
inspect the failed gate, revise, and continue; do not mark the queue task
failed until the same blocking lint has failed three revision attempts.

### 7. Render and finalise

Once all five checks are ready:

```bash
ctox report render RUN_ID --format md --out /tmp/feasibility_study.md
ctox report render RUN_ID --format docx --out /tmp/feasibility_study.docx
python3 skills/system/research/deep-research/scripts/render_check.py \
    --docx /tmp/feasibility_study.docx \
    --out-dir /tmp/feasibility_study_render_check
ctox report finalise RUN_ID
```

The DOCX render automatically applies the bundled layout-polish pass
(`scripts/polish_docx_layout.py`) after the semantic manuscript render. Do
not replace it with an ad-hoc DOCX builder. Show the operator the markdown
render, the polished DOCX path, and the visual-check output path. The run is
now sealed.

## Asking the operator for input

If you genuinely cannot resolve something autonomously (operator asked for
an option you can't evaluate without a constraint they didn't give, conflict
between two equally valid interpretations of the topic, etc.):

```bash
ctox report ask-user --run-id RUN_ID \
    --question "What is the target inspection frequency? Per-flight, per-month, per-quarter?" \
    --question "Which CFRP layup family is in scope — UD, woven, hybrid?"
```

Then surface those questions to the operator in your chat reply and stop.
Use this sparingly — the operator gave you a topic, you have web access,
make autonomous calls where reasonable.

## Hard rules — never break these

1. **Source-grounded sentences only.** Inline `[evidence_id]` references
   after the sentence. Before you write any sentence carrying a
   citation, you MUST have just read the abstract of that evidence_id
   via `ctox report evidence-show`. The sentence has to paraphrase or
   directly reference a specific element of that abstract (method, key
   parameter, numeric result, conclusion). Generic textbook claims
   that the abstract does not actually support, with citation tags
   bolted on, are the failure mode this skill is built to prevent. No
   fabricated DOIs, no fabricated authors, no fabricated study titles.
   If the abstract does not support what you want to say, either find
   a different source or drop the claim.
2. **No duplicate rationale strings inside one option's matrix row.** The
   `release_guard_check` lint catches it; that's the canonical slop
   signature.
3. **Verdict line uses the report type's `verdict_line_pattern` verbatim.**
   For `feasibility_study`: "Erfolgsaussichten (qualitativ): …". For
   `whitepaper` and `literature_review`: no verdict line at all.
4. **Cross-type block usage is forbidden.** Every `instance_id` must resolve
   to a `block_id` in the run's `report_type.block_library_keys[]`. The
   `block-apply` command will reject mismatches.
5. **Form-revision keeps facts unchanged.** When a check flags
   `needs_revision` and `goals` mention only length/order/clarity, do not
   introduce new claims in the rewrite.
6. **All five gates green before `finalise`.** `ctox report finalise` will
   refuse otherwise.

## When to abort instead of revising

- Same `release_guard_check` lint fails three revisions in a row → stop,
  tell the operator the lint code, ask for a structural change.
- Evidence floor unreachable after 3 web searches with different query angles
  → stop, raise `ask-user` with the unsatisfied evidence axis.
- Same `narrative_flow_check` violation on the same `instance_id` three
  revisions in a row → stop, the block has a structural problem; redesign
  it from scratch or ask the operator.
- Topic ambiguity that no amount of web research can resolve → raise
  `ask-user` immediately, do not spend evidence quota guessing.

## What this skill explicitly does not do

- **Does not invent photographs or fake source figures.** Figures must come
  from cited sources with usage rights, or be technical schematics/charts
  created from the report's own evidence and labelled as "own depiction".
- **Does not skip the evidence register.** The `release_guard_check` will
  flag claims with no `evidence_id`.
- **Does not finalise without all five checks green.** `ctox report
  finalise` enforces this.
