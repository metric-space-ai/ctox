# Stage contracts

Every `ctox report` stage that mutates state takes a JSON payload via `--from-file`. Fields named here are the exact keys the deserializer expects. Optional fields are marked.

## scope

```json
{
  "leading_questions": ["Which contactless method best detects grid defects under coatings?", "..."],
  "out_of_scope": ["Cost analysis is out of scope.", "..."],
  "assumptions": ["Coupons are representative of the production layup.", "..."],
  "disclaimer_md": "Scope and limitations: ... validation is required before adoption.",
  "success_criteria": ["A primary recommendation backed by evidence."]
}
```

Disclaimer must contain at least one of: `assumption`, `validation`, `Annahme`, `Validierung`, `limitation`, `Einschränkung`, and be ≥120 characters.

## frame (requirements)

Array of:

```json
{
  "code": "R1",
  "title": "Single-sided access",
  "description_md": "Inspection must work from one side of the panel.",
  "must_have": true,
  "derived_from_question_idx": 0
}
```

## enumerate (options)

Array of:

```json
{
  "code": "ECT",
  "label": "Eddy Current Testing",
  "summary_md": "Induced currents in conductive grids; impedance change exposes defects.",
  "synonyms": ["eddy current", "ECT array"]
}
```

## evidence add

Array of:

```json
{
  "citation_kind": "doi",
  "canonical_id": "10.1016/j.compositesb.2024.111982",
  "title": "Eddy current pulsed thermography ...",
  "authors": ["Liu R.", "Xu C."],
  "venue": "Composites Part B",
  "year": 2025,
  "publisher": "Elsevier",
  "landing_url": "https://doi.org/10.1016/j.compositesb.2024.111982",
  "full_text_url": null,
  "abstract_md": "...",
  "snippet_md": "Inductive heating layer enables subsurface defect detection in GFRP...",
  "license": null,
  "resolver": "manual"
}
```

`citation_kind` must be one of `doi`, `arxiv`, `url`, `book`, `standard`, `assumption`.

## evidence import

CLI flags only — no JSON file. Runs `ctox_deep_research` and harvests DOIs/arXiv-IDs/URLs into `report_evidence`.

## scoring define-rubric

Array of:

```json
{
  "axis_code": "coverage",
  "level_code": "high",
  "level_definition_md": "high coverage = single-shot field of view ≥ 1 m² with <1 s capture time.",
  "numeric_value": 3.0
}
```

`level_definition_md` must be ≥16 characters.

## scoring set-cell

Array of:

```json
{
  "matrix_kind": "main",
  "matrix_label": null,
  "option_code": "ECT",
  "axis_code": "coverage",
  "value_label": "high",
  "rationale_md": "Array probes scan large areas at a few cm/s in published trials.",
  "evidence_ids": ["ev_..."],
  "assumption_note_md": null,
  "rubric_anchor": "rubric:coverage:high"
}
```

`rubric_anchor` must point at a rubric defined for this run (`rubric:<axis_code>:<level_code>`). If omitted but `value_label` matches an existing rubric level, the anchor is filled in automatically.

A cell without `evidence_ids` requires `assumption_note_md`.

## scenarios add

Array of:

```json
{
  "code": "A",
  "label": "Grid is the first metal layer",
  "description_md": "Default layup: grid embedded directly under primer.",
  "impact_summary_md": null
}
```

## risks add

Array of:

```json
{
  "code": "R1",
  "title": "Closed metal foil shielding",
  "description_md": "An additional foil above the grid blocks THz signals.",
  "mitigation_md": "Verify layup with cross-section samples; switch to inductive method.",
  "likelihood": "medium",
  "impact": "high",
  "evidence_ids": ["ev_..."]
}
```

## claims add

Array of:

```json
{
  "section_id": "management_summary",
  "text_md": "Eddy current testing is the most promising option for direct grid imaging.",
  "claim_kind": "finding",
  "evidence_ids": ["ev_..."],
  "assumption_note_md": null,
  "confidence": "high",
  "primary_recommendation": false,
  "scenario_code": null,
  "rubric_anchor": null
}
```

`claim_kind` ∈ {`finding`, `recommendation`, `caveat`, `assumption`, `scope_note`}. `finding`/`recommendation`/`caveat` MUST carry at least one `evidence_id`; `assumption`/`scope_note` MUST carry `assumption_note_md` if `evidence_ids` is empty.

`primary_recommendation: true` only allowed on `claim_kind = recommendation`. Exactly one such claim is required in section `recommendation`.

## critique --mode external

```json
{
  "summary_md": "External critique by reviewer X.",
  "findings": [
    {
      "id": "F001",
      "category": "substantive",
      "severity": "error",
      "location_path": "section:management_summary/bullet[1]",
      "evidence": "Claim asserts X but matrix cell shows Y.",
      "corrective_action": "Update claim text to align with the matrix cell rationale."
    }
  ]
}
```

`category` ∈ {`wording`, `substantive`, `stale`}; `severity` ∈ {`info`, `warn`, `error`}.

## revise

```json
{
  "from_version_id": "ver_...",
  "manuscript": { "schema": "ctox.report.manuscript/v1", "...": "..." },
  "notes_md": "Resolved findings F001 and F002."
}
```

`manuscript.run_id` must equal the run-id flag. `body_hash` must differ from the parent's.

## render

CLI flags only:

```
--format md|docx|json
--out <path>           (optional; defaults to runtime/reports/<run_id>/<version_id>.<ext>)
--version-id <id>      (optional; defaults to latest version)
--force-no-check       (override the check gate; do not use in production)
```
