# Technical Drawing Review Checklist

Use this checklist during the review pass. It is a practical review guide, not a substitute for the governing standard or engineering judgment.

## Universal Handoff Checks

- Title block: drawing number, part number, title, revision, sheet count, scale, units, author/checker/approver/date.
- Revision control: visible revision matches all sheets and attachments.
- Material: material grade/specification, stock form when relevant, heat treatment/hardness, coating/plating, finish state.
- General notes: default tolerances, projection method, edge break/deburr, surface finish, cleanliness, inspection notes, applicable standards.
- Completeness: all pages readable, correct orientation, no cropped title block, no missing referenced detail/section sheets.
- Consistency: no conflicting dimensions, units, material, quantity, revision, part number, or notes across sheets.

## Dimensioning

- Missing dimensions on features needed for manufacture or inspection.
- Over-defined, duplicated, or conflicting dimensions.
- Chained dimensions where baseline/datum dimensioning is needed for functional control.
- Reference dimensions used as if they control manufacture.
- Ambiguous centerlines, hole patterns, slot locations, angles, radii, chamfers, tapers, or repeated features.
- Dimensions that are not inspectable from a clear datum or setup.

## Tolerances and Fits

- Missing local or general tolerance for functional dimensions.
- Fit classes without mating/context information when relevant.
- Tight tolerances on nonfunctional features, or loose/unspecified tolerances on critical features.
- Limit dimensions or unilateral tolerances that conflict with nominal geometry.
- Tolerance stack risks from chain dimensioning.
- General tolerance note absent, unreadable, or contradicted by local requirements.

## GD&T and Datums

- Datum features missing, duplicated, or impossible to establish.
- Feature control frames without required datums when functional control needs them.
- GD&T applied to unclear features.
- Position/profile/runout/perpendicularity requirements that cannot be inspected from the shown datum scheme.
- Datum references that conflict across views or sheets.

## Material, Finish, and Surface Integrity

- Missing material grade or ambiguous material family.
- Missing heat treatment, hardness, case depth, coating, passivation, anodizing, paint, or plating when implied by function.
- Functional surfaces without local or governing surface finish requirements.
- Surface finish symbols not tied clearly to surfaces.
- Edge break/deburr missing for parts with handling, sealing, or assembly concerns.

## Manufacturing Feasibility

Use process context when available. If unknown, phrase process-specific risks as `needs_context`.

- Machining: inaccessible pockets, sharp internal corners, deep narrow slots, impossible radii, undercuts, tool access, thread relief, keyway clarity.
- Turning/shaft parts: datum axis, fits, shoulders, grooves, runout, chamfers, reliefs, bearing seats, surface finish on journals.
- Sheet metal: bend radius, K-factor or bend allowance context, flat pattern, material thickness, grain direction, bend relief, hole-to-bend distance.
- Weldments: weld symbols, weld size/length/process, preparation, distortion control, post-weld machining, inspection.
- Castings/forgings: draft, parting line, machining allowance, datum setup, casting tolerance, surface class.
- Additive: build orientation, support removal, minimum wall, post-processing, critical surfaces.
- Gears/splines/threads: module/DP, tooth count, profile, pressure angle, hand, class, tolerances, inspection method.

## Inspection and Quality

- Critical features lack measurable acceptance criteria.
- Inspection setup unclear for CTQ dimensions.
- Datums or basic dimensions insufficient for CMM/fixture inspection.
- Notes use subjective terms such as "smooth", "clean", "as needed", or "typical" without acceptance criteria.
- Required certificates, inspection reports, or special process controls missing when implied by context.

## Finding Discipline

- Pin every issue to visible evidence.
- Use `needs_context` when the concern depends on design intent, supplier capability, or unavailable standards.
- Avoid flagging every possible best-practice improvement.
- Prefer root-cause findings over symptom lists.
- Never claim a standard violation unless the standard requirement is provided or visible context makes it unambiguous.
