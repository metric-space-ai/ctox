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
5. Follow with targeted `ctox_web_search`, `ctox_web_read`, `ctox_web_scrape`, and `ctox_browser_automation` calls for gaps the evidence bundle does not cover.
6. Build the module-specific evidence matrix before writing. Do not write the report directly from search snippets.
7. Separate evidence from inference, mark weak assumptions, and cite source-backed claims.
8. When the user asks for Word/PDF output, produce a document artifact with report sections, tables, figures where legally usable, and references.

## Source Policy

- Prefer primary sources, review papers, standards, patents, regulator/company filings, product docs, pricing pages, credible databases, and industrial validation over summaries or blogs.
- For current research or market facts, include publication/access date, year, and DOI/URL where available.
- Treat paywalled abstracts, metadata, snippets, and login-gated pages as limited evidence.
- Use Anna's Archive only as metadata-only bibliographic discovery. Do not download, request, summarize, or reproduce unauthorized copyrighted full text from it. Prefer DOI pages, abstracts, publisher landing pages, and lawful open-access copies.
- For figures, capture candidate source images with source URL and usage note. Embed only if lawful/reasonable for the context, or redraw as an original schematic and cite the source inspiration.

## DOCX Helper

When a Word report is requested, prefer the bundled helper:

```bash
python3 skills/system/research/deep-research/scripts/build_research_report_docx.py \
  --input /path/to/report.json \
  --output /path/to/report.docx
```

The JSON input should contain `title`, optional `subtitle`, `metadata`, `sections`, optional `figures`, optional `tables`, and `references`. Use `python-docx` from the bundled runtime if the system Python lacks it.
