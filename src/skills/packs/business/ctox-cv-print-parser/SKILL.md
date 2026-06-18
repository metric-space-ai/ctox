---
name: ctox-cv-print-parser
description: Parse a source CV/résumé document into the unified CTOX print profile (ctox.cv_print_profile.v1) for the Business OS cv-print-builder module.
class: installed_packs
state: stable
cluster: business
---

# CTOX CV Print Parser

You turn an unstructured CV/résumé document into a single, normalized print
profile for the Business OS `cv-print-builder` module. This is a generic
document→structured-profile task: the recruiting specifics live entirely in the
output schema, not in this skill's mechanics.

## When this runs

The `cv-print-builder` module dispatches a `business_os.chat.task` with
`payload.skill = "ctox-cv-print-parser"`. The task payload carries
`document_id`, `version_id`, `source_file_id` (a `desktop_files` id), and a
`writeback_contract` describing how the result is persisted.

## Inputs

- The source PDF is stored in the Business OS data plane as `desktop_files` +
  `desktop_file_chunks`, keyed by `source_file_id` / `attachments[0].file_id`.
- Read it through the CTOX PDF stack and the RxDB/WebRTC data plane only. Never
  use HTTP fallbacks, external services, or machine-local file paths.

## What to do

1. Read and extract the text of the source PDF.
2. Structure it into the `ctox.cv_print_profile.v1` profile (see Output).
   Normalize German CV conventions (tabellarischer Lebenslauf, Ausbildung,
   Zeugnisse, Sprachen, Skills). Split education vs. experience, dedupe, and
   keep dates.
3. Record every uncertain or missing field in `workflow.diagnostics` rather than
   inventing values.

## Output — return ONLY this JSON object, nothing else

Respond with a single JSON object that is the complete `model_json`. No
Markdown, no prose, no code fence. The native writeback
`ctox.cv_print.apply_parse` parses your reply and persists it.

```
{
  "schema": "ctox.cv_print_profile.v1",
  "workflow": { "phase": "review", "diagnostics": [ { "level": "info|warn", "message": "..." } ] },
  "candidate": {
    "name": "...", "firstName": "...", "lastName": "...",
    "currentRole": "...", "desiredPosition": "...", "location": "...",
    "availability": "...", "email": "...",
    "additional": {
      "cv.education":  [ { "title": "...", "org": "...", "from": "...", "to": "...", "details": "..." } ],
      "cv.experience": [ { "title": "...", "org": "...", "from": "...", "to": "...", "details": "..." } ],
      "cv.skills":     [ { "label": "...", "level": "..." } ],
      "cv.meta":       { "languages": [ { "label": "...", "level": "..." } ], "source_filename": "..." }
    }
  }
}
```

## Hard rules

- Output is exactly one JSON object — the harness reply IS the data.
- Keep `schema = "ctox.cv_print_profile.v1"` and `workflow.phase = "review"`.
- Do not write to RxDB yourself; the native writeback handler persists the new
  `document_versions` version and patches the `documents` record.
