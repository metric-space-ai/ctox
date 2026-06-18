# CTOX CV Print Parser — module contract

The `cv-print-builder` module starts parsing by dispatching a
`business_os.chat.task` (`payload.skill = "ctox-cv-print-parser"`). The skill
runs in the harness, reads the source PDF through the CTOX data plane + PDF
stack, and returns the structured profile. The native writeback
`ctox.cv_print.apply_parse` (in `src/core/business_os/store.rs`) persists that
result — the skill itself does not write RxDB.

The skill must be self-contained: no external services, no HTTP fallbacks, and
no machine-local reference paths. The data shape below is the canonical
contract.

## Input (task payload)

- `document_id`
- `version_id`
- `source_file_id` / `attachments[0].file_id` (`kind = "desktop_file"`) — the
  source PDF, stored in `desktop_files` + `desktop_file_chunks`
- `writeback_contract = { command_type: "ctox.cv_print.apply_parse",
  target_collection: "document_versions", document_id, expected_model_schema:
  "ctox.cv_print_profile.v1" }`

## Skill output

A single JSON object — the complete `model_json` — and nothing else:

- `schema = "ctox.cv_print_profile.v1"`
- `workflow.phase = "review"`
- `workflow.diagnostics` — list of `{ level, message }` for uncertain/missing
  fields
- `candidate` — `name`, `firstName`, `lastName`, `currentRole`, `location`,
  `email`, … plus `candidate.additional` keyed:
  - `cv.education`
  - `cv.experience`
  - `cv.skills`
  - `cv.meta`

## Native writeback (`ctox.cv_print.apply_parse`)

The native handler applies the skill's JSON:

- creates a new `document_versions` record (incremented `version`,
  `source_kind = "cv_pdf_parse"`, `blob_id` = source desktop file id,
  `model_json` = the skill output)
- patches the `documents` record: `current_version_id` → new version,
  `status = "review"`, `display_cache.phase = "review"`,
  `display_cache.candidate_name`, `index_text`
