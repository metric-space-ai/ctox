# PDF Benchmark Plan

This document defines the baseline benchmark stack for CTOX `pdf_parse`.

The goal is not to optimize against one hand-picked corpus such as `MK_Archiv`,
but to evaluate parser quality across the actual failure classes that matter for
layout-faithful plain-text extraction:

- reading order
- block and column separation
- word-fragment reconstruction
- table preservation
- parser robustness and speed

## Benchmark Layers

CTOX should benchmark PDF parsing in three layers:

1. Local regression corpus
2. Public end-to-end parsing benchmarks
3. Public subtask benchmarks

This split matters because no single public dataset covers all of:

- born-digital legal and business PDFs
- real-world reading-order failures
- layout diversity
- tables
- OCR / scan fallback

## Layer 1: Local Regression Corpus

Purpose:

- prevent regressions while iterating on heuristics
- keep a small, fast smoke-test set for every parser change

Recommended corpus shape:

- 10 to 30 PDFs
- each PDF tagged by failure class
- at least one page-level golden output per file

Recommended classes:

- simple single-column text
- two-column scientific papers
- legal newsletters with sidebars
- tables-heavy PDFs
- invoices and forms
- scanned or image-based PDFs
- PDFs with rotated or mixed-orientation text
- PDFs with known font / kerning fragmentation

Suggested local seeds already available in Downloads:

- `MK_Archiv` for legal multi-column and sidebar cases
- `Backpacking_The_World.pdf` for aggressive intra-word fragmentation
- invoices and forms in `~/Downloads` for narrow-layout business documents

Minimum metrics for the local regression set:

- parse success rate
- page-level runtime
- normalized edit distance against curated plain-text gold
- fragmentation rate:
  split words, merged words, stray punctuation artifacts
- manual notes for known remaining failures

## Layer 2: Public End-to-End Benchmarks

These are the closest match for CTOX's real parser goal.

Current repo-local smoke layer:

- `corpus.public-opendataloader.json` for prose, tables, and two-column papers
- `corpus.public-pdfjs.json` for widgets/forms, annotations, footer URLs, and rotated text

These are not replacements for larger public benchmarks like OmniDocBench, but
they give CTOX a fast reproducible public regression layer with pinned sample
assets.

Queued harder public candidates that are useful but not yet part of the green
smoke layer:

- `chinese_scan.pdf` from OpenDataLoader as a scan/OCR gap probe
- `SimFang-variant.pdf` from `pdf.js` for CJK font and spacing issues
- `ArabicCIDTrueType.pdf` from `pdf.js` for RTL/CID text-layer stress
- `TrueType_without_cmap.pdf` from `pdf.js` for broken-encoding edge cases

### OmniDocBench

Use for:

- diverse real-world document parsing
- full-page component extraction
- OCR, layout, tables, formulas, and recognition-oriented evaluation

Why it matters:

- broad document diversity
- public dataset and code
- good anchor for modern document parsing systems

Primary use in CTOX:

- end-to-end benchmark bucket for parser quality across document types
- not just layout or just OCR in isolation

Source:

- <https://github.com/opendatalab/OmniDocBench>

### OpenDataLoader Benchmark

Use for:

- practical PDF-to-Markdown evaluation across real-world corpora
- reading order, table fidelity, heading hierarchy, and speed

Why it matters:

- directly measures the kinds of output quality dimensions that also matter for
  CTOX plain-text linearization
- useful as a reproducible external comparison harness even though CTOX targets
  layout-faithful plain text rather than Markdown

Primary use in CTOX:

- benchmark design reference
- external sanity check for reading-order and table metrics

Source:

- <https://opendataloader.org/docs/benchmark>
- <https://github.com/opendataloader-project/opendataloader-pdf>

### READoc

Use for:

- realistic PDF-to-structured-markdown evaluation

Why it matters:

- strong later-stage benchmark once CTOX goes beyond plain text into richer
  structured conversion

Primary use in CTOX:

- later-stage benchmark, not the first parser baseline

Source:

- <https://arxiv.org/abs/2409.05137>

### ExtractBench

Use for:

- PDF-to-JSON extraction under schema constraints

Why it matters:

- relevant later once CTOX builds structured extraction and write/edit loops

Primary use in CTOX:

- not a baseline for the current plain-text parser
- useful later for schema-driven document extraction

Source:

- <https://arxiv.org/abs/2602.12247>

## Layer 3: Public Subtask Benchmarks

These do not replace end-to-end parsing benchmarks, but they cover failure
classes that matter for parser internals.

### ReadingBank

Use for:

- reading-order evaluation

Why it matters:

- reading order is one of the main current failure modes for CTOX
- provides word coordinates and order labels derived from PDF-aligned sources

Important note:

- redistribution is restricted; use according to the repository terms

Source:

- <https://github.com/doc-analysis/ReadingBank>

### DocLayNet

Use for:

- general-purpose layout segmentation across diverse document layouts

Why it matters:

- more layout-diverse than scientific-only corpora
- good benchmark for whether block separation logic generalizes beyond papers

Source:

- <https://research.ibm.com/publications/doclaynet-a-large-human-annotated-dataset-for-document-layout-segmentation>

### PubLayNet

Use for:

- large-scale document layout analysis

Why it matters:

- strong baseline for page-layout detection
- less diverse than DocLayNet, but still useful as a standard reference

Source:

- <https://github.com/ibm-aur-nlp/PubLayNet>

### DocBank

Use for:

- token-level document layout analysis

Why it matters:

- useful when debugging text-block semantics and layout labeling

Important note:

- redistribution restrictions apply; check repository terms

Source:

- <https://github.com/doc-analysis/DocBank>

### PubTables-1M

Use for:

- table detection
- table structure recognition

Why it matters:

- tables are a separate failure class and should not be degraded while fixing
  prose or sidebar reconstruction

Source:

- <https://github.com/Performl/microsoft-table-transformer>

## CTOX Baseline Recommendation

For the current `pdf_parse` phase, the first correct baseline is:

1. Local regression corpus for fast parser iteration
2. OmniDocBench as the primary public end-to-end benchmark
3. ReadingBank for reading order
4. DocLayNet for general layout robustness
5. PubTables-1M for tables

Do not optimize against `MK_Archiv` alone.

`MK_Archiv` should remain in the local regression layer because it is valuable,
but it must not define parser behavior by itself.

## Current Repo Entry Points

The current repo wiring for `tools/pdf-parse` is:

- local regression corpus:
  `tools/pdf-parse/tests/fixtures/corpus.downloads.json`
- public sample corpus:
  `tools/pdf-parse/tests/fixtures/corpus.public-opendataloader.json`
- public sample manifest:
  `tools/pdf-parse/tests/fixtures/samples/public/opendataloader/samples.json`

Helper commands:

- fetch public sample PDFs:
  `cargo run --manifest-path tools/pdf-parse/Cargo.toml --example fetch_public_samples tools/pdf-parse/tests/fixtures/samples/public/opendataloader/samples.json`
- run the public sample corpus:
  `cargo run --manifest-path tools/pdf-parse/Cargo.toml --example eval_fixture tools/pdf-parse/tests/fixtures/corpus.public-opendataloader.json`

## Metrics CTOX Should Track

At minimum:

- parse success rate
- panic / crash rate
- pages per second
- normalized text edit distance
- reading-order score
- fragmentation score:
  suspicious intra-word splits and bad merges
- block mixing score:
  main text incorrectly mixed with sidebars / headers / footers
- table fidelity score for table pages

For the current plain-text target, the most important metrics are:

- reading order
- fragmentation score
- block mixing score

## Immediate Next Step

Before further parser heuristics:

1. define a small local gold corpus
2. wire a benchmark runner around `tools/pdf-parse`
3. add at least one public benchmark adapter, starting with OmniDocBench

Only after that should parser changes be judged as improvements.
