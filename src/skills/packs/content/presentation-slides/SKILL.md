---
name: presentation-slides
description: Create or edit a Workjet project presentation in the Learnordie handwriting canvas and 3D style, then store revisions through CTOX presentation tools. Use for Jour fixe decks and canvas edits, not PowerPoint exports.
class: installed_packs
state: stable
cluster: content
---

# Presentation slides

Create a source-backed `learnordie.slide.v1` document using the
`learnordie.slide-agent.v1` authoring contract. Workjet renders its blocks as a
1600×900 handwriting canvas and registered three.js scenes. Preserve the
Learnordie themes, font family 1, runtime provenance and bundled licences.

Call `business_os.presentation_read` with
`{"action":"read_guide","request":{}}` first. The installed guide and tool
descriptors define the schemas, limits, scene registry and edit operations.
Use the current installed contract rather than copying a schema into prompts.

## Authoring

- Resolve the project and current meeting through `business_os.jour_fixe_read`.
  The presentation MCP tools require that project's currently leased native
  Supervisor. A different meeting, a user role claimed in a prompt or a guessed
  Supervisor identity is not authority.
- Gather actual goals, stored KPI values, worker activity, merged PRs, decisions
  and confirmed to-dos. Build a brief and outline with stable source ids before
  drafting. Every slide has `sourceRefs`; include source locators and spoken
  `talkingPoint` notes. Never invent model ids or account capabilities.
- For a Regeltermin, cover goals, KPIs, Essential KPI/exit value, workers, merged
  PRs and next decisions. Compare with the **penultimate** earlier occurrence
  returned by `read_history`, with the same definition, unit and time window.
  Missing sources, a missing baseline or a zero denominator have no invented
  value or percentage. Name the gap on the slide.
- Give each slide one idea and a small amount of concrete text. Use
  `business.kpi-bars` or `business.trend` only for measured data supported by
  sources. Preserve an honest missing-data slide when no scene can be supported.
  A deck demonstrating the requested style includes a data-backed 3D scene;
  an unrelated lecture scene does not substitute for evidence.
- Prefer structured blocks; Workjet converts them to handwriting canvas.
  Explicit canvas text uses `fontFamily: 1`. Scene embeds use the installed
  guide's sentinel link and typed `customData.learnordie` payload.

[Authoring examples](references/native-workflow.md) show an isolated fixture,
a repair batch and canvas-save recovery. The
[example document](references/isolated-example.json) is synthetic authoring
material, never a real project's KPI source.

## Store and edit

Validate the draft with `presentation_read` action `validate_document`, bound to
the authorised `project_id` and `meeting_id` in the installed descriptor.
Repair errors at their supplied paths using `repairHint`; shorten crowded
slides when layout warnings reveal an unreadable composition.

Create once with `presentation_update` action `create_presentation`.
Use `apply_edits`, `replace_document` or `save_canvas` for subsequent
revisions with the manifest's `expected_revision`. Retain one
`operation_id` and its complete request until the native receipt is known.
An uncertain acknowledgement is recovered by replaying that identical request.
A conflict requires a fresh read and an explicit reconciliation, not an
unconditional overwrite.

Canvas is authoritative once the owner has sketched on a slide. Preserve its
elements, ids and embeds; do not regenerate its blocks or canvas to fix text
elsewhere. Apply structured edits to unaffected slides or notes and sources.
Keep slide ids stable so comments, to-dos and narration remain attached.

For a Jour fixe, `publish_deck` derives the meeting's narration deck from the
stored presentation revision. Read the current meeting revision and narrate
through `business_os.jour_fixe_update`; an audio reference alone is not proof
of playable narration. Confirmed to-dos remain the Supervisor's goal through
the existing meeting tools.

Presentations and canvas changes persist through native policy-checked commands
and the Business OS RxDB/WebRTC path. A local export, browser cache or HTTP
upload does not count as a saved presentation.

## Completion

Report the actual native revision and any missing evidence. Product acceptance
requires the installed Workjet room: generated handwriting and 3D are visible,
a text edit and moved shape save successfully, and both persist after full
quit/reopen. Keep end-state screenshots; source validation alone is not this
acceptance.
