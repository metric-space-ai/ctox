# Native workflow examples

These ids and numbers belong to an isolated authoring fixture. Replace them
with the currently authorised project, meeting and stored measurements for a
real deck. Never copy fixture values into a customer's presentation.

## Draft and validation

Use `learnordie.slide-agent.v1` to form the source brief and slide outline:
group the supplied sources into topics, state the meeting's decision, assign
stable slide ids and source ids, and then produce `learnordie.slide.v1`.
The native `read_guide` lists the engine's current layouts and registered
business scenes; do not guess their fields.

The adjacent `isolated-example.json` contains two slides. Its source reference
identifies the values as a test fixture. One slide contains explicit font-family
1 handwriting and a movable rectangle; the other carries a typed KPI scene.
It omits a historical baseline and says why, rather than generating a delta.

Call `business_os.presentation_read` with this envelope, placing the complete
JSON document in `request.document`:

```json
{"action":"validate_document","request":{"project_id":"isolated-project","meeting_id":"isolated-meeting","document":{}}}
```

The empty object above illustrates the envelope only and is not a valid deck.
If validation reports a missing source, cite the actual stored source on the
named slide. Do not add a fictitious citation to silence the validator.

A create request then supplies `operation_id`, `project_id`, `meeting_id`
and the validated `document`. Save the native receipt and manifest revision.

## Repair a structured slide

For the isolated fixture at revision 1, the following batch changes an existing
heading and narration without changing slide ids:

```json
{
  "action": "apply_edits",
  "request": {
    "operation_id": "fixture-repair-1",
    "project_id": "isolated-project",
    "meeting_id": "isolated-meeting",
    "expected_revision": 1,
    "operations": [
      {
        "kind": "patchBlock",
        "slideId": "s-kpis",
        "blockId": "kpi-heading",
        "patch": {"text": "Erfasste Testläufe"}
      },
      {
        "kind": "upsertSpeakerNote",
        "slideId": "s-kpis",
        "note": {
          "id": "kpi-note",
          "kind": "talkingPoint",
          "text": "Diese Zahlen stammen nur aus der isolierten Testquelle. Ein historischer Vergleich ist nicht verfügbar."
        }
      }
    ]
  }
}
```

If a slide now has an owner canvas, a block edit returns
`edit.canvas_authoritative`. Read that slide, preserve the sketch, and choose
a canvas edit with the owner or change only its notes and sources.

## Save and recover a canvas edit

Read the current presentation manifest and slide before editing. Keep the whole
scene and update its selected text or element coordinates. For the fixture
`s-canvas`, change both `text` and `originalText` on `canvas-heading`,
and move `canvas-box` by adjusting its `x` and `y`. Keep all other elements,
ids, files and embedded scene metadata.

`presentation_update` action `save_canvas` takes:
`operation_id`, `project_id`, `meeting_id`, `expected_revision`,
`slide_id` and the complete `scene`. The revision must come from the fresh
manifest, not from this example.

If the connection drops after submitting, repeat exactly the same request with
the same operation id. If the server reports a revision conflict, read the new
manifest and slide; do not replay a stale canvas under a new id. After a
successful receipt, reopen through the room's native read path and check the
saved text and rectangle positions.

For the meeting player, publish the stored presentation revision with the
current `expected_meeting_revision` and next `deck_revision`. Narration,
comments and to-dos retain the same slide ids.
