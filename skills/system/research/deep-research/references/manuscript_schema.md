# Manuscript v1 schema

The deterministic intermediate produced by `ctox report draft` and consumed by `ctox report render`.

```json
{
  "schema": "ctox.report.manuscript/v1",
  "run_id": "run_...",
  "preset": "feasibility",
  "language": "en",
  "title": "Feasibility Study",
  "subtitle": "Topic phrase from `report new`",
  "version_label": "Working draft | <ISO timestamp>",
  "scope": {
    "leading_questions": ["..."],
    "out_of_scope": ["..."],
    "assumptions": ["..."],
    "disclaimer_md": "Scope and limitations: ...",
    "success_criteria": ["..."]
  },
  "sections": [
    {
      "id": "management_summary",
      "heading_level": 1,
      "heading": "Management Summary",
      "blocks": [
        { "kind": "bullets", "items": [
            {
              "text_md": "...",
              "evidence_ids": ["ev_..."],
              "primary_recommendation": false,
              "assumption_note_md": null,
              "scenario_code": null
            }
        ]}
      ]
    }
  ],
  "citation_register": [
    {
      "evidence_id": "ev_...",
      "display_index": 1,
      "citation_kind": "doi",
      "canonical_id": "10.xxxx/...",
      "title": "...",
      "authors": ["..."],
      "venue": "...",
      "year": 2025,
      "landing_url": "https://doi.org/...",
      "full_text_url": null
    }
  ]
}
```

## Block kinds

- `paragraph` — `{ text_md, evidence_ids[] }`
- `bullets` — `{ items: [BulletItem] }`
- `numbered` — same shape as bullets
- `options_table` — `{ options: [{code, label, summary_md}] }`
- `requirements_table` — `{ rows: [{code, title, must_have, description_md}] }`
- `matrix_table` — `{ matrix_kind, label, axes: [{code, label}], rows: [{option_code, option_label, cells: [...]}] }`
- `scenario_block` — `{ code, label, description_md }`
- `risk_register` — `{ rows: [{code, title, description_md, mitigation_md, likelihood, impact, evidence_ids[]}] }`
- `citation_register` — sentinel; renderer materializes the register from the top-level `citation_register` array
- `note` — `{ text_md }`

## Section IDs (feasibility preset)

In document order:

1. `title_block`
2. `scope_disclaimer`
3. `management_summary` (claims-driven)
4. `context_and_question` (claims-driven; deterministic blocks for leading questions and scenarios prefixed)
5. `requirements`
6. `options_overview`
7. `main_matrix`
8. `scenario_matrix`
9. `detail_assessment` (claims-driven, requires ≥1 claim per option)
10. `risks`
11. `recommendation` (claims-driven, requires exactly one `primary_recommendation = true`)
12. `appendix_sources`

A section that has nothing to render is omitted by the draft assembler. Do not paper over emptiness with prose.
