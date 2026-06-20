# ATS Synthetic Data — Entity Catalog & Value Pools

Companion to `00-overview-and-runbook.md`. Field schemas are grounded in the
verified smoke test (`tests/business-os/ats_golive_smoke.sh` +
`ats_golive_seed.sql`) and the native handlers in
`src/core/business_os/store.rs`. **Re-check field names against the handler
before a run** — a wrong field is a silent no-op.

Conventions for every record: `id == record_id`; include `"_deleted": false`,
`"synthetic": true`, `"batch_id": "<batch>"`, `created_at_ms`, `updated_at_ms`.
Timestamps are epoch **milliseconds**.

---

## A. Funnel (ratios of `SCALE`, default 500 candidates)

A realistic Personalvermittler funnel — wide at intake, tapering to hires:

| Stage / entity | Share | At SCALE=500 |
|---|---|---|
| `candidates` | 100 % | 500 |
| `vacancies` | 10 % | 50 (across ~25 client accounts) |
| `applications` | 100 % (1 per candidate) | 500 |
| → stage `neu` | 30 % | 150 |
| → `screening` | 22 % | 110 |
| → `telefoninterview` | 16 % | 80 |
| → `kundenvorstellung` | 12 % | 60 |
| → `vertragsangebot` | 8 % | 40 |
| → `eingestellt` | 7 % | 35 |
| → `abgelehnt` / `on-hold` | 5 % | 25 |
| `business_consents` (present_to_client) | ~40 % of candidates | 200 |
| `business_credentials` | ~1.5 per AÜG candidate | ~400 |
| `submissions` | ~30 % | 150 |
| `placements` | ~10 % (of which `AUE_SHARE`=0.55 AÜG) | 50 |
| `planning_time_records` + sign-off | 1–3 per AÜG placement | ~60 |
| `signature_requests` | ~1 per offer/AÜG contract | ~50 |
| `interview_meetings` | candidates in interview stages | ~140 |
| Edge/blocked records | `EDGE_SHARE`=0.12 | sprinkled |

---

## B. Direct-write stammdaten (seed into `business_records`)

### `vacancies`
```json
{ "id": "...", "title": "<job title>", "client_account_id": "acct_<n>",
  "location": "<DE city>", "placement_type": "aue|festanstellung",
  "tariff_group": "<tariff>", "open_positions": 1-5, "status": "open" }
```

### `candidates`
```json
{ "id": "...", "first_name": "...", "last_name": "...",
  "email": "<first>.<last>@example.de", "phone": "+49 ...",
  "skills": ["...","..."], "status": "active" }
```
> All candidates are **fictional** (`example.de` domain, fabricated names). Never
> seed real PII.

### `business_consents`  (gates `ats.submission.present`)
```json
{ "id": "...", "subject_id": "<candidate id>", "purpose": "present_to_client",
  "legal_basis": "consent", "granted_at_ms": <past>, "withdrawn_at_ms": 0,
  "expires_at_ms": 0 }
```
- For submission to pass: `legal_basis="consent"`, `granted_at_ms>0`, not
  withdrawn, not expired, `purpose="present_to_client"`.
- If `CTOX_BUSINESS_OS_REQUIRE_LEGAL_BASIS_EVIDENCE=1`, non-consent bases need a
  non-empty `basis_evidence` — for synthetic data just use `legal_basis=consent`.

### `business_credentials`  (gates `ats.placement.create` AÜG + `ats.deployment.check`)
```json
{ "id": "...", "subject_id": "<candidate id>", "credential_type": "<type>",
  "deployment_blocking": true, "verified": true, "valid_until_ms": <future> }
```
- A credential counts as valid when `verified=true`, `credential_type` matches a
  `CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS` entry, and `valid_until_ms` is in
  the future. Edge cases: `verified=false` → Unverified; past `valid_until_ms`
  → Expired.

### `planning_time_records`  (the Leistungsnachweis; sign-off bills it)
```json
{ "id": "...", "subject_id": "<cand>", "candidate_id": "<cand>",
  "entleiher_account_id": "<client acct>",
  "entries": [ {"type":"regular","hours":40.0}, {"type":"nacht","hours":4.0} ],
  "surcharge_pct": { "nacht": 25.0 } }
```
- `entries[].type` ∈ {`regular`,`nacht`,`sonntag`,`feiertag`,`mehrarbeit`}
  (`HOUR_TYPES` in `shiftflow/core/leistungsnachweis.js`) with matching
  `surcharge_pct` keys. Hours positive. Sign-off needs a `charge_rate` passed in
  the command (§C).
- **Note — collection is overloaded:** the `shiftflow/schema.js`
  `planning_time_records` schema is a *generic timesheet* (employee_id, shift_id,
  start_time/end_time, breaks, billing_status). The **ATS Leistungsnachweis** uses
  the *same collection* with the `entries[]`/`surcharge_pct`/`subject_id`/
  `entleiher_account_id` shape above, which is what the `ats.leistungsnachweis.
  signoff` handler + `compute_nachweis_billing` actually read (proven by the smoke
  test: net_total=1575). Seed the ATS shape; both shapes coexist via
  `additionalProperties`.

### `interview_meetings`
```json
{ "id":"...", "candidate_id":"<cand>", "vacancy_id":"<vac>",
  "parties":[{"name":"..."}], "start":<future_ms>, "end":<+1h_ms>,
  "location_mode":"video|onsite|phone", "video_link":"https://...",
  "state":"scheduled" }
```

---

## C. Command-created flow entities (dispatch)

### `ats.intake.capture` → `applications`
```json
{ "name":"<full name>", "email":"...", "phone":"...",
  "vacancy_id":"<vac>", "channel":"<source>" }
```
Result: `{ ok, application_id, dedupe_key }`. (Or seed `applications` directly
with `data.pipeline.stage` to control the funnel stage precisely.)

### `ats.submission.present` → `submissions`
```json
{ "candidate_id":"<cand>", "client_account_id":"<acct>",
  "vacancy_id":"<vac>", "client_contact_id":"<contact|null>" }
```
Result ok: `{ allowed:true, submission_id }`. Blocked (no/invalid consent, or
re-presenting same candidate→client): `{ allowed:false, blockers:[{reason}] }`
with `double_submission` + `conflicting_submission_id`.

### `ats.placement.create` → `placements`
```json
{ "candidate_id":"<cand>", "client_account_id":"<acct>",
  "placement_type":"arbeitnehmerueberlassung|festanstellung",
  "required_types":["<aue cred type>", ...],   // drives the AÜG gate
  "fee": <number>, "guarantee_days": <int> }
```
Result ok: `{ ok:true, allowed:true, placement_id, fee_invoice_id? }`. AÜG without
a valid credential: `{ ok:true, allowed:false, blockers:[{credential_type:"<type>",
reason:"missing|expired|unverified"}] }` — blockers is a **structured array** (one
entry per failing required credential), not a flat field.

### `ats.leistungsnachweis.signoff`
```json
{ "collection":"planning_time_records", "record_id":"<nachweis id>",
  "charge_rate": <€/h, finite >0>, "entleiher_account_id":"<acct>",
  "signature_request_id":"<id, if REQUIRE_ENTLEIHER_SIGNATURE=1>" }
```
Result is always the **unified shape** `{ ok:true, record_id, billing_released,
blockers:[], invoice_id, net_total }`: on success `billing_released:true` +
`invoice_id` + `net_total>0` + empty `blockers`; when gated `billing_released:false`
+ `blockers:["missing_charge_rate"|"no_billable_hours"|...]` + empty `invoice_id` +
`net_total:0`.

### `ats.signature.request` / `ats.signature.sign`
```json
// request:
{ "document_id":"<doc>", "subject_kind":"offer|aue_contract",
  "signers":["<id>", ...] }     // -> request_id, status:"sent"
// sign (per signer):
{ "request_id":"<id>", "signer_id":"<id>" }   // -> status (completed when all signed)
```

### `ats.placement.early_leave`  (edge: guarantee clawback)
```json
{ "placement_id":"<id>", "left_at_ms": <within guarantee window> }
```
Result: clawback + credit note.

---

## D. German value pools (LOCALE=de)

**First names:** Lukas, Leon, Felix, Jonas, Maximilian, Elias, Paul, Ben, Noah,
Luis · Marie, Sophie, Emma, Hannah, Lena, Laura, Lea, Mia, Anna, Julia ·
(diverse) Mehmet, Ayşe, Ivan, Oliwia, Andrei, Nguyen, Fatima, Goran.
**Last names:** Müller, Schmidt, Schneider, Fischer, Weber, Meyer, Wagner,
Becker, Schulz, Hoffmann, Koch, Richter, Klein, Wolf, Yılmaz, Nowak, Kowalski.
**Cities:** Berlin, Hamburg, München, Köln, Frankfurt, Stuttgart, Düsseldorf,
Dortmund, Essen, Leipzig, Bremen, Hannover, Nürnberg, Duisburg.

**Job titles (mix temp & perm):** Gabelstaplerfahrer (m/w/d), Produktionshelfer,
Lagerist, Kommissionierer, CNC-Fräser, Elektroniker, SHK-Monteur,
Pflegefachkraft, Altenpfleger, Servicekraft, LKW-Fahrer (CE), Schweißer,
Industriemechaniker, Bürokauffrau, Buchhalter, IT-Administrator,
Maschinenbediener, Reinigungskraft, Sicherheitsmitarbeiter, Köchin.

**Credential types** (`credential_type` — align with the tenant's
`AUE_REQUIRED_CREDENTIALS`): `aufenthaltstitel`, `a1_bescheinigung`, `g25`,
`staplerschein`, `fuehrerschein_ce`, `gesundheitszeugnis`, `sachkunde_34a`,
`schweisserpass`, `ersthelfer`, `masernschutz` (Pflege).

**Tariff groups** (Zeitarbeit, iGZ/BAP-style): `ZAG-E1` … `ZAG-E9` (or
`EG1`…`EG9`). **Charge rate €/h:** 18–55 by tariff. **Fee (Personalvermittlung):**
1.5–2.5 monthly salaries → ~4 000–14 000 €. **Guarantee days:** {90, 120, 180}.

**Channels:** `indeed`, `stepstone`, `arbeitsagentur`, `empfehlung`, `website`,
`xing`, `linkedin`, `initiativbewerbung`.

**Surcharges (`surcharge_pct`):** nacht 25, sonntag 50, feiertag 100, mehrarbeit
25 (illustrative; align to the tenant's tariff).

---

## E. Internal consistency rules (the generator must hold these)

1. A `submissions` row ⇒ its candidate has a valid `business_consents`
   (`present_to_client`) **unless** it is an intentional EDGE block.
2. An AÜG `placements` row ⇒ its candidate has valid `business_credentials` for
   every `required_types` entry **unless** intentional EDGE.
3. `applications.vacancy_id`, `submissions.vacancy_id`,
   `placements.client_account_id` reference seeded vacancies/accounts (no
   dangling refs).
4. `placement_type` on a placement matches the vacancy's `placement_type`.
5. `planning_time_records` exist only for AÜG placements; `entleiher_account_id`
   matches the placement's client.
6. Stage distribution sums to the candidate count; ids are deterministic
   (`syn_<entity>_<batch>_<n>`).
