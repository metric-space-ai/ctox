---
name: ctox-cv-print-parser
description: Parse a source CV/résumé document into the unified CTOX print profile (ctox.cv_print_profile.v1) for the Business OS cv-print-builder module.
class: installed_packs
state: stable
cluster: business
---

# CTOX CV Print Parser

You turn an unstructured CV/résumé document into the normalized
qualification-profile model used by the Business OS `cv-print-builder` module.
The output shape mirrors the NinjaWorkflowTool qualification profile print view
(`NinjaWorkflowTool_Extension/executions/find-job-for-candidate/view/printView.js`)
and candidate schema (`NinjaWorkflowTool_Extension/data/jobmatchSchema.js`).

## When this runs

The `cv-print-builder` module dispatches a `business_os.chat.task` with
`payload.skill = "ctox-cv-print-parser"`. The task payload carries
`document_id`, `version_id`, `source_file_id` (a `desktop_files` id), and a
`writeback_contract` describing how the result is persisted. The CTOX daemon
reconstructs the PDF from the Business OS file chunks and adds a bounded
`CV PDF extracted text` section to the queue prompt before this skill runs.

## Inputs

- The source PDF is stored in the Business OS data plane as `desktop_files` +
  `desktop_file_chunks`, keyed by `source_file_id`.
- Use the bounded `CV PDF extracted text` prompt section as the primary source.
  Never use HTTP fallbacks, external services, or Ninja Workflow services.
- Do not call tools or reopen the PDF when `CV PDF extracted text` is present.
  The daemon already reconstructed and extracted the document through the CTOX
  PDF stack.
- If the extracted-text section is missing or obviously corrupt, return a
  minimal review profile with a warning diagnostic instead of starting a long
  tool-driven fallback.

## What to do

1. Use the extracted text provided by the CTOX PDF stack.
2. Structure it into the `ctox.cv_print_profile.v1` qualification profile (see Output).
   Normalize German CV conventions (tabellarischer Lebenslauf, Ausbildung,
   Zeugnisse, Sprachen, Skills). Split education vs. experience, dedupe, and
   keep dates.
3. Record every uncertain or missing field in `workflow.diagnostics` rather than
   inventing values.
4. Return minified JSON and avoid raw CV dumps, but preserve all clearly
   extracted stations and skill groups. Do not reduce the profile to a tiny
   preview.

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
    "currentRole": "...", "location": "...",
    "availability": "...", "email": "...", "phone": "...",
    "birthDate": "...", "nationality": "...",
    "highestDegree": "...", "degree": "...",
    "skills": [ "..." ],
    "languages": [ { "label": "...", "level": "..." } ],
    "additional": [
      { "key": "cv.experience", "label": "Berufserfahrung (CV)", "type": "json", "value": [ { "job_title": "...", "employer": "...", "location": "...", "start_date": "...", "end_date": "...", "job_description": [ "..." ] } ] },
      { "key": "cv.education", "label": "Ausbildung (CV)", "type": "json", "value": [ { "degree": "...", "institution": "...", "major": "...", "specialization": "...", "location": "...", "start_date": "...", "end_date": "...", "details": [ "..." ] } ] },
      { "key": "cv.skills", "label": "Skills (CV)", "type": "json", "value": { "Fachkenntnisse": [ "..." ], "Sprachkenntnisse": [ "..." ], "Weitere Fähigkeiten": [ "..." ] } },
      { "key": "cv.meta", "label": "Stammdaten (CV)", "type": "json", "value": { "birthDate": "...", "nationality": "...", "highestDegree": "...", "degree": "...", "availabilityFrom": "...", "languages": [ { "label": "...", "level": "..." } ], "source_filename": "..." } }
    ]
  }
}
```

## Hard rules

- Output is exactly one JSON object — the harness reply IS the data.
- Keep `schema = "ctox.cv_print_profile.v1"` and `workflow.phase = "review"`.
- Do not write to RxDB yourself; the native writeback handler persists the new
  `document_versions` version and patches the `documents` record.
