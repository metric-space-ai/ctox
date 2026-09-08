# Spreadsheets: operation-to-surface map and gating status

Source of truth for feature status:
`src/apps/business-os/office-engine/features.json`. A task class is usable
when its feature group is `shipped`; `differential_passed` allows use behind
the same rollout flag as the editor itself. Update this file when statuses
change; the skill must not promise operations the build cannot perform.

## Editor surface

| Operation class | Feature group | Status |
|---|---|---|
| Open workbook, render sheets/ranges (visual QA) | `spreadsheet.open-render-sheets` | differential_passed |
| Cell edits, save | `spreadsheet.edit-save` | differential_passed |
| Undo, clipboard, fill | `spreadsheet.undo-clipboard-fill` | differential_passed |
| Cell formats, rows/columns | `spreadsheet.cell-format-rows-columns` | differential_passed |
| Formulas and references | `spreadsheet.formulas-references` | differential_passed |
| Multi-sheet, merge, freeze | `spreadsheet.multi-sheet-merge-freeze` | differential_passed |
| Sort, filter, tables | `spreadsheet.sort-filter-tables` | differential_passed |
| Validation, conditional formatting | `spreadsheet.validation-conditional-formatting` | differential_passed |
| Comments, names, protection | `spreadsheet.comments-names-protection` | differential_passed |
| Charts | `spreadsheet.charts` | differential_passed |
| Pivot, print layout | `spreadsheet.pivot-print-layout` | differential_passed |
| XLSX round-trip corpus | `spreadsheet.xlsx-roundtrip-corpus` | differential_passed |

Verify with `node src/scripts/check-office-skill-gating.mjs` after edits.

## Package operations (ctox office / ctox-office-engine)

| Operation | Op | Status |
|---|---|---|
| Inspect package (sheets, parts, structure) | `inspect` | available |
| Prepare source package as editor payload | `prepare-editor` | available |
| Inspect a prepared editor payload | `inspect-editor` | available |
| Export (byte-preserving round-trip/merge) | `export` | available |
| Workbook data extraction (values/formulas as data) | `read` | available; formula results are cached values |
| Typed cell batch with exact base SHA-256 and new output file | `spreadsheet-patch` | available; recalculation required |

The embedded `ctox office` command and standalone `ctox-office-engine` use the
same native implementation. `spreadsheet-patch` supports text, numbers,
booleans, formula text and clearing cells; it refuses protected worksheets,
array/shared/data-table formulas and duplicate targets. It is not a formula
evaluator. Recalculate dependent values in the editor before relying on them.
See `docs/native-office-cli.md` for the typed batch schema and limits.

These are local-file tools, not managed-record writeback commands. Persisting
Business OS versions still requires the server-authoritative command policy
and WebRTC/RxDB data path; do not report a local output as saved in the app.

## Practical consequence

All spreadsheet feature groups have passed differential acceptance. They are
usable only when the same typed rollout configuration that enables the CTOX
Spreadsheets editor is active. `differential_passed` is not equivalent to
global `shipped`: if the rollout selects the legacy engine or disables a
feature, report that editor-dependent operation as blocked. Package-level
`inspect`, `prepare-editor`, `inspect-editor`, and `export` remain available
independently of the browser rollout.

Not ported from the Codex lineage: live control of a running Excel instance
(Office.js) — conceptually out of scope for CTOX engines.

See `docs/ctox-office-skills-adaptation-plan.md` for the adaptation rationale.
