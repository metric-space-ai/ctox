# Workjet project exit assessment v1

The native control contract is `ctox.workjet.exit_model.v1`. E5 is the
probability-weighted nominal EUR proceeds for 100% equity at **60 calendar
months** from `as_of`, before sale fees and personal tax. Failed and unsold
states remain in the distribution. This is a forecast, not today's fair value.

Controls use the existing authenticated Business OS command plane, with
workspace DataRead for `ctox.workjet.exit_model.read` and DataWrite for
`.refresh` / `.submit`. Every handler additionally checks the canonical project
owner (including native verified aliases). Research also requires the current
owner's CTOX task-create permission and existing native supervisor binding.
Business data and command receipts travel through CTOX Sync/RxDB/WebRTC.

## Requests and response

`read`: `{project_id}`. `refresh`: `{project_id, as_of?, resources?}`.
`submit`: `{project_id, as_of?, inputs}`. Routing `inbound_channel` is optional;
all other unknown keys are rejected. The existing command ID is the immutable
intent/idempotency key. `record_id`, if present, must equal `project_id`.
Dates are canonical `YYYY-MM-DD`; omitted `as_of` is today's UTC date.

Resources are a **proposal**, not hidden monthly financial allocations:

```json
{"hours_per_week":20,"monthly_budget_eur":300,"comparison_mode":"equal_resources"}
```

These numbers illustrate the shape only. No project gets defaults. Comparison
mode is `equal_resources` or `project_specific`; hours are bounded to 0..168.
The last recorded proposal is retained when a later refresh omits resources.
Research may propose explicit allocations with documented assumptions, but
cannot exceed these hours or any month's expense budget. Additional sources,
rights and a confirmed 60-month plan are still required before calculation.

Native response: `{ok:true, assessment}`. Assessment's required fields are:

```json
{
  "contract":"ctox.workjet.exit_model.v1", "project_id":"project-id",
  "run_id":null, "as_of":null, "exit_date":null, "refresh_due":null,
  "status":"not_started", "missing_inputs":["resource_plan","research_inputs"],
  "findings":[], "sources":[], "plan_summary":null, "result":null,
  "scenarios":[], "history":[]
}
```

Status vocabulary is `not_started`, `researching`, `blocked`, `provisional`,
`ready`, `failed`. This implementation never assigns `ready`: it has no
independent evidence/calibration review adapter. A source-backed finite
calculation is explicitly `provisional`, with a finding explaining that limit.
Structural invalidity fails the command; missing evidence/plan/rights/adapter
creates a blocked immutable run; arithmetic overflow or unsuitable EBITDA
creates a failed run without a Euro result.

Objects have these exact fields:

- `findings[]`: `{code, message}`.
- `sources[]`: `{id, reference, observed_at, valid_until, kind}`; kind is
  `observed`, `derived`, or `assumed`. References are retained evidence pointers,
  not proof that a URL supports the claim. Expired/future sources block.
- `plan_summary`: `{mode:"committed_plan", comparison_mode, confirmed,
  budget_eur, hours_per_week, assumptions:[string]}`. Budget is the **60-month
  sum of marketing, brand, cash operating expenses and capex**, excluding equity
  funding. The resource proposal stays separate from this plan.
- `result`: `{expected_exit_equity_eur, sale_probability,
  expected_price_given_sale_eur, probability_zero_proceeds, p10_eur, p50_eur,
  p90_eur}`. Conditional price is null if no sale has positive probability.
- `scenarios[]`: `{state, probability, sale_probability, equity_price_eur,
  contribution_eur}`.
- `history[]`: `{run_id, as_of, exit_date, status, result, missing_inputs}`.
  Reads include the last 24 runs; all runs remain durable, immutable SQLite rows.

Optional audit fields are `resource_proposal` (the resources object),
`engine_version`, `currency:"EUR"`,
`basis:"100_percent_equity_before_fees_and_personal_tax"`,
`compiled_parameters_hash` (SHA-256 of the retained typed inputs), and
`diagnostics[]`: `{state, probability, sale_probability, equity_price_eur,
source_ids:[string], ev_eur, excess_cash_eur, funding_eur,
cash_failure_month:null|integer}`. The persisted inputs retain the exact plan,
source snapshot references, assumptions and calculation parameters. Diagnostic EUR
values must be finite and nonnegative; cash failure month is null or 1..60.
Each source ID array has at most 200 items, each a nonempty string of at most
128 bytes. Accepted joint weights within the 1e-10 sum tolerance are normalized
before scenario contributions and bounded aggregate probabilities are emitted.

The native shell guest accepts `project.exit_model.read`, `.refresh`, `.submit`
with `commandId`, `projectId`, optional `asOf`, `inputs`, or camelCase resources
`{hoursPerWeek, monthlyBudgetEur, comparisonMode}`. It maps resources to native
snake_case and echoes `{action,commandId,projectId,assessment}` only after a
correlated terminal receipt and unchanged session. `project.list` with
`includeConfiguration:true` returns additive `exitModel` metadata for every
confirmed project, including projects with no local chat/worktree. Native
`project.list` returns its owner-scoped `exit_models` map. Metadata updates
preserve valuation state.

## Inputs and adapters

`Inputs` is an exact object with `adapter`, `sale_perimeter`, `rights_confirmed`,
`perimeter_source_ids`, `probability_source_ids`, `sources`, `plan`, `scenarios`,
`outcomes`, `build_probability`, `technical_failure_sale_probability`, and
`technical_failure_equity_price`. The sale perimeter must be
`100_percent_equity_before_fees_and_personal_tax`. Source ID arrays must be
nonempty and resolve to submitted current source records. Numeric zero is
allowed as a sourced value; absence is never converted to zero.

Plan's exact keys: `version`, `mode`, `comparison_mode`, `confirmed`,
`hours_per_week`, `assumptions`, `source_ids`, `opening_customers`, `opening_cash`,
`paid_marketing`, `brand_budget`, `cash_opex`, `equity_funding`, `capex`,
`sales_capacity`, `onboarding_capacity`, `service_capacity`, `reserve_months`,
`tax_proxy`. The eight array fields each contain exactly 60 nonnegative finite
monthly values. Equity funding means committed cash inflows, not hypothetical
funding caps. `funding_dependent_plan` is unsupported and remains blocked.

### `saas_reference_v1`

This adapter ports the small operational kernel from section 22 of the
five-year exit-model concept. Scenario exact keys: `name`, `source_ids`,
`weight_given_build`, `launch_month`, `sales_lag`, `sam0`, `sam_annual_growth`,
`cpl`, `conversion`, `organic_leads0`, `organic_annual_growth`, `logo_churn`,
`arpa0`, `arpa_annual_growth`, `gross_margin`, `brand0`, `brand_ceiling`,
`brand_speed`, `brand_reference_budget`, `brand_organic_lift`, `brand_price_lift`,
`multiple_basis`, `multiple`, `sale_probability`, `debt_like_exit`,
`wc_adjustment`, `failure_sale_probability`, `failure_equity_price`.

Weights conditioned on successful build sum to one; each path's joint weight
is `build_probability * weight_given_build`. A distinct technical-failure state
retains the remaining mass. Launch is 1..61 (61 is beyond the horizon), sales
lag 0..60. Quotas are 0..1; annual growth exceeds -1; CPL and brand reference
budget are positive. ARR, LTM_REVENUE and EBITDA are the only multiple bases;
EBITDA requires positive normalized horizon EBITDA.

Month-by-month customers respect market, sales, onboarding and service limits.
Brand lift is incremental over the existing brand. Paid costs, capex, tax proxy
and committed equity inflows affect cash. Negative cash enters the explicitly
priced failure state. EV plus excess cash, minus debt-like exit claims, plus
signed working-capital adjustment gives nonnegative equity price. Diagnostics
separate EV, funding and excess cash, so new capital is not mistaken for better
operating economics. The kernel retains the reference limitations: fluid
customer counts, simple cohorts/funnel, full-month revenue, tax proxy, no
uncertain financing, no within-state simulation or empirical calibration.

### `terminal_equity_grid_v1`

A separate finite-state aggregation adapter for **already researched horizon
equity prices**. Each outcome has `{state,probability,sale_probability,
equity_price_eur,source_ids}` plus optional diagnostic fields with defaults zero
and `cash_failure_month` null. Outcome weights sum to one and already include
technical/financial failure. Therefore `build_probability` must equal 1 and
technical failure probability/price both zero. `scenarios` must be empty.
This adapter does not derive game/IP prices. An unsupported game/IP operational
model must remain blocked unless independently supported horizon prices and a
valid equity-sale perimeter can be supplied. ARR is never used as a substitute.

Aggregation sums `p*q*equity`; each state contributes both sold price mass and
unsold zero mass. Weighted quantiles come from the complete discrete proceeds
distribution. A zero-price deal differs from no deal; both have zero proceeds.

## Bounded research and monthly lifecycle

Refresh without resources or a registered supervisor records a blocked run.
With both, it uses the existing native Threads/supervisor durable turn producer,
not new unconstrained question workers or an HTTP path. There is at most one
research admission per project/calendar month; repeated updates reuse it. An
active turn is reused even across a month boundary. Changed proposals cannot
start parallel research. No recurring schedule is activated by this feature.
`refresh_due` is the next calendar month; each new run gets a rolling 60-month
horizon, and all prior runs remain intact.

The researcher returns exactly one JSON object with either
`{"exit_model_inputs":<Inputs>}` or
`{"exit_model_blocked":{"missing_inputs":["explicit gap"]}}`.
After the existing native terminal review gate permits the queue reply, the
adapter validates it, enforces resource bounds, rechecks current owner/policy,
and submits it through the same typed control. The research admission remains
immutable; the calculation or blocked result is a new run. A failed queue turn
also produces an explicit blocked outcome. No numeric E5 is assigned merely
because a task was queued or a model emitted a prose valuation.

Research's references are not independently verified by this calculator, and
shared portfolio capacity is not reconciled here. Those are explicit future
review/plan adapters, not `ready` claims. Monthly change bridges, calibration
and native game/IP simulation are also intentionally unimplemented.
