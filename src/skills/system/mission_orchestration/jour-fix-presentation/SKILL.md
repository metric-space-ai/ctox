---
name: jour-fix-presentation
description: Author the Jour fixe (Regeltermin) presentation of a Workjet project as a learnordie SlideDocument with handwriting canvas and 3D scenes, and store it through business_os.presentation_update.
---

# Jour fixe presentation

The Owner reviews each project in a weekly Jour fixe. Its deck is not a business
slide template. Workjet draws every slide as a hand-drawn canvas in a handwriting
font, with interactive three.js scenes for numbers, and the Owner sketches and
edits directly on that canvas. Write for that: few words, one idea per slide,
numbers shown as scenes, honest gaps named as gaps.

You write a `SlideDocument` (`schemaVersion: "learnordie.slide.v1"`) made of
structured blocks. Do not draw canvas elements yourself unless a slide truly
needs a sketch: Workjet converts blocks into the handwriting canvas on its own,
and once a slide has a canvas, the canvas becomes that slide's source of truth.

## Workflow (current leased Supervisor turn only)

1. `business_os.jour_fixe_read` `read_meeting` with `{project_id, meeting_id}`:
   configuration, previous confirmed goal, deck revision, meeting revision.
2. `business_os.project_kpi` action `read` with `{project_id}`: the current KPI
   values. It holds no history. Use only stored values; missing or stale values
   are named on the slide as missing. Never estimate or invent numbers.
3. `business_os.presentation_read` `read_history` with `{project_id,
   meeting_id, limit}`: this project's earlier meetings, newest first
   (`index` 1 = last, 2 = penultimate), with the scene data their presentations
   actually showed. The binding comparison for every ±% is the **penultimate**
   Regeltermin (`index` 2). If it is missing, or did not show that KPI, the
   comparison is missing: show the current value without `previous` and say so.
4. Collect evidence you may cite: merged PRs (number, title, URL), worker runs,
   decisions, open questions, comments and final transcript of the previous
   meeting. Every slide cites at least one source in `sourceRefs`.
5. Draft the document and check it without storing:
   `business_os.presentation_read` `validate_document`
   `{project_id, meeting_id, document}`. It checks the schema and the content
   rules below and returns every issue. Fix every `error` using its
   `repairHint`; warnings (layout budgets, a repeated sentence) are advice. A
   deck with content errors is not stored and not published.
6. Store it: `business_os.presentation_update` `create_presentation`
   `{operation_id, project_id, meeting_id, document}`. A meeting has exactly one
   presentation; later changes use `replace_document`, `apply_edits` or
   `save_canvas` with the current `expected_revision`.
7. Derive the narration deck: `presentation_update` `publish_deck`
   `{operation_id, project_id, meeting_id, presentation_revision,
   expected_meeting_revision, deck_revision}` with the meeting revision from
   step 1 and `deck_revision` = current deck revision + 1. This stores one
   meeting slide per presentation slide (same ids) exactly like `prepare_deck`;
   the speaker notes become the narration text.
8. Narrate every slide with `business_os.jour_fixe_update` `narrate`, as the
   jour-fix skill describes (read the new meeting revision before each slide).

Recover an uncertain write by repeating the same `operation_id` with the
identical request. A different request needs a new `operation_id`; ids are
scoped to the meeting's presentation. Only the project's latest meeting accepts
writes, before or during the meeting; earlier presentations stay as they were
shown, because later decks compare against them. A revision
conflict means: read again (`read_presentation`), then decide.

## Deck for a Regeltermin

Language: the project's language (German unless the project says otherwise).
Three to nine slides. Suggested order, adapt to the evidence:

| # | Slide | Layout | `intent` | Content |
|---|---|---|---|---|
| 1 | Titel | `title_statement` | `title` | Project, date, and the one question this meeting must answer: the most important open Owner decision, otherwise the goal question |
| 2 | Ziel und Stand | `comparison_split` | `comparison` | Previous confirmed goal from `read_meeting` (left) against what is done (right) |
| 3 | KPIs | `technical_figure_right` | `comparison` | `business.kpi-bars` scene: current value against the penultimate Regeltermin, two to four bullets reading the deltas |
| 4 | Exitwert | `technical_figure_right` | `explanation` | `business.trend` scene of the five-year exit value (E5) over the stored Regeltermine (`read_history`) plus today, with target if configured |
| 5 | Worker-Aktivität | `technical_figure_left` | `summary` | `business.kpi-bars` of merged PRs or runs per worker, one sentence of reading |
| 6 | Gemergte PRs | `table_focus` | `summary` | Table: PR, title, effect; each PR also as a `url` source `https://github.com/<owner>/<repo>/pull/<n>` |
| 7 | Entscheidungen | `technical_one_column` | `summary` | `callout` (tone `key`) per decision that needs the Owner |
| 8 | Nächste Schritte | `technical_one_column` | `summary` | `numberedList` of next acceptance criteria |

A slide whose evidence is entirely missing is left out; say on slide 2 what
is missing. Never keep an empty slide. When almost nothing is measured yet (a
first Regeltermin), the deck has three slides: the title with the question,
goal and status naming every gap once, and the decisions for today.

Every slide's `title` is drawn as its handwritten headline: at most 60
characters, and do not repeat it as a `heading` block. Use `heading` blocks
only for sub-headings (on the title slide: one short subtitle line).

Text: bullets short and concrete, no filler, no marketing words. Numbers carry
units and German formatting in text (`1.980`, `4,8 Mio €`, `−5`, dates
`12.10.2026`); `data` fields hold plain JSON numbers (`1980`, `4.8`). Say
"keine Daten" plainly when a source is missing. In the PR table, "effect" is
one short line you can support from the evidence, otherwise `keine Daten`.

## Write about the project, never about the slides

The Owner reads every sentence. Each one states a project fact, a change or a
decision. These rules are checked (`content.*` issues) on everything the
Supervisor stores:

- Never write about the slide, the deck, the layout or the display: no "diese
  Folie", "bleibt leer", "erscheint hier", "links steht". State the fact.
- Never make the data plumbing the message: no "laut Katalog", "aus der
  Konfiguration", "keine frühere Präsentation". Say what is missing for the
  project ("Gemergte PRs: Quelle noch nicht angebunden").
- No internal ids, field names, recipe names or system states in slides or
  notes: no `missing_source`, `project_tasks_total`, "KPI-Prompts", "Rezepte",
  "gebundene Werte", "Native-Quellen". Use the Owner's words.
- Name every gap once, on slide 2. A slide or table that would only say
  "keine Daten" is left out.
- A heading never repeats the slide title; a callout title never restates it.
- "Nächste Schritte" does not restate "Entscheidungen": decisions say what the
  Owner approves today, next steps say what changes afterwards. Each fact
  appears once in the deck.

## Document shape

```json
{
  "schemaVersion": "learnordie.slide.v1",
  "id": "jf-greppy-2026-10-12",
  "title": "Regeltermin greppy.xyz – 12.10.2026",
  "language": "de",
  "aspect": "16:9",
  "theme": "learnordie-north",
  "deckSettings": {"defaultTransition": "fade", "showSlideNumbers": true, "allowFragments": false, "mobileMode": "scaled"},
  "slides": [ … ],
  "assets": [],
  "createdBy": {"mode": "agent", "model": "<the model id your runtime reports>", "promptVersion": "jour-fix-presentation.v1"}
}
```

Ids: 1–120 characters, start with a letter or digit, then letters, digits,
`.` `_` `:` `-`. Every id is unique within the whole document (prefix ids with
the slide id, e.g. `s-kpis-scene`, `s-kpis-src-1`).
Themes: `learnordie-north` (light, default), `learnordie-technical`,
`learnordie-dark-room`.

Slide:

```json
{
  "id": "s-kpis", "title": "KPIs seit dem letzten Termin",
  "layout": "technical_figure_right", "intent": "comparison",
  "blocks": [ … 1 to 24 blocks … ],
  "speakerNotes": [{"id": "s-kpis-n1", "kind": "talkingPoint", "text": "Spoken sentences …"}],
  "sourceRefs": [{"id": "src-kpi", "sourceType": "manual", "label": "Native KPI-Auswertung vom 12.10."}]
}
```

`intent`: `title`, `concept`, `definition`, `explanation`, `derivation`,
`example`, `comparison`, `summary`, `quiz`, `transition`.
Sources: `{id, sourceType, label, url?, locator?}`; `label` ≤220 characters,
`url` for links (use `sourceType: "url"`), `locator` (≤180) for where in the source
(e.g. `KPI read 12.10. 10:58`). `sourceType`: `material`, `asset`, `url`,
`legacy`, `manual`, `import`. At least one and at most 20 sources per slide.

Blocks (all fields strict, texts trimmed):

| type | fields |
|---|---|
| `heading` | `text` ≤140, `level` 1–3 |
| `paragraph` | `text` ≤1200 |
| `bulletList`, `numberedList` | `items` 1–10, each ≤220 |
| `callout` | `tone` `key`/`info`/`warning`/`tip`, `title`?, `text` ≤1200 |
| `definition` | `term` ≤160, `definition` ≤1200, `example`? |
| `table` | `columns` 1–8, `rows` 1–40 of 1–8 cells (≤400, same width as columns), `mobileStrategy` `stack`/`scroll`/`cards`, `caption`? |
| `process` | `steps` 2–8 of `{title, text?}` |
| `comparison` | `left`, `right`: `{title, body?, items? (1–6)}`; neither side empty |
| `quote` | `text`, `attribution`? |
| `spacer` | `size` `small`/`medium`/`large` |
| `scene3d` | `sceneId`, `altText` ≤320 (what the scene shows, for screen readers), `caption`?, `accent`? (`#rrggbb`), `data` (business scenes) |

Layouts and the block types they show well: `title_statement` and
`section_divider` (heading, paragraph, quote), `technical_one_column` and
`technical_two_column` (all), `technical_figure_right` / `technical_figure_left`
(text blocks plus one `scene3d` or figure), `table_focus` (table), `comparison_split`
(comparison), `process_steps` (process), `case_study` (all).

## 3D scenes for numbers

The values below only show the shape. A real deck carries the measured values
and nothing else; never copy these numbers.

`business.kpi-bars` — current against the penultimate Regeltermin:

```json
{"id": "s-kpis-scene", "type": "scene3d", "sceneId": "business.kpi-bars",
 "altText": "Vier KPIs mit Vorwert: MRR steigt, offene Bugs sinken.",
 "caption": "Stand 12.10. gegenüber dem vorletzten Regeltermin (28.09.)",
 "data": {"items": [
   {"label": "Umsatz MRR", "value": 18400, "previous": 16900, "unit": "€"},
   {"label": "Aktive Nutzer", "value": 1240, "previous": 1310},
   {"label": "Gemergte PRs", "value": 23, "previous": 17},
   {"label": "Offene Bugs", "value": 9, "previous": 14, "better": "lower"}]}}
```

One to eight items; `label` ≤48. `unit` on the scene applies to every item
without its own `unit`; leave it out when items carry units, and never send
an empty string. `previous` is the value shown at the
penultimate Regeltermin and is omitted when that value is not stored (no delta then);
`better` is `lower` for costs, bugs, latency. The scene computes the ±% itself;
do not write percentages into labels. Bullets name the change in words or in
absolute numbers (`412 statt 301`).

`business.trend` — one value over time:

```json
{"id": "s-exit-scene", "type": "scene3d", "sceneId": "business.trend",
 "altText": "Exitwert E5 seit August, zuletzt 4,8 Mio €, Ziel 6 Mio €.",
 "data": {"label": "Exitwert E5", "unit": "Mio €", "target": 6,
   "points": [{"label": "07.09.", "value": 3.9}, {"label": "14.09.", "value": 4.1},
              {"label": "28.09.", "value": 4.4}, {"label": "12.10.", "value": 4.8}]}}
```

Two to twenty-four points, `label` ≤24 (`dd.mm.`), only stored or measured
values. A meeting without a stored value gets no point; name the gap in the
caption. With fewer than two real points, leave the trend out and say why. Business scenes always need `data`;
the lecture scenes (`modell.*`) take no data and do not belong in a Jour fixe.
Put one scene per slide, in a `technical_figure_right` or `_left` layout.

## Speaker notes are the narration

`publish_deck` turns each slide's `talkingPoint` notes into the text that is read
aloud. Write them as two to five spoken sentences in the deck language, at most
about 900 characters per slide, no markdown, no lists, no URLs. Write as you
would speak: spell out units and signs (Euro, Millisekunden, Pull Request
Nummer 14). Say what changed
and what the Owner should decide. Notes talk about the project, never about the
slide layout ("links steht …") or your own choices ("ich zeige keine Werte"). Notes of kind `source` hold citations; they
are not read aloud.

## Editing an existing presentation

`apply_edits` takes learnordie edit operations, applied in order and validated
as a whole: `updateDocument {patch}`, `insertSlide {slide, index|beforeSlideId|afterSlideId}`,
`updateSlide {slideId, patch: title|layout|intent|sourceRefs|speakerNotes|quizAnchors}`,
`deleteSlide {slideId}`, `moveSlide {slideId, …}`, `insertBlock {slideId, block, …}`,
`patchBlock {slideId, blockId, patch}`, `replaceBlock {slideId, blockId, block}`,
`deleteBlock {slideId, blockId}`, `moveBlock {slideId, blockId, …}`,
`upsertSpeakerNote {slideId, note}`, `deleteSpeakerNote {slideId, noteId}`.
Each operation has `kind` plus these fields.

If the Owner has already drawn on a slide, that slide has a `canvas`; block and
title edits on it fail with `edit.canvas_authoritative`. Leave such slides as
they are, or change only notes and sources. Never overwrite the Owner's canvas.

A canvas you write yourself (`save_canvas {slide_id, scene}`) is a
`learnordie.excalidraw.v1` scene: `{version, width: 1600, height: 900,
backgroundColor: "#fffef8", elements, files: {}}`. Text elements need `text`,
`originalText`, `fontSize`, `fontFamily: 1` (the handwriting font); arrows and lines need
`points`; a 3D scene is an `embeddable` with
`customData.learnordie = {type: "scene3d", sceneId, data}` and
`link: "https://learnordie.invalid/embed/<element id>"`. Prefer blocks.

## Limits

Document at most 4 MiB per request and 8 MiB stored, 1–160 slides, at most 100
slides for the meeting deck. Tool answers above 192 KiB are refused: use
`read_presentation` (manifest and outline) and `read_slide` for detail.
