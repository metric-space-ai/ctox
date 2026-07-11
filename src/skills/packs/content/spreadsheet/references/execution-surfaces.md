# Spreadsheets: operation-to-surface map and gating status

Source of truth for feature status:
`src/apps/business-os/office-engine/features.json`. A task class is usable
when its feature group is `shipped`; `differential_passed` allows use behind
the same rollout flag as the editor itself. Update this file when statuses
change; the skill must not promise operations the build cannot perform.

## Editor surface

| Operation class | Feature group | Status (2026-07-11, re-baselined) |
|---|---|---|
| Open workbook, render sheets/ranges (visual QA) | `spreadsheet.open-render-sheets` | oracle_captured |
| Cell edits, save | `spreadsheet.edit-save` | oracle_captured |
| Undo, clipboard, fill | `spreadsheet.undo-clipboard-fill` | oracle_captured |
| Cell formats, rows/columns | `spreadsheet.cell-format-rows-columns` | oracle_captured |
| Formulas and references | `spreadsheet.formulas-references` | oracle_captured |
| Multi-sheet, merge, freeze | `spreadsheet.multi-sheet-merge-freeze` | oracle_captured |
| Sort, filter, tables | `spreadsheet.sort-filter-tables` | oracle_captured |
| Validation, conditional formatting | `spreadsheet.validation-conditional-formatting` | discovered |
| Comments, names, protection | `spreadsheet.comments-names-protection` | discovered |
| Charts | `spreadsheet.charts` | discovered |
| Pivot, print layout | `spreadsheet.pivot-print-layout` | discovered |
| XLSX round-trip corpus | `spreadsheet.xlsx-roundtrip-corpus` | discovered |

Verify with `node src/scripts/check-office-skill-gating.mjs` after edits.

## Package operations (ctox-office-engine)

| Operation | Op | Status |
|---|---|---|
| Inspect package (sheets, parts, structure) | `inspect` | available |
| Export (byte-preserving round-trip/merge) | `export` | available |
| Workbook data extraction (values/formulas as data) | planned read API analog to inspect | planned |

## Practical consequence

The feature matrix was re-baselined on 2026-07-11: previously
differential-passed groups are back to `oracle_captured` pending
re-validation. Until groups pass differential acceptance again, the editor
surface must be treated as unavailable; the usable surface is the package
level — `inspect`, `export`, and the batch ops. Report editor-dependent
authoring as blocked.

Not ported from the Codex lineage: live control of a running Excel instance
(Office.js) — conceptually out of scope for CTOX engines.

See `docs/ctox-office-skills-adaptation-plan.md` for the adaptation rationale.
