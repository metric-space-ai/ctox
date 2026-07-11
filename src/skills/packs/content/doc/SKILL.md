---
name: "doc"
description: "Create, edit, redline, review, and comment on .docx documents through the CTOX documents engine (Euro-Office port), with a strict render-and-verify workflow. Authoring and layout changes run as editor flows; deterministic OOXML batch operations run as native office-engine ops. Never assume a document is correct without rendering and visually inspecting every page."
cluster: content
---

# Documents Skill (Read - Create - Edit - Redline - Comment)

Use this skill for `.docx` work in CTOX: creating documents, editing existing
ones with minimal surgical changes, managing tracked changes and comments,
auditing structure and accessibility, and verifying layout visually before
delivery.

## Execution contract (CTOX)

Document work runs on CTOX's own engines. Do not use python-docx, LibreOffice,
Poppler, or any ad-hoc document library, and do not install dependencies at
runtime. There are two execution surfaces:

1. **Editor flows (layout-affecting work).** Authoring, formatting, styles,
   tables, images, sections, and in-context review actions run against the
   headless CTOX documents editor (Euro-Office port) on the same code path
   users operate interactively. Rendering for visual QA comes from the same
   engine.
2. **Native batch operations (deterministic OOXML work).** Operations that
   transform the package without needing layout — accepting tracked changes in
   bulk, extracting comments, scrubbing metadata, redaction, protection,
   merging, audits — run as `ctox-office-engine` operations (thin CLI for the
   harness, `business_commands` for apps).

Capability gating: each operation class below is bound to a feature group in
`src/apps/business-os/office-engine/features.json` or to a planned engine op.
If the required capability is not shipped in this build, that is a blocker —
report exactly what is missing and what part of the task cannot be done. Do
not fall back to external document tooling. See
`references/execution-surfaces.md` for the full operation-to-surface map and
current gating status.

Available today: `inspect`, `export` (byte-preserving OOXML round-trip), and
the first batch-op slice — `comments-extract`, `a11y-audit`, `privacy-scrub`,
`tracked-changes-accept`, `protection-set`. Everything listed as gated or
planned must be treated as unavailable until its status says otherwise.

## Non-negotiable: render, inspect, iterate

You do not know a DOCX is correct until you have rendered it and inspected the
page images. Text extraction and raw OOXML reading miss layout defects:
clipping, overlap, broken tables, spacing drift, header/footer issues, missing
glyphs.

Shipping gate, before delivering any DOCX:

- Render every page through the engine render surface
  (`document.open-render-zoom`).
- Inspect every page at 100% zoom — no spot checks for final delivery.
- Fix and re-render until every page is clean.
- Persist the final verification renders as process evidence for the run. The
  render gate must be checkable by review, not asserted.

If rendering is unavailable in this build, deliver only with an explicit
statement that visual QA could not run, and use the structural checks below as
the fallback. Never imply the render gate passed when it did not.

Rendering validates layout, fonts, spacing, tables, and whether tracked
changes appear. It is not reliable for comments — verify comments
structurally (comment parts, anchors, relationships) via `inspect`.

## Authoring doctrine (new documents and major rewrites)

Plan before writing:

1. Decide the document archetype (memo, report, SOP, proposal, form, manual)
   and calibrate visual ambition to its real-world purpose. Formal documents
   get their polish from typography, spacing, and hierarchy — not decoration.
2. Design the page system first: title placement, heading ladder, first-page
   treatment, page rhythm, header/footer furniture.
3. Resolve the design into explicit numeric tokens before drafting: page
   geometry, margins, type scale, paragraph spacing, heading treatments, list
   indents, table geometry, colors. Never rely on editor defaults, inherited
   spacing, or renderer-dependent behavior for a value the design controls.
4. Implement tokens through real mechanisms: named styles for
   Normal/Title/Headings, real numbering definitions for lists, explicit
   table geometry. Never fake headings with bold text, fake bullets with
   unicode characters or hyphens, or fake numbering by typing numbers.
5. Keep one coherent style system for the whole document. Record deliberate
   exceptions as named overrides and reuse them consistently.

Map every major content unit to a deliberate form factor: prose sections for
narrative and rationale; a lead callout for the decision or key takeaway;
numbered steps for procedures; grouped bullets for unordered factors;
checklists for acceptance criteria; note boxes for warnings; definition lists
for metadata; tables only for genuinely comparable row/column data; form
layouts for things people fill in; source lists for evidence.

Table gate: use a table only when the content is repeated records with shared
fields where comparison or lookup helps. If cells turn into mini-paragraphs,
convert to prose or bullets. Before finalizing, audit for table overuse and
for adjacent sections that all use the same visual form.

Table quality is a hard constraint: explicit column widths chosen by content
(short fields compact, narrative fields wide), no fixed row heights that can
truncate text, generous and consistent cell padding, deliberate vertical and
horizontal alignment per column type, clear separation from surrounding text,
repeated header rows when a table spans pages.

## Edit discipline (existing documents)

When editing an existing document, preserve it:

- Study the existing format, styles, and conventions first (render + inspect
  before changing anything).
- Make minimal, local changes; prefer small inline replacements over
  rewriting paragraphs.
- Keep the original structure unless there is a strong reason; restructure
  surgically and explain via comments at the point of change.
- Never blanket-delete and rewrite; the goal is trackable improvement, not a
  fresh draft.

## Review lifecycle (tracked changes and comments)

- Redlines (tracked insertions/deletions) are for edits the author should
  accept or reject; comments are for feedback, questions, and rationale that
  should not change the text itself. Choose deliberately.
- Anchor comments at the exact point of change, not collected at the end.
- Finalization means: accept or reject every tracked change, resolve or strip
  every comment — via the batch operations, then re-render and verify.
- After any review-layer change, verify structurally via `inspect` that
  comment parts, anchors, and relationships are consistent.

## Batch operations (native engine ops)

These are deterministic OOXML operations planned as `ctox-office-engine` ops
with CLI and `business_commands` surfaces (see
`references/execution-surfaces.md` for status): accept/reject tracked
changes; add/extract/resolve/strip comments; privacy scrub (author metadata,
revision ids, custom properties); layout-preserving redaction; edit
protection; document merge; accessibility audit (alt text, heading order,
table headers, link text); style lint and conservative normalization; field
materialization for deterministic rendering; watermark add/audit/remove;
table-to-data and data-to-table conversion.

Until an op ships, its task class is blocked; say so instead of improvising
with external tools.

## Deliverables and references

- The deliverable is the persisted document (Business OS record / desktop
  file or the requested output path). QA renders and intermediates stay in
  the run's scratch area and are not delivered unless explicitly requested.
- When referencing documents or records in responses inside Business OS
  contexts, use Business OS deep links.
- Final responses describe the document result; they do not link builders,
  QA renders, or debug output.

## Where to go deeper

- `references/execution-surfaces.md` — operation → surface → gating status.
- `references/authoring-design.md` — token method, visual registers,
  first-page treatment, template-following mode, design audit.
- `references/review-lifecycle.md` — redline/comment semantics, OOXML
  essentials, finalization with engine ops, delivery checklist.

## Quality bar

- No visible defects: clipping, overlap, broken tables, unreadable glyphs,
  header/footer misplacement, awkward page breaks, orphaned captions.
- No walls of text; readers get visual anchors appropriate to the archetype.
- Color and emphasis used intentionally and sparingly; restrained for formal
  documents.
- ASCII punctuation; no exotic unicode dashes that render inconsistently.
- No leaked tool tokens or placeholder strings; citations human-readable.
