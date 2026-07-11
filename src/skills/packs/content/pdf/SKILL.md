---
name: "pdf"
description: "Use when tasks involve reading, reviewing, or verifying PDF files where rendering and layout matter. Prefer visual checks by rendering pages to PNGs (Poppler, same path as the CTOX report pipeline). PDF creation routes through the CTOX documents engine, not ad-hoc PDF generator libraries."
cluster: content
---

# PDF Skill

## When to use

- Read or review PDF content where layout and visuals matter.
- Verify a PDF deliverable's final rendering before handoff.
- Produce a PDF deliverable (routed through the documents engine, see below).

## Execution contract (CTOX)

- Rendering uses `pdftoppm` (Poppler), the same dependency the CTOX report
  pipeline shells out to (`src/core/report/cli.rs`). It is operator-provisioned
  tooling. Do not install dependencies at runtime (`pip`, `uv`, `brew`,
  `apt-get`) and do not add Python PDF libraries as a workaround.
- If `pdftoppm` is missing, that is a blocker: report exactly what is missing
  and stop the visual-QA path. Do not claim the render gate passed. Structural
  checks may continue, but say clearly that visual QA was not performed.
- Text extraction for quick checks may use existing CTOX report tooling. Never
  rely on text extraction for layout fidelity.

## Creating PDFs

Do not generate PDFs with ad-hoc generator libraries (reportlab, pdf-lib,
wkhtmltopdf and similar). PDF deliverables are produced by CTOX engines:

1. Author the content as a document via the `doc` skill (CTOX documents
   engine), following its design-preset and render-verify contract.
2. Export to PDF through the engine's export path.
   <!-- gated: engine PDF export ships with the Euro-Office port print path;
        until then, state the limitation instead of falling back to a
        generator library. -->
3. Verify the exported PDF with the render workflow below before delivery.

For report-shaped deliverables, the CTOX report renderer
(`src/core/report/render/`) is the existing native path.

## Verification workflow

1. Render PDF pages to PNGs: `pdftoppm -png "$INPUT_PDF" "$OUTPUT_PREFIX"`.
2. Inspect every page at 100% zoom. Do not spot-check for final delivery.
3. After each meaningful update, re-render and verify alignment, spacing, and
   legibility.
4. Persist the render results of the final verification pass as process
   evidence for the run; the render gate must be checkable by review, not
   asserted.

## Quality expectations

- Maintain polished visual design: consistent typography, spacing, margins,
  and section hierarchy.
- Avoid rendering issues: clipped text, overlapping elements, broken tables,
  black squares, or unreadable glyphs.
- Charts, tables, and images must be sharp, aligned, and clearly labeled.
- Use ASCII hyphens only. Avoid U+2011 (non-breaking hyphen) and other
  Unicode dashes.
- Citations and references must be human-readable; never leave tool tokens or
  placeholder strings.

## Deliverable conventions

- The deliverable is the persisted artifact (Business OS record / desktop
  file or the requested output path), never QA intermediates.
- Keep intermediate renders in the run's scratch area; do not deliver PNGs or
  debug output unless the user explicitly asks.
- Keep filenames stable and descriptive.

## Final checks

- Do not deliver until the latest PNG inspection shows zero visual or
  formatting defects.
- Confirm headers/footers, page numbering, and section transitions look
  polished.
- If visual QA could not run (missing renderer), say so explicitly in the
  final response and do not imply that the document passed the render gate.
