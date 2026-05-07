# Shared Deep Research Core

Use this file for every deep-research module.

## Evidence Workflow

1. Turn the prompt into a concise research brief: decision, audience, geography, timeframe, required artifact, and non-goals.
2. Build a query plan with broad, specialized, negative-evidence, and source-type queries.
3. Run `ctox_deep_research` first. Inspect `research_call_counts`; for deep work, hundreds of source contacts are expected unless the niche is genuinely small.
4. Open the created `research_workspace.path`. Use it as the durable project folder across loops and compactions.
5. Add targeted reads/scrapes for missing facts, source conflicts, pricing pages, tables, standards, patents, or full-text open-access sources.
6. Inspect `data_links.json`. Follow relevant GitHub repositories, datasets, notebooks, supplementary material, or public data portals. Save notes under `synthesis/` and convert useful data into tables/diagrams for the report.
7. Deduplicate sources by canonical URL/DOI/company/product.
8. Create a source ledger before synthesis:
   - source
   - type
   - claim supported
   - date/year
   - credibility
   - limitation
   - citation URL/DOI
9. Write the report from the ledger, not from search results alone.

## Quality Gates

- Important factual claims need citations.
- Important recommendations need both supporting and limiting evidence.
- Weak assumptions must be explicit.
- If call counts are low for a supposedly deep task, state that the evidence base is insufficient and run additional targeted searches.
- If a figure is discovered, record source page, image URL, caption/context, and usage status before embedding.
- If data/repository links are discovered, state whether they were inspected and whether they changed the findings. Do not ignore relevant code/data links.
- If the user gives a reference/control artifact for evaluation, keep it out of the research and writing context. Use it only after the candidate deliverable exists, and only to compare quality/coverage.

## Long-Running Writing Discipline

For reports that may span multiple turns or compactions:

- Update `CONTINUE.md` or add `synthesis/status.md` after each major loop.
- Keep open questions and missing evidence in `synthesis/open-questions.md`.
- Keep a running citation map from report claims to `sources.jsonl` indices.
- Do not skip directly from evidence gathering to DOCX generation.
- After compaction, resume from `manifest.json`, `CONTINUE.md`, and `synthesis/status.md`.

## Standard Report Metadata

Include these fields in Word/PDF reports when available:

- Auftrag / question
- Stand / date
- Research module
- Geography and timeframe
- Research depth and call counts
- Research workspace path
- Source mix
- Key limitations
