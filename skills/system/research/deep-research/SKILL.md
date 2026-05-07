---
name: deep-research
description: Drive a `ctox report` run end-to-end for evidence-based reports. Triggers on requests for "Machbarkeitsstudie", "feasibility study", "deep research report", or any multi-iteration evidence-based document. Orchestrates the `ctox report` subcommands stage by stage and runs the critique/revise loop until `ctox report check` returns overall_pass=true. Currently ships the `feasibility` preset.
class: system
state: active
cluster: research
---

# Deep Research

This skill produces decision-grade reports through the `ctox report` CLI. The CLI is the deterministic state machine; this skill carries the procedural knowledge for filling each stage with evidence-backed content.

## Hard rules — never break these

1. **No claim without evidence.** Every `claim_kind = finding|recommendation|caveat` must carry at least one `evidence_id` that resolves to a `report_evidence` row in this run. The CLI rejects unsupported claims at insert time; do not work around it.
2. **No structure without content.** If a section has nothing concrete to say, leave it empty. The deterministic draft assembler omits empty sections; do not invent prose to fill space.
3. **No verdict before evidence.** Run `ctox report evidence import` (or `evidence add`) before scoring or claiming. Scoring without evidence is rejected.
4. **No numeric score without rubric.** Define rubrics with `ctox report scoring define-rubric` before scoring. Numeric values without `rubric_anchor` are rejected.
5. **No render before check.** `ctox report render` refuses to run unless the latest `ctox report check` for that version has `overall_pass=true`.
6. **No witness, no revise.** A revised manuscript with the same `body_hash` as its parent is rejected. Real edits required.
7. **No filler, no hedging without anchor, no internal vocabulary.** The check stage rejects "im Folgenden werden …", "could possibly", "in some cases" without a confidence anchor or assumption note. It also rejects internal CTOX vocabulary leaking into prose.
8. **Reference documents are never silent input.** If the operator hands you a sample report, do NOT extract its structure, prose, or images into the agent's prompt. Read the brief, then start fresh from `ctox report new`.

## Workflow

```bash
# 1. Create the run.
ctox report new feasibility --topic "Contactless inspection of the LSP grid in CFRP" --language en

# 2. Bound the scope (leading questions, out-of-scope, assumptions, disclaimer).
ctox report scope --run-id <id> --from-file scope.json

# 3. Frame requirements from the leading questions.
ctox report frame --run-id <id> --from-file requirements.json

# 4. Enumerate the option space.
ctox report enumerate --run-id <id> --from-file options.json

# 5. Gather evidence. Use `import` for an automated pass via the deep_research engine,
#    `add` for hand-curated DOIs/arXiv-IDs/URLs.
ctox report evidence import --run-id <id> --query "<your search>" --depth standard
ctox report evidence add --run-id <id> --from-file extra_evidence.json

# 6. Define scoring rubrics. Each (axis_code, level_code) needs a definition.
ctox report scoring define-rubric --run-id <id> --from-file rubrics.json

# 7. Fill the matrix. Each cell pins to evidence_ids and an optional rubric_anchor.
ctox report scoring set-cell --run-id <id> --from-file cells.json

# 8. Add scenarios.
ctox report scenarios add --run-id <id> --from-file scenarios.json

# 9. Add risks and mitigations.
ctox report risks add --run-id <id> --from-file risks.json

# 10. Add the prose claims that will appear in management_summary, detail_assessment,
#     recommendation. Each claim row carries text + claim_kind + evidence_ids.
ctox report claims add --run-id <id> --from-file claims.json

# 11. Build the deterministic manuscript.
ctox report draft --run-id <id>

# 12. Run validators. If overall_pass=false, iterate.
ctox report check --run-id <id>

# 13. Critique loop (only if check failed). Self-mode reads the check report;
#     external-mode accepts a structured findings file you produced.
ctox report critique --run-id <id> --mode self
ctox report revise --run-id <id> --from-file new_manuscript.json

# 14. Render once check passes. Markdown is fast; DOCX uses the bundled python-docx helper.
ctox report render --run-id <id> --format md
ctox report render --run-id <id> --format docx --out output/report.docx

# 15. Mark final.
ctox report finalize --run-id <id>
```

See [references/stage_contracts.md](references/stage_contracts.md) for every JSON payload shape, and [references/manuscript_schema.md](references/manuscript_schema.md) for the Manuscript v1 format.

## Source policy

- Prefer DOIs, arXiv IDs, peer-reviewed venues, standards bodies, manufacturer datasheets, and patent filings over blog posts and summaries.
- The `evidence import` command runs `ctox_deep_research`. Set `--depth exhaustive` for decision-grade reports.
- DOIs found in snippets are normalized via Crossref → OpenAlex (fallback) before being recorded. Failures fall back to a `kind=url` evidence row.
- Use [scripts/doi_resolve.py](scripts/doi_resolve.py) when you have a DOI list outside an evidence bundle.

## When to abort instead of revise

If `ctox report check` fails the same hard validator three times in a row, the run is structurally broken. Abort with `ctox report abort --run-id <id> --reason "<text>"` and re-scope. Fixing slop by piling on more revise rounds is an anti-pattern.

## Output expectations

- `render md` produces a Markdown deliverable suitable for review. Use this for internal review and for diffing between revisions.
- `render docx` produces a Word document via [scripts/render_manuscript.py](scripts/render_manuscript.py). Open it in Word/LibreOffice and refresh the table of contents (right-click → "Update Field").
- `render json` produces the raw Manuscript v1 — useful for debugging or as input to a custom renderer.

## What this skill does not do

- It does not produce reports without a real evidence pass. The `min_evidence_count` validator enforces a floor.
- It does not run the LLM directly. The skill orchestrates `ctox report` subcommands; the LLM-shaped calls (writing claims, drafting recommendations, producing critique findings) happen in the surrounding agent loop.
- It does not generate figures. Figures come from external sources or are explicitly marked as "to be drafted by the operator".
