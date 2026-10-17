---
name: parser
description: Parse the extracted text of one CV/résumé PDF into the unified CTOX print profile (ctox.cv_print_profile.v1) for the cv-print-builder app. Use when the cv-print-builder app queues a parse task; reply with exactly one JSON object, the app stores it itself.
---

# CV Print Parser (cv-print-builder)

Quelle: `src/skills/packs/business/ctox-cv-print-parser/SKILL.md` und die frühere Vertragsdoku
`modules/cv-print-builder/PARSER_SKILL.md`, zusammengeführt und auf den App-Ablauf (Shell v4, §16.30) zugeschnitten.

You turn an unstructured CV/résumé document into the normalized qualification-profile model used by the
Business OS `cv-print-builder` app. The output shape mirrors the NinjaWorkflowTool qualification profile print view
(`NinjaWorkflowTool_Extension/executions/find-job-for-candidate/view/printView.js`) and candidate schema
(`NinjaWorkflowTool_Extension/data/jobmatchSchema.js`).

## When this runs

The app command `ctox.cv_print.apply_parse` reads the source PDF from Business OS `files`, extracts bounded text
through the CTOX PDF stack and queues this task. The instruction carries a `CV PDF extracted text` section plus the
command reference (`command_id`, `document_id`, `version_id`, `source_file_id`). When you finish, CTOX hands your
reply to the app (`ctox.cv_print.apply_parse.done`), which creates the new `document_versions` version and patches
the `documents` record. A failed or cancelled task reaches the app as `ctox.cv_print.apply_parse.failed`.

## Inputs

- Use the bounded `CV PDF extracted text` section as the primary and only source.
- Never use HTTP fallbacks, external services, or Ninja Workflow services.
- Do not call tools or reopen the PDF when `CV PDF extracted text` is present. The text was already extracted
  through the CTOX PDF stack before the task was queued.
- If the extracted-text section is missing or obviously corrupt, return a minimal review profile with a warning
  diagnostic instead of starting a long tool-driven fallback.

## What to do

1. Use the extracted text provided by the CTOX PDF stack.
2. Structure it into the `ctox.cv_print_profile.v1` qualification profile (see Output). Normalize German CV
   conventions (tabellarischer Lebenslauf, Ausbildung, Zeugnisse, Sprachen, Skills). Split education vs.
   experience, dedupe, and keep dates.
3. Record every uncertain or missing field in `workflow.diagnostics` rather than inventing values.
4. Return minified JSON and avoid raw CV dumps, but preserve all clearly extracted stations and skill groups.
   Do not reduce the profile to a tiny preview.

## Output — return ONLY this JSON object, nothing else

Respond with a single JSON object that is the complete `model_json`. No Markdown, no prose, no code fence. The app
parses your reply and persists it.

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

Field groups in `candidate.additional`:

- `cv.experience`: rows with `job_title`, `employer`, `location`, `start_date`, `end_date`, `job_description[]`
- `cv.education`: rows with `degree`, `institution`, `major`, `specialization`, `location`, `start_date`,
  `end_date`, `details[]`
- `cv.skills`: grouped object, for example `Fachkenntnisse[]`, `Sprachkenntnisse[]`, `Weitere Fähigkeiten[]`
- `cv.meta`: `birthDate`, `nationality`, `highestDegree`, `degree`, `availabilityFrom`, `languages[]`,
  `source_filename`

## What the app does with your reply

- extracts the first complete JSON object from the reply,
- sets `schema` (`ctox.cv_print_profile.v1`), `workflow.phase = "review"`, `workflow.view_mode = "split"`,
  `workflow.command_id`, the `source` block (file reference, filename, MIME type, size, sha256) and missing `print`
  defaults,
- writes a new `document_versions` record (incremented `version`, `source_kind = "cv_pdf_parse"`,
  `model_json` = your object) and patches the `documents` record (`current_version_id`, `status = "review"`,
  `display_cache.phase`, `display_cache.candidate_name`, `index_text`, `title`).

## Hard rules

- Output is exactly one JSON object — your reply IS the data.
- Keep `schema = "ctox.cv_print_profile.v1"` and `workflow.phase = "review"`.
- Do not write records, call Business OS commands, the CTOX CLI or MCP yourself; the app persists the result.
