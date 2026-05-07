# Shared Deep Research Core

Use this file for every deep-research module.

## Evidence Workflow

1. Turn the prompt into a concise research brief: decision, audience, geography, timeframe, required artifact, and non-goals.
2. Build a query plan with broad, specialized, negative-evidence, and source-type queries.
3. Run `ctox_deep_research` first. Inspect `research_call_counts`; for deep work, hundreds of source contacts are expected unless the niche is genuinely small.
4. Add targeted reads/scrapes for missing facts, source conflicts, pricing pages, tables, standards, patents, or full-text open-access sources.
5. Deduplicate sources by canonical URL/DOI/company/product.
6. Create a source ledger before synthesis:
   - source
   - type
   - claim supported
   - date/year
   - credibility
   - limitation
   - citation URL/DOI
7. Write the report from the ledger, not from search results alone.

## Quality Gates

- Important factual claims need citations.
- Important recommendations need both supporting and limiting evidence.
- Weak assumptions must be explicit.
- If call counts are low for a supposedly deep task, state that the evidence base is insufficient and run additional targeted searches.
- If a figure is discovered, record source page, image URL, caption/context, and usage status before embedding.

## Standard Report Metadata

Include these fields in Word/PDF reports when available:

- Auftrag / question
- Stand / date
- Research module
- Geography and timeframe
- Research depth and call counts
- Source mix
- Key limitations
