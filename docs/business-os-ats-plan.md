# Business OS — ATS Build-Out Plan

Status: working backlog · Owner: Business OS app platform · Scope: deliver a
complete Applicant Tracking System (ATS) and staffing-agency workflow on top of
CTOX Business OS, by **generalizing/extending existing apps and adding the few
missing generic engines** — never by building recruiter-only apps.

This document is cut for **parallel sub-agent execution**. Each ticket names the
generic engine it serves, the recruiting *configuration* layer, the exact build
layer (frontend vs CTOX backend), the files it owns, and its dependency wave.

---

## 0. Prime directive — Business OS is a Baukasten, not a recruiter app

**This is the first rule and it overrides everything below.**

Business OS is a construction kit for *many* industries (Steuerberatung,
Maklerbüro, Pflegedienst, Handwerk, Agentur, …). A Personalvermittler is just
one tenant. Therefore:

1. **No ticket may produce a recruiter-only app.** Every gap is closed by a
   *generic engine* (the reusable app/module) plus a *recruiting configuration*
   (data, matching definition, channel config, document template, skill prompt,
   locale strings, preset/seed records). The recruiting knowledge lives in the
   config layer, **not** in bespoke module code.

2. **The litmus test for every ticket:** *"Would a tax advisor, a real-estate
   office, a care service, or a marketing agency use this same engine by
   swapping config/templates/data?"* If the answer is no, the design is wrong —
   re-cut it as a generic engine + a thin recruiting profile.

3. **Follow the precedent that already exists.** `matching` is already a generic
   two-sided engine: configurable `source` / `object` / `match` columns, with
   `candidate_job.v1` shipped merely as the *default* definition and specialized
   per tenant via `ctx.matchingDefinition`. Every new engine must be that shape:
   generic mechanism, recruiting profile as data.

4. **Where recruiting specifics are allowed to live:**
   matching definitions · channel/account config · document templates · skill
   prompts (`SKILL.md`) · locale `de.json`/`en.json` · preset & seed records ·
   per-tenant pipeline-stage labels. Nowhere else.

5. **Naming/IDs stay generic.** Collections, command types, and module ids are
   named for the generic capability (`business_credentials`, not
   `candidate_certificates`; `record_pipeline`, not `vacancy_kanban`). Recruiting
   labels are locale/config, not schema.

If a reviewer can read a new collection/command/module name and tell it is
"for recruiting only," reject it as a Baukasten violation.

---

## 1. How to read this plan

### 1.1 Build-layer tags (this is the "JS vs backend" axis the plan must make explicit)

| Tag | Meaning | What a sub-agent touches |
|-----|---------|--------------------------|
| 🟢 **JS-only** | Pure browser logic over already-contracted collections. No schema change, no Rust. | `modules/<id>/*.js`, locales, templates |
| 🟡 **Schema-plumbing** | New/changed **replicated** collection. Browser `schema.js` **plus** mandatory native contract + hash regeneration (mechanical, guarded — but it *is* a backend touch). No new Rust handler logic. | `modules/<id>/schema.js` + regenerate contract & hash fixtures + rebuild `dist/` + bump 3 cache-busters |
| 🔴 **Native backend** | A new server-authoritative `business_commands` handler, projection loop, or policy gate. CTOX Rust function does not exist yet. | `src/core/business_os/*.rs` (+ schema-plumbing) |
| 🧠 **Native + Skill** | Needs a real LLM via a harness `SKILL.md` **and** a native writeback handler. | `src/skills/.../SKILL.md` + `src/core/business_os/store.rs` writeback (+ schema-plumbing) |

> **Why 🟡 is still a backend touch:** the WebRTC handshake *silently quiesces*
> any collection whose schema hash the native peer does not know or that differs
> by a byte. So **every replicated collection** must exist byte-identically on
> both sides. See §2.1. A collection that is genuinely browser-local (never
> replicated) is the only 🟢 schema case, and is rare.

### 1.2 The "missing CTOX backend functions" register

Section §5 is the consolidated list of Rust/skill functions that **do not exist
today** and must be built. Every 🔴/🧠 ticket points back to an entry there. This
is the answer to *"where in the CTOX backend are functions still missing."*

### 1.3 New apps are created through the App Creator — never hand-rolled

Any *new* module (not an extension of an existing one) **must** be produced via
the **App Creator native agent-driven path** and satisfy the hardened fragment
contract (§3). Hand-authoring module files outside that contract is rejected by
the validators and by the queue. Do **not** touch the app queue — CTOX service
owns completion.

### 1.4 Parallelization & collision rules

- Tickets in the same **wave** (§4) are independent and may run concurrently.
- **One module per agent.** An agent owns exactly its `module_id` directory (new
  app) or the explicitly listed files of one existing module (extension). The
  App Creator's bounded-shell rules already forbid writing another module's dir.
- **Shared-file hotspots** (`src/core/business_os/store.rs`,
  `business_os_schema_contract.json`, `business_os_schema_hashes.json`,
  `rxdb/src/schema.mjs`, `dist/ctox-rxdb-js.mjs`, `registry.json`) are
  **serialized**: only one schema-plumbing/native ticket lands at a time, commit
  atomically, rebase-autostash around the parallel Codex agent on `main`. See
  §4.0 "merge discipline."
- Recruiting **config/data** tickets (matching definitions, templates, locales,
  presets) never collide and can always run in parallel.

---

## 2. The architectural seams every ticket stands on

(Verified against the live code. These are the load-bearing facts; tickets
reference them rather than re-deriving them.)

### 2.1 Schema seam — adding a collection

- Source of truth: `src/apps/business-os/modules/<id>/schema.js`
  (`export const collections = { <name>: <RxJsonSchema> }` + optional
  `migrationStrategies`). Registered lazily per-module via
  `app.js registerModuleSchemas()` → shell `db.addCollections()`.
- Fans out to: native contract `src/core/business_os/business_os_schema_contract.json`
  (generated by `src/core/rxdb/tools/build_business_os_schema_contract.mjs` from a
  curated `MODULES` list) **and** the schema-hash parity registry
  (`CTOX_BUSINESS_OS_SCHEMA_HASHES` in `rxdb/src/schema.mjs` ↔
  `src/core/business_os/business_os_schema_hashes.json`).
- **Replication requires byte-identical schema on both sides + a matching hash
  entry.** A drifted/unknown collection is quiesced silently (room stays up).
- **Never hand-edit** `dist/ctox-rxdb-js.mjs` or the generated contract; run the
  generators, rebuild `dist/`, bump the three `?v=` cache-busters
  (`shared/sync.js`, `matching/ui/businessOsDataSource.js`, +1 — see
  `docs/ctox-rxdb.md`).
- Guards: `cargo test --manifest-path src/core/rxdb/Cargo.toml`
  (`native_all_schema_hashes_match_browser_contract_fixture`),
  `node src/apps/business-os/rxdb/tests/run-all.mjs`.
- If the collection must be populated from CTOX core state, add a projection loop
  in `src/core/business_os/rxdb_peer.rs`; decide required-vs-optional via
  `is_required_native_collection()`.

### 2.2 Command-bus seam — server-authoritative actions

- Path: browser `ctx.commandBus.dispatch` → `business_commands` doc →
  `store::accept_rxdb_business_command` match arm → typed mutation → session +
  policy eval → native handler (trust boundary) → `write_rxdb_control_command_outcome`
  → reproject in `rxdb_peer.rs`.
- **Frontend-only (no command)** when data is local/user-authority, no server
  policy gate, no external effect: write the RxDB doc directly
  (`collection.insert`/`doc.patch`/`incrementalUpsert`).
- **Native handler required** for policy-gated, server-authoritative, external-
  effect, or cross-projection actions. New handler = new arm in
  `accept_rxdb_business_command` (literal or `is_<module>_active_command` +
  `handle_<module>_active_command`), mirroring customers/invoices.
- Note: `src/core/business_os/invoices.rs` exists but is **unwired** — declare in
  `mod.rs` and call from the router when used as the offer/placement template.

### 2.3 Skill seam — the two AI execution paths

- **Seam A (deterministic, no model):** branch in
  `store.rs process_source_parse_command` (~9841) + a handler. Today's
  `matching.match` scoring is here: native German keyword-overlap in
  `importer.rs compute_matching_result` (`dimension_score`/`build_match_items`).
- **Seam B (real LLM):** browser dispatches `business_os.chat.task` with
  `payload.skill` + a `writeback_contract`; a harness `SKILL.md` runs; a **native
  writeback command** persists results. `suggested_skill_for_command`
  (`store.rs` ~21830) maps command_type/module → skill name.
- **Native generation alternative:** call the model gateway
  `src/core/execution/responses/gateway.rs` from a service/agent context.
- Skill packs live under `src/skills/packs/business/` (e.g.
  `business-os-import-parser`, `business-os-requirement-matching`).

### 2.4 Policy seam — two layers, server-authoritative

- `src/apps/business-os/shared/permissions.js` = **presentation only**
  (show/enable/grey-out). Never the sole gate for a server mutation.
- `src/core/business_os/policy.rs` + `store.rs handle_business_command`
  (~12300+) + `mcp_channel.rs` = **enforcement**. New gated action must gate
  **both** entry paths: browser→RxDB business_commands **and** the external MCP
  channel.
- Denied path emits `business_os.policy.denied` + a failed outcome with
  `display_reason`; audit via `record_business_policy_decision_event`. The
  block-on-condition precedent is `module_release_data_access_review_summary`
  (~1857) — the model for the credential/consent gates.

### 2.5 App-Creator / lifecycle seam

- Shell↔app contract is `export function mount(ctx)`; the shell injects the DB
  handle as `ctx.db` (apps resolve `ctx.db.collection(name)` — **never import
  upstream `rxdb`, never invent sync**). Standard layout is **pane-mode**.
- New apps are produced via the App Creator (§3). Source modules live in
  `src/apps/business-os/modules/<id>/` + `registry.json`; runtime-installed
  modules live in `runtime/business-os/installed-modules/<id>/` and surface via
  the native catalog projection `write_module_catalog_projection_to_rxdb`
  (`store.rs` ~3540) into `business_module_catalog` — **no manual Rust step** for
  install.

---

## 3. App Creator contract (mandatory for every new app)

New modules are built on the **native agent-driven path**, not the browser
template generator. The hardened prompt block lives in
`src/core/business_os/store.rs:21748-21804`; validators are
`src/apps/business-os/scripts/validate-app-module.mjs` and
`.../business-os-app-module-development/scripts/module_static_check.mjs`. The
authoring skill is
`src/skills/system/product_engineering/business-os-app-module-development/`
(`SKILL.md` + `references/module-contract.md` + `references/green-checklist.md`).

### 3.1 The fragment contract (hard rules — black-box validator gates)

- `index.html` is a **shell fragment only** — no `<!doctype>`, `<html>`,
  `<head>`, `<body>`, `<link>`, `<script>`, `<meta>`, `<title>`, `<style>`.
- `index.js` mounts **exactly** via
  `fetch(new URL('./index.html', import.meta.url))` → `ctx.host.innerHTML`, then
  attaches `./index.css` via `new URL('./index.css', import.meta.url)`; wire
  selectors **after** `innerHTML`. No other runtime network fetch.
- CSS scoped under the module root class; no custom props on `:root`/`html`/
  `body`; never redefine shell tokens (`--surface`, `--text`, `--line`,
  `--accent`).
- Required file inventory: `module.json`, `collections.schema.json`, `schema.js`,
  `index.html`, `index.css`, `index.js`, `icon.svg`, `core/automation.mjs`,
  `core/records.mjs`, `locales/de.json`, `locales/en.json`, `tests/<id>.test.mjs`.
- Manifest: `entry="installed-modules/<id>/index.html"`,
  `install_scope="installed"`, **SemVer `x.y.z`** (no `v` prefix), no inline SVG.
- `schema.js`/`collections.schema.json` export **only module-owned** collections
  (`module.json` may *list* shell collections like `business_commands`).
- **≥1 real automation** through `ctx.commandBus.dispatch` only, with `type` and
  `command_type` both exactly `business_os.chat.task` + a `record_snapshot`. No
  direct `business_commands` writes, no `window.dispatchEvent`.
- Layout: **pane-mode** is standard; `layout.right`/third pane/right-resizer
  requires an explicit justification comment.
- No frameworks (React/Vue/Svelte/…), no `from 'rxdb'`, no `node:*`, no bare
  packages, no `http(s)://`/`cdn.`, no `localStorage`/`sessionStorage`/
  `ctx.db.raw`. Browser-safe ESM only.
- Few-shot: inspect **exactly** the three shipped modules `customers`,
  `shiftflow`, `outbound`. Never use generated/`bench_*`/prior-creator apps as
  templates.

### 3.2 Build loop (per new-app agent)

1. Receive the queue task; build **only** the given `module_id` in
   `runtime/business-os/installed-modules/<id>/`.
2. If an installed app directory already exists, inspect and repair it in place.
   If it does not exist, the App Creator agent must build the app files itself
   in `runtime/business-os/installed-modules/<id>/` using the Business OS app
   module skill. Do not use a scaffold command or generic generated app.
3. Customize with **bounded edits** (no Python/Node writer scripts, no base64,
   no here-doc dumps). Keep the app small: one workbench + one create/edit + one
   automation.
4. Validate: `node --check`, `node --test tests/*.test.mjs`, then
   **`ctox business-os app validate <id> --installed`**. A green custom test does
   **not** count while `validate` is red. Repair validator bullets in order.
5. **Do not touch the queue.** Green → CTOX service acks/completes; red → service
   re-leases to `review_rework` with a `"Business OS app artifact validation
   failed."` prompt. Never call `ctox queue ack/complete/release/fail/block`.

### 3.3 When to extend vs create

- **Extend an existing module** (edit its files directly, no App Creator) when
  the capability is config/schema/JS over an existing engine (customers,
  matching, outbound, shiftflow, calendar, conversations, documents, reports,
  invoices). Most of this plan is extension.
- **Create a new app** (via App Creator) only for the genuinely new generic
  engines: credential/expiry vault, submission/share-out, offer/placement
  lifecycle, e-signature, consent/retention. Even these must be named &
  schema'd generically (Baukasten rule).

---

## 4. The backlog — parallelizable tickets by generic engine

### 4.0 Merge discipline (read before any 🟡/🔴/🧠 ticket)

The repo moves under you (parallel Codex agent on `main`). Stage explicitly,
commit atomically per ticket, `git pull --rebase --autostash`. Schema-plumbing
and native tickets that touch the shared hotspots (§1.4) land **one at a time**.
Run the matching guard suite (§2.1) before pushing any schema change.

### Wave A — foundation (high parallelism, mostly config/schema)

---

#### [PIPELINE-1] Generic record-pipeline / Kanban state-machine → job-order + vacancy + candidate stages · 🟡
- **Generic engine:** stage-ordered record pipeline (a `stage` + `position` +
  `last_stage_changed_at_ms` indexed record set) — the same engine drives a
  sales pipeline, a support flow, *or* a vacancy/candidate board.
- **Recruiting config:** a "requisition" record-pipeline (the job order) and a
  linked "candidate-stage" record-pipeline (Neu → Screening → Telefon →
  Kundenvorstellung → Angebot → Eingestellt → Abgelehnt), as stage *labels* in
  locale/config.
- **Reuses:** `customers` (`customer_opportunities` already ships
  `stage`/`opportunity_type`/`position`/`[stage,position]` index = the generic
  Kanban) + `matching_requirements` as the job-order record body.
- **Frontend/schema:** generalize the customers pipeline component into a shared
  helper; add a structured job-order header (title, dept, location, headcount,
  start, contract_type, shift_model, account_id, contact_id) as a new
  collection **or** as typed fields on `matching_requirements`. Cross-link to
  `customer_accounts`/`customer_contacts`.
- **Native:** schema-plumbing only (contract + hash regen). No new handler.
- **Depends on:** —  **Wave:** A
- **Done when:** vacancy + candidate boards render from synced collections;
  matching guard suite green.

#### [PARSE-1] Generic document-ingest + LLM-structuring skill → CV parsing + enrichment · 🧠
- **Generic engine:** "ingest a document → LLM-structure into typed fields →
  writeback into a versioned record + a faceted `index_text`." Reusable for CVs,
  invoices-in, contracts, any inbound doc.
- **Recruiting config:** a CV/`Lebenslauf` parse profile (work history, Ausbildung,
  certifications/Zeugnisse, languages, skills; German CV conventions) + an
  enrichment pass (seniority, total experience, Mobilität/radius, Kündigungsfrist,
  availability, missing-mandatory flags).
- **Reuses:** `documents`/`document_versions`/`desktop_files` (no new doc store);
  the existing frontend dispatch pattern in `cv-print-builder` (it already sends
  `business_os.chat.task` with `payload.skill` + `writeback_contract`).
- **Backend MISSING (build):** the named skill **`ctox-cv-print-parser`**
  (`SKILL.md`, Seam B) and the native writeback handler
  **`ctox.cv_print.apply_parse`** in `store.rs` — see §5.1. Add
  `suggested_skill_for_command` mapping.
- **Depends on:** — **Wave:** A
- **Done when:** dropping a PDF/DOCX produces a structured, versioned candidate
  profile with certifications + facets; the two half-built parsers are unified
  (retire the dead `cv-print-builder` stub path).

#### [MATCH-1] Generic two-sided scoring upgrade + bulk shortlist + knock-out + AGG audit · 🧠
- **Generic engine:** requirements × objects → scored results, with (a) a real
  LLM scoring path, (b) bulk shortlisting, (c) deterministic knock-out pre-screen
  rules, (d) an audit event for defensible rejection reasons.
- **Recruiting config:** the `candidate_job` matching definition (already the
  default); must-have rule sets (qualification/licence/location/availability/
  work-permit) as config; AGG guardrail (block protected-attribute filters) +
  required non-discriminatory rejection reason codes.
- **Reuses:** `matching` + `importer.rs compute_matching_result` +
  `business-os-requirement-matching` skill pack.
- **Backend MISSING (build):** real bulk scoring (today
  `shortlistObjectsForRequirement` in `matchingTools.js` is a **stub** returning
  `score:0` + first-N; native scorer is keyword-only) → §5.2; a generic compliance
  **audit event** extending `record_business_policy_decision_event` with rejection
  reason codes → §5.3; promote match status off note-hashtag derivation
  (`deriveStatusesFromNotes`) into a structured `stage` field on the pipeline
  (coordinate with PIPELINE-1).
- **Depends on:** PIPELINE-1 (shares the stage field)  **Wave:** A
- **Done when:** a pool of N candidates yields a ranked, evidence-backed
  shortlist; knock-outs auto-flag with recorded reason; rejections write an
  immutable audit row.

#### [VAULT-1] Generic credential / expiry vault (storage) · 🔴
- **Generic engine:** `business_credentials` (subject_id, credential_type,
  issuer, valid_from_ms, valid_until_ms, document_id, verified_by, status) — any
  expiring artifact for any subject (certs, licences, right-to-work, also
  non-HR: insurance, ISO certs, contracts).
- **Recruiting config:** credential-type catalog (Staplerschein, G25/G37,
  Schweißerprüfung, Führerschein, Pflege-Fortbildungen, Aufenthaltstitel,
  Führungszeugnis) as config; expiry reminders via `calendar`.
- **Backend MISSING (build):** new collection (schema-plumbing) + a native
  writeback for verification status. The **block-on-condition gate** is a
  separate ticket (VAULT-2, Wave B) so storage can ship first.
- **Depends on:** — **Wave:** A
- **Done when:** credentials with expiry render + remind; verification is
  server-authoritative.

#### [DISPATCH-1] Generic dispatch / timesheet engine → temp-staffing disposition · 🟡
- **Generic engine:** assignments/shifts + time records with billing status +
  internal/external rate calc + a max-duration/clock tracker. Field-service,
  events, *or* Arbeitnehmerüberlassung.
- **Recruiting config:** the agency→worker→Entleiher triangle as data
  (`planning_projects` = client/location with external rate; `planning_shifts` =
  Einsatz; shift types Früh/Spät/Nacht); ArbZG rule profile (11h Ruhezeit, 8/10h,
  weekly limits) as config that the engine actually evaluates.
- **Reuses:** `shiftflow` (`planning_employees`/`projects`/`shifts`/
  `time_records`) almost wholesale.
- **Frontend/schema:** add Entleiher/shift-type/ArbZG fields; replace the
  **hardcoded** "Keine Regelverletzungen gefunden" string with a real evaluator
  (JS over synced records); cumulative Überlassungsdauer counter per
  worker×Entleiher; Leistungsnachweis record with Entleiher sign-off flag.
- **Native:** schema-plumbing; the **Entleiher-signed → billing-release** gate
  and surcharge calc that must be authoritative may need a 🔴 handler (split into
  DISPATCH-2, Wave B).
- **Depends on:** MASTERDATA-1 (worker rate/tariff)  **Wave:** A
- **Done when:** a dispatcher board with real ArbZG conflict detection + a
  duration counter renders over synced data.

#### [MASTERDATA-1] Generic master-data record + classification → worker Stammdaten + tariff · 🟡
- **Generic engine:** a person/entity master record with attributes + a
  classification/grouping dimension feeding downstream calc.
- **Recruiting config:** Leiharbeitnehmer Stammdaten (Steuer-ID, SV-Nummer,
  Steuerklasse, Krankenkasse, IBAN, Aufenthalts-/Arbeitserlaubnis) + iGZ/BAP/GVP
  Entgeltgruppe + Branchenzuschlagstarif as a `tariff_group` classification field.
- **Reuses:** `shiftflow planning_employees` as the worker record; classification
  is a new field; ID uploads reuse `desktop_files` chunked blob.
- **Native:** schema-plumbing only.
- **Depends on:** — **Wave:** A
- **Done when:** a worker record carries tariff group + payroll-input fields that
  DISPATCH-1 rate calc consumes.

#### [SEQUENCE-1] Generic sequenced-outreach engine → candidate outreach + talent-pool reactivation · 🟡
- **Generic engine:** campaigns + sequences + per-recipient pipeline items +
  engagement tracking + suppression + sender assignment. Audience-agnostic.
- **Recruiting config:** retarget the audience entity from `outbound_companies`
  to the candidate pool; talent-pool tags (silver-medalist, ex-Zeitarbeitnehmer)
  + saved searches as config; WhatsApp/SMS/InMail send channels.
- **Reuses:** `outbound` (`outbound_campaigns`/`sequences`/`pipeline_items`/
  `engagements`/`messages`/`approvals`/`suppression`) + its native `outbound.*`
  send/approval handlers. **Generalize the entity layer — do NOT fork the
  7.7k-LOC engine.**
- **Native:** schema-plumbing for the generalized audience ref; new send channels
  may need a 🔴 handler if they hit an external transport (else reuse existing).
- **Depends on:** PARSE-1 (candidate pool)  **Wave:** A
- **Done when:** a candidate-directed sequence runs with suppression + approvals,
  reusing the outbound engine via config.

#### [SCHEDULE-1] Generic scheduling + structured-form (scorecards) → interview coordination · 🟡
- **Generic engine:** booking/availability/holds + a generic structured-form
  record (a "scorecard" is one form definition).
- **Recruiting config:** interview booking-page templates (multi-party
  recruiter+candidate+client), role-templated interview guides + per-competency
  AGG-defensible scorecards as form definitions.
- **Reuses:** `calendar` (`calendar_availability_rules`/`booking_pages`/`holds`/
  `bookings`/`events`); scorecards = new structured-record collection.
- **Native:** schema-plumbing. **Multi-party + Teams/Zoom link generation** and
  **transcription** are split into SCHEDULE-2 (Wave B, 🧠).
- **Depends on:** — **Wave:** A
- **Done when:** interviews schedule against calendar; scorecards attach to a
  candidate×vacancy record.

#### [CONSENT-1] Generic consent / legal-basis ledger + retention engine · 🔴
- **Generic engine:** a consent ledger (subject_id, purpose, legal_basis,
  granted_at_ms, withdrawn_at_ms) reusable by **every** module, + a native
  retention/deletion (purge-on-expiry) engine with audit events.
- **Recruiting config:** candidate-pool DSGVO purposes (Art. 6/9), Löschfristen /
  Aufbewahrungsfristen profiles, right-to-erasure.
- **Backend MISSING (build):** the ledger collection (schema-plumbing) + a native
  pre-flight that **refuses consent-requiring commands** without a valid consent
  row, + a retention purge loop. Precedent: `mcp_channel.rs audit_retention_days`
  pruning — but it reads a **legacy ENV map**; per AGENTS.md rule 4, move it to
  typed runtime config (no new env toggles). See §5.5.
- **Depends on:** — **Wave:** A (gate wiring lands with consumers in Wave B)
- **Done when:** consent rows exist + are enforced server-side; retention purges
  run from runtime config, audited.

### Wave B — depends on Wave A

---

#### [INTAKE-1] Generic multi-channel intake inbox → application intake · 🔴
- **Generic engine:** inbound capture from shared mailbox / web form / apply
  endpoint, normalized into channel-account/thread/message + a created business
  record. Reusable for support intake, lead intake, *or* applications.
- **Recruiting config:** career-site/job-board/Easy-Apply/QR channel accounts;
  one normalized application record → candidate shard feeding PARSE-1;
  Eingangsbestätigung + stage-status notifications.
- **Reuses:** `conversations` (`communication_accounts`/`threads`/`messages`) for
  the inbox/send backend; `documents` blob for attachments; the webhook-register
  pattern from `iot`/`browser` for form/QR capture.
- **Backend MISSING (build):** the inbound **capture handler** (webform/apply →
  normalized record) is server-authoritative and does not exist; mailbox ingest
  binding. See §5.4.
- **Depends on:** PARSE-1, PIPELINE-1  **Wave:** B
- **Done when:** an inbound application lands as a candidate record with detached
  documents + an auto-acknowledgement.

#### [SHAREOUT-1] Generic submission / share-out + consent ledger + dedupe guard + LLM exposé · 🧠
- **Generic engine:** assemble a shortlist → render an artifact → deliver to a
  recipient → create a tracked submission record (subject×target×recipient×sent_at)
  with a uniqueness/dedupe guard + per-recipient consent rows + a feedback hook.
  Reusable for any "present records to an external party" flow.
- **Recruiting config:** anonymized candidate exposé + consultant write-up to a
  client contact; double-submission/ownership-conflict guard (protects placement
  fee); consent-to-present ledger; client feedback (interested/interview/reject+
  reason) → back into `matching_results`.
- **Reuses:** `outbound` suppression/approvals primitives + `command_id` dedupe
  in `accept_rxdb_business_command`; `documents` for exposé versions; CONSENT-1
  ledger.
- **Backend MISSING (build):** an exposé-generation **skill** (Seam B) + a native
  **present-candidate** policy scope (§5.3) + the submission/consent writeback.
- **Depends on:** PARSE-1, PIPELINE-1, CONSENT-1  **Wave:** B
- **Done when:** a submission is delivered + tracked, blocked on missing consent
  or double-submission, with feedback flowing back to scoring.

#### [LIFECYCLE-1] Generic offer / contract lifecycle engine → offer + placement + guarantee clock · 🔴
- **Generic engine:** a lifecycle record with a status state-machine
  (draft→extended→negotiating→accepted/declined/withdrawn), number series,
  approvals, + a generic time-window/clock field. Reusable for any quote/offer/
  agreement.
- **Recruiting config:** offer (salary/start/role/package), negotiation notes,
  confirmed-placement record linked to the order with fee/salary basis,
  guarantee/replacement clock, early-leave handling.
- **Reuses:** `invoices` lifecycle primitives (`accounting_number_series`,
  `accounting_invoice_approvals`, status machine) as the **template**; the
  guarantee clock reuses VAULT block-on-condition; emits a placement-fee draft
  into `invoices`.
- **Backend MISSING (build):** a native lifecycle handler — **note `invoices.rs`
  is unwired**; either wire it or follow its pattern (§5.6). Placement-fee billing
  bridge + unblocking the stubbed Storno/credit-note handler is BILLING-1.
- **Depends on:** PIPELINE-1, MATCH-1  **Wave:** B
- **Done when:** a scored match becomes an offer → placement with a running
  guarantee clock + an auto-drafted fee invoice.

#### [ESIGN-1] Generic document-template + e-signature service · 🔴
- **Generic engine:** template render (exists) + a generic e-signature service
  (signature_request → signer(s) → signed-artifact writeback + status). Reusable
  for any signable document.
- **Recruiting config:** Arbeitsvertrag / Vermittlungsvertrag / AÜG
  Überlassungsvertrag templates with merge fields.
- **Reuses:** `documents` template+render path (`documents`/`document_versions`,
  render skill) for generation.
- **Backend MISSING (build):** e-signature is **entirely net-new** (repo-wide
  zero `e-signatur`/`docusign` matches) — a `signature_requests` collection + a
  native signer/provider handler (§5.7). Provider integration must respect the
  no-HTTP-data-bridge rule (use a control-plane connector, not the data plane).
- **Depends on:** LIFECYCLE-1  **Wave:** B
- **Done when:** a placement generates a contract from a template and routes it
  for signature with status tracked back on the placement.

#### [VAULT-2] Block-on-condition deployment gate · 🔴
- **Generic engine:** a pre-flight gate that refuses a state transition when a
  required credential is missing/expired (generic "block transition on condition").
- **Recruiting config:** block a temp deployment (DISPATCH) or a placement
  (LIFECYCLE) when a deployment-blocking credential is expired.
- **Reuses:** VAULT-1 storage; gate modeled on
  `module_release_data_access_review_summary` (§2.4).
- **Backend MISSING (build):** the native gate handler (§5.8). **Sequence after
  DISPATCH-1/LIFECYCLE-1** (the gate needs a transition to block).
- **Depends on:** VAULT-1, DISPATCH-1, LIFECYCLE-1  **Wave:** B

#### [DISPATCH-2] Leistungsnachweis sign-off → billing release + surcharge calc · 🔴
- **Generic engine:** authoritative "external party signs off a record → release
  downstream billing," + a rate/surcharge calculator splitting cost vs charge.
- **Recruiting config:** Entleiher-signed Leistungsnachweis gates the invoice;
  Manteltarif/Branchenzuschlag surcharge engine splitting Lohn vs Verrechnung;
  Arbeitszeitkonto.
- **Reuses:** DISPATCH-1 records; emits `sale_out` lines into `invoices`.
- **Backend MISSING (build):** native sign-off + surcharge handler (§5.9).
- **Depends on:** DISPATCH-1, MASTERDATA-1  **Wave:** B

#### [SCHEDULE-2] Multi-party links + interview transcription · 🧠
- **Generic engine:** multi-party slot coordination + video-link generation + a
  generic STT transcription/summary skill writing to a record.
- **Recruiting config:** Teams/Zoom/Meet links; interview transcript + competency
  extraction attached to candidate×vacancy with consent/retention (CONSENT-1).
- **Backend MISSING (build):** an STT transcription **skill** + writeback (§5.10);
  no STT capability exists today. Video-link generation via control-plane
  connector.
- **Depends on:** SCHEDULE-1, CONSENT-1  **Wave:** B

### Wave C — read-only / closing

---

#### [ANALYTICS-1] Generic analytics surface → recruiting funnel KPIs · 🟢
- **Generic engine:** read-only aggregation over synced record/pipeline
  collections (counts, conversion rates, time-in-stage). No new authority.
- **Recruiting config:** fill-rate, time-to-fill/submit, stage-conversion,
  cost-per-application, source-of-hire, stalled-order watchlist as report
  definitions.
- **Reuses:** `reports` module shell + pure-browser aggregation over
  `matching_requirements`/`matching_results`/`customer_opportunities` stage
  history/`time_records`. **Start as a reporting tab in `matching`**, not a heavy
  standalone (the `reports` module is a bug/feature tracker, not analytics).
- **Native:** none. **Wave:** C
- **Done when:** core funnel KPIs render live from synced data.

#### [BILLING-1] Placement-fee billing bridge + unblock invoices Storno/credit-note · 🔴
- **Generic engine:** lifecycle-event → auto-draft billing document; unblock the
  generic credit-note/Storno path.
- **Recruiting config:** confirmed placement → fee invoice; guarantee early-leave
  → pro-rata credit note.
- **Backend MISSING (build):** unblock the **stubbed** `create_credit_note`/Storno
  handler in `invoices.rs` ("not yet implemented") + a placement-fee draft path
  (§5.6/§5.11).
- **Depends on:** LIFECYCLE-1  **Wave:** C

#### [ONBOARD-1] Generic per-record checklist → onboarding · 🟢/🟡
- **Generic engine:** a per-record structured checklist with completion gating.
- **Recruiting config:** pre-start/first-day checklist (documents complete,
  Sicherheitsunterweisung, PSA issued, access granted) gating a handoff.
- **Reuses:** calendar (first-day), VAULT-1/MASTERDATA-1 (completeness). Likely a
  tab inside LIFECYCLE-1.
- **Wave:** C

---

## 5. Missing CTOX backend functions (the authoritative build list)

> **STATUS (2026-06): all 11 delivered + on `origin/main`, then hardened through
> two adversarial review rounds (single Codex review, then a 4-reviewer
> gpt-5.5-xhigh pass: accounting/GoBD · security/data-boundary · DSGVO/AÜG ·
> Rust/remediation).** Key commits: 2b6673e9 (5.8/5.9), 6eeba6fa (5.2),
> 6593f47e (5.3), d5692f45 (5.10 + 5.6/5.11), 807d1016 (review-1 remediation),
> 23d38c7c + 5e8d9ab0 (review-2 remediation incl. transactional post), 3533f026
> (capability-token foundation for the actor-trust finding). Per-function notes:
> 5.1 cv-print skill + writeback ✅ · 5.2 matching now binds the LLM scoring skill
> (root-cause was a skill-name typo); bulk-pool auto-scoring is a remaining UX
> gap ⚠️ · 5.3 DSGVO audit trail ✅ · 5.4 intake ✅ · 5.5 consent/retention (purge
> now redacts PII, not soft-delete) ✅ · 5.6/5.11 invoices wired + Storno/§17
> credit note, atomic post ✅ (advanced suite — dunning/recurring/payments/
> proposals — stays a documented stub) · 5.7 e-sign ✅ · 5.8 AÜG gate ✅ (still
> opt-in via `required_types` — see §9) · 5.9 Leistungsnachweis billing ✅ · 5.10
> STT skill+writeback ✅ in code, transcription unverifiable here (no GGUF
> weights) ⚠️. **The plan's original build list is closed; the remaining work to
> reach _production ready_ is in §9.**

| # | Missing function | Kind | Lives in | Used by |
|---|------------------|------|----------|---------|
| 5.1 | `ctox-cv-print-parser` skill **+** `ctox.cv_print.apply_parse` writeback handler | Skill + native | `src/skills/.../SKILL.md`; `store.rs` command arm + `suggested_skill_for_command` | PARSE-1 |
| 5.2 | Real bulk/pool LLM scoring (replace `shortlistObjectsForRequirement` stub; native scorer is keyword-only) | Native/skill | `matching/ui/matchingTools.js`, `importer.rs compute_matching_result`, scoring skill | MATCH-1 |
| 5.3 | Generic compliance audit event with rejection reason codes **+** `present-candidate`/`release-placement` policy scopes | Native policy | `policy.rs`, `store.rs` (~12300+), `mcp_channel.rs`, `permissions.js` | MATCH-1, SHAREOUT-1, LIFECYCLE-1 |
| 5.4 | Inbound application **capture handler** (webform/apply/mailbox → normalized record) | Native handler | `store.rs` command arm; conversations projections in `rxdb_peer.rs` | INTAKE-1 |
| 5.5 | Consent ledger enforcement pre-flight **+** retention/deletion purge loop (move `audit_retention_days` off ENV → runtime config) | Native | `policy.rs`/`store.rs`; migrate `mcp_channel.rs` env map | CONSENT-1 |
| 5.6 | Offer/placement lifecycle handler (**`invoices.rs` is unwired** — declare in `mod.rs` + router or follow its pattern) | Native handler | `src/core/business_os/invoices.rs`, `mod.rs`, router | LIFECYCLE-1, BILLING-1 |
| 5.7 | E-signature service: `signature_requests` collection + signer/provider handler (net-new; control-plane connector, not data plane) | Native + schema | new `*.rs` + schema-plumbing | ESIGN-1 |
| 5.8 | Block-on-condition transition gate (model on `module_release_data_access_review_summary` ~1857) | Native gate | `store.rs`/`policy.rs` | VAULT-2 |
| 5.9 | Leistungsnachweis sign-off → billing-release gate + surcharge/rate calc | Native handler | `store.rs`; emits to `invoices` | DISPATCH-2 |
| 5.10 | STT interview transcription skill + writeback (no STT today) | Skill + native | `src/skills/...`; gateway `src/core/execution`; `store.rs` writeback | SCHEDULE-2 |
| 5.11 | Unblock stubbed `create_credit_note`/Storno ("not yet implemented") + placement-fee draft path | Native handler | `src/core/business_os/invoices.rs` | BILLING-1 |

Everything **not** in this table is frontend (🟢) or schema-plumbing (🟡): no new
CTOX Rust function — only `schema.js` + the mechanical contract/hash/dist regen.

---

## 6. Execution waves (dependency graph)

```
Wave A (parallel):  PIPELINE-1  PARSE-1  MATCH-1  VAULT-1  DISPATCH-1
                    MASTERDATA-1  SEQUENCE-1  SCHEDULE-1  CONSENT-1
                         │ │ │ │ │ │
Wave B (parallel):  INTAKE-1  SHAREOUT-1  LIFECYCLE-1  ESIGN-1
                    VAULT-2  DISPATCH-2  SCHEDULE-2
                         │ │ │
Wave C:             ANALYTICS-1  BILLING-1  ONBOARD-1
```

- Wave A is mostly 🟡/🧠 over existing engines → highest parallelism, lowest risk.
- Wave B carries the 🔴 closing surfaces (submission, offer/placement, e-sign,
  deployment gate) — these unlock the commercial + temp-staffing core.
- Wave C is read-only + billing wiring.

## 7. Coverage target

Closing Waves A–B takes the staffing workflow from ~38% domain coverage to a
complete ATS + Zeitarbeit loop **without a single recruiter-only app** — every
deliverable is a generic Business OS engine plus a recruiting configuration that
any other industry can re-skin. The strongest existing assets (matching scoring,
outbound sequencing, customers Kanban/dedupe, the documents/calendar/conversations/
invoices substrate) are reused, not rebuilt.

## 8. Validation per ticket

- 🟢: `node --test` on the module's tests; manual board render.
- 🟡: §2.1 guard suite — `node src/apps/business-os/rxdb/tests/run-all.mjs`,
  `cargo test --manifest-path src/core/rxdb/Cargo.toml`,
  `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`; rebuild `dist/`
  + bump 3 cache-busters.
- 🔴/🧠: above **plus** targeted `cargo test`/`cargo check` for the changed
  handler + a policy-denied path test (both browser and MCP entry points).
- New apps: `ctox business-os app validate <id> --installed` must be green; the
  CTOX service owns queue completion.

If a check can't be run, state exactly which and why (per AGENTS.md).

---

## 9. Production-Readiness gate (authoritative — what "green AND production ready" means)

The §4 backlog and the §5 build list are **closed**: every generic engine + the
11 backend functions are implemented, on `origin/main`, and reviewed twice
(single Codex review → 4-reviewer gpt-5.5-xhigh pass). "Green" (all automated
gates pass) is true today. "**Production ready**" is **not yet** true as a whole.
This section is the gate: it must be fully checked before the ATS is declared
production ready. Items are ranked by severity; each has a concrete acceptance
criterion. Update the checkboxes as work lands.

### 9.0 Green baseline — verified today (keep green on every change)

- [x] native `ats_gates` 8/8 · `invoices` 18/18 · `capability` 5/5
- [x] guards: rogue-`invoices.*` rejected · matching-skill binding · native↔browser schema parity
- [x] `node src/apps/business-os/rxdb/tests/run-all.mjs` 37/0 · ATS engine cores 74/0
- [x] `ctox business-os app validate nachweise` OK · whole `main` tree compiles
- [x] live (CLI `commands dispatch`, isolated store): every `ats.*`/`invoices.*` handler, pos+neg

> Re-run this baseline after **any** change here; a red gate blocks the release.

### 9.1 P0 — Security: finish the actor-trust fix (BLOCKER)

Native authorization still trusts the browser-asserted `client_context.actor`
unless a capability token is present and enforced. The native half is done
(commit 3533f026); production requires the **browser half + enforcement on**.

- [ ] Browser obtains a capability token after login (calls `issue_business_os_capability_token` / a thin control-plane endpoint) and attaches it as `client_context.capability_token` on **every** command. *(web stack / sync layer — coordinate with the parallel agent; do not bypass RxDB/WebRTC.)*
- [ ] Token refresh before the 12h expiry; revoke-on-logout (drop the token).
- [ ] Flip `CTOX_BUSINESS_OS_REQUIRE_CAPABILITY_TOKEN=1` in the runtime store for hardened instances; document the rollout (legacy browsers fail closed once on).
- [ ] Guard/e2e test: a command claiming chef/admin **without** a valid token is denied with enforcement on (CLI proof exists; add a browser-path test).
- **Acceptance:** with enforcement on, a forged/absent token cannot perform any manage-all `ats.*`/`invoices.*` mutation; legitimate logged-in users are unaffected.

### 9.2 P1 — DSGVO / German-staffing legal correctness (from the 4-reviewer pass)

- [x] **AÜG gate mandatory, server-derived.** *(commit 43c91d5d)* `ats.placement.create` reads `placement_type`; for an Arbeitnehmerüberlassung/Zeitarbeit arrangement it unions the caller's `required_types` with the mandatory set from runtime config `CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS` and fails closed (`aue_required_credentials_unconfigured`) if empty — a caller can no longer omit the list to skip the gate. Legal credential list stays in config (Baukasten). Live-verified.
- [ ] **Legal-basis evidence model for consent.** `contract`/`legal_obligation`/`legitimate_interest` are currently auto-valid for `present_to_client` without proof/purpose-scope/notice/objection. Model legal-basis evidence per purpose + data category; require documented balancing or explicit consent before client sharing. *(deferred — `consent_valid` is a pure gate; doing this right needs a `basis_evidence` data model + migration + config, not a half-measure that breaks existing consents/tests.)*
- [x] **External Entleiher signature proof for Leistungsnachweis.** *(commit 43c91d5d)* With `CTOX_BUSINESS_OS_REQUIRE_ENTLEIHER_SIGNATURE=1`, `signoff` only marks `entleiher_signed`/releases billing when a COMPLETED `signature_request` backs it (by id or matching `document_id`); else `entleiher_signature_proof_missing`. Off by default (backward compatible). Live-verified.
- [ ] **Command-gated audit for direct RxDB PII writes.** ATS PII collections can be written by the browser via replication, bypassing the command-path `record_ats_governance_event`. Either make ATS PII collections command-only or add native write/delete audit triggers per collection. *(sync layer — guard-protected + parallel-agent-owned; coordinate, do not edit unilaterally.)*
- [x] **DSGVO data-subject rights (retention + Art. 15 export + Art. 17 erasure)** *(commits 23d38c7c, a758b579, a5051963)*: `ats.retention.purge` redacts the payload to a non-PII tombstone (real erasure, not soft-delete); `ats.subject.export` gathers every PII record + audit row for a subject across the ATS collections (Art. 15 access); `ats.subject.erase` redacts + tombstones every PII record for a subject and returns an erasure report (Art. 17 right to be forgotten). All chef/admin gated + self-audited. Remaining only: confirm desktop-file chunks/embeddings tied to a candidate are swept (file-store concern, outside the ATS records).
- **Acceptance:** a DSGVO/AÜG reviewer can show, for every candidate-PII create/share/delete, a legal basis + an immutable audit row, and an unprivileged placement cannot skip mandatory deployment credentials.

### 9.3 P2 — Live (model + WebRTC) end-to-end verification

- [ ] **STT transcription** runs on real audio (install the Voxtral Q4 GGUF weights; `runtime stt-smoke` returns real text); then drive `ats.interview.transcribe` with a real `source_file_id`.
- [ ] **LLM matching live turn**: a `matching.match` command actually binds the `business-os-requirement-matching` skill through the running daemon's gateway (llm.ctox.dev) and writes back scores (skill binding fixed in code; full live turn not yet run).
- [ ] **Full browser→WebRTC→native→sync round-trip** for the core flows (parse → intake → present → placement → billing → e-sign) against a running instance, not just CLI dispatch.
- **Acceptance:** each model-dependent feature produces a correct artifact end-to-end on a real instance; no feature is "code-complete but never run".

### 9.4 P2 — Bulk matching quality

- [x] *(commit 61fa3757)* `shortlistObjectsForRequirement` now accepts + uses `llmChat`/`sourceId`/`maxObjectsInPrompt` (the UI already passed them): for unscored, non-knocked-out candidates it drives `computeRequirementMatch` (enqueues a `matching.match` LLM-scoring command through the harness, bounded), returns `scoringTriggered`/`scoringPending`; a follow-up call returns the full LLM ranking. Read-only when no `llmChat`. node --check clean; matching core 17/0. *(Full end-to-end ranking lands once the enqueued scores write back — see 9.3 live turn.)*
- **Acceptance:** a shortlist over an unscored pool returns LLM-ranked candidates, not "noch nicht bewertet".

### 9.5 P3 — Surface maturity (UX, not correctness)

- [x] *(commit 925f6d21 — placements)* Placements promoted from a record list to a working surface: `placement_type` select (Festanstellung vs Arbeitnehmerüberlassung) + a `required_types` field so the §9.2 server-derived AÜG gate fires from the UI; rich rows (status badge, candidate → client, fee, guarantee days, fee-invoice id, Storno credit-note id); a per-placement **Frühausstieg** action that dispatches `ats.placement.early_leave` and surfaces the clawback + credit-note. node --check clean; placements engine tests 11/0.
- [x] *(commit 954b336e — six modules)* The remaining mounts promoted to engine-grounded surfaces, payload fields + result shapes grounded in the real native handlers (`src/core/business_os/store.rs`), node --check clean on all six:
  - **intake** — `ats.intake.capture` (name/email/phone/vacancy_id/channel); rich `applications` rows (status badge, contact, dedupe_key, doc count).
  - **submissions** — `ats.submission.present` (handler-exact fields + blocker rendering); candidate → client rows (status, vacancy, consent, feedback).
  - **interviews** — schedules into `interview_meetings` (plain RxDB write — no native command); renders meetings + scorecards with engine-computed state/score + per-meeting state-transition actions.
  - **esign** — `ats.signature.request` + per-row `ats.signature.sign` action; rows show signer counts + `signature_request` status.
  - **nachweise** — `business_credentials` rows with `credentialStatus`/`isDeploymentBlocking` badges + per-row `ats.deployment.check`; standalone `ats.leistungsnachweis.signoff` form.
  - **consent** — DSGVO surface: `ats.consent.check` form, `business_consents` ledger rows, per-subject Art. 15 (`ats.subject.export`) / Art. 17 (`ats.subject.erase`) actions; export payload rendered `esc()`'d.
- **Acceptance:** a recruiter can run each step from a real UI, not a record list. *(Dynamic mount behaviour — live RxDB handles + WebRTC dispatch round-trip — still needs the running shell to render-verify; this pass verified syntax + handler-field grounding + an adversarial field/XSS/cleanup review against the live handlers.)*

### 9.6 P4 — Out-of-ATS-core billing (optional, separate project)

- [ ] Invoices advanced suite — dunning, recurring, payment allocation, proposals, `assign_payment_terms` — currently documented stubs. A general Business OS billing build, not ATS-blocking (placement-fee / clawback / Leistungsnachweis invoicing already work).
- **Acceptance:** out of scope for ATS production-ready; track as its own billing-app plan.

### 9.7 Definition of "production ready" for this ATS

All of **9.0 green**, **9.1 (security) closed**, **9.2 (DSGVO/AÜG) closed**, and
**9.3 (live e2e) demonstrated**. 9.4–9.5 strongly recommended (matching quality +
usable UI). 9.6 explicitly out of scope. Until 9.1 is closed the instance is
**not** safe for multi-user production regardless of the green baseline.

### 9.8 End-state of the "work the plan to the end" pass

Everything in §9 that is closeable **inside this environment without touching
guard-protected/parallel-agent-owned layers or absent infrastructure** is done
and on `main`: 9.1 native capability foundation + the control-plane issuance
endpoint (`/api/business-os/auth/capability`, commit 3b89185b) + browser token
attachment (`command-bus.js` `getCapabilityToken()`, commit 3b89185b), 9.2
AÜG-mandatory gate, 9.2 external Entleiher signature proof, 9.2 retention erasure
(records), 9.4 bulk auto-scoring wiring, 9.5 placements rich UI (commit 925f6d21).
The boxes still open are open for a specific reason, not for lack of work:

- **Rollout decision (not coordination-gated, but a deliberate cut-over):** 9.1
  flipping `CTOX_BUSINESS_OS_REQUIRE_CAPABILITY_TOKEN=1` in the runtime store to
  move from "verify-if-present" to "reject-if-absent". The full path
  (issue → attach → verify) is live end-to-end; enforcement stays off until every
  client demonstrably sends a token, so flipping it is an operational go/no-go, not
  more engineering.
- **Coordination-gated (sync layer is guard-protected and the parallel Codex
  agent's domain):** 9.2 command-gated audit for direct RxDB writes. Editing the
  replication accept path unilaterally would fight the data-plane guards and risk
  a collision — it needs a coordinated change, not an ATS-side patch.
- **Infrastructure-blocked:** 9.3 STT (no Voxtral Q4 GGUF weights here); 9.3 live
  LLM matching turn + full WebRTC round-trip (the only running instance belongs
  to the parallel agent; CLI dispatch exercises the sync native handler, not the
  harness skill path).
- **Larger design (out of an ATS hardening pass):** 9.2 legal-basis evidence
  model. *(9.5 module UIs are now done — placements (925f6d21) + the remaining six
  intake/submissions/interviews/esign/nachweise/consent (954b336e), each grounded
  in the live native handlers and adversarially field/XSS/cleanup-reviewed.)*

So: the ATS-core native logic is hardened to the limit of what this session can
own, the capability-token path ships end-to-end (issue → attach → verify), and all
seven ATS module UIs are promoted from record lists to engine-grounded surfaces.
The residual is an operational enforcement flip (9.1), one sync-layer audit (9.2)
to coordinate with the parallel agent, and absent infra (9.3 STT weights + a live
shell/WebRTC round-trip). The plan is the single source of truth for that residual.
