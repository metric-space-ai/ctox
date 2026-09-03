---
name: systematic-research
description: Durable, evidence-bound systematic research. Use for scientific or technical source discovery, literature reviews, datasets, research libraries, living Knowledge, and cited reports. The workflow mines seed papers and datasets, follows backward references and forward citations breadth-first, verifies canonical originals, persists every discovery decision, and stops only after two orthogonal rounds add no eligible source.
class: system
state: active
cluster: research
---

# Systematic Research

This is the single entry point for durable research in CTOX. You are the
parent researcher. Do not spawn free child agents. Use the typed CTOX Web Stack,
persist work after every round, and leave a resumable research graph.

## Non-Negotiable Outcome

A research run is not a search result or a prose answer. It must leave:

1. a complete candidate ledger, including duplicates and rejections;
2. a verified source catalog containing canonical original content only;
3. **a content evaluation of every admitted source** — a relevance verdict and
   its engineering claims, not just the fact that the file was fetched;
4. a persisted literature/discovery graph in which every admitted source is
   connected through at least one claim or relevance edge;
5. claim-to-evidence-to-snapshot-to-source lineage;
6. the requested Knowledge tables, Skillbooks, Runbooks, and/or document;
7. an explicit saturation record.

A corpus of verified but unread sources is a failed run. Verification proves
that the bytes are authentic; it says nothing about what the source contains.

Search snippets, metadata records, abstracts, DOI resolver pages, and
`ctox_deep_research` inventories are candidates, never evidence.

## First Actions

1. Read the current task's `Research Run ID`, `Research Command ID`,
   `Research Attempt ID`, `web_stack_plan`, and canonical exclude list.
2. Run `ctox knowledge search --query "<topic>"` and inspect relevant existing
   tables. Extend the existing lineage instead of creating a parallel corpus.
3. Create or resume the candidate ledger and discovery graph before searching.
4. Identify the evidence gaps and choose several orthogonal seed queries.
5. For technical or scientific work, start with scholarly, agency, standards,
   and dataset adapters. Generic web search is not the only discovery surface.

## Web Stack Commands

In a managed Harness run, prefer the equivalent typed tools directly. Running
these CLI commands through the worker shell is also receipt-verifiable —
completion validation recognizes a plain `ctox web …` invocation recorded in
the durable rollout exactly like the typed call (the typed handler spawns the
same CLI). Chained commands, pipes, or redirects around the CLI are **not**
recognized and fail completion. In an operator shell, use the commands exactly
as shown.

### 1. Inspect available source adapters

```sh
ctox web sources list --tier P --tier S
ctox web sources info --id <source-id>
```

Use this before assuming an adapter is unavailable. For scientific work,
confirm that scholarly, agency, dataset, and standards sources are present.

### 2. Find scholarly seeds

```sh
ctox web scholarly search \
  --query "<precise scientific topic>" \
  --provider auto \
  --max-results 25 \
  --with-oa-pdf
```

Use additional provider-specific calls when one index has a gap:

```sh
ctox web scholarly search --query "<topic>" --provider openalex --max-results 25 --with-oa-pdf
ctox web scholarly search --query "<topic>" --provider crossref --max-results 25 --only-doi
ctox web scholarly search --query "<topic>" --provider semantic_scholar --max-results 25 --with-oa-pdf
```

Typed equivalent: `ctox_scholarly_search`. Persist each result as a candidate;
select strong seeds and continue with original reads.

### 3. Run focused web and portal searches

```sh
ctox web search \
  --query "<focused query or exact paper title>" \
  --context-size high \
  --include-sources
```

Constrain official portals when useful:

```sh
ctox web search --query "<query>" --domain ntrs.nasa.gov --context-size high --include-sources
ctox web search --query "<query>" --domain zenodo.org --context-size high --include-sources
```

Typed equivalent: `ctox_web_search`. Search results remain candidates.

### 4. Run one broad discovery sweep

```sh
ctox web deep-research \
  --query "<topic>" \
  --focus "<current evidence gap>" \
  --depth exhaustive \
  --max-sources 40 \
  --workspace "$PWD/research/deep-research-$(date +%s)"
```

Typed equivalent: `ctox_deep_research`. Use the command's requested depth
without downgrading it. One sweep is one discovery round, not the workflow.

### 5. Read and verify an original

```sh
ctox web read \
  --url "<canonical original full-text or data URL>" \
  --query "<precise fact, measurement, method, or reference-list intent>" \
  --workspace "$PWD/research/read-$(date +%s)"
```

Use `--find` for targeted extraction without replacing the reading intent:

```sh
ctox web read \
  --url "<canonical URL>" \
  --query "<evidence intent>" \
  --find "References" \
  --find "Data availability" \
  --workspace "$PWD/research/read-$(date +%s)"
```

Typed equivalent: `ctox_web_read`. Only this original-content read may create
admissible evidence. Preserve its receipt and snapshot unchanged.

### 6. Use browser/unblocking only when required

```sh
ctox web browser-prepare --dir "$PWD/research/browser" --install-browser
ctox web browser-capture \
  --url "<interactive source URL>" \
  --dir "$PWD/research/browser" \
  --out-dir "$PWD/research/capture"
ctox web unlock list-probes
ctox web unlock list-vectors
```

Browser captures can discover an original download route. They do not replace
the typed original read and evidence receipt.

Never pipe Web Stack results through `head`, `tail`, or byte truncators. Use
`--max-results`, `--max-sources`, or other command-native limits.

## Core Loop: Mine The Literature Graph

Run this loop until the saturation gate passes:

1. **Discover seeds**
   - Use `ctox_scholarly_search` for papers, DOI/OA records, authors, references,
     and cited-by records.
   - Use `ctox_web_search` for focused official portals, exact titles, datasets,
     standards, and alternate original routes.
   - Use `ctox_deep_research` only as one broad candidate sweep. It never owns
     or completes the overall workflow.
2. **Read strong seeds**
   - Resolve at least the strongest three relevant records to lawful canonical
     full text or original data.
   - Call typed `ctox_web_read` with a precise factual reading query.
   - Persist the original snapshot and immutable receipt.
3. **Extract the source network**
   - Extract relevant bibliography entries, cited datasets, supplementary
     files, authors, identifiers, and repository links from every admitted
     seed.
   - Resolve backward references.
   - Resolve forward citations through scholarly adapters.
   - Follow useful references breadth-first, not by repeatedly issuing the
     original broad keyword query.
4. **Persist every edge before continuing**
   - Every candidate row must carry `discovery_round`,
     `discovery_method`, `seed_source_id`, `seed_identifier`,
     `citation_hop`, `citation_direction`, `relation_type`, and
     `discovery_paths_json`. The JSON field preserves every discovery path
     when one canonical candidate is reached through multiple seeds.
   - Allowed directions are `seed`, `backward`, `forward`, `dataset`,
     `supplement`, `author`, `facet`, and `alternate_route`.
   - `citation_hop=0` is a direct search seed. A reference found in that seed
     is hop 1. Continue with monotonically increasing integer hops.
   - Keep rejected and duplicate candidates with their decision and reason.
5. **Verify, deduplicate, and promote**
   - Deduplicate by normalized DOI/stable ID, canonical URL, and content hash.
   - Fetch the canonical original. Require 2xx, real content, snapshot,
     byte count, SHA-256, sufficient relevance, and a valid server receipt.
   - Promote only after the evidence guard passes.
6. **Reformulate from gaps**
   - Use missing evidence classes and graph frontiers to choose the next facet.
   - Do not repeat the same provider/query immediately after CAPTCHA, 429, or
     provider failure. Continue from admitted references or another adapter.
7. **Check saturation**
   - Record candidates added, duplicates, rejections, admitted originals, and
     unresolved graph frontiers for each round.
   - Stop only after two consecutive, complete, orthogonal citation/facet
     rounds add no new eligible source.

Using one search surface, stopping after one envelope, or failing to inspect
the references of relevant scientific papers is a discovery failure.

## Mandatory Second Loop: Evaluate Every Admitted Source

Discovery ends with a verified corpus. It does not end the run. Every source in
`source_catalog` is then read and evaluated. Skipping this leaves the typical
failure mode: hundreds of verified sources, a handful of claims, and a graph in
which most sources hang unconnected.

Work in slices of about five sources so a failure never costs the whole corpus,
and write each result immediately.

For every admitted source produce exactly one evaluation record:

1. **Relevance verdict** — `core` (delivers the target quantities directly),
   `context` (transferable method, norm, adjacent measurement), or `off_topic`
   (no bearing on the question; keep it, mark it, do not silently drop it).
2. **Summary** — subject, method (test rig, simulation, norm, dataset), main
   findings with numbers and units, limits.
3. **Claims** — 3 to 10 for a relevant source, at most 12. Each claim is:
   - a self-contained engineering statement in the operator's language, with
     numbers and units, 40-400 characters, no citation apparatus, no URLs;
   - `statement_type` ∈ `direct_measurement`, `normative`, `analytical`,
     `assumption`, `observation`;
   - a **verbatim quote** from the extracted original, 25-400 characters,
     copied, never paraphrased or reconstructed from memory;
   - a locator the reader can follow (page, section, table, file);
   - the knowledge book / topic it belongs to;
   - limitations: scope, assumptions, transferability;
   - the numeric values as `quantity`/`value`/`unit`/`condition` when present.
4. **Tabular data flag** — does the source carry extractable measurement rows,
   which quantities, how many operating points, is extraction feasible.
5. **Related sources** — other admitted sources this one agrees with,
   contradicts, extends, cites, or shares a dataset with.
6. **Open questions** the source leaves for the engineering task.

Rules that make the result usable instead of decorative:

- The claim states what the **source** says, never your own conclusion.
- Every number in a claim must be in the quote or at the quoted location.
- No text fragments, no metadata (authors, DOI, publisher) as a claim.
- An `off_topic` source gets at most two claims explaining what it does carry.
- Verify each quote mechanically against the extracted text before accepting
  the record. A quote that cannot be found is a defect of the record, not of
  the check. Tolerate only whitespace and hyphenation artefacts of the text
  extraction, never changed words.

Extract text once per source and evaluate from that text. For PDFs use reading
order, not layout mode: layout mode interleaves two-column papers line by line
and tears verbatim quotes apart.

## Consolidate Across Sources

Single-source claims are the raw material, not the result. After the evaluation
loop, cluster the claims per knowledge book:

- A cluster is one engineering statement supported by **at least two different
  sources**. Claims from the same source never form a cluster alone.
- The canonical claim covers all members; numbers only when the members agree,
  otherwise give the range or drop the number. Never invent a value.
- `statement_type` of a cluster is the strongest common type
  (direct_measurement > normative > analytical > observation > assumption);
  confidence is the lowest of its members.
- Where sources disagree, do **not** cluster. Record a contradiction with both
  sides, what exactly differs, and the likely cause (different boundary
  conditions, scale, method, bearing variant).
- Record, per knowledge book, which statements rest on a single source — those
  are the gaps that justify the next research round.

## Typed Tool Rules

- Managed runs use the directly exposed `ctox_scholarly_search`,
  `ctox_web_search`, `ctox_deep_research`, and `ctox_web_read` tools.
- Do not substitute shell calls, native web search, browser downloads, curl, or
  memory for typed reads that must become evidence.
- `ctox_web_search` must report the Google-first multi-engine cascade. Silent
  collapse to one provider is a platform defect and must fail closed.
- A typed `ctox_web_read` intended as evidence must include a precise `query`.
- If typed research tools are absent, stop and report the platform defect.
- Never invent a URL, source, quote, measurement, unit, or relationship.

## Evidence Gate

Evidence requires all of:

- canonical non-metadata original URL;
- current 2xx response with actual full text or original data;
- server-written snapshot and immutable
  `ctox.web-read.workspace-evidence.v3` receipt;
- matching byte count and SHA-256;
- machine-computed `evidence_relevance_score >= 8`;
- exact run, command, attempt, and workspace binding;
- a claim quote present in the extracted original, or a hash-bound original
  data excerpt with row/column/unit provenance.

Reject fail-closed:

- 404/410 and soft-404;
- login/cookie walls without captured original content;
- JavaScript shells and metadata-only landing pages;
- search snippets, aggregators, mirrors, and unresolved DOI pages;
- fabricated or reconstructed receipts;
- evidence whose original bytes cannot be independently checked.

Read [references/evidence_integrity.md](references/evidence_integrity.md) and
run `scripts/evidence_guard.py` before promotion or publication. The required
manifest is `validation/evidence-manifest.json`.

## Technical Data Rules

- Keep direct measurements, dimensionless coefficients, and physical
  derivations in separate tables.
- Never infer radial bearing force from axial thrust or torque.
- Split propeller notation such as `9x5` into numeric diameter and pitch.
- Use SI units and machine-readable decimal dots in canonical CSV; German
  Excel exports use semicolon delimiters and decimal commas.
- CT/CP-derived torque is
  `Q = CP * rho * n^2 * D^5 / (2*pi)`, with `n = RPM/60`.
- Record formula, constants, assumptions, units, and exact source-row lineage.
- Treat verified archives that have not been row-extracted as datasets, not as
  measurement rows.
- ENOLA archive members require archive URL/hash, manifest/hash, complete member
  path/hash, source ID, and propeller geometry binding.

Use `scripts/dashboard_knowledge_build.py` for builder-owned dashboard tables.
Do not hand-author them and do not write Business OS databases directly.

Persist semantic graph nodes only as `topic`, `concept`, `source`, `evidence`,
or `measurement`. Persist edges only as `supports`, `measures`,
`derived_from`, `part_of`, `contradicts`, `correlates_with`, or `co_occurs`.
A claim is an `evidence` node; its thematic edge is `claim -> topic` with
`relation_type=part_of`. Every node and edge carries admitted source IDs and
claim/evidence provenance.

## Durable Output Modes

Choose one or combine them:

- **Library**: shared-schema research records in `ctox knowledge data`.
- **Knowledge**: Skillbooks for reusable factual/method knowledge; Runbooks only
  for repeatable procedures. Every knowledge statement retains claim,
  evidence, snapshot, source, and canonical URL lineage.
- **Decision report**: cited DOCX through `ctox report`.
- **Combined**: build and verify the library/Knowledge first, then write the
  report from that admitted corpus.

Workspace Markdown/CSV/JSON/Parquet files are build receipts or import inputs,
not the durable deliverable by themselves. Register outputs in Knowledge,
Documents, Spreadsheets, and Files before completion.

For detailed schemas, report blueprints, CLI forms, review behavior, and
writeback mechanics, read [WORKFLOW_CONTRACT.md](WORKFLOW_CONTRACT.md) only for
the phase being executed. Do not load unrelated report references during
discovery.

## Completion Gate

Do not claim completion until:

- all candidate and graph rows are persisted;
- every admitted source passes the Evidence gate;
- **every admitted source carries a relevance verdict and, unless off_topic, at
  least two claims** — an unevaluated verified source blocks completion;
- **every claim quote is mechanically verified against the extracted original**;
- **cross-source consolidation ran** and contradictions plus single-source gaps
  are recorded per knowledge book;
- **every admitted source is connected in the graph** through at least one
  claim or relevance edge;
- every claim resolves through immutable lineage;
- direct and derived data remain separated;
- the requested durable outputs are imported and linked;
- two orthogonal zero-addition rounds are recorded;
- deterministic source, data, claim, and physics audits pass;
- the independent CTOX review passes;
- unresolved gaps and rejected candidates remain visible.

If a planned exhaustive sweep or verification cannot complete, preserve the
task as pending or blocked with the exact defect. Never downgrade depth or
declare success from a partial envelope.
