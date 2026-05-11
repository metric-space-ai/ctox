# Technical Drawing Review JSON Schema

Use this shape for UI mockups, API payloads, or downstream annotation rendering.

```json
{
  "drawing": {
    "source": "drawing.pdf",
    "pages_reviewed": [1],
    "units": "mm",
    "part_number": null,
    "revision": null
  },
  "summary": {
    "overall_status": "review_needed",
    "finding_count": 3,
    "highest_severity": "major"
  },
  "findings": [
    {
      "id": "TD-001",
      "severity": "major",
      "category": "tolerance",
      "title": "Functional hole spacing lacks explicit tolerance",
      "evidence": "The hole pattern is dimensioned, but no local tolerance or applicable general tolerance note is visible.",
      "risk": "Supplier may apply an unintended default tolerance, causing assembly mismatch.",
      "recommendation": "Add a specific tolerance or confirm the governing general tolerance standard in the title block.",
      "pin": {
        "page": 1,
        "x": 0.42,
        "y": 0.36,
        "anchor": "front_view_hole_pattern"
      },
      "confidence": 0.78,
      "status": "open"
    }
  ]
}
```

Allowed `overall_status` values:

- `review_needed`
- `accepted_with_comments`
- `no_actionable_findings`
- `blocked_missing_context`

Allowed `status` values:

- `open`
- `needs_context`
- `resolved`
- `false_positive`

Use `needs_context` when a concern depends on unavailable design intent, process capability, or supplier standard.
