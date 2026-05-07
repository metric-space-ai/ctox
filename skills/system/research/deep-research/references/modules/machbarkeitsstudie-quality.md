# Machbarkeitsstudie Quality Rubric

Use this rubric for technical feasibility Word reports. It is derived from the expected
shape of a client-ready engineering feasibility study, not from source content.

## Required Study Architecture

A feasibility study must contain:

1. Title page with study title, subtitle, date/status, context, and evidence limitation note.
2. Table of contents or explicit section roadmap.
3. Abbreviation table for domain terms and methods.
4. Management summary with concrete recommendation and key caveats.
5. Ausgangslage / problem framing with explicit research questions.
6. Bauteilaufbau / system model with at least one layer-stack figure or schematic.
7. Requirements and boundary conditions.
8. Technology screening with a stated scoring logic.
9. Qualitative technology matrix whose scores are justified by evidence.
10. Scenario matrix for relevant stack variants, such as:
    - first metal layer is the grid/EMF
    - additional nearly closed metallic foil
    - deeper grid or thicker cover stack
11. Brief assessment of secondary/low-priority methods.
12. Detailed assessment of the shortlisted methods.
13. Recommended system concept and phased experimental design.
14. Defect/coupon catalogue with defect IDs.
15. Risks, dependencies, mitigations, and decision gates.
16. Conclusion and recommendation.
17. Curated references, not a raw source dump.

## Evidence Requirements

- Every score in a matrix must trace to evidence IDs or an explicit physics inference.
- Every detailed method section must include: mechanism, why it may work, why it may fail, what the metallic foil changes, test parameters, and a validation gate.
- Do not cite a generic source list as proof. Cite the specific sources that support the claim.
- Do not use raw call-count JSON in the narrative. Summarize it human-readably.
- Metadata-only sources are allowed in the bibliography but cannot dominate key claims.

## Writing Anti-Patterns

Reject the report if it contains:

- Repeated generic paragraphs across technologies.
- A score matrix with no source-linked derivation.
- A "Quellenbasis" section that is just a long source dump before the argument is complete.
- JSON blobs or tool output pasted into the report body.
- Empty figure plan or no figures/diagrams in a visual engineering study.
- "Evidenz noch schwach" in a final section without follow-up research or an explicit unresolved-risk treatment.
- An apparently precise recommendation where the underlying layer stack is unknown.

## Minimum Visual / Table Set

The final DOCX should include:

- At least 3 visual elements or diagrams for a technical feasibility study:
  layer stack, method interaction, workflow/decision gate, score visualization, or legally usable source figure.
- At least 4 tables:
  abbreviation table, technology screening matrix, scenario matrix, defect/coupon catalogue.
- Tables must have domain-specific headers; generic metadata tables do not count.

## Final QA Questions

Before declaring success, answer yes/no in `synthesis/qa-notes.md`:

- Does each technology score have evidence or a stated physics rationale?
- Does the report explain the metallic foil as a scenario-specific confounder?
- Are weak/no-go technologies discussed without overclaiming?
- Is the experimental design specific enough to execute?
- Are source figures either legally usable or redrawn as original schematics?
- Would the report still make sense if the source list were moved to the appendix?
