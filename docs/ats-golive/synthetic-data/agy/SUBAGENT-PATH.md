# Sub-agent generation path (Claude sub-agents, when agy is rate-limited)

agy/Antigravity rate-limits after ~15–20 generations (empty output, see README).
The reliable path is to generate the rich content with **Claude sub-agents** (a
Workflow: 4 agents × 15 = 60 records per run, structured output) and land it with
`ats_inject.py`. Run ~60 per invocation (not thousands at once); repeat to grow.

The injector is hardened against LLM field variation (sub-agents vary keys freely):
- `interview.scorecard` may be a LIST of competencies or a dict — handled both.
- credential type key may be `credential_type` | `type` | `name`; issuer `issuer`|`issued_by`.
- `nachweis.surcharge_pct` may be a number — coerced to {} when not a dict.
- a candidate's `client_account_id` is often null (only `vacancy_id` set) — resolved
  from the vacancy's client, else the batch's single account.

Verified on ninja: 60 records, 55/60 distinct names, real CVs (Deutsche Bahn, DPD,
Kühne+Nagel…), rich scorecards/job posts, both gates firing (25 submissions, 10
placements, 15 invoices), 0 consistency violations.
