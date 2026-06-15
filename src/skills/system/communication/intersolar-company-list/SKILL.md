---
name: intersolar-company-list
description: Use when the deliverable is the COMPLETE Intersolar exhibitor/company list as an .xlsx with a PLZ column. Declares the spreadsheet deliverable contract (minimum row count + required columns) that the review gate enforces by hard keys instead of a hardcoded company literal.
cluster: communication
deliverable_format: xlsx
min_data_rows: 200
required_columns: PLZ
---

# Intersolar Company List

## Deliverable Contract

This skill's deliverable is the COMPLETE Intersolar exhibitor/company list as a
single `.xlsx` workbook. The review gate enforces the contract below by hard keys
(NOT a company-name literal): a workbook short of the row count, or missing a
required column, is a deterministic review failure.

- `min_data_rows: 200` — at least 200 data rows (excluding the header row).
- `required_columns: PLZ` — a `PLZ` (postal code) column header must be present.

Rebuild the workbook from the full source set, verify the row count and the PLZ
column, then rerun the reviewed send.

## CTOX Runtime Contract

- The Review Gate is a quality checkpoint, not a control loop. After review
  feedback, continue the same main work item whenever possible and incorporate
  the feedback there; do not create review-driven internal work cascades.
- Every durable follow-up, queue item, plan emission, or internal work item must
  have a clear parent/anchor: message key, work id, thread key, ticket/case id,
  or plan step.
- Task spawning is allowed only for real bounded work steps that add mission
  progress, external waiting, recovery, or explicit decomposition.
