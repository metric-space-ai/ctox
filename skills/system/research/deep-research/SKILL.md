---
name: deep-research
description: Use when a user asks for rigorous deep research, feasibility studies, literature reviews, market research, competitive analysis, scientific-paper search, technology comparison, or decision-grade Word/PDF reports. Routes to specialized research modules while using CTOX web search, deep research evidence gathering, browser use, web read/scrape, lawful paper metadata discovery, and document generation.
class: system
state: active
cluster: research
---

# Deep Research

This skill is a router plus shared research contract. Pick exactly one primary module before researching; use secondary modules only when the user explicitly asks for a hybrid report.

## Module Router

- **Machbarkeitsstudie / technical feasibility**: use `references/modules/machbarkeitsstudie.md` for technology downselects, scientific/engineering feasibility, NDT, physical effects, experiment design, patents, standards, or material/process risks.
- **Marktrecherche / market research**: use `references/modules/marktrecherche.md` for market sizing, customer segments, demand drivers, pricing, procurement behavior, distribution, regulation, adoption barriers, and go-to-market evidence.
- **Wettbewerbsanalyse / competitive analysis**: use `references/modules/wettbewerbsanalyse.md` for competitor landscapes, product comparisons, positioning, feature/pricing matrices, moats, channel conflict, and strategic gaps.
- **Unclear or mixed brief**: start with `references/modules/shared-core.md`, infer the dominant module from the deliverable, and state any module blending in the final report.

## Required Workflow

1. Reframe the user request as a decision problem and select the module.
2. Read `references/modules/shared-core.md`, then the selected module reference.
3. Extract constraints, assumptions, unknowns, target geography, time horizon, and output format.
4. Run `ctox_deep_research` first when available. Use `depth: "exhaustive"` for decision-grade reports, feasibility studies, scientific claims, market maps, or competitive landscapes.
5. Treat the returned `research_workspace.path` as the authoritative project folder. Read/write notes there so the work can continue after compaction or handoff.
6. Follow with targeted `ctox_web_search`, `ctox_web_read`, `ctox_web_scrape`, and `ctox_browser_automation` calls for gaps the evidence bundle does not cover.
7. Inspect `data_links.json`. If sources point to GitHub, datasets, notebooks, repositories, or supplementary data, inspect them when relevant; derive tables/diagrams from the data when it materially improves the report.
8. Build the module-specific evidence matrix before writing. Do not write the report directly from search snippets.
9. Run the report-writing loop: evidence matrix -> outline -> section drafts -> tables/figures -> final DOCX/PDF -> QA notes.
10. Separate evidence from inference, mark weak assumptions, and cite source-backed claims.
11. When the user asks for Word/PDF output, produce a document artifact with report sections, tables, figures where legally usable, and references.

## Source Policy

- Prefer primary sources, review papers, standards, patents, regulator/company filings, product docs, pricing pages, credible databases, and industrial validation over summaries or blogs.
- For current research or market facts, include publication/access date, year, and DOI/URL where available.
- Treat paywalled abstracts, metadata, snippets, and login-gated pages as limited evidence.
- Use Anna's Archive only as metadata-only bibliographic discovery. Do not download, request, summarize, or reproduce unauthorized copyrighted full text from it. Prefer DOI pages, abstracts, publisher landing pages, and lawful open-access copies.
- For figures, capture candidate source images with source URL and usage note. Embed only if lawful/reasonable for the context, or redraw as an original schematic and cite the source inspiration.

## Research Workspace Contract

Every serious run should leave a folder with:

- `manifest.json` for the run summary and artifact paths.
- `evidence_bundle.json` for the full machine-readable evidence.
- `sources.jsonl` for deduplicated sources.
- `reads/` for per-source read payloads.
- `snapshots/` for saved HTML/PDF/text snapshots when available.
- `data_links.json` for GitHub/data/notebook links discovered in sources.
- `synthesis/` for agent-written interim notes, matrices, outlines, and report drafts.
- `CONTINUE.md` with instructions for resuming after context compaction.

Use the folder as a research project, not as an output dump. For long reports, write interim files such as `synthesis/evidence-matrix.md`, `synthesis/report-outline.md`, and `synthesis/open-questions.md` before drafting the final document.

## Report Writing Contract

- Never use control/reference documents as hidden input to the research or writing agent. If the user supplies a sample only for later evaluation, do not attach it, quote it, paraphrase it, extract images from it, or copy its structure into the agent prompt.
- Write from `evidence_bundle.json`, `sources.jsonl`, `reads/`, `snapshots/`, inspected data links, and agent-created synthesis notes.
- Empty or placeholder evidence is a hard failure. Do not mark a deep-research report done when `sources.jsonl` is empty, `evidence_bundle.json` has no sources, `reads/` is empty, or the synthesis files contain placeholders such as "provisional", "research underway", "checks planned", "TBD", or "placeholder".
- Create these files before the final document for decision-grade work:
  - `synthesis/evidence-matrix.md`
  - `synthesis/report-outline.md`
  - `synthesis/technology-scores.md` or module equivalent
  - `synthesis/figure-plan.md`
  - `synthesis/report-draft.md`
  - `synthesis/qa-notes.md`
- Figures must be either legally usable source figures with explicit usage notes, or original schematics/diagrams generated from the synthesis. Do not reuse figures from a reference/control document.
- The final document must be a written study, not a raw evidence export. It needs an argument, recommendations, uncertainty, decision gates, tables, and references.
- The final `.docx` path must be a ZIP/DOCX file. A directory ending in `.docx` is a hard failure.
- Before answering success, run the deliverable validator. If it fails, continue the research/writing loop or report the failure instead of claiming completion.

## DOCX Helper

When a Word report is requested, prefer the bundled helper:

```bash
python3 skills/system/research/deep-research/scripts/build_research_report_docx.py \
  --input /path/to/report.json \
  --output /path/to/report.docx
```

The JSON input should contain `title`, optional `subtitle`, `metadata`, `sections`, optional `figures`, optional `tables`, and `references`. Use `python-docx` from the bundled runtime if the system Python lacks it.

Then validate:

```bash
python3 skills/system/research/deep-research/scripts/validate_research_deliverable.py \
  --workspace /path/to/research-workspace \
  --docx /path/to/report.docx \
  --min-sources 20 \
  --min-reads 5 \
  --min-draft-chars 8000 \
  --require-call-counts
```

For exhaustive scientific reports, raise the thresholds rather than lowering them. A validator failure means the deliverable is not complete.
