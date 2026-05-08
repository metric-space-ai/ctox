---
name: deep-research
description: Produces decision-grade reports across seven types — Machbarkeitsstudien (feasibility), Marktanalyse (market research), Wettbewerbsanalyse, Technologie-Screening, Whitepaper, Stand-der-Technik (literature review), Entscheidungsvorlage (decision brief). The harness LLM drives the run by calling deterministic `ctox report …` CLI subcommands.
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
- "Marktstudie / Marktrecherche zu …" / "market study / market research on …"
- "Wettbewerbsanalyse mit Bewertungsmatrix …" / "competitive analysis with scoring matrix …"
- "Entscheidungsvorlage mit Optionsbewertung …" / "decision memo with option evaluation …"
- "Stand-der-Technik-Übersicht für …" / "state of the art review for …"

Skip this skill for short answers, summaries of a single paper, code
explanations, or anything that does not need a multi-section cited report.

## Report types

Every run is bound to exactly one `report_type_id`. Pick from these seven:

| `report_type_id`         | When                                                                    | Typical chars | Min sections |
| ------------------------ | ----------------------------------------------------------------------- | ------------- | ------------ |
| `feasibility_study`      | "Geht das Verfahren X für Anwendung Y?" — option matrix + verdict       | ~30 000       | 9            |
| `market_research`        | "Wie groß ist Markt X, wer kauft, was zahlen sie?"                      | ~25 000       | 7            |
| `competitive_analysis`   | "Wer sind die Anbieter, wie schneiden sie ab?"                          | ~20 000       | 8            |
| `technology_screening`   | "Welche Methoden gibt es überhaupt, welche schaffen es in die Shortlist?" | ~22 000     | 8            |
| `whitepaper`             | "Hier ist die These und die Argumente"                                  | ~18 000       | 5            |
| `literature_review`      | "Was ist der Stand der Forschung zu Thema X?"                           | ~28 000       | 6            |
| `decision_brief`         | "Eine-Seite-Vorlage mit Optionen und Empfehlung"                        | ~8 000        | 5            |

Cross-type invariants for every report:

- Length aligned to the type's `typical_chars` (±15% tolerance).
- Every non-trivial claim cites at least one entry from the run's evidence
  register by `evidence_id`.
- For types with a matrix block (feasibility, competitive, technology screening,
  decision brief): every cell carries a short rationale plus an `evidence_id`.
  The same rationale string MUST NOT appear in two cells of the same option.
- Verdict line per option uses the type's `verdict_line_pattern` from the
  asset pack (e.g. for feasibility: `Erfolgsaussichten (qualitativ): …`).

## How to run a report — the actual commands

Read the run state, do research, draft markdown, stage it, apply, run the
checks, render, finalise. All via Bash.

### 1. Create the run

```bash
ctox report new feasibility_study \
    --domain ndt_aerospace \
    --depth standard \
    --language de \
    --topic "Kontaktlose Prüfung des Blitzschutz-Kupfergitters in CFK-Strukturen"
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
- `references/rascon_archetype.md` — worked example of a feasibility study
  that passes all checks.

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

# Scholarly search across Crossref + OpenAlex + arXiv + Anna's Archive
ctox web scholarly search --query "<query>" --max-results 12

# Fetch a specific URL and return markdown-extracted content
ctox web read --url "<url>"
```

Each `add-evidence` prints the new `evidence_id` (`ev_<hex>`) plus the
abstract/snippet length actually stored. Keep a notes file mapping
`evidence_id → topic` so you can later cite the right ones.

**Verify before drafting:** after registering N evidences, run
`ctox report status RUN_ID --json` and check that
`evidence_register_size >= depth_profile.min_evidence_count`. If the
register is full of titles but no abstracts, the prose you'll write will
be hallucinated — the lint engine catches that, but it's faster to fix
the evidence step first.

### 4. Draft block markdown — MUST load abstracts first

**Hard rule: before drafting any block, load the abstracts of every
evidence_id you intend to cite into your working context.** The
resolver path (`add-evidence --doi`) puts the abstract into the SQLite
DB but it does NOT stay in your conversation context across turns. If
you write block prose without re-loading the abstracts, you will
hallucinate from priors and the prose will not actually use the
sources.

```bash
# Per evidence id:
ctox report evidence-show --run-id RUN_ID --evidence-id ev_abc123

# Or for the whole run at once (preferred for multi-block drafting):
ctox report evidence-show --run-id RUN_ID --all --json > /tmp/evidences.json
```

The output gives you `title`, `authors`, `year`, `venue`, `url`, plus
the **full `abstract_md`** in the source's own words. Read it before you
write a single sentence that cites that evidence_id. If the abstract is
empty (`abstract: (none)`), that source contributes only a citation —
do not put specific factual claims behind it; either re-fetch the page
with `ctox web read` and re-register, or pick a different source.

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

Save each draft to a temp file:

```bash
cat > /tmp/block_executive_summary.md <<'EOF'
# Executive Summary

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
    --used-reference-ids "ev_abc123,ev_def456"
```

The `instance_id` shape is `<doc_id>__<block_id>`, where the `block_id` comes
from the report type's `block_library_keys[]` and `doc_id` is the run's
primary document (typically the report_type's id with `_main` suffix; check
`document_blueprint.sequence[]` for the canonical doc_ids).

When you have staged a few blocks, commit them:

```bash
ctox report block-apply --run-id RUN_ID
```

To inspect:

```bash
ctox report block-list --run-id RUN_ID --pending --json
ctox report block-list --run-id RUN_ID --json
```

### 6. Run the four deterministic checks

After every block-apply, run each check. All four must report
`ready_to_finish=true` (or `check_applicable=false`) before you can
finalise.

```bash
ctox report check --run-id RUN_ID completeness --json
ctox report check --run-id RUN_ID character_budget --json
ctox report check --run-id RUN_ID release_guard --json
ctox report check --run-id RUN_ID narrative_flow --json
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

### 7. Render and finalise

Once all four checks are ready:

```bash
ctox report render RUN_ID --format md --out /tmp/feasibility_study.md
ctox report render RUN_ID --format docx --out /tmp/feasibility_study.docx
ctox report finalise RUN_ID
```

Show the operator the markdown render and the docx path. The run is now
sealed.

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
6. **All four gates green before `finalise`.** `ctox report finalise` will
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

- **Does not invent figures or photographs.** Figures must come from cited
  sources with usage rights, or be marked `to be drafted by operator` with
  a one-sentence description.
- **Does not skip the evidence register.** The `release_guard_check` will
  flag claims with no `evidence_id`.
- **Does not finalise without all four checks green.** `ctox report
  finalise` enforces this.
