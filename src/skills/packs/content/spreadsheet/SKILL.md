---
name: "spreadsheet"
description: "Create, edit, analyze, and verify .xlsx workbooks through the CTOX spreadsheets engine (Euro-Office port). Formula-driven, auditable models with typed values, invariant number formats, and a mandatory visual verification pass. Authoring capabilities are gated on the engine's feature matrix; read/render/analyze paths are available first."
cluster: content
---

# Spreadsheet Skill (Create - Edit - Analyze - Verify)

Use this skill for workbook work in CTOX: building models and structured
sheets, editing existing workbooks without breaking their conventions,
answering questions about workbook contents, and verifying results both
numerically and visually before delivery.

## Execution contract (CTOX)

Spreadsheet work runs on CTOX's own engines. Do not use openpyxl, xlsxwriter,
pandas.ExcelWriter, Office.js, or any external workbook library, and do not
install dependencies at runtime. The execution surfaces are:

1. **Workbook read/render surface.** Inspect sheets, values, and formulas;
   render sheets or ranges to images for visual QA. Backed by the headless
   CTOX spreadsheets editor (`spreadsheet.open-render-sheets`,
   `spreadsheet.edit-save`).
2. **Editor flows (authoring).** Cell edits, formatting, formulas, charts,
   conditional formatting, comments, and protection run against the headless
   editor on the same code path users operate interactively. Most authoring
   feature groups are still in progress — check gating before promising work.
3. **Package export.** `ctox-office-engine export` produces the final `.xlsx`
   with byte-preserving round-trip of untouched parts.

Capability gating: operations bind to `spreadsheet.*` feature groups in
`src/apps/business-os/office-engine/features.json`. If a required group is
not shipped in this build, that is a blocker — report exactly what is missing
instead of falling back to external tooling. See
`references/execution-surfaces.md` for the operation map and current status.

Check `references/execution-surfaces.md` for the live gating state — the
matrix moves in both directions (a 2026-07-11 re-baseline set previously
passed groups back to `oracle_captured`). When editor groups are not passed,
the usable surface is package-level (`inspect`, `export`, batch ops) and
editor-dependent authoring must be reported as blocked.

## Formula rules (auditable models)

- Put assumptions and raw inputs in dedicated, clearly delineated cells or
  input ranges; follow the reference workbook's organization when one exists.
- Derived values must be formulas, never hardcoded results. Keep lookup,
  mapping, scoring, and quality-control rules in visible cells or tables and
  reference them.
- No magic numbers inside formulas: reference an input cell
  (`=A5*(1+$A$6)`, not `=A5*1.05`).
- Prefer consistent formula patterns across a range (all projection periods
  share one pattern). Use absolute/relative references deliberately so
  fill/copy behaves correctly.
- Keep formulas simple and legible; use helper cells for intermediate steps
  so a reader can trace inputs to outputs without reverse-engineering.
- Reference other sheets as `='Sheet Name'!A1` (always quote sheet names).
- Comment cells that carry complex formulas or load-bearing assumptions.

Correctness checklist before delivery: no formula errors (`#REF!`, `#DIV/0!`,
`#VALUE!`, `#NAME?`, `#N/A`), correct references, no off-by-one ranges, edge
cases handled (zero, negative), no unintended circular references, key totals
reconciled against source definitions.

## Data formatting rules

- Store numbers, percentages, currency, and dates as typed values, never as
  preformatted strings. Text is only for true identifiers (ZIP codes, IDs,
  SKUs, labels).
- Use locale-invariant number format codes (`#,##0`, `0.0%`, `"$"#,##0.00`,
  `yyyy-mm-dd`, `mmm yyyy`); never swap `.` and `,` in format codes to mimic
  a locale — the render locale controls separators.
- Match precision to meaning: counts `#,##0`; rates `0.0%` (analysis) or `0%`
  (dashboards), `0.00%` where small differences matter; currency in whole
  units unless cents matter.

## Edit discipline (existing workbooks)

- Before modifying, study the existing format, style, and conventions —
  render and inspect first, read the related values and formulas.
- Start with the smallest plausible local change; no sheet-wide autofit,
  wrapping, or restyling unless requested.
- Keep structures consistent: when adding rows or columns to a table, extend
  its conditional formatting, ranges, and dependent charts to cover them.
- Never overwrite established formatting except to extend it to added ranges.

## Answering questions (read-only requests)

- Answer from the workbook; do not edit or export.
- Locate the requested output by its row/column labels and period, inspect
  the displayed value and its formula, and trace precedents back to labeled
  assumptions or raw inputs — do not stop at an intermediate total.
- Preserve units and period conversions; for "what drives X" questions, rank
  the inputs that actually drive the output rather than guessing from labels.

## Verification before delivery

1. Inspect key ranges (values and formulas) on every sheet that changed.
2. Scan for formula errors across the workbook.
3. Render every sheet at least once and check: layout organized and legible,
   important numbers and callouts visible, nothing clipped or awkwardly
   wrapped, labels/titles appear once, merged ranges where labels
   intentionally span columns.
4. Fix severe defects before finalizing (broken charts, clipped headers,
   unreadable colors, stray blank sheets, content outside the working area).
   Stop polishing once the workbook is correct, legible, and exported; note
   minor limitations briefly instead of looping.
5. Persist the final verification renders as process evidence for the run;
   the visual pass must be checkable by review, not asserted.
6. Export via the engine and deliver exactly one final workbook — no extra
   variants unless asked.

## Sources and citations inside workbooks

- Cite row-wise researched data in a dedicated source column (plain-text
  URLs); cite model-input sources in cell comments.
- Keep source notes compact: file name, section or table label, enough
  context to audit the number. Do not paste large source excerpts into the
  workbook.

## Where to go deeper

- `references/execution-surfaces.md` — operation → surface → gating status.
- `references/charts-and-models.md` — model zoning, chart selection and
  discipline, conditional formatting and validation doctrine.

## Deliverables

- The deliverable is the persisted workbook (Business OS record / desktop
  file or the requested output path). QA renders and intermediates are not
  delivered unless explicitly requested.
- In Business OS contexts, reference records via deep links.
