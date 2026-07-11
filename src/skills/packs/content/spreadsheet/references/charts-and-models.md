# Charts and model structure

Engine-agnostic doctrine for workbook models and in-sheet charts. Chart
authoring is gated on `spreadsheet.charts` — check
`references/execution-surfaces.md` before promising chart work.

## Model structure

Organize every non-trivial workbook so a stranger can audit it:

- **Inputs zone**: assumptions and raw data in clearly labeled, visually
  distinct cells or ranges (one convention per workbook, e.g. a dedicated
  assumptions sheet or a colored input style). Every number a user might
  want to change lives here.
- **Calculation zone**: formula-driven, one consistent pattern per row or
  column family. Helper columns beat clever single-cell formulas — the trace
  from input to output must be followable by clicking precedents.
- **Output zone**: the dashboard or summary the reader consumes; references
  calculations, contains no logic of its own.

Rules that keep the model auditable: no constants inside calculation
formulas (reference an input cell); lookup and mapping tables visible on a
sheet, not buried in formulas; period columns share one formula pattern;
units and period conversions labeled; load-bearing assumptions carry cell
comments naming their source.

## Number and date presentation

Typed values with invariant format codes, always. Choose precision by
meaning, not habit: counts as whole numbers with thousands separators; rates
with one decimal for analysis and none for dashboards, two where basis
points matter; money in whole units unless cents are the point; dates in an
unambiguous format (`yyyy-mm-dd` or `mmm yyyy`).

## Charts

A chart earns its place when it answers a question faster than the table
would. Decide the question first, then the form:

- Trend over time: line chart; one series per entity, few enough to read.
- Comparison across categories: bar chart, sorted by value unless a natural
  order exists.
- Composition: stacked bars for few categories; avoid pie charts beyond
  three or four slices.
- Relationship: scatter, with axes labeled including units.

Chart discipline:

- The chart references live ranges; when the table grows, the chart's range
  must grow with it (extend ranges when adding rows/columns).
- Axis labels, units, and a specific title are mandatory; a chart must
  survive being copied out of context.
- No default-styled charts on a styled sheet: match the workbook's palette
  and typography.
- Do not truncate value axes to exaggerate differences; if you must zoom,
  say so on the chart.
- After any chart change, render the sheet and check: legend readable, no
  clipped labels, series distinguishable, empty-data placeholders absent.

## Conditional formatting and validation

Use conditional formatting to encode meaning (thresholds, exceptions,
heat), not decoration. Keep the rule set small and document non-obvious
rules in a legend or note. When a formatted table grows, extend the rules to
the new range — a half-formatted table is worse than an unformatted one.
Data validation belongs on every input cell whose domain is known (lists,
ranges, dates); pair it with an input message rather than a separate
instructions sheet.
