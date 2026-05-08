# Deep Research — Tool Schema Contracts

Schema contracts for every tool in the deep-research manager inventory,
plus the host loop-end gate. This document is read by integrators wiring
the tools and by manager runtime debuggers checking why a call failed.

All tools return a JSON envelope of the form:

```json
{ "ok": true, "data": <payload> }
```

or

```json
{ "ok": false, "error": { "code": "<machine_code>", "message": "<human_text>", "context": <optional_object> } }
```

Schemas below describe `<payload>` (the success body) and the input
parameters object.

## workspace_snapshot output schema

Input: `{}` (no parameters).

Output payload fields:

| Field                          | Type                            | Meaning                                                                                                          |
| ------------------------------ | ------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `topic`                        | `string`                        | The operator-supplied research topic.                                                                            |
| `language`                     | `string`                        | BCP-47 tag for the target language (e.g. `de-DE`, `en-US`).                                                      |
| `depth_profile`                | `"orienting" \| "standard" \| "decision_grade"` | Resolved depth profile. Drives `min_evidence_count` and target length.                                |
| `report_type_id`               | `"feasibility_study" \| "market_research" \| "competitive_analysis" \| "technology_screening" \| "whitepaper" \| "literature_review" \| "decision_brief"` | The run-bound report type. Set at run creation; never changes mid-run. The manager refuses to start without one. |
| `report_type`                  | `object`                        | Resolved snapshot of the active report type so the manager doesn't keep re-querying. Shape: `{ id, label, verdict_line_pattern: string \| null, verdict_vocabulary[]: string[], typical_chars: number, min_sections: number, block_library_keys[]: string[], document_blueprint_id: string, default_modules[]: string[] }`. |
| `current_date`                 | `string`                        | ISO-8601 date used for "as of" stamping in the report.                                                           |
| `config`                       | `object \| null`                | Resolved deep-research package config (depth, tolerance, render flags).                                          |
| `package_summary`              | `object \| null`                | Resolved package: `{ archetype, reference_profile, report_type_id, domain_profile_id, style_profile_id, docs[], modules[] }`. The three `*_profile_id` / `*_id` fields are the layered profile triple the manager threads through every brief. |
| `expected_blocks[]`            | `array<object>`                 | Each: `{ instance_id, block_id, template_id, title, doc_id, order, required, status }`.                          |
| `existing_blocks[]`            | `array<object>`                 | Each: `{ instance_id, block_id, title, doc_id, chars, reason }`. Already-committed content.                       |
| `completeness`                 | `object`                        | `{ total_required, done_required, missing_instance_ids[], thin_instance_ids[], ready_to_finish }`.                |
| `character_budget`             | `object`                        | `{ target_chars, actual_chars, delta_chars, tolerance, within_tolerance, severely_off_target, status }`.           |
| `min_evidence_count`           | `number`                        | Floor: number of evidence excerpts required before any `write_with_skill` call.                                  |
| `evidence_axes[]`              | `array<string>`                 | The topic axes the evidence pre-phase must cover.                                                                |
| `answered_questions[]`         | `array<object>`                 | Each: `{ id, section, question, answer, answered_at }`.                                                          |
| `open_questions[]`             | `array<object>`                 | Each: `{ id, section, question, allow_fallback, raised_at }`.                                                    |
| `blocking_open_questions[]`    | `array<object>`                 | Subset of `open_questions[]` with `allow_fallback=false`.                                                        |
| `review_feedback`              | `object \| null`                | `{ matched_blocks[], active_form_revision, general_count, recent_notes[] }`.                                      |
| `user_notes[]`                 | `array<object>`                 | Free-text notes the operator added before/during the run.                                                        |
| `available_research_ids[]`     | `array<string>`                 | All `research_id` values currently in provenance.                                                                |
| `available_skill_ids[]`        | `array<string>`                 | All `skill_id` values for staged but not-yet-applied sub-skill runs.                                              |

## asset_lookup input/output schema

Input parameters:

```json
{
  "instance_ids": ["string", "..."],
  "report_type_id": "string (optional; defaults to the run's report_type_id)",
  "include_report_type": false
}
```

`instance_ids` is optional (default `[]`). Empty array means "return the
full block and reference catalogue for the resolved package, scoped to
the run's `report_type`". Non-empty array means "return only the
rubrics and references for these blocks". The host validates that every
supplied `instance_id` resolves to a `block_id` in the run's
`report_type.block_library_keys[]`; cross-type ids are rejected with
`error.code = "cross_type_block"`.

`report_type_id` is optional. If omitted, the host uses the run-bound
`report_type_id` from `workspace_snapshot`. Passing a different value is
rejected with `error.code = "report_type_mismatch"`; the run is bound
to one type for its entire lifetime. The parameter exists so manager
debug logs and integrators can be explicit.

`include_report_type` (default `false`). When `true`, the response also
contains the resolved `report_type` object (full schema as documented
on `workspace_snapshot.report_type`). The manager calls with
`include_report_type=true` exactly once at bootstrap; later calls leave
it `false` because the manager has cached the object locally.

Output payload fields:

| Field             | Type            | Meaning                                                                                          |
| ----------------- | --------------- | ------------------------------------------------------------------------------------------------ |
| `block_defs[]`    | `array<object>` | Each: `{ instance_id, block_id, template_id, title, doc_id, order, required, min_chars, rubric }` where `rubric` is `{ description, must_cover[], reference_ids[], style_guide_keys[] }`. **Restricted to `block_id ∈ report_type.block_library_keys[]`** — the asset_lookup tool filters this server-side, so the manager never sees a cross-type block. |
| `references[]`    | `array<object>` | Each: `{ id, kind, title, citation, excerpt, source_url, doi, license, usage_rights }`. Scoped to the references reachable from the in-scope `block_defs[]`. |
| `style_guide`     | `object`        | `{ tone, voice, person, sentence_length_band, paragraph_length_band, forbidden_phrases[], report_type_arc[] }`. The `report_type_arc[]` field is the ordered list of section-level beats `narrative_flow_check` checks the manuscript against; it varies by `report_type_id`. |
| `document_flow[]` | `array<object>` | Ordered: `{ doc_id, doc_title, blocks[]: [{ instance_id, title, order, required }] }`. Scoped to the run's `report_type`. |
| `reference_length_stats` | `object` | `{ mean_chars, median_chars, p10_chars, p90_chars }` over the reference corpus for the resolved `report_type` (different types benchmark against different reference corpora). |
| `report_type`     | `object \| undefined` | Present iff `include_report_type=true`. Same shape as `workspace_snapshot.report_type`. |

## public_research input/output schema

Input parameters:

```json
{
  "question": "string (>= 5 chars, declarative)",
  "focus": "string (default: \"\")"
}
```

Output payload fields (success):

| Field            | Type             | Meaning                                                                                              |
| ---------------- | ---------------- | ---------------------------------------------------------------------------------------------------- |
| `research_id`    | `string`         | Stable id assigned by the host (e.g. `research_abc123`). Use it as `used_research_ids` argument.     |
| `question`       | `string`         | Echo of the input question.                                                                          |
| `focus`          | `string`         | Echo of the input focus.                                                                             |
| `summary`        | `string`         | <= 800 chars. Synthesis across the returned sources, written by the research tool, not the writer.   |
| `evidence_axis`  | `string`         | Which `evidence_axes[]` entry this call was meant to cover. Used by the manager to track coverage.   |
| `sources[]`      | `array<object>`  | See below.                                                                                            |

Each `sources[]` entry:

| Field                   | Type            | Meaning                                                                                            |
| ----------------------- | --------------- | -------------------------------------------------------------------------------------------------- |
| `evidence_id`           | `string`        | Stable id (e.g. `e_017`). Used as the citation key in block markdown.                              |
| `kind`                  | `"doi" \| "arxiv" \| "openalex" \| "web"` | Source class.                                                                            |
| `title`                 | `string`        | Verbatim from the resolver (Crossref-canonical for DOIs).                                          |
| `authors[]`             | `array<string>` | Verbatim from the resolver. Manager never edits.                                                    |
| `year`                  | `number \| null`| Publication year if known.                                                                          |
| `venue`                 | `string`        | Journal / conference / publisher / domain.                                                          |
| `doi`                   | `string \| null`| Crossref-resolved. Empty for non-DOI sources.                                                       |
| `url`                   | `string`        | Canonical URL.                                                                                      |
| `excerpts[]`            | `array<object>` | Each: `{ excerpt_id, text, locator }`. `text` is a short verbatim quote; `locator` is page/section. |
| `license`               | `string`        | License declaration if discoverable; empty otherwise.                                                |
| `crossref_resolved`     | `boolean`       | `true` if the DOI passed Crossref resolution (DOIs only). Always `false` for non-DOI sources.       |

Failure codes: `research_disabled`, `research_budget_reached`,
`research_empty`, `research_timeout`, `research_aborted`, `research_error`.

## write_with_skill input/output schema

Input parameters:

```json
{
  "instance_ids": ["string", "..."],
  "brief": "string"
}
```

`instance_ids` is `min(1) max(6)`. `brief` is required (default empty
string allowed but the manager always populates it; see
`manager_path.md`).

The host builds the full sub-skill input bundle from the manager
arguments plus the workspace state. The bundle (sent to the Block
Writer Skill) contains:

| Field                 | Type            | Meaning                                                                            |
| --------------------- | --------------- | ---------------------------------------------------------------------------------- |
| `package_context`     | `object`        | From `workspace_snapshot.package_summary`. Includes `report_type_id`, `report_type` (resolved), `domain_profile_id`, `domain_profile` (resolved), `style_profile_id`, `style_profile` (resolved). These six fields are the conditioning levers for the writer sub-skill: `report_type` selects the verdict-line shape and matrix presence; `domain_profile` selects vocabulary, methodology references, and risk-axis defaults; `style_profile` selects tone, sentence-length band, person, and forbidden-phrase list. The writer reads all three on every call. |
| `character_budget`    | `object`        | From `workspace_snapshot.character_budget`.                                        |
| `style_guide`         | `object`        | From `asset_lookup.style_guide`. Includes `report_type_arc[]` for the resolved type. |
| `document_flow`       | `array<object>` | From `asset_lookup.document_flow`, scoped to the packet's `doc_id`.                 |
| `workspace_snapshot`  | `object`        | The full snapshot, for the writer's reference.                                     |
| `selected_blocks[]`   | `array<object>` | Block defs for the requested `instance_ids`. All `block_id` values are guaranteed to be in `report_type.block_library_keys[]`; the host rejects writes that include cross-type blocks before they reach the sub-skill. |
| `selected_references[]` | `array<object>` | Reference catalogue entries for the packet's `reference_ids`.                  |
| `existing_blocks[]`   | `array<object>` | Already-committed adjacent blocks for cohesion context.                            |
| `answered_questions[]`| `array<object>` | Carried forward from the workspace.                                                |
| `open_questions[]`    | `array<object>` | Carried forward from the workspace.                                                |
| `review_feedback`     | `object \| null`| Carried forward.                                                                   |
| `user_notes[]`        | `array<object>` | Carried forward.                                                                   |
| `research_notes[]`    | `array<object>` | All `public_research` results currently in provenance.                             |
| `brief`               | `string`        | Manager-supplied; see `manager_path.md`. Must include the `report_type_id`, `domain_profile_id`, `style_profile_id` triple as an in-text header line. |
| `goals[]`             | `array<string>` | Empty for `write`; populated for `revision`.                                       |

Sub-skill output schema (validated host-side):

```json
{
  "summary": "string",
  "blocking_reason": "string (default: \"\")",
  "blocking_questions": ["string", "..."],
  "blocks": [
    {
      "instance_id": "string",
      "doc_id": "string",
      "block_id": "string",
      "title": "string",
      "order": 0,
      "markdown": "string",
      "reason": "string",
      "used_reference_ids": ["string", "..."]
    }
  ]
}
```

`blocking_questions` is `max(3)`. `blocks` is `max(6)`.
`used_reference_ids` is `max(8)` per block.

The tool wraps this output and returns to the manager:

```json
{
  "ok": true,
  "data": {
    "skill_id": "skill_write_xxxx",
    "summary": "string",
    "count": 0,
    "instance_ids": ["string", "..."],
    "titles": ["string", "..."],
    "needs_user_input": false,
    "open_questions": ["string", "..."]
  }
}
```

## revise_with_skill input/output schema

Input parameters:

```json
{
  "instance_ids": ["string", "..."],
  "goals": ["string", "..."],
  "form_only": false
}
```

`instance_ids` is `min(1) max(6)`. `goals` is `min(1) max(8)`.
`form_only` defaults to `false`. The full sub-skill input bundle
(identical structure to write_with_skill above) is built host-side,
with `goals` and `form_only` passed through.

Sub-skill output schema is identical to write_with_skill. The tool
wraps it identically except the returned id is `skill_revise_xxxx`.

## apply_block_patch input/output schema

Input parameters:

```json
{
  "skill_id": "string (>= 1 char)",
  "instance_ids": ["string", "..."],
  "used_research_ids": ["string", "..."]
}
```

`instance_ids` is `max(12)` (default `[]`; empty means "commit every
block in the staged skill run"). `used_research_ids` is `max(8)`.
**No `markdown` field. Schema-enforced.**

Output payload:

```json
{
  "skill_id": "string",
  "changed_blocks": ["string", "..."],
  "count": 0
}
```

## completeness_check output schema

Input: `{}`.

Output:

```json
{
  "total_required": 0,
  "done_required": 0,
  "missing_required": ["string", "..."],
  "thin_required": ["string", "..."],
  "missing_optional": ["string", "..."],
  "ready_to_finish": false
}
```

`ready_to_finish` is `true` iff `total_required > 0`,
`done_required === total_required`, and `thin_required` is empty.

## character_budget_check output schema

Input: `{}`.

Output:

```json
{
  "target_chars": 0,
  "actual_chars": 0,
  "delta_chars": 0,
  "tolerance": 0.0,
  "within_tolerance": false,
  "severely_off_target": false,
  "status": "not_started | within | low | high | severely_off",
  "summary": "string",
  "adjustment_hint": "string",
  "reference_average_chars": 0,
  "offset_percent_label": "string"
}
```

The host loop-end gate uses `severely_off_target` to decide whether the
check is satisfied; minor over/undershoots inside `tolerance` are
acceptable for `decision: "finished"`.

## release_guard_check output schema

Input: `{}`.

Output (deterministic lint suite, no LLM):

```json
{
  "summary": "string",
  "check_applicable": true,
  "ready_to_finish": true,
  "needs_revision": false,
  "candidate_instance_ids": ["string", "..."],
  "goals": ["string", "..."],
  "reasons": ["string", "..."]
}
```

`candidate_instance_ids` is `max(6)`. `goals` is `max(8)`. `reasons` is
`max(6)`. The lint catalogue is in `release_guard_lints.md`. Each
`reasons[]` entry names a lint code and the offending block; each
`goals[]` entry names the corrective action the manager should pass
verbatim into `revise_with_skill.goals`.

### Unconditional vs report-type-conditional lints

Some lints fire on every run; others are gated on the active
`report_type_id`. The tool reads the run's `report_type` from the
workspace and silently skips lints that do not apply. From the
manager's point of view the response shape is unchanged — the only
visible difference is which lint codes can ever appear in
`reasons[]`.

- **Unconditional (every report type).** Anti-slop language lints:
  LINT-DUPLICATE-PHRASE, LINT-EMPTY-FILLER, LINT-OVERHEDGE,
  LINT-AI-GIVEAWAY-PHRASE, LINT-FABRICATED-CITATION,
  LINT-UNRESOLVED-DOI.
- **Matrix-only lints.** Fire only when the type has
  `has_matrix=true` (`feasibility_study`, `technology_screening`,
  `competitive_analysis`, `decision_brief`):
  LINT-DUPLICATE-MATRIX-RATIONALE,
  LINT-MATRIX-CELL-MISSING-EVIDENCE,
  LINT-MATRIX-OPTION-WITHOUT-VERDICT.
- **Evidence-register-only lints.** Fire only when the type has
  `has_evidence_register=full` (currently `feasibility_study`,
  `literature_review`): LINT-EVIDENCE-REGISTER-THIN,
  LINT-CLAIM-WITHOUT-EVIDENCE-ID,
  LINT-EVIDENCE-AXIS-UNDERCOVERED.
- **Verdict-line-only lints.** Fire only when
  `report_type.verdict_line_pattern != null`:
  LINT-VERDICT-MISMATCH, LINT-VERDICT-LINE-MISSING. They never fire
  for `whitepaper` or `literature_review`.

The future `references/release_guard_lints.md` (a parallel agent is
updating it to be report-type-aware) carries the full lint definitions,
applicability matrix, and corrective-goal phrasings. This contract
file documents the schema; the lint file documents the rules.

## narrative_flow_check output schema

Input: `{}`. Calls the Flow Review Sub-Skill (LLM, schema-validated).

Output (identical envelope to release_guard_check):

```json
{
  "summary": "string",
  "check_applicable": true,
  "ready_to_finish": true,
  "needs_revision": false,
  "candidate_instance_ids": ["string", "..."],
  "goals": ["string", "..."],
  "reasons": ["string", "..."]
}
```

If fewer than two ready blocks exist, the tool returns
`check_applicable=false` and `ready_to_finish=true` without invoking
the LLM. This matches the Förderantrag agent's behaviour.

The arc the Flow Review sub-skill checks the manuscript against is the
run's `dossier_story_model[]`, sourced from
`asset_lookup.style_guide.report_type_arc[]`. The arc varies by
`report_type_id`:

- `feasibility_study`: "Frage → Domänenmodell → Anforderungen →
  Optionsraum → Bewertungslogik → Matrix → Szenarien →
  Detailbewertung → Risiken → Empfehlung".
- `market_research`: "Markt → Segmente → Treiber → Wettbewerb →
  Eintrittsoptionen → Empfehlung".
- `competitive_analysis`: "Scope → Wettbewerber → Capability-Matrix
  → Positionierung → Lücken → Empfehlung".
- `technology_screening`: "Screening-Frage → Longlist →
  Kriterien → Matrix → Shortlist → nächste Schritte".
- `whitepaper`: "These → Kontext → Argumentationskette →
  Implikationen".
- `literature_review`: "Scope → Methode → Themen-Synthese →
  themenübergreifende Integration → offene Forschungsfragen".
- `decision_brief`: "Entscheidungsfrage → Optionen → Bewertung →
  Empfehlung".

The flow_review sub-skill receives the active arc as
`style_guide.report_type_arc[]` from the asset pack and reports
deviations against that arc, not against any hard-coded template.

## Loop-end host gate

Pseudocode for the override the host applies after the manager's last
turn. The manager cannot bypass this; the override is what makes the
four checks binding rather than advisory.

The host gate logic itself does not change between report types — all
four checks must still be ready (or `check_applicable=false`) for a
`decision: "finished"` to survive. What changes is the failing-check
naming in the operator-facing override message: the host appends the
run's `report_type_id` so the operator can tell at a glance which type
of report failed which gate ("character_budget_check (severely_off) on
feasibility_study", "release_guard_check (LINT-DUPLICATE-MATRIX-
RATIONALE) on competitive_analysis", etc.).

```text
let final = manager.finalOutput
let report_type_id = ctx.workspaceSnapshot.report_type_id
let blocks_ready = count(committed_blocks_with_text) >= 1
let two_blocks_ready = count(committed_blocks_with_text) >= 2

let completeness_ready = ctx.lastCompleteness?.ready_to_finish == true

let charBudget_required = blocks_ready
let charBudget_ready = !charBudget_required
                    || (ctx.lastCharacterBudget != null
                        && !ctx.lastCharacterBudget.severely_off_target)

let releaseGuard_required = blocks_ready
let releaseGuard_ready = !releaseGuard_required
                    || (ctx.lastReleaseGuard != null
                        && (ctx.lastReleaseGuard.ready_to_finish
                            || ctx.lastReleaseGuard.check_applicable == false))

let flow_required = two_blocks_ready
let flow_ready = !flow_required
              || (ctx.lastNarrativeFlow != null
                  && (ctx.lastNarrativeFlow.ready_to_finish
                      || ctx.lastNarrativeFlow.check_applicable == false))

if final.decision == "finished"
   && (!completeness_ready || !charBudget_ready
       || !releaseGuard_ready || !flow_ready):
    final.decision = "blocked"
    let failing = name_of_first_failing_check(
        completeness_ready, charBudget_ready,
        releaseGuard_ready, flow_ready)
    final.summary = failing + " on " + report_type_id
    final.reason  = failing + " on " + report_type_id

if (pending_blocking_questions || any_blocking_open_questions)
   && final.decision != "blocked":
    final.decision = "needs_user_input"
    final.open_questions = current_blocking_question_texts()
```

Net effect: a `decision: "finished"` from the LLM survives only when
every applicable check has reported ready. The first failing check
names the user-visible blocker, qualified by the run's
`report_type_id`.
