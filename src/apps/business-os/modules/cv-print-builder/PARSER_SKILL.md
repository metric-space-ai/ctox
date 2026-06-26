# CTOX CV Print Parser — module contract

The `cv-print-builder` module starts parsing by dispatching a
`business_os.chat.task` (`payload.skill = "ctox-cv-print-parser"`). The CTOX
daemon reconstructs the source PDF through the CTOX data plane + PDF stack,
adds bounded extracted text to the queue prompt, and the skill returns the
structured profile. The native writeback
`ctox.cv_print.apply_parse` (in `src/core/business_os/store.rs`) persists that
result — the skill itself does not write RxDB.

The skill must be self-contained: no external services, no HTTP fallbacks, and
no Ninja Workflow services at runtime. The data shape below intentionally
matches the NinjaWorkflowTool qualification profile model used by
`NinjaWorkflowTool_Extension/executions/find-job-for-candidate/view/printView.js`
and `NinjaWorkflowTool_Extension/data/jobmatchSchema.js`.

## Input (task payload)

- `document_id`
- `version_id`
- `source_file_id` — the source PDF, stored in `desktop_files` +
  `desktop_file_chunks`
- Queue prompt section `CV PDF extracted text` — bounded text extracted by the
  daemon before the skill call; this is the primary parse input
- Do not call tools or reopen a local PDF path when `CV PDF extracted text` is
  present. The daemon has already reconstructed and extracted the PDF through
  the CTOX PDF stack.
- If the extracted text is missing or corrupt, return a minimal review profile
  with a warning diagnostic instead of starting a long fallback extraction.
- `writeback_contract = { command_type: "ctox.cv_print.apply_parse",
  target_collection: "document_versions", document_id, expected_model_schema:
  "ctox.cv_print_profile.v1" }`

## Skill output

A single JSON object — the complete `model_json` — and nothing else:

- `schema = "ctox.cv_print_profile.v1"`
- `workflow.phase = "review"`
- `workflow.diagnostics` — list of `{ level, message }` for uncertain/missing
  fields
- `candidate` — short fields (`name`, `firstName`, `lastName`, `currentRole`,
  `location`, `availability`, `email`, `phone`, `birthDate`, `nationality`,
  `highestDegree`, `degree`), plus `skills`, `languages`, and
  `candidate.additional` as an array of `{ key, label, type, value }` entries:
  - `cv.experience`: rows with `job_title`, `employer`, `location`,
    `start_date`, `end_date`, `job_description[]`
  - `cv.education`: rows with `degree`, `institution`, `major`,
    `specialization`, `location`, `start_date`, `end_date`, `details[]`
  - `cv.skills`: grouped object, for example `Fachkenntnisse[]`,
    `Sprachkenntnisse[]`, `Weitere Fähigkeiten[]`
  - `cv.meta`: `birthDate`, `nationality`, `highestDegree`, `degree`,
    `availabilityFrom`, `languages[]`, `source_filename`

Keep output minified and avoid raw CV dumps, but preserve all clearly extracted
stations and skill groups. Do not artificially truncate the profile to a tiny
preview.

## Native writeback (`ctox.cv_print.apply_parse`)

The native handler applies the skill's JSON:

- creates a new `document_versions` record (incremented `version`,
  `source_kind = "cv_pdf_parse"`, `blob_id` = source desktop file id,
  `model_json` = the skill output)
- patches the `documents` record: `current_version_id` → new version,
  `status = "review"`, `display_cache.phase = "review"`,
  `display_cache.candidate_name`, `index_text`
