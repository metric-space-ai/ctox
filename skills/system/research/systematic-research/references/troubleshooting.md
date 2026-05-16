# Deep Research Skill — Troubleshooting

This catalogue covers the failure modes a deep-research operator will
actually hit. Each section names the symptom, the typical cause, and the
fix. The lint codes referenced under "release_guard_check fails
repeatedly" are defined in `references/release_guard_lints.md`; the tool
schemas are in `references/check_contracts.md`. Read this file from the
top — most issues are early-phase issues that prevent the run from ever
reaching the rendering step.

## Run cannot start

`ctox report new <report_type> --topic "..."` returns an error and never
prints a `run_id`. The four common causes:

- **Unknown `report_type_id`.** The CLI accepts only the seven supported
  ids: `feasibility_study`, `market_research`, `competitive_analysis`,
  `technology_screening`, `whitepaper`, `literature_review`,
  `decision_brief`. Anything else is rejected before the run is created.
  Re-check the asset pack:

  ```bash
  jq '.report_types[].id' \
    /Users/michaelwelsch/Documents/ctox/skills/system/research/systematic-research/references/asset_pack.json
  ```

- **Unknown `domain_profile_id`.** `--domain` accepts only the ids in
  `domain_profiles[]`. Currently: `ndt_aerospace`,
  `materials_method_assessment`, `manufacturing_process`,
  `biotech_lab_methods`, `software_evaluation`, `general`. Misspellings
  like `--domain ndt-aerospace` (hyphen instead of underscore) hit this
  path. Same `jq` trick:

  ```bash
  jq '.domain_profiles[].id' \
    /Users/michaelwelsch/Documents/ctox/skills/system/research/systematic-research/references/asset_pack.json
  ```

- **Unknown `depth_profile_id`.** Only `orienting`, `standard`,
  `decision_grade` are valid. The CLI alias `--depth deep` does not
  exist; pass `--depth decision_grade` literally.

- **DB lock / WAL contention.** A second CTOX process holds the runtime
  store. Symptom: the create call hangs for 5 seconds and then returns
  a `database is locked` error. Diagnose with:

  ```bash
  lsof /Users/michaelwelsch/Documents/ctox/runtime/ctox.sqlite3
  ```

  If two `ctox` processes appear, end the older one. The CTOX runtime
  store is single-writer; concurrent writers serialise via WAL but a
  long-running mission daemon can starve a foreground command.

- **Missing asset pack.** Symptom:
  `error: asset pack not found at .../asset_pack.json`. The skill
  bundle did not install or was deleted. Re-pull the bundle. Verify:

  ```bash
  ls /Users/michaelwelsch/Documents/ctox/skills/system/research/systematic-research/references/asset_pack.json
  ```

  If the file is missing, the rest of the skill's references is
  probably also missing — re-clone or re-install before running.

## Manager keeps calling public_research

Symptom: the run sits in the `evidence` phase for 5+ minutes; the log
streams `public_research(...)` calls back-to-back without ever advancing
to `drafting`. The run cannot satisfy `min_evidence_count`. Causes, in
order of likelihood:

1. **Crossref / OpenAlex / arXiv unreachable.** Quick test:

   ```bash
   python3 /Users/michaelwelsch/Documents/ctox/skills/system/research/systematic-research/scripts/doi_resolve.py \
       10.3390/coatings9110727
   ```

   If that returns `"resolver_used": "none"` and `"ok": false`, the
   problem is network-side (DNS, proxy, firewall). Fix the network or
   work behind the proxy CTOX expects.

2. **Topic is too narrow.** No public sources exist on the exact thing
   you asked about. The resolver does not invent sources; it returns
   `research_empty`. Three consecutive empties on the same axis trigger
   the abort path.

   Fix: broaden the topic, or supply seed DOIs:

   ```bash
   ctox report new feasibility_study \
       --topic "Kontaktlose Pruefung von Funktionsschichten in CFRP" \
       --seed-doi 10.3390/coatings9110727 \
       --seed-doi 10.1080/17686733.2024.2448049
   ```

3. **Quota or rate limit.** If `public_research` returns
   `research_error` with a `429` or `503` message, the upstream backend
   is throttling. CTOX will back off and retry, but if quota is hard-
   capped you may need to wait. Check the ctox logs for the failing
   request URL — it tells you which backend (Crossref / OpenAlex /
   arXiv) is throttling and you can side-step it for a while.

4. **Depth profile is too aggressive.** A `decision_grade` run on a
   thin topic asks for 20 sources where the corpus has 8. Drop to
   `standard` (12 sources) or even `orienting` (6) for the first pass:

   ```bash
   ctox report new technology_screening --depth orienting --topic "..."
   ```

   You can always re-run at higher depth later when the topic has been
   sharpened.

When the manager has called `public_research` more than the soft cap
(orienting=5, standard=12, decision_grade=30) without satisfying the
floor, it emits `decision: "blocked"` with a summary naming the
unsatisfied evidence axis. Do not retry the same topic in the same
phrasing — reformulate, or accept that the report cannot be evidence-
graded today.

## Sub-skill returns blocking_reason

Symptom: `ctox report status RUN_ID --json` shows
`decision: "needs_user_input"` and `open_questions[]` lists 1-3
questions. The writer or revisor sub-skill cannot make a fact-bearing
decision without operator input.

The questions are intentionally not answered by the manager — answering
them autonomously would be the kind of fabrication this skill exists to
prevent. Read each question, answer it specifically, continue the run:

```bash
ctox report status r_xxxx --json | jq '.open_questions'
ctox report answer r_xxxx --question-id Q1 --answer "Concrete answer."
ctox report continue r_xxxx
```

What the writer typically blocks on per report type:

- **`feasibility_study`** — layup details (which schichtaufbau? open
  grid vs. closed metal foil?), defect classes (which D1-D6?), access
  geometry (one-sided / two-sided?), required POD threshold.
- **`market_research`** — target geography (EU? DACH? global?), market
  segment definition (mid-market vs. enterprise threshold?), reference
  year for sizing.
- **`competitive_analysis`** — explicit competitor list (the manager
  does not invent competitors), capability axes that matter for the
  user's decision.
- **`technology_screening`** — the screening question itself if the
  topic was vague, the criteria the operator considers binding.
- **`whitepaper`** — the thesis statement. A whitepaper without a
  thesis is the canonical blocking case. Without a falsifiable thesis,
  the writer either invents one (rejected) or hedges into nothing
  (rejected by `LINT-WP-THESIS-DRIFT`).
- **`literature_review`** — scope window (year range), explicit
  inclusion / exclusion criteria, themes the operator considers
  central.
- **`decision_brief`** — criteria + weights, the explicit decision the
  brief must answer, the default option if the criteria are tied.

If you do not have an answer and the question carries
`allow_fallback: true`, you can supply a deliberate "we accept the
default" answer. If `allow_fallback: false`, the run cannot continue
without a real answer — supply one or abort.

## release_guard_check fails repeatedly

Symptom: `ctox report status RUN_ID --json | jq '.checks.release_guard'`
reports `ready_to_finish: false` and `reasons[]` carries one or more
lint codes. The manager has already attempted at least one revision; the
lint is sticky. Below are the lints operators see most often, with the
typical cause and the targeted fix.

Cross-reference: every lint code lives in `references/release_guard_lints.md`
with full prose, applicability matrix, and corrective-goal phrasing.

### LINT-FAB-DOI / LINT-FAB-ARXIV — fabricated DOI or arXiv id

Cause: the writer cited a DOI / arXiv id that did not come from a
`public_research` result. Fix: the manager should re-run
`public_research` for the offending block's evidence axis, then re-call
`revise_with_skill` with a goal naming the fabricated ids and the
replacement reference_ids.

If the run keeps generating fab-DOIs after two retries, the topic is
research-empty and the writer is filling the void with plausible
strings. Abort and broaden the topic.

### LINT-UNCITED-CLAIM — quantitative claim without source

Cause: the writer produced a sentence with a number, unit, or method-
specific factual statement that has no entry in `used_reference_ids[]`
on the block. Fix: revise the offending block with goal of either
attaching an evidence_id or removing the unsourced number. Form-only
revision is wrong here — this is a fact problem, not a form problem.

### LINT-CITED-BUT-MISSING — dangling reference id

Cause: the writer cited an evidence_id that is not in the workspace
provenance. Usually a copy-paste from an earlier run, or a
hallucinated id with the right shape. Fix: revise with a goal naming
the dangling id and the correct id (or removal).

### LINT-DOI-NOT-RESOLVED — register entry not in Crossref

Cause: a DOI cited in the evidence register did not pass Crossref
resolution at research time. The CTOX research tool should have dropped
the source before it reached the writer; if it still landed in a block,
the operator can verify with the bundled helper:

```bash
python3 .../scripts/doi_resolve.py 10.3390/example.id
```

If `resolver_used: "none"`, the DOI is invalid and the source must be
dropped. Revise with a goal of removing the dangling citation.

### LINT-EVIDENCE-FLOOR — register too small for depth profile

Cause: the evidence register has fewer entries than the depth profile
requires. `orienting` requires >= 6, `standard` >= 12,
`decision_grade` >= 20. The lint fires when the writer hit
`min_evidence_count` during the evidence pre-phase but several sources
were later dropped (Crossref resolution failure, deduplication, etc.).
Fix: re-run `public_research` for the under-covered axes; the manager
will pick this up automatically and re-attempt the gate. If the run
keeps re-firing this lint, see `Manager keeps calling public_research`.

### LINT-EVIDENCE-CONCENTRATION — single source dominates

Cause: more than 40% of the citations in the report point at the same
evidence_id. Fix: revise with a goal of redistributing citations to the
other sources in the workspace. Form-only is wrong; the goals must name
specific blocks where alternative evidence_ids should be cited.

### LINT-DEAD-PHRASE — dead Foerderdeutsch

Cause: the writer used one of the phrases in
`style_guidance.dead_phrases_to_avoid[]` ("im Folgenden werden",
"vor diesem Hintergrund", etc.). Fix: form-only revision with a goal
naming the offending phrase and the block it appears in.

### LINT-META-PHRASE — forbidden meta-/Akten-Sprache

Cause: the writer used one of the phrases in
`style_guidance.forbidden_meta_phrases[]` ("nach dem vorliegenden
Kontext", "soweit beigefuegt", etc.). Same fix as LINT-DEAD-PHRASE.

### LINT-DUPLICATE-RATIONALE — copy-paste rationales in matrix cells

Cause: two cells of the same option in a matrix block carry identical
rationale strings. This is the canonical slop signature that killed the
predecessor skill. Fix: revise with a goal naming the option, the two
criteria with duplicated rationales, and the differential evidence
each rationale should anchor on. Specifically applicable to
`feasibility_study`, `competitive_analysis`, `technology_screening`,
`decision_brief` (matrix-heavy types).

### LINT-VERDICT-MISMATCH — detail verdict differs from matrix cell

Cause: the matrix says `Eddy Current = mittel` but the detail
assessment block writes "Erfolgsaussichten (qualitativ): hoch …". One
of the two is wrong. Fix: revise with a goal naming the offending
option and which side the operator wants to keep (matrix or detail).
Applicable only to types with `verdict_line_pattern != null`.

### LINT-MISSING-DISCLAIMER — scope_disclaimer lacks required substrings

Cause: the scope_disclaimer block was generated but does not contain
the verbatim disclaimer substrings the asset pack mandates. Fix: revise
the `scope_disclaimer` block with a goal of including the required
substrings (the goal text comes verbatim from the lint reason).

### LINT-INVERTED-PERSPECTIVE — first-person plural in third-person register

Cause: the writer slipped into "Wir empfehlen …" / "We believe …"
language. Fix: form-only revision with a goal of switching the
offending sentences back to neutral third-person. The recommendation
block is permitted to use active voice; nothing else is.

### LINT-WP-THESIS-DRIFT — thesis lacks a single position

Cause (whitepaper-only): the thesis block lists multiple positions or
hedges instead of taking one. Fix: revise the thesis block with a goal
naming the single position the operator wants — supply it via
`ctox report answer` if the thesis is genuinely ambiguous, then
re-attempt.

### LINT-DB-HEDGE-RECOMMENDATION — decision_brief hedges

Cause (decision_brief-only): the `recommendation_brief` block does not
take a position. Decision briefs without a recommendation are
structurally broken; either the operator did not supply a real
decision question, or the writer flinched. Fix: revise with a goal of
"recommend / recommend with caveats / not recommended" plus the
explicit conditions, sourced from `verdict_vocabulary[]` for
`decision_brief`.

### LINT-LR-NO-GAPS-SECTION — literature_review lacks gaps section

Cause (literature_review-only): the run produced themes and synthesis
but skipped `gaps_and_open_questions`. The block is required in the
literature_review blueprint. Fix: revise the run to include the
missing block; if the writer cannot produce it (no real gaps in the
corpus), the run was probably scoped wrong — abort and re-scope.

### When a lint will not clear

If the same `release_guard_check` lint fires three rounds in a row
after revision, abort. The skill's `When to abort instead of revising`
contract names this as a hard stop. Looping more does not converge —
the issue is structural (missing block, wrong evidence base, wrong
report_type). Run:

```bash
ctox report abort RUN_ID --reason "LINT-XXX did not clear after 3 revisions"
```

Then re-scope the run.

## narrative_flow_check keeps marking the same block

Symptom: `narrative_flow_check.reasons[]` keeps naming the same
`instance_id` after every revision attempt. Three rounds = abort.

Causes:
- A required block is missing earlier in the arc, so the offending
  block reads as a non-sequitur. Run `completeness_check`; if any
  required instance_id appears in `missing_required[]`, fix that
  first.
- The report_type is wrong for the topic. A `whitepaper` arc on a
  topic that genuinely needs a feasibility study reads as a
  jumble — abort and re-create the run with the right
  `report_type_id`.
- The evidence in the offending category is empty. The flow review
  marks "no transition possible" when the writer had nothing to
  bridge with. Re-run `public_research` on that axis or re-scope.

Don't keep iterating on form-only fixes when narrative_flow keeps
flagging the same block. The structural root cause is upstream of
prose.

## DOCX rendering fails

Symptom: `ctox report render RUN_ID --format docx --out report.docx`
errors out, or produces a 0-byte file, or produces a file Word will not
open.

- **`python-docx` not installed.** Symptom: stderr says `python-docx not
  installed. Run: python3 -m pip install python-docx`. Fix:

  ```bash
  python3 -m pip install --user "python-docx>=1.1"
  ```

  The renderer is a Python subprocess; it does not bundle pip
  dependencies and will not install them silently.

- **LibreOffice not installed.** The DOCX renderer itself does not need
  `soffice`, but the visual review (`render_check.py`) does. Symptom:
  `render_check.py` exits with code 2 and prints an install hint:

  ```bash
  brew install libreoffice           # macOS
  sudo apt-get install libreoffice   # Ubuntu / Yoda
  ```

- **Manuscript JSON malformed.** Symptom: `render_manuscript.py` errors
  with `KeyError: 'docs'` or similar. The render path is:

  ```bash
  ctox report render RUN_ID --format json --out manuscript.json
  python3 -c 'import json,sys; json.load(open(sys.argv[1]))' manuscript.json
  ```

  If `python -c` reports a JSON error, the run state is corrupted —
  `ctox report abort` and re-run.

- **Forbidden meta-phrase in output.** The renderer emits a stderr
  warning when it detects a phrase from `style_guidance.forbidden_meta_phrases[]`
  in the manuscript text. The warning does not block rendering — the
  DOCX is still written — but it tells you `release_guard_check` should
  have caught the phrase and did not. If you see warnings like this in
  production, the lint suite is mis-configured for the run; file a
  skill-maintainer ticket. The render still proceeds so the operator can
  hand-edit if the run is otherwise sound.

- **Hyphen / dash drift.** The renderer normalises Unicode hyphens and
  dashes to ASCII before saving. If a hand-edited DOCX is the source,
  the script silently rewrites the chars. If you specifically want the
  Unicode em-dash, edit the DOCX after the renderer finishes.

## Run finishes with `decision: "blocked"` after `"finished"`

Symptom: the manager's last message in the log says
`emitting decision: "finished"` and the next line — from the host loop-
end gate — says `downgrading to "blocked": <check> on <report_type_id>`.
This is the host overriding the LLM. The override is intentional; it is
the entire point of the four binding gates.

Inspect which check failed:

```bash
ctox report status RUN_ID --json | jq '.checks'
```

Exactly one of the four will fail (or be absent if the gate never ran).
The mapping from failing check to action:

- **`completeness` not ready** — the LLM declared finished without
  every required block being committed. Edge case; usually means
  `apply_block_patch` failed on the last packet. Re-run
  `ctox report continue` — the manager will retry the missing
  `instance_ids`.

- **`character_budget` `severely_off_target: true`** — the report is
  more than 30% above or below `target_chars`. Fix:

  ```bash
  ctox report revise RUN_ID --form-only \
      --instance-id <largest_block> \
      --goal "Auf <target> Zeichen kuerzen ohne Faktenverlust."
  ```

  See `style_guidance.character_budget_check.adjustment_hint` in the
  workspace snapshot for the specific delta.

- **`release_guard` `ready_to_finish: false`** — see
  `release_guard_check fails repeatedly` above. Read the lint code in
  `reasons[]`, apply the targeted fix.

- **`narrative_flow` `ready_to_finish: false`** — see
  `narrative_flow_check keeps marking the same block` above. Three
  consecutive same-block flags = abort.

The host loop-end gate's `summary` field always names the failing
check qualified by `report_type_id`, so the message
`character_budget_check (severely_off) on feasibility_study` tells you
exactly what to fix. The mapping is in
`references/check_contracts.md > Loop-end host gate`.

## What to do when an entire report type stays unusable

Symptom: every `<report_type>` run for a given topic ends in `blocked`
no matter how the topic is reformulated. Three rounds is the cliff.

Operator escalation:

```bash
ctox report abort RUN_ID --reason "<concise structural cause>"
```

Then file a structured note for the skill maintainer. The note should
contain:

1. The `report_type_id` the runs targeted.
2. Three example topics that all blocked.
3. The failing check / lint code for each.
4. Whether `public_research` returned non-empty results (if not, the
   issue is corpus-side, not skill-side).
5. The rendered DOCX of the closest run, even if blocked.

Aborting a run is cheap. Iterating on a structurally bad run is not.
If you have already burned three revision rounds on the same lint or
the same instance_id without progress, abort and re-scope at the
operator level — do not let the manager keep retrying.

## Diagnostic helpers that ship with the skill

These three scripts live in
`/Users/michaelwelsch/Documents/ctox/skills/system/research/systematic-research/scripts/`
and are intended to be run by the operator from the command line:

- **`doi_resolve.py`** — resolves one or more DOIs through Crossref
  with OpenAlex fallback. Use it to verify outbound HTTPS works and
  that a specific DOI is real. Example:

  ```bash
  python3 scripts/doi_resolve.py 10.3390/coatings9110727 \
                                 10.1080/17686733.2024.2448049
  ```

  Stdlib-only; no extra packages needed.

- **`render_manuscript.py`** — turns a manuscript JSON (on stdin) into
  a DOCX. The Rust render module calls this script as a subprocess;
  the operator can invoke it directly to debug rendering:

  ```bash
  ctox report render RUN_ID --format json --out manuscript.json
  cat manuscript.json | python3 scripts/render_manuscript.py --out report.docx
  ```

- **`render_check.py`** — converts a DOCX to per-page PNGs via
  LibreOffice and pdftoppm, for visual review:

  ```bash
  python3 scripts/render_check.py --docx report.docx --out-dir /tmp/rc
  ```

All three scripts emit explicit install-hint messages when their
optional dependencies are missing; none silently swallow errors.
