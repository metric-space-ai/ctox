---
name: knowledge-maintenance
description: Maintain the Knowledge Books of the Knowledge app - change a skill, skillbook or resource document, change or execute a runbook, create, import or export a Knowledge Book, or answer a context chat about Knowledge data. Use for every ctox.knowledge.* task of the Knowledge app. Changes are delivered as one writeback file in the task workspace; the Knowledge app applies them when the task completes.
class: app
app: knowledge
state: active
cluster: knowledge
---

# Knowledge Maintenance (Knowledge app)

The Knowledge app shows Knowledge Books: a **skillbook** groups one main
**skill**, its **runbooks** (concrete, repeatable procedures, split into
labeled runbook items), **resources** and **dataframes** (Knowledge tables).
You receive one bounded task from the app. The instruction names the action;
the task payload carries the selection (`selected_item`, `runbook`,
`runbook_item`, `knowledge_book`, `import_request`, `export_request`,
`context`, `markdown`, `draft`).

You do **not** write into Business OS collections or databases yourself, and
you do not start child agents. Everything that changes Knowledge goes into one
result file; the app validates it and applies it to its collections
`knowledge_items` and `knowledge_runbooks` when the task completes.

## Actions

| Command type | What to do |
|---|---|
| `ctox.knowledge.document.modify` | Check the edited markdown of `selected_item` and persist it. Keep the skill / skillbook / runbook / resource structure; do not flatten or rename elements. |
| `ctox.knowledge.runbook.modify` | Apply `draft` (title, prompt) to `runbook`. Update runbook, its items, resource bindings and execution contract consistently; runbook item labels stay stable. |
| `ctox.knowledge.runbook.execute` | Execute the runbook text with the selected Knowledge as context. Deliver `required_output`: result, check status, open inputs, evidence lineage and writeback artifacts. Change Knowledge only if the runbook says so. |
| `ctox.knowledge.book.create` | Create a new Knowledge Book from `knowledge_book` (title, domain, status, summary, markdown): skillbook, an initial skill, and prepared runbook / dataframe slots. |
| `ctox.knowledge.book.import` | Import `import_request.source` (path, markdown, URL or runtime source) as a Knowledge Book. Extract skillbook, skill, runbooks and dataframe definitions; never overwrite existing Knowledge structure unchecked - reuse ids only where the content is the same element. |
| `ctox.knowledge.book.export` | Export the books in `export_request.scope` in `export_request.format` with skillbook, runbook and dataframe metadata. Write the export files into the task workspace under `export/`; list them in `artifacts`. Knowledge itself does not change. |
| `ctox.knowledge.chat` | Answer or carry out the user message about the given `context` (data mode). Change Knowledge only when the message asks for it. |

## Rules

1. **Stable identity.** Update existing elements in place under their `id`.
   New elements get a readable stable id (`<domain>/<kind>/<slug>`), never a
   random one. No parallel copies of the same book.
2. **Structure.** `kind` is one of `skillbook`, `skill`, `runbook`,
   `resource`, `dataframe`. Runbook items keep their labels (e.g. `REG-03`);
   the labeled item is the retrieval unit.
3. **Provenance.** Keep `source_path`, source references and evidence lineage
   that an element already has; add them for new content you derive from a
   source.
4. **Bounded.** Do exactly the requested action. If an input is missing, do
   not guess: report it under `open_inputs` and leave the element unchanged.

## Result (required)

Write `knowledge/writeback.json` (UTF-8 JSON) into the task workspace - also
when nothing changes (then with empty `items` and `runbooks`):

```json
{
  "schema_version": "ctox.knowledge.writeback.v1",
  "summary": "what was done, two or three sentences",
  "items": [
    { "id": "…", "kind": "skillbook|skill|resource|dataframe", "title": "…", "subtitle": "…", "summary": "…", "source_path": "…", "markdown": "full content", "payload": {} }
  ],
  "runbooks": [
    { "id": "…", "kind": "runbook", "title": "…", "summary": "…", "prompt": "runbook text", "items": [ { "label": "REG-01", "title": "…", "markdown": "…" } ], "payload": {} }
  ],
  "artifacts": ["export/…"],
  "open_inputs": []
}
```

The app rejects the whole file (nothing is applied, the task shows as failed)
when it is not valid JSON, has another `schema_version`, or an element has no
`id`, no `title` or an unknown `kind`. Finish your turn with a short
plain-text summary.
