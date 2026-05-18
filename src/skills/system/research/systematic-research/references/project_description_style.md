# Project Description / Fördervorhaben Style Contract

This reference is mandatory for `report_type_id=project_description`.
It must be read together with
`project_description_reference_archetype.md`, which captures the real
Fördervorhabenbeschreibung corpus. The style contract below governs the
process; the archetype governs the target shape.

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

The reference corpus shows that this understanding is normally built in this
order: company/legal profile -> company development -> products/services and
customers -> project introduction -> current state/problem area ->
development goal -> innovation/market delimitation -> challenges/measures ->
project costs and implementation period -> economic benefit. Do not compress
this into abstract headings only.

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
9. `reference-fit-plan.md`: short checklist mapping the planned document to
   the Fördervorhaben archetype: legal profile, company profile, history,
   products/customers, project introduction, current state/problem,
   development goal, market/state-of-art delimitation, measures, project
   costs, implementation period and benefit.

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
write conservatively and keep the limitation in internal notes. For
company-rich topics, twelve facts is only a floor; the document should absorb
the available legal, history, product, customer and project facts at the same
density as the references.

For the company profile and budget/timeline/status section, create compact
native Word tables whenever the facts are available. Prefer the deterministic
helper:

```bash
ctox report project-description-sync --run-id RUN_ID
```

It extracts the project-scope facts from the run topic and committed
project-scope prose, and company/legal facts from committed company prose and
the evidence register. It creates native tables bound to
`doc_company__company_legal` and `doc_project__project_1_costs_timeline`.
Use manual `ctox report table-add` only if the helper cannot parse a supplied
framing. These tables are part of the client document. The research/source
ledger is not, and visible tables must not contain source or evidence columns.

At standard depth, include at least one client-facing image or schematic.
The references use product, site, organisation, process or system visuals as
orientation devices. If no legally usable original image is available, create
an own clean schematic with `ctox report figure-add` that explains the project
architecture, workflow, service model or target operating model. The figure
must be embedded, captioned and briefly interpreted in the prose; it must not
be a decorative placeholder. When referencing the figure, write `Wie in
{{fig:...}} gezeigt...` instead of duplicating the label as `Abbildung
{{fig:...}}`.

## Drafting workflow

Do not write the eight chapters as independent mini-essays. Work through this
sequence:

1. Read the reference style and comments, then write a one-paragraph target
   document thesis: why this company needs this project now.
   If the reference is a commented DOCX, first make sure the comments were
   imported as `review_feedback` via `--review-doc` or
   `ctox report review-import`; a visible path in the operator prompt is not
   enough.
2. Build the internal fact-transfer ledger and mark each fact as one of:
   company, product/service, customer/market, technical baseline, project
   scope, economic mechanism.
3. Build the reference-fit plan and confirm where the legal profile, history,
   products/customers, current state/problem, development goal, measures,
   project costs and implementation period will appear.
4. Draft only the company/outcome spine first: company capability -> present
   bottleneck -> innovation jump -> target operating model.
5. Draft implementation and scope only after the problem/target chain is clear.
6. Run `project-description-sync` after the company and scope blocks exist.
7. Revise for client voice: remove analysis scaffolding, source language,
   evidence wording, and duplicated claims.

If a chapter cannot be made specific, do not fill it with generic funding
language. Go back to research or state the project assumption conservatively in
the internal notes, then write only the client-relevant consequence.

## Recommended document spine

When no user structure is prescribed, use this spine:

1. Title page / programme context
2. Gesellschaftsrechtliche Verhältnisse
3. Unternehmensprofil
4. Unternehmensentwicklung / Historie
5. Produkte, Leistungen und Kundensegmente
6. Vorstellung des Innovations- oder Digitalisierungsprojektes
7. Derzeitiger Stand / Problembereich
8. Entwicklungsziel / Zielbild
9. Abgrenzung zum Stand der Technik oder Markt
10. Herausforderungen und geplante Maßnahmen
11. Projektkosten
12. Umsetzungszeitraum
13. Wirtschaftlicher Nutzen / Verwertung

When the operator provides a chapter list, respect it, but still make sure the
substance above is covered. For example, an eight-chapter prompt can still
include company history and products under "Unternehmensausgangslage", and
challenges/measures under "Umsetzungsschwerpunkte". It is not enough that the
eight headings exist; the internal content must still look like the reference
documents.

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
- Are legal/company profile, history and products/customer segments present
  when such facts are available?
- Do the implementation measures solve the stated challenges?
- Are project costs, implementation period and project status present without
  invented numbers?
- Is the economic benefit a mechanism, not a vague promise?
- Are visible citations, source appendix and internal tooling language absent?
- Would the document still make sense if printed without any source appendix
  or research notes?
- Does every requested chapter earn its place, or is it repeating another
  chapter in different words?
