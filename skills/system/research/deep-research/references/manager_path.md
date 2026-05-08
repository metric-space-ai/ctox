# Deep Research — Manager Runbook

This document is the manager-side runbook. It is read by an agent who has
not seen the originating conversation and who only knows that they are
orchestrating the deep-research pipeline. The manager never writes prose
itself; it picks tools, builds tool inputs, and routes outputs.

The full architecture is in `../SKILL.md`. The schema contracts for every
tool are in `check_contracts.md`. The lint catalogue used by
`release_guard_check` is in `release_guard_lints.md`.

## Report-type binding

Every run starts with a known `report_type_id` — exactly one of
`feasibility_study`, `market_research`, `competitive_analysis`,
`technology_screening`, `whitepaper`, `literature_review`,
`decision_brief`. The manager reads it from
`workspace_snapshot.report_type_id` (and the resolved object from
`workspace_snapshot.report_type`). It never guesses; if both fields are
missing or the id is not one of the seven, the manager emits
`decision: "blocked"` with `summary: "missing report_type_id"` and stops.

The first call after `workspace_snapshot` is `asset_lookup` with
`{ "include_report_type": true }` (and any `instance_ids` if already
known). The asset_lookup tool resolves `report_types[id ==
report_type_id]` from the asset pack, returns the matching entry under
`report_type` (its `block_library_keys[]`, `verdict_line_pattern`,
`verdict_vocabulary[]`, `typical_chars`, `min_sections`,
`document_blueprint_id`, `default_modules`), and scopes the
`block_defs[]` and `references[]` payload to that type's
`block_library_keys[]`. The manager caches the resolved `report_type`
locally and uses it for every later decision (block selection, brief
construction, gate interpretation) without re-querying.

Layering parallel: this is the same architecture the Förderantrag agent
(`runAgentManager` in `Foerdervorhaben-Agent.html`) uses for
`program_profiles` × `funding_profiles` × `reference_profiles`. Here the
three layered profiles are `report_type` × `domain_profile` ×
`style_profile`. The manager reads all three at bootstrap and threads
their ids through every `brief` it writes.

## Tool order

Every run moves through five phases. The manager asks itself the question
in column three before each tool call; if the answer is no, it picks a
different tool. Column four names what changes by `report_type_id`.

| Phase           | Tool sequence                                                                              | Self-check before each call                                                                                            | Report-type-relevant note                                                                                          |
| --------------- | ------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `bootstrap`     | `workspace_snapshot` → `asset_lookup` (`{ include_report_type: true }`)                    | "Have I read the package config and expected blocks at least once this run? Did asset_lookup return `report_type`?"    | The first `asset_lookup` resolves and caches the run's `report_type` object; later calls inherit the same scope.   |
| `evidence`      | `public_research` (one or more passes) → `asset_lookup` (scoped to upcoming packet)        | "Do I have at least `depth_profile.min_evidence_count` excerpts covering the next packet's topic axes?"                | Depth profile and report type both shape the budget — `feasibility_study`/`standard` ≈ 8 sources; `market_research` or `decision_grade` may need 20; `whitepaper` may need only 6 well-chosen ones. |
| `drafting`      | `write_with_skill` → `apply_block_patch` → `completeness_check` (loop)                     | "Does the next packet share a `doc_id` and is it adjacent in `order`? Have I attached the matching `evidence_ids`?"    | `instance_ids` must be from the run's `report_type.block_library_keys[]`. The brief carries `report_type_id`, `domain_profile_id`, `style_profile_id`. |
| `iteration`     | `character_budget_check` → `release_guard_check` → `narrative_flow_check` → `revise_with_skill` (only when a gate flagged a block) → `apply_block_patch` | "Did the last failing gate return concrete `candidate_instance_ids` and `goals` I can pass through verbatim?"          | Lints fire conditionally on `report_type_id` (matrix-only lints, evidence-register-only lints, verdict-line-only lints). See the lint subsection below. |
| `finalisation`  | re-run failed gate, then any later gates → `decision: "finished"` only when all four pass  | "Are all four gates either `ready_to_finish=true` or `check_applicable=false` since the last patch was applied?"        | The host's loop-end gate names the failing check together with the run's `report_type_id` so the operator output is informative. |

`ask_user` is not a phase tool. It can be invoked from any phase the
moment a required fact cannot be obtained autonomously. After `ask_user`,
the run ends with `decision: "needs_user_input"`.

## When to call asset_lookup

- **Always once at bootstrap**, with empty arguments. This binds the
  full block list and reference catalogue for the resolved package.
- **Again whenever the active block-set changes.** Specifically: before
  every `write_with_skill` call whose `instance_ids` differ from the
  last call, and before every `revise_with_skill` call. Pass the
  `instance_ids` of the upcoming packet so the asset tool returns only
  the rubrics and reference patterns the writer actually needs. Smaller
  asset payloads keep the writer's context tight and the schema clean.
- **Do not call** between two consecutive writer calls that target the
  same instance_ids — it is wasted budget.

## How to call public_research

`public_research` is the mandatory pre-phase. The deep-research variant
differs from the Förderantrag agent in two ways: research is required
(not optional), and multiple calls per run are expected.

Phrasing pattern for `question`:

- One declarative research question per call. Pose it as the
  literature-search query a domain expert would type, not as a chatty
  prompt. Example: `"Vergleich zwischen Laser-Ultraschall und
  Wirbelstromprüfung an austenitischem Edelstahl, Sensitivität für
  Risse <0.5 mm"`.

Phrasing pattern for `focus`:

- Optional. Use it to narrow the result space when the question is
  broad. Examples: `"peer-reviewed since 2018"`,
  `"industrial standards EN ISO"`, `"non-academic vendor whitepapers
  acceptable only as supporting evidence"`.

Per-run budget by depth profile (matches `min_evidence_count` floor):

| Depth profile     | Min evidence count | Soft cap on calls per run |
| ----------------- | ------------------ | ------------------------- |
| `orienting`       | 3                  | 5                         |
| `standard`        | 8                  | 12                        |
| `decision_grade`  | 20                 | 30                        |

DOI handling: the tool resolves every detected DOI through Crossref
before returning. Manager never edits or invents a DOI. If Crossref
returns no record, the source is dropped from the result and replaced
with the next-best matching record from OpenAlex / arXiv / web.

If `public_research` returns `ok: false` with `research_empty` or
`research_error` after one retry, the manager reformulates the
question (different phrasing, narrower focus, alternative vocabulary)
before trying again. Three consecutive empty results on the same axis
trigger the abort path described in `SKILL.md > "When to abort
instead of revising"`.

## How to call write_with_skill

Instance-id selection rule:

- **Always batch by `doc_id` and adjacent `order`.** Pull up to 6
  consecutive blocks from one document. Do not cherry-pick across
  documents in one packet — the writer's context window is loaded with
  one document's flow at a time, and mixing degrades cohesion.
- **Required before optional.** Resolve every required block first,
  then revisit optional blocks only if the budget allows.
- **Do not include any block that already has `apply_block_patch`-
  committed content unless the manager wants to overwrite it.** Use
  `revise_with_skill` for in-place improvements to existing content.
- **`instance_ids` must intersect with the run's
  `report_type.block_library_keys[]`.** The manager refuses if not.
  Cross-type block usage triggers the abort path "cross-type block
  usage" described in `../SKILL.md > "When to abort instead of
  revising"`. The asset_lookup contract also enforces this server-
  side, but the manager validates locally before issuing the call.

`brief` content (free-text, but must include each of the following so
the writer's input bundle stays consistent across runs):

- Target depth (`orienting` | `standard` | `decision_grade`).
- Language (e.g. `de-DE`, `en-US`).
- Must-cover questions for this packet, one per line.
- Attached `evidence_ids` (the writer also receives the full evidence
  list automatically, but the brief names the ones the manager
  considers strongest for this packet).
- `target_chars` per block as a hint, derived from the asset-pack
  `min_chars` and the character budget.

The `brief` field must include the run's `report_type_id`, the active
`domain_profile_id`, and the active `style_profile_id` — the writer
sub-skill conditions its register, vocabulary, verdict-line shape, and
matrix presence on these three. Recommended phrasing: a short header
line such as `report_type=feasibility_study; domain=ndt_aerospace;
style=technical_de_formal` followed by the rest of the brief. Without
all three, the writer falls back to a generic register and the output
will not match the type's `verdict_line_pattern`.

## How to call revise_with_skill

`goals[]` content (1–8 items, schema-enforced). Each goal must be a
specific, actionable, falsifiable correction:

- Good: `"In Block 3.2 explicit Sensitivität in mm angeben, basierend
  auf evidence_id e_017 und e_022."`
- Good: `"Matrix-Zelle (Option A × Kriterium Reproduzierbarkeit):
  identische Begründung wie Zelle (Option A × Kriterium Genauigkeit)
  ersetzen."`
- Bad: `"make better"`, `"improve flow"`, `"more concrete"` — these are
  rejected because the revision sub-skill cannot turn vague directives
  into specific edits.

Use `form_only=true` when:

- A `narrative_flow_check` returned `needs_revision=true` for length,
  ordering, sentence rhythm, or transitions.
- A `character_budget_check` requested length adjustment without
  changing factual content.

Do not use `form_only=true` when:

- The flag would mask a missing fact, an unsupported claim, or a
  duplicate rationale string. Those need a fact-bearing revision with
  fresh evidence_ids in `goals`.

## How to call apply_block_patch

- Pass only `skill_id` (the value returned by the most recent
  `write_with_skill` or `revise_with_skill` call), plus optionally
  `instance_ids` (subset of the patch to commit) and `used_research_ids`
  (provenance link).
- **Never copy markdown into args.** The host already has the markdown
  in the staged skill run; passing it again is forbidden by contract
  and disabled by schema (the tool's input schema has no `markdown`
  field).
- After commit, the next call is always either another writer call,
  another patch, or a gate. Never call `apply_block_patch` twice in a
  row for the same `skill_id`.

## How to interpret each of the four checks

| Check                    | When `needs_revision=true`                                                                                                              |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `completeness_check`     | Not actionable directly — call `write_with_skill` for the next packet of `missing_required` instance_ids; for `thin_required`, call `revise_with_skill` with goals naming the rubric criteria the block falls short on. |
| `character_budget_check` | Call `revise_with_skill` with `form_only=true` and `goals` listing the specific blocks to lengthen or shorten by an explicit character delta. |
| `release_guard_check`    | Call `revise_with_skill` (NOT `form_only`) with `candidate_instance_ids` and `goals` translated from the lint reasons (see `release_guard_lints.md`). Form-only is wrong here because lints flag fact problems, not form problems. The lint set is conditional on `report_type_id` — see "Conditional lints" below. |
| `narrative_flow_check`   | Call `revise_with_skill` with `candidate_instance_ids` and `goals` from the check; `form_only=true` is correct unless `reasons` mention a missing fact or evidence link. |

### Conditional lints (release_guard_check by report_type)

Not every lint applies to every report type. `release_guard_check`
silently skips lints that do not apply to the active `report_type_id`,
returning `check_applicable=true` but with the inapplicable lint codes
absent from `reasons[]`. The manager does not need to filter; it only
needs to know which lints can ever fire so a clean run is recognisable.

- **Always-on (every report type).** The anti-slop language lints:
  LINT-DUPLICATE-PHRASE, LINT-EMPTY-FILLER, LINT-OVERHEDGE,
  LINT-AI-GIVEAWAY-PHRASE, LINT-FABRICATED-CITATION,
  LINT-UNRESOLVED-DOI.
- **Matrix-heavy types only**
  (`feasibility_study`, `technology_screening`, `competitive_analysis`,
  `decision_brief`): LINT-DUPLICATE-MATRIX-RATIONALE,
  LINT-MATRIX-CELL-MISSING-EVIDENCE,
  LINT-MATRIX-OPTION-WITHOUT-VERDICT.
- **Evidence-heavy types only**
  (`literature_review`, `feasibility_study`): LINT-EVIDENCE-REGISTER-
  THIN, LINT-CLAIM-WITHOUT-EVIDENCE-ID,
  LINT-EVIDENCE-AXIS-UNDERCOVERED.
- **Verdict-line types only**
  (every type whose `verdict_line_pattern != null` —
  `feasibility_study`, `market_research`, `competitive_analysis`,
  `technology_screening`, `decision_brief`): LINT-VERDICT-MISMATCH,
  LINT-VERDICT-LINE-MISSING. These do not fire for `whitepaper` or
  `literature_review`.

Full lint definitions and corrective-goal phrasings are in
`release_guard_lints.md`, which the parallel agent is updating to make
the lint suite report-type-aware.

After every revision-and-patch cycle, re-run the gate that flagged the
change, then any later gates that depend on it (release_guard depends
on facts; flow_check depends on length and order being settled).

## blocking_questions handling

When `write_with_skill` or `revise_with_skill` returns
`blocking_reason` or `blocking_questions[]` non-empty:

1. The host has already attached them to the workspace as blocking
   open questions.
2. The manager calls `ask_user` exactly once with the section name,
   the reason, and the questions.
3. The manager ends the run with `decision: "needs_user_input"` and
   `open_questions` equal to the question list. No further tool calls.
4. The host loop-end gate confirms `needs_user_input` and surfaces
   the questions to the operator.

Do not paraphrase the questions, do not bundle them with optimistic
language, do not attempt to answer them autonomously. The contract
expects them verbatim.

## Report-type quick reference

One-line summary per supported `report_type_id`. Authoritative source
is the asset pack's `report_types[]` array — this table is the
manager's in-memory lookup so it does not re-read the asset pack on
every turn. Numbers track the values listed in `../SKILL.md > "What
this skill produces"`.

| `report_type_id`         | typical_chars | min_sections | has_matrix | has_scenarios | has_evidence_register | `verdict_line_pattern`                                         |
| ------------------------ | ------------- | ------------ | ---------- | ------------- | --------------------- | -------------------------------------------------------------- |
| `feasibility_study`      | 30 000        | 9            | yes        | yes           | yes                   | "Erfolgsaussichten (qualitativ): {…}"                          |
| `market_research`        | 25 000        | 7            | no         | no            | yes                   | "Marktattraktivität: {…}"                                      |
| `competitive_analysis`   | 20 000        | 8            | yes        | no            | yes                   | "Wettbewerbsposition: {…}"                                     |
| `technology_screening`   | 22 000        | 8            | yes        | no            | yes                   | "Methodenpriorität: {…}"                                       |
| `whitepaper`             | 18 000        | 5            | no         | no            | partial               | `null` (no verdict line)                                       |
| `literature_review`      | 28 000        | 6            | no         | no            | yes                   | `null` (no verdict line)                                       |
| `decision_brief`         | 8 000         | 5            | yes        | no            | yes                   | "Empfehlung: {…}"                                              |

`has_matrix=yes` means the type's `block_library_keys[]` includes a
matrix block whose cells the writer must populate with rationale +
`evidence_id`. `has_scenarios=yes` means a scenario-branches block is
required. `has_evidence_register=partial` for `whitepaper` means the
type cites sources but does not require the full structured register
block.

The manager picks the right verdict pattern by reading the resolved
`report_type.verdict_line_pattern` directly, not by hard-coding it
from this table. The table is for the manager's mental model; the
asset pack is the source of truth.
