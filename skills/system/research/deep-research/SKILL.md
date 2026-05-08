---
name: deep-research
description: Produces decision-grade reports across seven types — Machbarkeitsstudien (feasibility), Marktanalyse (market research), Wettbewerbsanalyse, Technologie-Screening, Whitepaper, Stand-der-Technik (literature review), Entscheidungsvorlage (decision brief) — via a manager + three sub-skills + four gates pipeline.
class: system
state: active
cluster: research
---

# Deep Research

The deep-research skill is the architectural twin of the Förderantrag agent
(`Foerdervorhaben-Agent.html`, `runAgentManager`). The manager never writes
prose itself — it orchestrates an evidence-first pipeline of one workspace
introspection tool, one asset tool, one user-rescue tool, one mandatory public
research tool, two writer sub-skills, one patch-commit tool, and four
loop-end gates. The host (Rust side) overrides any `decision: "finished"`
the LLM emits if any of the four gates did not return
`ready_to_finish=true` (or `check_applicable=false`).

## Use this skill when

Trigger this skill whenever the operator asks for an evidence-grade,
decision-supporting written deliverable that requires multi-source synthesis.

Concrete trigger phrases (German + English):

- "Mach mir eine Machbarkeitsstudie zu …"
- "feasibility study on …"
- "berührungslose Prüfverfahren für …" / "contactless inspection method assessment for …"
- "Materialprüfungs-Methodenvergleich …" / "materials testing method comparison …"
- "Technologie-Screening …" / "technology screening for …"
- "Marktstudie / Marktrecherche zu …" / "market study / market research on …"
- "Wettbewerbsanalyse mit Bewertungsmatrix …" / "competitive analysis with scoring matrix …"
- "Entscheidungsvorlage mit Optionsbewertung …" / "decision memo with option evaluation …"
- "Stand-der-Technik-Übersicht für …" / "state of the art review for …"

Skip this skill for short answers, summaries of a single paper, code
explanations, or anything that does not need a multi-section cited report.

### Trigger phrases per report type

Every run is bound to exactly one `report_type_id` chosen from the seven
types below. The manager refuses to start a run without one. Operator
phrasing maps to type as follows:

- **`feasibility_study`** — Machbarkeitsstudien: "Mach mir eine
  Machbarkeitsstudie zu …", "feasibility study on …", "is method X
  feasible for Y?", "berührungslose Prüfverfahren für …" /
  "contactless inspection method assessment for …",
  "Materialprüfungs-Methodenvergleich für …" / "materials testing
  method comparison for …".
- **`market_research`** — Marktanalyse: "Marktstudie zu …" /
  "market research on …", "Marktgrößen- und Segmentanalyse für …" /
  "market sizing and segment analysis for …", "Nachfrageabschätzung
  für …" / "demand assessment for …", "Markteintrittsanalyse für …" /
  "market entry analysis for …".
- **`competitive_analysis`** — Wettbewerbsanalyse: "Wettbewerbsanalyse
  mit Bewertungsmatrix …" / "competitive analysis with scoring matrix
  …", "Anbieterlandkarte zu …" / "competitor map for …",
  "Positionierungsvergleich von …" / "positioning comparison of …",
  "Capability-Gap-Analyse zu …" / "capability gap analysis for …".
- **`technology_screening`** — Technologie-Screening: "Technologie-
  Screening …" / "technology screening for …", "Methoden-
  Vorauswahl …" / "method preselection …", "Optionsraum-
  Shortlist für …" / "option shortlisting for …",
  "Verfahrens-Longlist mit Kriterienscreening …".
- **`whitepaper`** — Konzept-/Thesen-Memo: "Whitepaper zu …" /
  "whitepaper on …", "Thesenpapier zu …" / "thesis paper on …",
  "Positionspapier zum Thema …" / "position paper on the topic of …",
  "Konzeptexposition zu …" / "concept exposition for …".
- **`literature_review`** — Stand-der-Technik / Übersichtsarbeit:
  "Stand-der-Technik-Übersicht für …" / "state of the art review
  for …", "Literaturüberblick zu …" / "literature review on …",
  "Themenorientierte Forschungslandschaft zu …" /
  "research-landscape synthesis on …",
  "themengruppierte Synthese zu …" / "theme-organised synthesis on …".
- **`decision_brief`** — Entscheidungsvorlage: "Entscheidungsvorlage
  mit Optionsbewertung …" / "decision memo with option evaluation
  …", "kompakte Entscheidungsvorlage zu …" / "compact recommendation
  document for the decision …", "Vorstandsvorlage zu …" /
  "executive decision brief on …".

## What this skill produces

A multi-section, decision-grade report delivered in two formats:

- **DOCX** — rendered with python-docx via the bundled renderer (the
  manuscript JSON is the structured intermediate; the renderer is
  deterministic).
- **Markdown** — fast preview, identical content, identical block ordering.

The deliverable shape varies per `report_type_id`. The manager reads the
`typical_chars`, `min_sections`, `block_library_keys[]`,
`document_blueprint_id`, `verdict_line_pattern` and `default_modules`
fields from the asset pack's `report_types[id == report_type_id]` entry
at first `asset_lookup` and conditions the writer sub-skill on them.

| `report_type_id`         | Typical chars | Min sections | DOCX + Markdown deliverable shape                                                                                          |
| ------------------------ | ------------- | ------------ | -------------------------------------------------------------------------------------------------------------------------- |
| `feasibility_study`      | ~30 000       | 9            | Executive summary → scope/method → evidence register → option × criterion matrix → scenario branches → risk register → recommendation → references → appendices. Verdict line per option. |
| `market_research`        | ~25 000       | 7            | Executive summary → market definition → segment breakdown → demand drivers → competitive landscape → entry options → recommendation → references. Sized estimates with evidence_ids. |
| `competitive_analysis`   | ~20 000       | 8            | Executive summary → scope → competitor profiles → capability matrix (vendor × capability) → positioning map → gap analysis → recommendation → references. Verdict line per competitor. |
| `technology_screening`   | ~22 000       | 8            | Executive summary → screening question → option longlist → screening criteria → option × criterion matrix → shortlist → next-step recommendation → references. Verdict line per shortlisted option. |
| `whitepaper`             | ~18 000       | 5            | Thesis → context → argumentation chapters → implications → references. No verdict line; `verdict_line_pattern: null`. |
| `literature_review`      | ~28 000       | 6            | Scope → method → theme-organised synthesis chapters → cross-theme integration → open research questions → references. No verdict line; `verdict_line_pattern: null`. |
| `decision_brief`         | ~8 000        | 5            | One-page summary → decision question → option grid (option × criterion) → recommendation with rationale → references. Compact verdict line per option. |

Cross-type invariants that hold for every report:

- Length aligned to the type's `typical_chars` and capped/floored by
  `character_budget_check` tolerance.
- Evidence register: every non-trivial claim cites at least one entry from
  `public_research.sources[]` by `evidence_id`. Crossref-resolved DOIs are
  required for any cited DOI.
- Where the type has a matrix block (feasibility_study,
  competitive_analysis, technology_screening, decision_brief): every cell
  carries a short rationale string and a reference to at least one
  evidence_id. The same rationale string MUST NOT appear in two cells of
  the same option (slop pattern caught by `release_guard_check`).
- Manuscript JSON: the raw structured form retained for debugging,
  re-rendering, and downstream review.

## Architecture in two sentences

The manager (one LLM agent, `toolUseBehavior: "run_llm_again"`) drives an
11-tool inventory plus three sub-skills (Block Writer, Revision, Flow
Review) in bounded packets of at most six instance_ids per call, never
writing prose itself and never copying markdown into tool arguments. The
loop ends only when the four gate tools (`completeness_check`,
`character_budget_check`, `release_guard_check`, `narrative_flow_check`)
all return `ready_to_finish=true` or `check_applicable=false`; the host
overrides `decision: "finished"` to `blocked` otherwise.

## Hard rules — never break these

These eleven rules are binding. Rules 1–8 encode concrete failure modes
that either appeared in the dead skill's output or are structurally
guaranteed by the contract. Rules 9–11 encode the report-type binding
that makes the skill general across all seven supported types.

1. **No markdown in tool arguments.** The manager passes only `skill_id`
   and `instance_ids` (and `goals` for revision). Justification: prose
   in tool args bypasses the schema validator on the sub-skill output and
   reintroduces fabrication and slop into the patch stream.
2. **Block writer max 6 instance_ids per call.** Schema-enforced via
   `z.array(...).min(1).max(6)`. Justification: larger packets exceed the
   sub-skill's reasoning budget and produce drafts that share boilerplate
   across blocks.
3. **No `decision: "finished"` while any of the four checks returns
   `ready_to_finish=false`.** The host overrides this; the manager must
   not rely on the override and must call the checks itself before
   declaring done. Justification: silent override hides the real blocker.
4. **No fabricated DOIs, no fabricated authors, no fabricated study
   titles.** Every cited source must come from a `public_research` result
   with a Crossref-resolved record (DOIs only) or a verifiable URL.
   Justification: the dead skill's defining failure was inventing
   plausible-sounding citations.
5. **Public research is a mandatory pre-phase before `write_with_skill`.**
   Unlike the Förderantrag agent (where research is optional and capped at
   3 calls), deep-research requires at least
   `depth_profile.min_evidence_count` excerpts in the workspace before any
   block-writer call. Multiple `public_research` passes per run are
   allowed and expected. Justification: the deliverable is evidence-grade;
   without evidence the writer must hedge or invent.
6. **Form-revision (`form_only=true`) must not introduce new facts.**
   Only length, ordering, sentence rhythm, transitions, and clarity may
   change. Justification: form-revision runs receive no evidence packet
   and cannot validate new claims.
7. **No duplicate rationale strings across matrix cells of the same
   option.** Caught by `release_guard_check`; the same option-level
   rationale repeated across multiple criteria is the slop signature that
   killed the previous skill. Justification: it is the canonical visible
   tell of an under-evidenced matrix.
8. **When evidence cannot be obtained for a required claim, raise
   `blocking_questions` instead of paraphrasing or hedging.** The
   sub-skill output schema includes `blocking_questions[max 3]`; the
   manager surfaces them via `ask_user` and ends the run with
   `needs_user_input`. Justification: hedged prose with no source is the
   exact pattern the deterministic gates cannot catch but the operator
   will reject.
9. **Every run must declare a `report_type_id` at start.** The manager
   reads it from `package_context.report_type_id` (workspace_snapshot
   surfaces it under that name and as the resolved `report_type` object).
   The manager refuses to begin a run without one — there is no implicit
   default. Justification: the asset pack's block library, verdict
   vocabulary, document blueprint, and default modules are all indexed
   by `report_type_id`; an untyped run cannot be wired correctly.
10. **The manager loads `report_types[id == report_type_id]` from the
    asset pack at the first `asset_lookup` call and treats its
    `block_library_keys[]` as the only legal block-id pool for the
    run.** Cross-type block usage is forbidden — no pulling a
    `market_research` segment block into a `feasibility_study` run, and
    so on. The asset_lookup tool itself scopes its `block_defs[]` and
    `references[]` payload to the run's report type so the wrong block
    can't enter the manager's view in the first place. Justification:
    each type's blocks share rubric assumptions (matrix presence,
    verdict shape, register tone) that make cross-mixing produce
    incoherent reports.
11. **The verdict-line pattern is report-type-specific.** Use the
    `verdict_line_pattern` from the resolved `report_type` definition
    verbatim — for `feasibility_study` it is "Erfolgsaussichten
    (qualitativ): …", for `competitive_analysis` it is a competitor-
    level rating line, for `decision_brief` it is a compact
    recommendation marker, etc. If a report type has
    `verdict_line_pattern: null` (currently `whitepaper` and
    `literature_review`), no verdict line is produced and
    `release_guard_check` does not enforce LINT-VERDICT-MISMATCH for
    that run. Justification: a hard-coded "Erfolgsaussichten…" line in
    a literature review is the giveaway that the manager treated every
    run as a feasibility study; the type-specific pattern prevents
    that.

## Workflow

Manager loads `package_context.report_type_id`. All subsequent tool
calls scope to that type — `asset_lookup` returns only the type's
block-library subset, `write_with_skill` accepts only `instance_ids`
from that subset, `release_guard_check` activates only the lint subset
applicable to that type, and `narrative_flow_check` verifies the type's
arc from `style_guide.report_type_arc[]`.

One full run, narrated end-to-end:

1. **Operator submits topic.** Optional fields: target language, domain
   hint, depth profile (`orienting` | `standard` | `decision_grade`),
   reference documents to include verbatim into the evidence packet,
   target character count override.
2. **Bootstrap.** Manager calls `workspace_snapshot` to read package
   config, expected blocks, character budget, open questions, and any
   review feedback. Then `asset_lookup` (default `{}`) to receive block
   definitions, rubrics, and the asset-pack reference list.
3. **Evidence pre-phase.** Manager calls `public_research` one or more
   times (depth-profile-bounded) until the workspace contains at least
   `depth_profile.min_evidence_count` excerpts covering the topic axes
   declared in the asset pack. Crossref-first DOI resolution is enforced
   tool-side. Each result is appended to provenance with a fresh
   `research_id` and a `sources[]` array.
4. **Targeted asset re-lookup.** Once evidence is in place, manager
   re-calls `asset_lookup` (now scoped to the upcoming block packet) to
   bind reference_ids to the resolved evidence.
5. **Drafting.** Manager calls `write_with_skill` for the first batch of
   up to 6 instance_ids, ordered by `(doc_id, order)` (no cross-document
   cherry-picking). The brief carries: target depth, language,
   must-cover questions, attached evidence_ids, target_chars per block.
   On return, manager calls `apply_block_patch` with only the returned
   `skill_id` (and optional `instance_ids` and `used_research_ids`).
6. **Iteration.** Manager calls `completeness_check`. If
   `ready_to_finish=false`, return to step 5 with the next packet. Loop
   until all required blocks are committed at min_chars threshold.
7. **Finalisation gates.** Manager runs, in order,
   `character_budget_check`, `release_guard_check`, `narrative_flow_check`.
   If any returns `needs_revision=true`, manager calls
   `revise_with_skill` with the returned `candidate_instance_ids` and
   `goals`, applies the patch, and re-runs the failed gate (and any later
   gates that depend on it). Repeat until all four gates report
   `ready_to_finish=true` or `check_applicable=false`.
8. **Done.** Manager emits `decision: "finished"`. The host loop-end
   gate verifies the four checks and either accepts or downgrades to
   `blocked`.

## When to abort instead of revising

These four patterns mean the run cannot complete autonomously and must
be returned to the operator with a clear failure reason. Do not loop.

- **Same `release_guard_check` lint fails three rounds in a row.** Stop,
  emit `decision: "blocked"`, name the lint code in `summary`, escalate.
- **`public_research` fails to deliver the minimum evidence count after
  the depth profile's research budget.** Stop, emit
  `decision: "blocked"`, name the unsatisfied evidence axis. Do not fall
  back to writing without evidence.
- **`narrative_flow_check` keeps marking the same instance_id three
  rounds in a row.** Stop, emit `decision: "blocked"`, name the
  instance_id. The block likely has a structural problem the form-only
  reviser cannot fix.
- **Manager attempts to write a block whose id is not in the run's
  `report_type.block_library_keys[]`.** Stop, emit
  `decision: "blocked"`, set `summary` to "cross-type block usage" and
  name the offending instance_id and the run's `report_type_id`. The
  asset_lookup contract should have prevented this; if it occurs,
  treat it as a manager bug, not as something to revise around. Do
  not silently substitute another block.

## Output expectations

- **Markdown** — preview format. Fast to render, identical content.
- **DOCX** — rendered with python-docx via the bundled renderer.
  Deterministic given the manuscript JSON. Headings, tables, evidence
  list, and recommendation matrix have explicit styles in the renderer.
- **Manuscript JSON** — the structured intermediate. Retained for
  debugging, re-rendering with a different style pack, and downstream
  review tools. Not an end-user artifact.

## What this skill explicitly does not do

- **Does not invent figures or photographs.** Figures must come from
  explicitly cited sources with usage rights, or be marked
  `to be drafted by operator` with a sentence describing what the figure
  should show. The manuscript JSON carries the placeholder; the renderer
  draws an empty captioned frame.
- **Does not write reports without a real research pass.** The
  `min_evidence_count` floor is binding tool-side; `write_with_skill`
  refuses to start without evidence in the packet.
- **Does not run the LLM directly from the manager prompt.** All prose
  comes from one of the three sub-skills (Block Writer, Revision, Flow
  Review) with schema-validated output. The manager's only LLM-facing
  outputs are tool calls and the final decision envelope.
