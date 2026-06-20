# Task: GENERATE mass, realistic synthetic ATS data with your own intelligence

You are populating a **disposable CTOX test instance** ("ninja") with realistic
German Personalvermittler (staffing/recruiting) data. **You author the content** —
believable people and jobs, not templated stubs. An injector lands your JSON into
the live ATS correctly; you focus on *realism, variety, and completeness*.

This is deliberately a large generative job. Do NOT shortcut it with repetitive or
formulaic values — that defeats the purpose.

## Access
```
ssh -i ./ninja_key -p 22012 -o StrictHostKeyChecking=accept-new ctox@217.182.134.181
export PATH=$HOME/.local/bin:$PATH
```
Copy `ats_inject.py` to ninja (`scp -i ./ninja_key -P 22012 ats_inject.py ctox@217.182.134.181:~/`).

## Loop (target: >= 150 candidates, in batches of ~15–20)
For each batch:
1. **Author** a JSON file of rich records (schema below) — write it locally, then
   `scp` it to ninja.
2. `ssh ... 'export PATH=$HOME/.local/bin:$PATH; python3 ~/ats_inject.py ~/batch_NN.json'`
3. Read the printed counts; keep going until >= 150 candidates exist.

## Realism bar (this is the whole point — vary EVERYTHING)
- **Names**: realistic German AND migrant-background names (Turkish, Polish, Arabic,
  Vietnamese, Russian, Italian, …) reflecting a real German labour pool. No repeats.
- **Emails**: varied real providers (gmx.de, web.de, t-online.de, gmail.com,
  outlook.de, freenet.de); realistic local-parts (initials, dots, numbers) — not a
  single formula. **Synthetic but plausible; never a real person's address.**
- **Phones**: varied German mobile/landline formats (+49 151…, 0171…, 030/…).
- **CV depth**: `summary` (2–4 sentences, role-specific), `work_history` (2–4 jobs
  with real-sounding German companies, roles, date ranges, 1-line descriptions),
  `education` (Ausbildung/Studium with institution + year), `certifications`,
  `languages` (with levels), `skills` (role-appropriate, varied), realistic
  `salary_expectation_eur`, `availability`, `city`, `birth_year`, `source_channel`.
- **Vacancies**: real job titles across sectors (Logistik, Pflege, Handwerk,
  Produktion, Office, IT, Gastro); a 3–5 sentence `description`; concrete
  `requirements`; `tariff_group`, `salary_range_eur`, `location`, `placement_type`
  (aue|festanstellung), an `industry`/account.
- **Interviews**: where present, a `scorecard` with per-competency ratings AND
  short free-text `note`s that sound like a real interviewer, plus a
  `recommendation`.

## Funnel + gate consistency (so the data is internally coherent)
Spread candidates across stages: neu, screening, telefoninterview,
kundenvorstellung, vertragsangebot, eingestellt, abgelehnt, on-hold (wide at the
top, few hired). Then:
- A candidate at `telefoninterview`+ that you `submit:true` MUST have a valid
  `consent` (purpose `present_to_client`, legal_basis `consent`,
  `granted_days_ago` small) — UNLESS you intend an edge block (omit/withdraw it).
- A `placement` with `placement_type:"arbeitnehmerueberlassung"` lists
  `required_types` (e.g. ["staplerschein","g25"]); the candidate MUST have valid
  `credentials` of those types — UNLESS you intend an edge block (expired/missing).
- ~10–15 % deliberate edge cases (withdrawn/missing consent, expired credential,
  no charge_rate) so blocked outcomes appear. The injector reports
  submissions_ok/blocked and placements_ok/blocked — both should be > 0.
- AÜG placements that should bill: add a `nachweis` (entries with hours, surcharge,
  charge_rate_eur) → produces an invoice.

## Input JSON schema (per batch file)
```json
{ "batch": "agy-001",
  "accounts": [ {"id":"acc_…","name":"… GmbH","industry":"…","city":"…"} ],
  "vacancies": [ {"id":"vac_…","title":"…","client_account_id":"acc_…","location":"…",
                  "placement_type":"aue|festanstellung","tariff_group":"…",
                  "description":"…","requirements":["…"],"salary_range_eur":"…","open_positions":2} ],
  "candidates": [ {
    "id":"cand_…","first_name":"…","last_name":"…","email":"…","phone":"…","city":"…","birth_year":1990,
    "headline":"…","summary":"…","skills":["…"],"languages":[{"lang":"Deutsch","level":"Muttersprache"}],
    "work_history":[{"company":"…","role":"…","from":"2019","to":"2023","desc":"…"}],
    "education":[{"institution":"…","degree":"…","field":"…","year":2015}],
    "certifications":[{"name":"…","issuer":"…","year":2021}],
    "salary_expectation_eur":42000,"availability":"sofort","source_channel":"stepstone",
    "stage":"kundenvorstellung","vacancy_id":"vac_…","client_account_id":"acc_…",
    "consent":{"purpose":"present_to_client","legal_basis":"consent","granted_days_ago":5,"evidence":""},
    "credentials":[{"credential_type":"staplerschein","verified":true,"valid_until_days":400,"issuer":"DEKRA"}],
    "submit":true,
    "placement":{"placement_type":"arbeitnehmerueberlassung","required_types":["staplerschein","g25"],"fee_eur":6800,"guarantee_days":90},
    "nachweis":{"entries":[{"type":"regular","hours":40},{"type":"nacht","hours":6}],"surcharge_pct":{"nacht":25},"charge_rate_eur":34.5},
    "interview":{"scheduled_in_days":4,"mode":"onsite","parties":[{"name":"…"}],
                 "scorecard":{"overall":4,"competencies":[{"name":"Fachkompetenz","rating":4,"note":"…"}],"recommendation":"einstellen","notes":"…"}}
  } ]
}
```
Most fields are optional per candidate — only candidates with `submit/placement/
nachweis/interview` exercise those steps; early-stage candidates can omit them.

## Report (your deliverable text)
The cumulative injector counts, the final total candidate count, 2 FULL sample
candidate records verbatim (one placed AÜG worker, one early-stage) so a reviewer
can judge realism, and any inconsistency you hit. Be factual.
