# CTOX CV Print Parser Skill Contract

This module starts parsing through `business_os.chat.task` and expects a CTOX
skill named `ctox-cv-print-parser`.

The parser must not call the NinjaWorkflowTool services. The Ninja project is a
local reference for the CV data shape and normalisation rules only:

- `/Users/michaelwelsch/Documents/NinjaWorkflowTool/NinjaWorkflowTool_Extension/bg/service_worker.js`
- `buildCandidateDocFromCvJson`
- `parseCvLanguagesFromSkills`
- `parseCvSkillsArray`
- `reclassifyCvEducationExperience`

## Input

The task payload contains:

- `document_id`
- `version_id`
- `source_file_id`
- `attachments[0].file_id` with `kind = "desktop_file"`
- `ninja_reference_file`

The source PDF is stored in `desktop_files` plus `desktop_file_chunks`. The
skill should read it through the CTOX Business OS data plane and the existing
CTOX PDF stack.

## Output

Create a new `document_versions` record with:

- `document_id` unchanged
- incremented `version`
- `source_kind = "cv_pdf_parse"`
- `blob_id` set to the source desktop file id or a parser artifact id
- `model_json.schema = "ctox.cv_print_profile.v1"`
- `model_json.workflow.phase = "review"`

Then patch the `documents` record:

- `current_version_id` to the new version id
- `status = "review"`
- `display_cache.phase = "review"`
- `display_cache.candidate_name`
- `index_text`

The `model_json.candidate` object should stay compatible with the structure
created by NinjaWorkflowTool's `buildCandidateDocFromCvJson`, especially the
`additional` entries:

- `cv.education`
- `cv.experience`
- `cv.skills`
- `cv.meta`
