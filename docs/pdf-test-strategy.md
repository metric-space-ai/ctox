# PDF Test Strategy

This document defines how CTOX should test whether a PDF page was converted into
layout-faithful plain text correctly.

The main point is:

Plain-text PDF parsing should not be tested as "one final string equals one
expected string" only.

That is too brittle and too shallow.

Instead, CTOX should test a page as a **linearization contract** with several
 layers:

- extraction correctness
- reading order correctness
- line-break correctness
- block / column separation correctness
- word-fragment reconstruction correctness
- cleanup correctness

## How `pdf_parse` Currently Works

The current pipeline is:

1. Pdfium extracts raw page characters and segments with geometry.
2. The backend converts those into `TextItem`s with coordinates.
3. Grid projection builds `ProjectionTextBox` entries from those `TextItem`s.
4. The projection layer groups boxes into lines and renders layout-faithful
   plain text.
5. Cleanup removes margin artifacts and normalizes post-processing artifacts.

Relevant code:

- parser entry: [parser.rs](../tools/pdf-parse/src/parser.rs)
- Pdfium extraction: [pdfium_backend.rs](../tools/pdf-parse/src/engines/pdf/pdfium_backend.rs)
- projection: [grid_projection.rs](../tools/pdf-parse/src/processing/grid_projection.rs)
- cleanup: [clean_text.rs](../tools/pdf-parse/src/processing/clean_text.rs)
- shared types: [types.rs](../tools/pdf-parse/src/core/types.rs)

Important consequence:

There are already multiple testable intermediate representations.

CTOX should use them.

## What "Correct" Means

For CTOX, "correct plain-text layout" does **not** mean:

- visually identical to the PDF
- Markdown conversion
- pixel-perfect text placement

It means:

- the visible reading order is preserved
- visible line breaks are preserved where they carry layout meaning
- columns and sidebars are not silently mixed into the main body
- tables remain readable as line-based plain text
- words are not spuriously split or merged
- cleanup does not destroy valid content

## Why Final-String Equality Alone Is Not Enough

A single gold string is still useful, but insufficient:

- harmless whitespace differences create false negatives
- some pages have multiple acceptable plain-text linearizations
- a parser can pass the final string for the wrong reasons
- it does not localize whether the failure came from extraction, ordering,
  line grouping, or cleanup

So final-string comparison should exist, but only as one layer.

## The Right Test Layers

### Layer 1: Engine Extraction Tests

Goal:

- verify that Pdfium-backed extraction emits usable `TextItem` geometry

Test at:

- `PdfiumBackend::extract_page()`
- `collect_page_chars()`
- `text_items_from_segments()`

What to assert:

- page extracts without panic
- non-empty pages produce non-empty `text_items`
- coordinates are finite and inside page bounds
- rotation and dimensions are coherent
- reconstructed segment text is not obviously worse than raw segment text

These are low-level sanity tests.

### Layer 2: Line Reconstruction Tests

Goal:

- verify that page geometry is turned into sensible line groups

Test at:

- `build_projection_boxes()`
- `bbox_to_lines()`
- `render_lines_minimal()`

What to assert:

- expected phrases appear on the same line
- expected phrases are on different lines
- line ordering matches visible reading order
- sidebars are not inserted into the middle of the main line sequence

This is the first place where layout correctness becomes directly testable.

### Layer 3: Cleanup Tests

Goal:

- verify that cleanup removes artifacts without destroying valid text

Test at:

- `clean_raw_text()`
- `normalize_page_text()`
- `normalize_line_artifacts()`

What to assert:

- soft hyphens are removed where appropriate
- duplicated punctuation artifacts are normalized
- page footers and InDesign residue are removed
- valid content is preserved

These tests should be synthetic and deterministic.

### Layer 4: Page-Level Gold Tests

Goal:

- verify that a page becomes the correct layout-faithful plain-text output

Test at:

- `parse_pdf_path()` or `parse_pdf_bytes()`

What to compare:

- normalized output text
- expected line sequence
- expected anchor ordering
- forbidden block-mixing patterns

This is the main regression layer.

## The Test Oracle Should Be Structured

The gold file for a page should not be just one blob string.

It should be a structured fixture, for example:

```json
{
  "pdf": "MK-10_2015.pdf",
  "page": 14,
  "expected_lines": [
    "Wohnraummiete MK",
    "VORGETÄUSCHTER EIGENBEDARF",
    "nicht jeder räumungsvergleich"
  ],
  "ordered_phrases": [
    "Der Vermieter kündigte das Mietverhältnis",
    "Der Mieter einen Räumungsvergleich",
    "Revision hat Erfolg"
  ],
  "same_line_groups": [
    ["Wohnraummiete", "MK"]
  ],
  "separate_line_groups": [
    ["VORGETÄUSCHTER EIGENBEDARF", "1. Der Fall des BGH"]
  ],
  "forbidden_patterns": [
    "Wohnung ihr PLuS im netZ",
    "Ri OLG"
  ]
}
```

This structure lets CTOX test the actual linearization contract instead of only
 one exact final serialization.

## Core Metrics For Page-Level Tests

Each page test should compute:

- exact expected-line hits
- ordered phrase hit rate
- forbidden-pattern violations
- suspicious split count
- suspicious merge count

### Suspicious Split Count

Examples:

- `Ve rmieter`
- `Räumungsve rgleich`
- `t he`

These are clear layout-reconstruction failures.

### Suspicious Merge Count

Examples:

- `WohnraummieteMK`
- `desSchadenersatzanspruchs`
- `PracticalGuide`

These are also layout-reconstruction failures.

The important point:

These should be measured explicitly rather than buried inside one edit-distance
 number.

## A Good CTOX Fixture Shape

Recommended fixture layout:

```text
tools/pdf-parse/tests/fixtures/
  corpus.json
  pages/
    mk_10_2015_p14.json
    mk_08_2015_p12.json
    backpacking_world_p1.json
  samples/
    MK-10_2015.pdf
    MK-08_2015.pdf
    Backpacking_The_World.pdf
```

Recommended fields per fixture:

- source PDF path or sample id
- page number
- document class
- expected line list
- ordered phrases
- same-line constraints
- separate-line constraints
- forbidden patterns
- allowed alternatives
- optional notes

## Which Tests Should Be Synthetic vs Real PDF

Use synthetic tests for:

- cleanup behavior
- punctuation normalization
- soft-hyphen handling
- line-merging edge cases

Use real PDF fixtures for:

- reading order
- columns and sidebars
- intra-word fragmentation
- table preservation
- mixed layout blocks

## The Most Important Practical Rule

When a parser change is made, the test should answer:

1. Did the final page text improve?
2. Did reading order improve or regress?
3. Did word fragmentation improve or regress?
4. Did block mixing improve or regress?
5. Did table fidelity regress?

If the test setup cannot answer those five questions, it is not yet the right
 benchmark for CTOX.

## Immediate Implementation Recommendation

The first useful test stack for CTOX should be:

1. unit tests for cleanup helpers
2. unit tests for line grouping / rendering on synthetic `TextItem` inputs
3. page-level golden tests over a small local PDF corpus
4. later: adapters for public benchmark datasets

That gives CTOX both fast local correctness checks and a path to broader
 benchmark validity.
