# ATS Tenant Config Bundle (Go-Live G2)

This is the **Baukasten config layer** an operator uses to stand up the ATS for
a single Personalvermittler (staffing/recruiting) tenant. It is **config + data
+ docs only** — no module code, no recruiter-app logic. Pair this file with the
seed template `tests/business-os/ats_golive_seed.sql`.

> Defaults below are documented starting points. Every value tagged
> **`operator must set`** is tenant-specific — change it before go-live.

---

## 0. Two config mechanisms — which flag reads which

> **Important — verified against the code:** these flags do **not** all share one
> mechanism. There are two disjoint sources, and setting a flag in the wrong one
> silently no-ops. Pick the column that matches the flag's section below.

**(A) Runtime store** — read via
`crate::inference::runtime_env::env_or_config(root, KEY)`, which resolves typed
runtime-state → secret store → the `runtime_env_kv` table, and **never** reads
process env. Persisted in:

```
runtime/ctox-runtime.sqlite3   →   table runtime_env_kv(env_key, env_value)
```

(Source: `src/core/execution/models/runtime_env.rs` —
`RUNTIME_ENV_TABLE = "runtime_env_kv"`, columns `env_key TEXT PRIMARY KEY` /
`env_value TEXT NOT NULL`.) **All §1 flags + `MODULE_ALLOWLIST` use this.** This
is the mechanism the repo rule prefers (root `AGENTS.md` rule 4: no new
process-env toggles for runtime behavior).

**(B) Process environment** — read directly via `std::env::var(KEY)` at daemon
start; the `runtime_env_kv` table is **not** consulted for these. **The §2
connectivity/login flags use this.** They are bootstrap/infra settings, set in
the daemon's launch environment — a systemd unit `Environment=` line, a launcher
`export`, or the process env before `ctox serve` — and a row in `runtime_env_kv`
for one of these does nothing. (These are pre-existing bootstrap flags, not new
toggles.)

A process `export` survives only the life of that shell and is invisible to a
supervised respawn, so for **(B)** pin them in the service unit, not an ad-hoc
shell.

### Canonical set pattern (one row, upsert)

`runtime_env_kv` uses `env_key` as PRIMARY KEY, so the durable, idempotent write
is an `INSERT OR REPLACE` (equivalent to the native `ON CONFLICT(env_key) DO
UPDATE SET env_value = excluded.env_value` upsert in `set_runtime_env_value`):

```sh
# <CTOX_ROOT> is the CTOX working root (the dir that contains runtime/).
sqlite3 "<CTOX_ROOT>/runtime/ctox-runtime.sqlite3" \
  "INSERT OR REPLACE INTO runtime_env_kv(env_key, env_value)
   VALUES ('CTOX_BUSINESS_OS_REQUIRE_CAPABILITY_TOKEN', '1');"
```

To read a value back:

```sh
sqlite3 "<CTOX_ROOT>/runtime/ctox-runtime.sqlite3" \
  "SELECT env_value FROM runtime_env_kv WHERE env_key='CTOX_BUSINESS_OS_REQUIRE_CAPABILITY_TOKEN';"
```

To clear (revert to default behavior), delete the row:

```sh
# (use a mechanism-(A) key — a row for a §2 process-env flag would be a no-op)
sqlite3 "<CTOX_ROOT>/runtime/ctox-runtime.sqlite3" \
  "DELETE FROM runtime_env_kv WHERE env_key='CTOX_BUSINESS_OS_REQUIRE_CAPABILITY_TOKEN';"
```

> **Restart** the CTOX daemon after a batch of changes so every code path
> re-reads the store. Some flags are cached per-process.

> **Secrets caveat:** the runtime store rejects secret keys (`set_runtime_env_value`
> / `INSERT OR REPLACE` no-ops on `is_secret_key`). Credentials such as
> `CTOX_BUSINESS_OS_ROOM_PASSWORD` go through the CTOX secret store, never this
> table. They are intentionally **not** in the table below.

---

## 1. Runtime-store flags — mechanism (A), `runtime_env_kv`

These read through `env_or_config` (runtime store only). The four legal/DSGVO
flags gate recruiting-specific compliance behavior — for a real Personalvermittler
tenant going live, **all four should be `1`** (fail-closed); they default to off
so dev/demo instances keep working without setup. `MODULE_ALLOWLIST` scopes the
surface and is also a runtime-store value.

| Flag | Recommended | Effect | Set command (`runtime_env_kv`) |
|------|-------------|--------|-------------|
| `CTOX_BUSINESS_OS_REQUIRE_CAPABILITY_TOKEN` | `1` **(operator must set for prod)** | When `=1`, a privileged (manage-all) Business OS command without a valid signed capability token is **denied** instead of trusting the claimed actor. Off → legacy claimed-actor fallback so unprovisioned browsers still work. (store.rs ~L19082) | `INSERT OR REPLACE INTO runtime_env_kv(env_key,env_value) VALUES ('CTOX_BUSINESS_OS_REQUIRE_CAPABILITY_TOKEN','1');` |
| `CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS` | e.g. `aufenthaltstitel,a1_bescheinigung,g25` **(operator must set — legal list per tenant)** | Comma/space/`;`-separated credential types that an **Arbeitnehmerüberlassung (AÜG)** placement must check before deployment. The gate itself is mandatory for AÜG placements; an **empty** set makes every AÜG placement fail closed (`aue_required_credentials_unconfigured`). This is the Baukasten knob for "which papers are legally required." (store.rs `aue_mandatory_required_types` ~L22390) | `INSERT OR REPLACE INTO runtime_env_kv(env_key,env_value) VALUES ('CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS','aufenthaltstitel,a1_bescheinigung,g25');` |
| `CTOX_BUSINESS_OS_REQUIRE_ENTLEIHER_SIGNATURE` | `1` **(operator must set for prod)** | When `=1`, an `ats.leistungsnachweis.signoff` (Entleiher/hirer sign-off on a Leistungsnachweis) must be backed by a **COMPLETED `signature_requests` record**, not an internal admin assertion. Off → backward-compatible internal assertion accepted. (store.rs ~L22776) | `INSERT OR REPLACE INTO runtime_env_kv(env_key,env_value) VALUES ('CTOX_BUSINESS_OS_REQUIRE_ENTLEIHER_SIGNATURE','1');` |
| `CTOX_BUSINESS_OS_REQUIRE_LEGAL_BASIS_EVIDENCE` | `1` **(operator must set for prod)** | When `=1`, consent/legal-basis gates (e.g. candidate present-to-client, retention/purpose checks) require **evidence on the consent record**, not just a flag. Hardens the DSGVO trail. Off → weaker, demo-friendly check. (store.rs ~L22114, ~L22669) | `INSERT OR REPLACE INTO runtime_env_kv(env_key,env_value) VALUES ('CTOX_BUSINESS_OS_REQUIRE_LEGAL_BASIS_EVIDENCE','1');` |
| `CTOX_BUSINESS_OS_MODULE_ALLOWLIST` | the ATS module set (see below) **(operator must set to scope the surface)** | Comma/whitespace list of module ids the instance exposes. Use it to present **only** the recruiting Baukasten to the tenant. Read via `env_or_config` → runtime store. (store.rs ~L870) | `INSERT OR REPLACE INTO runtime_env_kv(env_key,env_value) VALUES ('CTOX_BUSINESS_OS_MODULE_ALLOWLIST','matching,placements,interviews,credentials,consent,nachweise,submissions,intake,buchhaltung,invoices,documents,calendar,conversations,ctox');` |

---

## 2. Connectivity / login flags — mechanism (B), process environment

> **These read `std::env::var` at daemon start — a `runtime_env_kv` row does
> NOT apply.** Set them in the daemon's launch environment (systemd unit
> `Environment=`, launcher `export`, or the process env before `ctox serve`).
> Restart the daemon to pick up changes.

| Flag | Recommended | Effect | How to set (process env) |
|------|-------------|--------|-------------|
| `CTOX_BUSINESS_OS_REQUIRE_LOGIN` | `1` **(operator must set for prod)** | `=1` forces explicit login: no auto-trusted local desktop session, and the "first user bootstraps" fallback is disabled. Off → local-dev convenience identity (`local-dev` / `admin`). For a real tenant this **must** be `1`. (store.rs ~L1059, ~L19136, `env::var`) | systemd: `Environment=CTOX_BUSINESS_OS_REQUIRE_LOGIN=1` |
| `CTOX_BUSINESS_OS_LOGIN_URL` | tenant login page URL **(operator must set)** | The login URL surfaced to an unauthenticated browser session (`login_url` in the session response). Empty/blank is ignored. (store.rs ~L1060, `env::var`) | systemd: `Environment=CTOX_BUSINESS_OS_LOGIN_URL=https://os.example-tenant.de/login` |
| `CTOX_BUSINESS_OS_SIGNALING_URLS` | tenant signaling endpoint(s) **(operator must set)** | Comma/whitespace list of WebRTC signaling URLs used to bootstrap the RxDB-over-WebRTC mesh. This is the **data-plane** rendezvous; browser↔CTOX sync is WebRTC-only, never HTTP. (store.rs `signaling_urls_config` ~L25761, `env::var`) | systemd: `Environment=CTOX_BUSINESS_OS_SIGNALING_URLS=wss://signal.example-tenant.de` |
| `CTOX_BUSINESS_OS_ICE_SERVERS` | STUN/TURN JSON for the tenant **(operator must set if NAT/firewalled)** | ICE server list (STUN/TURN) for WebRTC connectivity across NAT. Required for reliable sync when peers are not on the same LAN. (store.rs ~L951, `env::var`) | systemd: `Environment=CTOX_BUSINESS_OS_ICE_SERVERS=[{"urls":"stun:stun.l.google.com:19302"}]` |
| `CTOX_BUSINESS_OS_DEFAULT_ROLE` | `user` (keep default) | Role assigned to an authenticated session that is not an explicitly configured user. Must be one of `chef`/`admin`/`founder`/`user` (normalized). Keep `user` so unscoped sessions are least-privileged; grant `chef`/`admin` per row in `business_users`. (store.rs `default_session_role` ~L1162, `env::var`) | systemd: `Environment=CTOX_BUSINESS_OS_DEFAULT_ROLE=user` |

### Recommended `MODULE_ALLOWLIST` (§1) for a Personalvermittler tenant

Grounded in `src/apps/business-os/modules/` (recruiting-relevant ids):

- `matching` — candidate↔job matching pipeline (Kanban)
- `placements` — placements / AÜG deployment
- `interviews` — interview scheduling + scorecards
- `credentials` — credential / expiry vault (AÜG papers)
- `consent` — DSGVO consent ledger + retention
- `nachweise` — Leistungsnachweise (timesheets / sign-off)
- `submissions` — candidate present-to-client submissions
- `intake` — inbound CV / application intake
- `buchhaltung`, `invoices` — accounting + invoicing
- `documents`, `calendar`, `conversations`, `ctox` — supporting surfaces

Trim to taste; anything not listed is hidden from this tenant.

---

## 3. Flags intentionally NOT in this bundle

- **`CTOX_BUSINESS_OS_ROOM_PASSWORD`** — a **secret**; rotate via the CTOX
  secret store, not `runtime_env_kv` (the table no-ops secret keys). If a
  process env override is set, unset it before rotating the persisted value.
- `CTOX_BUSINESS_OS_DESKTOP_USER` / `_DISPLAY_NAME` / `_ROLE`,
  `CTOX_BUSINESS_OS_SESSION` / `_SESSION_TOKEN`,
  `CTOX_BUSINESS_OS_ENABLE_SMOKE_CONTROLS`,
  `CTOX_BUSINESS_OS_DISABLE_BACKGROUND_FILE_INDEX` — dev/desktop/test toggles,
  not part of a production tenant go-live.
- `CTOX_BUSINESS_OS_MCP_*` — the external-agent MCP channel. Out of scope for
  this G2 bundle; configure separately only if the tenant integrates external
  agents, and keep it least-privilege.

---

## Config-Schicht (Baukasten — where recruiting specifics live)

Per the Baukasten rule, the **engine** is generic and the **recruiting-specific
content** lives in config/definition files, not hard-coded in the engine. These
are pointers, not full templates — edit the referenced files (or their tenant
override) to fit the Personalvermittler:

- **Matching definition `candidate_job.v1`** —
  `src/apps/business-os/modules/matching/ui/matchingDefinition.js`
  (`DEFAULT_MATCHING_DEFINITION`, `id: 'candidate_job.v1'`). Roles
  (source/object/match), German labels (Anforderungen / Matches / CV / Stelle),
  drawer sections, search placeholders. This is the tenant's "what matches
  what" config.

- **Pipeline-stage labels** —
  `src/apps/business-os/modules/matching/core/pipeline.js` (`CANDIDATE_STAGES`):
  `neu → screening → telefoninterview → kundenvorstellung → vertragsangebot →
  eingestellt | abgelehnt | on-hold`. The engine is a generic ordered-stage
  pipeline; relabel/reorder stages here for the tenant's recruiting funnel.

- **Document templates** (Angebot / AÜG-Vertrag / Leistungsnachweis) — authored
  as Business OS documents / module templates and surfaced through
  `documents` / `nachweise` / `esign` (and `cv-print-builder` for CV layout).
  Leistungsnachweis sign-off is gated by
  `CTOX_BUSINESS_OS_REQUIRE_ENTLEIHER_SIGNATURE` (§1) against the
  `signature_requests` collection. Store tenant-specific template bodies as
  documents; do not hard-code them in module code.

- **Locales (`de.json`)** — per-module under
  `src/apps/business-os/modules/<module>/locales/de.json` (e.g. `matching`,
  `placements`, `interviews`, `credentials`, `consent`, `nachweise`,
  `submissions`). Override wording for the tenant here; the matching labels in
  `candidate_job.v1` are an additional, definition-level localization layer.

> Rule of thumb: **engine = code (do not edit for a tenant); content = config**
> (matching definition, stage labels, document templates, locales). The flags in
> §1–§2 select behavior; this Config-Schicht selects the words and the legal
> content.
