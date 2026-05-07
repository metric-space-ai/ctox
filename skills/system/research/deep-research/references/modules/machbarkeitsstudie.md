# Module: Machbarkeitsstudie / Technical Feasibility

Use this module for scientific, engineering, technical, medical-device, industrial, material, process, inspection, or technology feasibility studies.

## Decision Frame

State the decision as: "Which technical approaches are feasible under the stated constraints, what is the likelihood of success, and what experiment would validate or falsify the recommendation?"

Extract:

- target system/material/process
- physical constraints and geometry
- required contact mode, throughput, accuracy, safety, and deployment context
- confounders and unknowns
- acceptable and unacceptable technologies
- target maturity level and budget/time constraints if known

## Evidence Matrix

Build a matrix with these columns:

- technology / approach
- physical principle
- evidence sources
- penetration or observability expectation
- sensitivity to target anomaly
- throughput / single-shot potential
- stand-off and access constraints
- confounders
- maturity / TRL
- safety/regulatory issues
- success probability with uncertainty
- next experiment

Save this as `synthesis/evidence-matrix.md` before writing prose.

## Search Coverage

For technical studies, cover:

- primary scientific papers and review papers
- standards, handbooks, patents, and industrial application notes
- negative evidence and known failure modes
- relevant adjacent industries
- validation methods and ground truth
- supplementary datasets, GitHub repositories, notebooks, simulation models, benchmark images, and raw measurement data when linked from papers or project pages

For hidden metal structures in composites/coatings, also consider:

- Terahertz time-domain / frequency-domain imaging
- Eddy-current testing, pulsed eddy current, remote field eddy current, eddy-current thermography
- Magnetic flux leakage / magneto-optical imaging when material physics permits
- Microwave / millimeter-wave imaging and radar
- Infrared thermography, pulse thermography, lock-in thermography
- Laser shearography / vibrothermography for secondary defect signatures
- X-ray radiography / computed tomography when acceptable despite radiation/access constraints
- Electrical capacitance / impedance / dielectric spectroscopy
- Optical / hyperspectral imaging only when there is a credible optical path or indirect surface signature

## Default Report Structure

1. Executive Summary
2. Objective and Scope
3. System / Material Stack and Inspection Problem
4. Methodology and Search Strategy
5. Candidate Technologies
6. Evidence Review
7. Feasibility Matrix
8. Data, Figures, and Reproducible Artifacts
9. Recommended Experimental Design
10. Risks, Unknowns, and Decision Gates
11. Conclusion and Recommendation
12. References

## Writing Loop

Perform these steps in the research workspace:

1. Write `synthesis/evidence-matrix.md` from the source ledger.
2. Write `synthesis/technology-scores.md` with one row per technology, numeric/qualitative score, uncertainty, and evidence IDs.
3. Write `synthesis/report-outline.md`; the outline must be generated from the evidence and decision question, not copied from any reference document.
4. Write `synthesis/figure-plan.md` listing each figure/table, its data source, and whether it is original, source-derived, or omitted for licensing.
5. If GitHub/data links are present, inspect relevant repositories/datasets and note findings in `synthesis/data-artifacts.md`; build diagrams/tables from them only when they add evidence.
6. Write `synthesis/report-draft.md` as a full narrative Machbarkeitsstudie with citations.
7. Generate the DOCX from the draft and matrix. Then write `synthesis/qa-notes.md` covering opening/ZIP checks, visual/render checks where available, missing evidence, and whether the report satisfies the decision question.

Do not produce a final DOCX until the draft has become a coherent study with management summary, problem framing, technology comparison, feasibility scoring, risk discussion, and recommended experiments.

## Completion Criteria

- Clear recommendation, not just a literature summary.
- Feasibility score with uncertainty for each major approach.
- Explicit handling of confounders.
- Source-backed claims with citations.
- Explicit statement of inspected GitHub/data/supplement links and any diagrams/tables derived from them.
- Short validation plan with pass/fail gates.
- Final Word/PDF document opens successfully and contains a synthesized study, not copied source text or a formatted evidence dump.
