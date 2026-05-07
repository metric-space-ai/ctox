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

Use the report architecture in `machbarkeitsstudie-quality.md`. The minimum structure is:

1. Title page, status/context note, and evidence limitation note
2. Table of contents / section roadmap
3. Abbreviation table
4. Management Summary
5. Ausgangslage, Prüfobjekt, and Fragestellung
6. Bauteilaufbau and reference/schematic figures
7. Requirements and boundary conditions
8. Technology screening with stated scoring logic
9. Qualitative matrix and scenario matrix
10. Detailed assessment of shortlisted approaches
11. Recommended system concept and phased experimental design
12. Defect/coupon catalogue
13. Risks, dependencies, mitigations, and decision gates
14. Conclusion and curated references

## Writing Loop

Perform these steps in the research workspace:

0. Verify the evidence base is sufficient before synthesis: at least 20 credible sources for normal decision-grade studies, saved source reads, non-empty `sources.jsonl`, and source-backed evidence in `evidence_bundle.json`. For exhaustive scientific studies, target substantially more sources and record why the final source count is sufficient.
1. Write `synthesis/evidence-matrix.md` from the source ledger.
2. Write `synthesis/technology-scores.md` with one row per technology, numeric/qualitative score, uncertainty, and evidence IDs.
3. Write `synthesis/score-rationale.md`: one subsection per technology, with score-by-score rationale and evidence IDs. No score may be unexplained.
4. Write `synthesis/scenarios.md` for stack variants, especially grid/EMF as first metal layer, additional nearly closed foil, and deeper/thicker cover stack.
5. Write `synthesis/defect-catalog.md` with defect IDs and coupon variants.
6. Write `synthesis/report-outline.md`; the outline must follow the study architecture, be generated from the evidence and decision question, and not copy a reference document.
7. Write `synthesis/figure-plan.md` listing each figure/table, its data source, and whether it is original, source-derived, or omitted for licensing.
8. If GitHub/data links are present, inspect relevant repositories/datasets and note findings in `synthesis/data-artifacts.md`; build diagrams/tables from them only when they add evidence.
9. Write `synthesis/report-draft.md` as a full narrative Machbarkeitsstudie with citations.
10. Generate the DOCX from the draft and matrix. The output path must be a real `.docx` file, not a directory.
11. Run `scripts/validate_research_deliverable.py` and `scripts/validate_study_quality.py` against the research workspace and final DOCX. Then write `synthesis/qa-notes.md` with both validator JSON outputs, opening/ZIP checks, visual/render checks where available, missing evidence, and whether the report satisfies the decision question.

Do not produce a final DOCX until the draft has become a coherent study with management summary, problem framing, technology comparison, feasibility scoring, risk discussion, and recommended experiments.

## Completion Criteria

- Clear recommendation, not just a literature summary.
- Feasibility score with uncertainty for each major approach.
- Explicit handling of confounders.
- Source-backed claims with citations.
- Explicit statement of inspected GitHub/data/supplement links and any diagrams/tables derived from them.
- Short validation plan with pass/fail gates.
- Final Word/PDF document opens successfully and contains a synthesized study, not copied source text or a formatted evidence dump.
- `validate_research_deliverable.py` exits successfully. If it fails, the final answer must say the deliverable is not complete and list the failing gates.
- `validate_study_quality.py` exits successfully. If it fails, the final answer must say the study is not client-ready and list the failing gates.
