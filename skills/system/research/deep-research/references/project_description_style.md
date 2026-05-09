# Project Description / Fördervorhaben Style Contract

This reference is mandatory for `report_type_id=project_description`.

## What the deliverable is

A project description for funding, financing or approval is an applicant-side
business and innovation narrative. It is not a research paper, source review,
market study or feasibility options report. Research supports facts and
wording in the background, but the final Word document should mostly hide the
research mechanics.

The reader should understand:

1. who the applicant is and why it is capable of the project;
2. what operational, market, technical or organisational bottleneck exists now;
3. what changes through the project;
4. why this is an innovation or meaningful development step rather than routine
   procurement;
5. how the project will be implemented;
6. what budget, timeline and status apply;
7. why the project creates economic benefit and is plausible.

## Silent research ledger

Use research to validate company facts, market context, competitor baseline,
technology vocabulary, funding context and plausibility. Persist evidence in
the CTOX evidence register. However, do not surface the evidence as a visible
academic apparatus unless explicitly requested.

Client-facing project descriptions must not contain:

- bracket citations like `[1][3][4]`;
- a bibliography or "Quellen und Recherchebasis" appendix;
- DOI lists, raw URLs, source IDs, run IDs, workspace notes or QA language;
- phrases such as "the evidence shows", "the sources suggest", "market
  evidence indicates" as a recurring writing pattern;
- generic consultant filler that could apply to any company.

Research should appear only as concrete, integrated facts:

- company age, location, legal/factual context, segments, products, numbers;
- plausible competitive or state-of-the-art framing;
- market norms that explain why the project is necessary;
- specific operational constraints, customer groups, processes and cost blocks.

## Required preparation artifacts

Before drafting blocks, create short synthesis notes in the workspace:

1. `company-material.md`: legal/company profile, history, products, customers,
   numbers, current strengths, prior innovations.
2. `project-material.md`: project title, status, scope, budget, costs,
   timeline, constraints, supplied user facts.
3. `bottleneck-logic.md`: status quo, operational bottleneck, why it matters,
   and how it leads to the project.
4. `innovation-logic.md`: what is new, what is merely enabling investment, and
   how the project differs from standard procurement.
5. `implementation-logic.md`: work packages, dependencies, milestones,
   measures, risks and responsible capability areas.
6. `benefit-logic.md`: economic mechanisms, scaling effect, service/process
   effect, customer/market effect, funding plausibility.
7. `style-review.md`: self-review against this contract before rendering.
8. `fact-transfer-ledger.md`: table with `Fact cluster | Source/evidence |
   Required target chapter | Visible wording | Status`.

These notes are internal. They do not belong in the final Word file.

## Fact transfer contract

The common failure mode for this report type is to research facts, then write a
generic project narrative that does not contain them. Avoid that explicitly.

Before writing the client-facing blocks, extract concrete, non-prompt facts
from the evidence register. At standard depth, use at least twelve such facts
when available; for richer source material, use more. Cover as many of these
categories as the evidence supports:

- legal/company facts: name, legal form, register, location, history,
  management or ownership context;
- capability facts: products, services, installed base, operating model,
  certifications, process capabilities, sales/service structure;
- customer/segment facts: target groups, use cases, reference segments,
  sector-specific requirements;
- technical facts: named product lines, modules, components, data points,
  capacities, interfaces, software functions, constraints;
- market facts: named competing approaches, standard products, baseline
  features, regulatory or industry expectations;
- project facts: supplied budget, status, cost blocks, timeline, work packages,
  dependencies and exclusions;
- benefit facts: concrete economic mechanisms tied to operations, customers,
  service, margin, scalability, quality, sustainability or resilience.

Every major chapter should contain at least one concrete fact that did not come
only from generic prompt wording. Facts must be integrated into smooth
applicant-side prose; do not cite them visibly, do not list source IDs, and do
not expose the fact-transfer ledger. If a category cannot be substantiated,
write conservatively and keep the limitation in internal notes.

For the budget/timeline/status section, create one compact native Word table
whenever the operator provides Laufzeit, Status, Budget or Kostenblöcke. Prefer
the deterministic helper:

```bash
ctox report project-description-sync --run-id RUN_ID
```

It extracts the project-scope facts from the run topic and committed
project-scope prose, then creates a native table bound to
`project_scope_budget_timeline`. Use manual `ctox report table-add` only if
the helper cannot parse a supplied framing. This table is part of the client
document. The research/source ledger is not.

## Drafting workflow

Do not write the eight chapters as independent mini-essays. Work through this
sequence:

1. Read the reference style and comments, then write a one-paragraph target
   document thesis: why this company needs this project now.
2. Build the internal fact-transfer ledger and mark each fact as one of:
   company, product/service, customer/market, technical baseline, project
   scope, economic mechanism.
3. Draft only the company/outcome spine first: company capability -> present
   bottleneck -> innovation jump -> target operating model.
4. Draft implementation and scope only after the problem/target chain is clear.
5. Run `project-description-sync` after the scope block exists.
6. Revise for client voice: remove analysis scaffolding, source language,
   evidence wording, and duplicated claims.

If a chapter cannot be made specific, do not fill it with generic funding
language. Go back to research or state the project assumption conservatively in
the internal notes, then write only the client-relevant consequence.

## Recommended document spine

When no user structure is prescribed, use this spine:

1. Title page / metadata
2. Gesellschaft & Unternehmensprofil
3. Unternehmensentwicklung / Historie
4. Produkte, Leistungen und Kundensegmente
5. Projektbeschreibung Innovation
6. Derzeitiger Stand / Problembereich
7. Entwicklungsziel / Zielbild
8. Abgrenzung zum Stand der Technik oder Markt
9. Herausforderungen und Maßnahmen
10. Arbeitspakete / Umsetzungsschritte
11. Projektkosten & Zeitraum
12. Wirtschaftlicher Nutzen / Verwertung

When the operator provides a chapter list, respect it, but still make sure the
substance above is covered. For example, an eight-chapter prompt can still
include company history and products under "Unternehmensausgangslage", and
challenges/measures under "Umsetzungsschwerpunkte".

## Storytelling and comments

Reference documents and Word comments often reveal the real quality bar. Use
them as review criteria:

- build a path from company development to the project problem;
- connect status quo, challenges, goals and work packages with the same logic;
- reduce hard facts when they interrupt readability, but keep enough specifics
  for credibility;
- avoid duplicating the same point under different headings;
- write work packages as flowing implementation logic when the reference asks
  for fewer list-like steps.

## Voice and wording

Preferred voice:

- concrete, applicant-side, explanatory;
- confident but not promotional;
- paragraph-led, with concise lists only where they improve clarity;
- "Die Gesellschaft ..." or "Das Vorhaben ..." instead of meta commentary;
- "Im Ausgangszustand ..." -> "Daraus ergibt sich ..." -> "Ziel des Vorhabens
  ist ..." to preserve the red thread.

Avoid:

- "Förderlogik ist entscheidend" as a visible phrase;
- "Client-ready", "working draft", "evidence", "research basis";
- a detached analyst voice that rates the company from outside;
- generic digitalisation language without company-specific mechanisms;
- long citation chains and repeated competitor/source mentions.

## Release self-check

Before rendering, the document must answer yes to all:

- Does it look like a project/funding document, not a research report?
- Are company profile, development history, products/customers and operating
  context specific enough?
- Is the bottleneck stated before the target picture?
- Do the implementation measures solve the stated challenges?
- Are budget, timeline and project status present without invented numbers?
- Is the economic benefit a mechanism, not a vague promise?
- Are visible citations, source appendix and internal tooling language absent?
- Would the document still make sense if printed without any source appendix
  or research notes?
- Does every requested chapter earn its place, or is it repeating another
  chapter in different words?
