# Spreadsheets: operation-to-surface map and gating status

Source of truth for feature status:
`src/apps/business-os/office-engine/features.json`. A task class is usable
when its feature group is `shipped`; `differential_passed` allows use behind
the same rollout flag as the editor itself. Update this file when statuses
change; the skill must not promise operations the build cannot perform.

## Editor surface

| Operation class | Feature group | Status (2026-07-11) |
|---|---|---|
| Open workbook, render sheets/ranges (visual QA) | `spreadsheet.open-render-sheets` | differential_passed |
| Cell edits, save | `spreadsheet.edit-save` | differential_passed |
| Undo, clipboard, fill | `spreadsheet.undo-clipboard-fill` | differential_passed |
| Cell formats, rows/columns | `spreadsheet.cell-format-rows-columns` | differential_passed |
| Formulas and references | `spreadsheet.formulas-references` | differential_passed |
| Multi-sheet, merge, freeze | `spreadsheet.multi-sheet-merge-freeze` | discovered |
| Sort, filter, tables | `spreadsheet.sort-filter-tables` | discovered |
| Validation, conditional formatting | `spreadsheet.validation-conditional-formatting` | discovered |
| Comments, names, protection | `spreadsheet.comments-names-protection` | discovered |
| Charts | `spreadsheet.charts` | discovered |
| Pivot, print layout | `spreadsheet.pivot-print-layout` | discovered |
| XLSX round-trip corpus | `spreadsheet.xlsx-roundtrip-corpus` | discovered |

## Package operations (ctox-office-engine)

| Operation | Op | Status |
|---|---|---|
| Inspect package (sheets, parts, structure) | `inspect` | available |
| Export (byte-preserving round-trip/merge) | `export` | available |
| Workbook data extraction (values/formulas as data) | planned read API analog to inspect | planned |

## Practical consequence

Core model authoring (values, formulas, references, cell/row/column formats)
has passed differential acceptance and is usable behind the editor rollout
flag, alongside reading, analyzing, rendering, and byte-preserving export.
Still blocked and to be reported as such: multi-sheet/merge/freeze,
sort/filter/tables, validation and conditional formatting, comments/names/
protection, charts, and pivot/print work.

Not ported from the Codex lineage: live control of a running Excel instance
(Office.js) — conceptually out of scope for CTOX engines.

See `docs/ctox-office-skills-adaptation-plan.md` for the adaptation rationale.
