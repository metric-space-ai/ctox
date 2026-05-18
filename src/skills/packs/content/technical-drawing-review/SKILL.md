---
name: technical-drawing-review
description: Review mechanical technical drawings, engineering drawing PDFs, CAD exports, and manufacturability handoff drawings; produce localized issue lists with pins, severity, evidence, and follow-up checks.
metadata:
  short-description: Review technical drawings with pinned findings
cluster: content
---

# Technical Drawing Review

Use this skill when the user wants an AI-assisted first-pass review of a mechanical technical drawing, CAD-exported PDF, manufacturing drawing, or drawing handoff package. The expected output is a practical issue list with pins on the drawing, similar to review tools that mark missing tolerances, conflicting dimensions, incomplete specifications, or manufacturability concerns.

This skill is not a substitute for a qualified engineer's release decision. Treat findings as review candidates that need human confirmation.

The review capability should be implemented with a vision-capable model using explicit drawing-review prompts. The model inspects rendered drawing pages and emits structured findings; deterministic scripts only validate and package the output.

## Inputs

Accept any of:

- Drawing PDF, raster image, or CAD-exported drawing page.
- Multi-page PDF or TIFF files with one or more drawing sheets.
- E-mail attachment files saved to disk.
- `.eml` files containing one or many PDF/image/TIFF/ZIP attachments.
- ZIP packages containing PDFs, images, TIFFs, nested `.eml` files, or nested ZIPs.
- Optional part purpose, material, process, supplier capabilities, house standard, tolerance class, revision history, or inspection plan.
- Optional output target: standalone interactive HTML review, concise review, JSON findings, UI mockup data, or implementation-ready annotations.

If the drawing is a PDF, render pages to images before visual review. Use the PDF skill if layout fidelity or PDF rendering is part of the task.

Use `scripts/prepare_review_inputs.py` to normalize input files before review. It accepts direct PDFs/images/TIFFs, `.eml` files, ZIPs, and directories; extracts attachments, renders PDF and TIFF pages, copies supported images, and writes a `manifest.json` with page image paths. Treat the manifest as one review package even when it came from many attachments.

For durable learning, use CTOX source-skill knowledge. Read `references/knowledge-integration.md` when setting up, querying, or updating the Skillbook/Runbook layer for technical drawing review.

## Review Workflow

1. Render each drawing page to a legible image if needed.
2. For files or e-mail attachments, normalize them first:

```bash
skills/packs/content/technical-drawing-review/scripts/prepare_review_inputs.py \
  --input drawing.pdf \
  --output-dir output/technical-drawing-review/work
```

3. Run the vision model prompt pipeline in `references/vision-prompts.md`:
   - extraction pass: produce visible drawing evidence
   - review pass: compare evidence against `references/review-checklist.md`
   - annotation pass: deduplicate, pin, calibrate confidence, and emit final findings JSON
4. Establish drawing context: page count, sheet size if visible, revision, title block, scale, units, material, finish, part number, projection method, and target process.
5. Segment the drawing mentally into zones: title block, general notes, each view, section/detail views, dimension clusters, GD&T frames, surface finish symbols, material/finish notes, BOM or weld notes.
6. Read the drawing in passes:
   - Metadata pass: required title-block and general-note information.
   - Geometry pass: views, sections, hidden lines, centerlines, datums, pattern features.
   - Dimensioning pass: missing, duplicate, chained, conflicting, reference, and non-inspectable dimensions.
   - Tolerance pass: missing tolerances, impossible tolerances, tolerance stack risks, ambiguous datum scheme, GD&T issues.
   - Manufacturing pass: process feasibility, tool access, sharp internal corners, thin walls, weld/cut/bend ambiguity, surface finish, heat treatment, material form.
   - Inspection pass: measurable requirements, datum setup, critical-to-quality features, unclear acceptance criteria.
7. For each issue, place a pin at the most specific visible location. If the issue is global, pin the title block, general note area, or the dominant affected feature.
8. Separate definite issues from questions. Do not claim a defect when the drawing lacks enough context; mark it as `needs_context`.
9. Validate the model output against the finding schema.
10. For user-facing delivery, prefer a standalone HTML review artifact over plain JSON when the user wants to inspect the drawing interactively.

## Knowledge Loop

Before finalizing a review, query CTOX source-skill knowledge for `technical-drawing-review` when available and incorporate the best matching Runbook item guidance. After human feedback, promote only verified repeatable review behavior into Skillbook/Runbook knowledge. One-off drawing facts belong in ticket/context evidence, not in reusable Runbook items.

When communication arrives after a review with criticism, corrections, or new instructions, run the feedback ingest workflow before the next comparable review:

```bash
skills/packs/content/technical-drawing-review/scripts/ingest_review_feedback.py \
  --feedback-file feedback.txt \
  --findings findings.json \
  --manifest output/technical-drawing-review/work/manifest.json \
  --review-artifact output/technical-drawing-review/review.html \
  --output-dir output/technical-drawing-review/feedback
```

If the feedback belongs to a real CTOX ticket/case, publish it as a learning candidate:

```bash
skills/packs/content/technical-drawing-review/scripts/ingest_review_feedback.py \
  --feedback-file feedback.txt \
  --findings findings.json \
  --manifest output/technical-drawing-review/work/manifest.json \
  --review-artifact output/technical-drawing-review/review.html \
  --output-dir output/technical-drawing-review/feedback \
  --publish \
  --case-id <case-id> \
  --ctox-bin target/debug/ctox \
  --workspace-root /Users/michaelwelsch/Documents/ctox
```

If the feedback is not attached to a case, `--publish` creates CTOX self-work of kind `runbook-learning-candidate` for later owner review. Add `--remote-publish-self-work` only when the ticket adapter should also publish that work item externally. Only after approval, promote it into the source-skill bundle and re-import it:

```bash
skills/packs/content/technical-drawing-review/scripts/ingest_review_feedback.py \
  --feedback-file feedback.txt \
  --output-dir output/technical-drawing-review/feedback \
  --promote-to-bundle output/technical-drawing-review/knowledge-seed

ctox ticket source-skill-import-bundle \
  --system technical-drawing-review \
  --bundle-dir output/technical-drawing-review/knowledge-seed
```

## Pin Contract

Use normalized page coordinates so UI layers can render pins on any drawing size:

- `page`: 1-based page number.
- `x`: horizontal coordinate from left edge, 0.0 to 1.0.
- `y`: vertical coordinate from top edge, 0.0 to 1.0.
- `anchor`: short human-readable area, such as `title_block`, `front_view_hole_pattern`, or `section_A-A`.

When exact location is uncertain, still provide the best pin and set `confidence` below `0.7`.

## Finding Schema

For implementation-ready output, return JSON matching `references/review-schema.md`. Use `scripts/validate_findings.py` to check basic structure when producing a file.

Use `references/vision-prompts.md` for the system/developer/user prompt structure when asking a vision model to produce findings.

Minimum fields per finding:

- `id`: stable short id such as `TD-001`.
- `severity`: `critical`, `major`, `minor`, or `info`.
- `category`: one of `metadata`, `dimensioning`, `tolerance`, `gd_and_t`, `material_finish`, `manufacturing`, `inspection`, `consistency`, `standards`, `needs_context`.
- `title`: concise issue title.
- `evidence`: what is visible on the drawing.
- `risk`: why it matters for manufacturing, inspection, cost, schedule, or quality.
- `recommendation`: concrete next action.
- `pin`: normalized page location.
- `confidence`: 0.0 to 1.0.

## Output Style

Default to a standalone interactive HTML review when a PDF or image drawing is provided and the user wants a mockup, review view, or handoff artifact. Otherwise use a short summary plus a table/list of pinned findings. For app integration, output JSON and include a separate human-readable summary.

## Standalone HTML Review

The standalone HTML artifact must:

- Embed rendered drawing page images as base64 data URLs so the file works offline.
- Render pins at normalized coordinates from the finding schema.
- Link every pin to a finding in the side panel and every finding back to its pin.
- Show severity, category, confidence, evidence, risk, recommendation, and status.
- Support multi-page drawings when multiple page images exist.
- Keep all CSS and JavaScript inline. Do not depend on CDNs, external assets, or a running server.
- Use a split review layout by default: large drawing canvas on the left, fixed issue panel on the right, numbered blue pins matching numbered issue cards.
- Keep the issue panel German when the user's context is German; otherwise match the user's language.

Recommended build sequence:

1. Render each PDF page to a PNG, preferably at 180-220 DPI for legibility:

```bash
mkdir -p output/technical-drawing-review/rendered
pdftoppm -png -r 200 drawing.pdf output/technical-drawing-review/rendered/page
```

For mixed inputs or e-mail attachments, prefer the normalizer:

```bash
skills/packs/content/technical-drawing-review/scripts/prepare_review_inputs.py \
  --input mail-with-drawing.eml \
  --input supplier-package.zip \
  --input extra-detail.png \
  --output-dir output/technical-drawing-review/work
```

2. Create or update `findings.json` using the three-pass vision prompt pipeline and `references/review-schema.md`.

3. Validate findings:

```bash
skills/packs/content/technical-drawing-review/scripts/validate_findings.py findings.json
```

4. Generate the standalone HTML:

```bash
skills/packs/content/technical-drawing-review/scripts/generate_review_html.py \
  --findings findings.json \
  --manifest output/technical-drawing-review/work/manifest.json \
  --output output/technical-drawing-review/review.html
```

For ad-hoc use without a manifest, pass `--page-image PAGE=PATH` once per page.

Prioritize issues that a manufacturing reviewer would act on:

- Missing or ambiguous tolerances on functional dimensions.
- Conflicting dimensions or notes.
- Missing material, finish, heat treatment, edge break, thread, weld, bend, or surface requirements.
- Missing datum scheme or inspection setup for GD&T.
- Requirements that cannot be verified from the drawing.
- Manufacturability problems visible from the drawing.

Avoid generic advice that cannot be pinned to a visible feature or note.

## Dataset Use

TechMB can be used as a first benchmark for drawing-reading ability, not as direct supervised data for pinned issue detection. Read `references/techmb.md` before using it for evaluation or dataset work.

## Quality Bar

- Every non-global finding must point to a visible feature, dimension, note, symbol, or title-block field.
- The issue list should be de-duplicated; merge multiple symptoms into one finding when they share the same root cause.
- Use cautious language for model-inferred risks.
- If no clear issues are visible, report `no_actionable_findings` and list any context needed for a stronger review.
- If delivering HTML, open it locally or inspect the generated file enough to confirm pins, images, and finding navigation are present.
