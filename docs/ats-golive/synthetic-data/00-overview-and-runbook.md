# ATS Synthetic Test-Data — Spec & Runbook (for a coding agent)

Goal: hand this file (+ `10-entity-catalog.md`) to a coding agent so it can
generate **mass, realistic, internally-consistent** synthetic ATS data on a
**test/sandbox CTOX instance** (e.g. the `ninja` instance), to exercise the
Business OS ATS end-to-end (UIs, gates, billing, DSGVO, audit) at volume.

> This is for a **throwaway test instance only**. Do **not** run it against a
> production tenant or the operator's live instance. Every record is tagged
> `synthetic: true` + a `batch_id` so the whole batch can be purged.

---

## 1. Hard rules (read first)

1. **Never invent an alternate data path.** Business OS data lives in CTOX DB.
   The two supported ways to create records on a test instance you have host
   access to:
   - **Command dispatch** (preferred for anything with a server command — it
     exercises the real policy gates + writes audit events + invoices):
     `ctox business-os commands dispatch --json '{...}'`.
   - **Direct store seed** for browser-authority *stammdaten* that have **no**
     server command (see §3): one `INSERT` into `business_records` on the
     instance host, exactly as the proven smoke test
     (`tests/business-os/ats_golive_smoke.sh`) does. This is acceptable **only**
     on a test instance you administer; it is the same write the browser UI would
     replicate, minus the browser.
2. **No raw SQL / HTTP / browser-remote against a production instance**, and no
   MCP `push_rxdb_record` (the typed MCP channel does not create raw records).
   MCP-only access is therefore **insufficient** for the stammdaten in §3 — you
   need host/CLI (or SSH) access to the test instance.
3. **Mark everything synthetic.** Every record payload carries
   `"synthetic": true` and `"batch_id": "<run-id>"`. This is the purge handle.
4. **Respect the gates.** Generate in the §4 order so each gated command can
   succeed (a submission needs a valid consent; an AÜG placement needs a valid
   credential; a Leistungsnachweis sign-off needs a `charge_rate`). Producing
   *some* deliberately-blocked records is desirable (§5) — but do it on purpose,
   not by accident.
5. **Determinism.** Seed the RNG from a fixed value so a run is reproducible;
   derive record ids deterministically (`syn_<entity>_<batch>_<n>`), so a re-run
   is idempotent (`INSERT OR REPLACE`) instead of duplicating.

---

## 2. Mechanism — how to write each record

### 2a. Command dispatch (gate-exercising flow entities)

```bash
ctox business-os commands dispatch --json '{
  "command_type": "ats.intake.capture",
  "module": "intake",
  "payload": { ...see catalog... },
  "client_context": { "actor": { "id": "<chef-or-admin-user-id>" } }
}'
```

- The handler JSON result is nested under `.result` of the response.
- **Every dispatch needs a unique `command_id`** — a repeat returns the stored
  outcome instead of acting. The CLI generates one per call; if you template the
  envelope yourself, vary `command_id`.
- Mutating `ats.*` commands require a **chef/admin** actor (seed one first, §4.0).
- If `CTOX_BUSINESS_OS_REQUIRE_CAPABILITY_TOKEN=1` is set on the instance, mint a
  token first: `ctox business-os auth issue-capability --user <id>` and pass it as
  `client_context.capability_token`. For a sandbox, leaving enforcement off is
  simpler.

### 2b. Direct store seed (stammdaten with no command)

```bash
sqlite3 "<CTOX_ROOT>/runtime/business-os.sqlite3" \
  "INSERT OR REPLACE INTO business_records(collection,record_id,rev,deleted,updated_at_ms,payload_json)
   VALUES('business_consents','syn_consent_<batch>_<n>','1-syn',0,<now_ms>, json('{ ...see catalog... }'));"
```

Columns are fixed: `collection, record_id, rev, deleted, updated_at_ms,
payload_json`. The payload `id` must equal `record_id`; always include
`"_deleted": false`, `"synthetic": true`, `"batch_id": "<batch>"`.

> Prefer a small generator program (Node/Python) that emits batched `INSERT`s in
> one transaction (`BEGIN; … ; COMMIT;`) — thousands of single `sqlite3`
> invocations are slow. Dispatch calls stay one-process-per-call (they go through
> the native handler).

---

## 3. Command coverage — which entity uses which mechanism

Grounded in `src/core/business_os/store.rs` (the ATS handlers) — **verify before
running**, the surface can change:

| Entity / collection | Has server command? | Mechanism |
|---|---|---|
| `applications` (candidate intake) | ✅ `ats.intake.capture` | dispatch (or seed) |
| `submissions` | ✅ `ats.submission.present` | **dispatch** (consent gate) |
| `placements` | ✅ `ats.placement.create` | **dispatch** (AÜG gate) |
| `signature_requests` | ✅ `ats.signature.request` / `.sign` | dispatch |
| `planning_time_records` sign-off | ✅ `ats.leistungsnachweis.signoff` (updates) | dispatch |
| `vacancies` | ❌ none | **seed** |
| `candidates` | ❌ none | **seed** |
| `business_consents` | ❌ none (only `ats.consent.check` reads) | **seed** |
| `business_credentials` | ❌ none (only `ats.deployment.check` reads) | **seed** |
| `planning_time_records` (the record) | ❌ none (sign-off only updates) | **seed** |
| `interview_meetings` / `interview_scorecards` | ❌ none | **seed** |
| `offers` | ❌ none | **seed** |

> **Finding worth surfacing:** the gate-critical stammdaten (consents,
> credentials, vacancies, nachweise) have **no create command**, so a remote
> MCP-only agent cannot seed them. If we want mass generation without host
> access, the platform would need test-only seed commands (or an admin
> bulk-import). For now the runbook assumes host/CLI access to the sandbox.

---

## 4. Generation order (so gates are satisfiable)

`SCALE` parameterizes volume (default `SCALE=500` candidates; everything else is
a ratio of it — see `10-entity-catalog.md §Funnel`).

0. **Seed actors:** a `chef` + `admin` row in `business_users` (the dispatch
   actor). See `tests/business-os/ats_golive_seed.sql`.
1. **Stammdaten (seed):** `~SCALE/20` clients/accounts → `~SCALE/10` `vacancies`
   (mix `placement_type` aue/festanstellung) → `SCALE` `candidates`.
2. **Consents (seed):** for the share of candidates that reach submission, a
   valid `business_consents` row (`purpose=present_to_client`,
   `legal_basis=consent`, granted, not withdrawn/expired).
3. **Credentials (seed):** for AÜG-bound candidates, valid `business_credentials`
   covering the tenant's `CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS` types
   (`verified:true`, future `valid_until_ms`). Sprinkle expired/unverified ones
   for edge cases (§5).
4. **Applications (dispatch `ats.intake.capture`):** one per candidate; spread
   `status`/stage across the funnel.
5. **Submissions (dispatch `ats.submission.present`):** for the submitted share —
   succeeds only where step 2 gave a valid consent.
6. **Placements (dispatch `ats.placement.create`):** for the placed share; for
   AÜG placements pass `required_types` → succeeds only where step 3 gave valid
   credentials.
7. **Leistungsnachweise:** seed `planning_time_records` (entries+hours) for AÜG
   placements, then dispatch `ats.leistungsnachweis.signoff` with a `charge_rate`
   → invoices.
8. **Signatures (dispatch `ats.signature.request` + `.sign`):** for offers /
   AÜG contracts.
9. **Interviews (seed):** `interview_meetings` (+ some `interview_scorecards`)
   for candidates in interview stages.

---

## 5. Edge-case mix (generate ~10–15 % deliberately "off")

So the gates, UIs and DSGVO paths are exercised, not just the happy path:

- expired / unverified / missing credentials → blocked AÜG placements
  (`aue_license=missing`, `medical_clearance=missing`).
- withdrawn or wrong-purpose consents → blocked submissions
  (`double_submission` by re-presenting the same candidate→client).
- Leistungsnachweise with no `charge_rate` or zero hours → `missing_charge_rate`
  / `no_billable_hours` blocks.
- placements within the guarantee window → dispatch `ats.placement.early_leave`
  → clawback credit notes.
- a few subjects flagged for a later `ats.subject.erase` (DSGVO Art. 17) test.

---

## 6. Safety — purge the batch

Every record carries `synthetic:true` + `batch_id`. To remove a run on a test
instance:

```bash
sqlite3 "<CTOX_ROOT>/runtime/business-os.sqlite3" \
  "DELETE FROM business_records
   WHERE json_extract(payload_json,'$.batch_id') = '<batch>';"
```

(Command-created records — applications/submissions/placements/invoices/
signature_requests — inherit the `synthetic`/`batch_id` fields only if the
payload carried them; for those, either include the markers in the command
payload where the handler passes them through, or purge by id-prefix
`record_id LIKE 'syn_%'` / by the deterministic id list you emitted.)

For a DSGVO-path purge of specific subjects, dispatch `ats.subject.erase`
(`payload.subject_id`) — that exercises the real Art. 17 redaction+tombstone.

---

## 7. Verify the batch (don't trust the inserts)

After a run, assert consistency — mirror the smoke test's style:

1. **Counts** per collection + per pipeline stage match the intended funnel.
2. **Gate consistency:** every `submissions` row has a matching valid
   `business_consents`; every AÜG `placements` row has matching valid
   `business_credentials`. A row that violates this means the generator wrote a
   stammdaten step out of order.
3. **Spot-dispatch reads:** `ats.consent.check` / `ats.deployment.check` for a
   sample of subjects return the expected allow/deny.
4. **Invoices:** AÜG sign-offs produced postable `accounting_invoices`
   (`net_total > 0`).
5. `tests/business-os/ats_golive_smoke.sh` still passes against a fresh root
   (the generator must not have changed handler behavior).

---

## 8. Parameters (summary)

| Param | Default | Meaning |
|---|---|---|
| `SCALE` | 500 | number of candidates; all other volumes are ratios of it |
| `SEED` | 42 | RNG seed (reproducible) |
| `BATCH` | `syn-<date>` | batch id stamped on every record (purge handle) |
| `AUE_SHARE` | 0.55 | fraction of placements that are Arbeitnehmerüberlassung |
| `EDGE_SHARE` | 0.12 | fraction of records in a deliberately-blocked/edge state |
| `LOCALE` | `de` | value pools are German (see catalog) |

See `10-entity-catalog.md` for the exact per-entity field schema, the German
value pools, and the funnel ratios.
