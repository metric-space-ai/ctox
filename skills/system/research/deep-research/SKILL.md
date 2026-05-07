---
name: deep-research
description: Use when a user asks for a rigorous research study, feasibility assessment, literature review, technology comparison, scientific-paper search, or decision-grade report. This skill combines CTOX web search, deep research evidence gathering, browser use, web read/scrape, lawful paper metadata discovery, and document generation.
class: system
state: active
cluster: research
---

# Deep Research

Use this skill when the task requires more than a single web search: scientific or technical literature, competing hypotheses, feasibility scoring, current industrial evidence, patents, standards, or a polished research report.

## Required Workflow

1. Restate the research question as a decision problem.
2. Extract constraints, assumptions, and unknowns from the user prompt and any attached documents.
3. Run `ctox_deep_research` first when available. Use `depth: "exhaustive"` for feasibility studies, technology downselects, or scientific claims.
4. Follow with targeted `ctox_web_search`, `ctox_web_read`, `ctox_web_scrape`, and `ctox_browser_automation` calls when the evidence bundle has gaps.
5. For scientific literature, search across publisher pages, DOI/Crossref/Semantic Scholar metadata, PubMed/PMC where relevant, arXiv or institutional repositories, patents, and industry/application notes.
6. Use Anna's Archive only as metadata-only bibliographic discovery. Do not download, request, summarize, or reproduce unauthorized copyrighted full text from it. Prefer DOI pages, abstracts, publisher landing pages, and lawful open-access copies.
7. Build an evidence matrix before writing: technology, physical principle, penetration/stand-off expectations, sensitivity, throughput, confounders, maturity, key sources, and uncertainty.
8. Separate evidence from inference. Mark weak assumptions explicitly.
9. When the user asks for a Word/PDF report, produce the document artifact with citations, figures/tables where legally usable, and a concise executive summary.

## Research Standards

- Prefer primary sources, review papers, standards, patents, and industrial validation over blog posts.
- For current research, include publication year and DOI when available.
- Treat paywalled abstracts and metadata as limited evidence. Do not claim details that were not visible.
- Use at least two independent sources for important feasibility claims.
- Include negative evidence and technology failure modes, not only promising candidates.
- For engineering feasibility, score candidates on explicit axes rather than ranking by narrative impression.

## Report Structure

Use this default structure unless the user specifies otherwise:

1. Executive Summary
2. Objective and Scope
3. System / Material Stack and Inspection Problem
4. Methodology and Search Strategy
5. Candidate Contactless Technologies
6. Evidence Review
7. Feasibility Matrix
8. Recommended Experimental Design
9. Risks, Unknowns, and Decision Gates
10. Conclusion and Recommendation
11. References

## Technology Coverage Checklist

For hidden metal structures in composites or coatings, consider at least:

- Terahertz time-domain / frequency-domain imaging
- Eddy-current testing, pulsed eddy current, remote field eddy current, eddy-current thermography
- Magnetic flux leakage / magneto-optical imaging when material physics permits
- Microwave / millimeter-wave imaging and radar
- Infrared thermography, pulse thermography, lock-in thermography
- Laser shearography / vibrothermography for secondary defect signatures
- X-ray radiography / computed tomography when acceptable despite radiation and access constraints
- Electrical capacitance / impedance / dielectric spectroscopy
- Optical / hyperspectral imaging only when there is a credible optical path or indirect surface signature

## Completion Criteria

The final answer or report must include:

- A clear recommendation, not only a literature summary.
- A feasibility score with uncertainty for each major technology.
- Explicit handling of confounders such as CFK conductivity, copper mesh geometry, coatings, primer, and any continuous metallic foil behind the mesh.
- Source-backed claims with citations.
- A short list of next experiments that would falsify or validate the recommendation.

## DOCX Helper

When a Word report is requested, prefer the bundled helper:

```bash
python3 skills/system/research/deep-research/scripts/build_research_report_docx.py \
  --input /path/to/report.json \
  --output /path/to/report.docx
```

The JSON input should contain `title`, optional `subtitle`, `metadata`, `sections`, optional `figures`, optional `tables`, and `references`. Use `python-docx` from the bundled runtime if the system Python lacks it.
