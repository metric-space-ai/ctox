# liteparse-rs

A focused Rust transposition of the LiteParse core.

## What is ported

This crate ports the algorithmic center of LiteParse into a Rust-first layout pipeline:

- `src/core/config.rs` — config defaults and override merging
- `src/core/types.rs` — page, text box, OCR, anchors, and output types
- `src/processing/clean_text.rs` — margin stripping and page text cleanup
- `src/processing/text_utils.rs` — OCR table-artifact cleanup + super/subscript helpers
- `src/processing/bbox.rs` — bounding box helpers
- `src/processing/grid_projection.rs` — rotation normalization, line grouping, word merging, dot-garbage filtering, and a pragmatic grid renderer
- `src/parser.rs` — orchestration layer over a PDF backend trait
- `src/engines/pdf/*` — backend trait and a `pdfium-render` oriented backend design

## PDF backend choice

For Rust, `pdfium-render` is the most practical backend because it can:

- load PDFs
- render pages at configurable DPI
- extract text
- expose per-character geometry and rotation metadata
- expose image/page object information

That makes it the closest single-backend replacement for LiteParse's current split between PDF.js text extraction and PDFium page rendering.

The crate is wired so the algorithmic core is backend-agnostic through the `PdfEngine` trait. The `PdfiumBackend` is the intended production backend, but the parser core itself does not depend on Pdfium details.

## Parity coverage

The `parity/fixtures/` directory contains source-derived fixtures based on the original LiteParse tests and helper logic. `parity/run_parity.py` runs those fixtures against a reference implementation that mirrors the Rust algorithms in this crate.

Covered parity checks:

- `bboxToLine()` merge / split / markup / sort cases
- `projectToGrid()` simple single-column case
- `projectToGrid()` two-column case
- `projectToGrid()` dot-garbage filtering
- `buildBoundingBoxes()`
- `cleanOcrTableArtifacts()`
- `cleanRawText()`

## Important note

This environment did not provide `cargo` / `rustc`, so the Rust crate could not be compiled in-sandbox. The source tree and Rust tests are complete enough to continue locally, and the executed parity checks were run via the included Python fixture harness.
