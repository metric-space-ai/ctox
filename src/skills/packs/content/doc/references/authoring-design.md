# Authoring design: from intent to explicit tokens

Method for designing a document before writing it, and for keeping the design
honest during implementation. This is engine-agnostic doctrine; execution
runs through the editor-flow surface.

## Why tokens, not taste

A document design only survives rendering if every value the design controls
is set explicitly. Defaults differ between editors, templates, and renderers;
inherited spacing and theme-dependent values drift. So: pick a design, then
resolve it into a concrete token map — numbers, names, and colors — and apply
exactly those tokens everywhere.

A usable token map covers: page size and margins; the type scale (body,
title, three heading levels); paragraph rhythm (space before/after, line
height); heading treatments (size, weight, color, spacing); list indents and
marker alignment; table geometry defaults (column-width policy, cell
margins, header treatment); callout/note styling; header/footer furniture;
and the accent color system.

## Choosing a visual register

Match the design to the document's real-world job, not to maximal polish:

- Formal briefs, memos, decision documents: restrained palette, strong
  typographic hierarchy, no decorative furniture.
- Dense operator references, checklists, guides: compact type scale, tighter
  rhythm, heavier use of structural elements (tables, checklists, callouts).
- Narrative proposals and long-form prose: generous line height and margins,
  fewer structural interruptions, clear section rhythm.

Whatever the register: implement headings as named styles, lists as real
numbering definitions, and titles as styled paragraphs — never ad-hoc bold
text, typed numbers, or unicode bullets. Fake structure breaks navigation,
TOC generation, accessibility, and downstream tooling.

## First-page treatment

Decide the opening block deliberately: plain title stack (title, subtitle,
metadata line), a header band, or a cover page. Keep it consistent with the
register — a compact reference gets a minimal title stack, a proposal may
carry a cover. One pattern per document; do not mix.

## Template-following mode

When an existing document or template is the design authority, distill its
actual token values first (measure, do not guess): page geometry, styles in
use, heading treatments, table conventions, header/footer content. The
distilled tokens replace any generic preset — do not "improve" the template's
design unless explicitly asked. Record deviations you must make as named,
intentional exceptions.

## The design audit (before final render review)

Walk the token map against the document: page geometry, style definitions,
heading spacing and colors, list indents, table widths and cell margins,
callout styling, header/footer content, and every direct-formatting
exception. Additionally hunt for: fake headings or bullets, tables used as
layout hacks, clipped or edge-hugging cell text, mixed spacing systems,
adjacent sections that all render as the same visual form, and page-break
orphans (a table or figure pushed to the next page leaving a large gap —
rescale, split with repeated headers, or reflow).
