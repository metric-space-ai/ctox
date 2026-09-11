---
name: outbound-lead-generation-research
description: Research a company and its contacts for the Business OS "Outbound Lead Generation" app with the CTOX web stack — browser, search, source adapters and the scraping pipeline — and return the result to the lead through the command bus. Trigger on "Starte eine Outbound Nachrecherche für <Firma> [<lead-id>] (Auftrag <command-id>)" or "Starte eine Outbound Neurecherche für …".
cluster: research
---

# Outbound Lead Generation · Recherche über den CTOX Web-Stack

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Everything you do goes through the `ctox` CLI. There is no other data path: the CLI runs inside the daemon and writes to the CTOX SQLite stores; the Business OS UI receives results through replication of the collection the command bus writes.

## 1. Wie die App, der Harness und der Web-Stack zusammenspielen

- The app button only formulates the assignment (one sentence) and puts the data you need into the command payload. It does not research anything.
- You are the research. You get this skill plus the assignment, read the payload, use the web stack, and send exactly one writeback command per lead at the end.
- The web stack has two working modes, use both:
  - **Browser** — open, read, click, submit, keep a persistent session per human owner. When a site needs a login or blocks you, the browser streams live to the human (auth assist); the human signs in or solves the challenge, and you continue with the same session.
  - **Scraping** — for sources you will need again, write an extraction script once, register it as a revision on a scrape target in SQLite, run it through the pipeline, and query the records. Drift is repaired, not re-researched.

## 2. Read your assignment

```bash
ctox business-os commands inspect <command-id>
```

The command id stands in the assignment sentence. The payload carries:

| key | meaning |
| --- | --- |
| `lead_id`, `company`, `country` | the lead (record id in `outbound_lead_generation_leads`), company name, `DE`/`AT`/`CH` |
| `mode` | `new_record` (first research) or `update_firm` (Nachrecherche) |
| `fields` | the requested field keys (default: all 32) |
| `lead_snapshot` | the lead's current `data.*` values and `contacts[]` — what is already known |
| `known_person_records` | Sellify persons (name, function, e-mail) — keep, complete, never re-guess |
| `research_instructions` | **the app's maintained research procedure, steps 0..x — binding, see §2a** |
| `research_instructions_variant` | `followup` (the Nachrecherche procedure is maintained separately) or `default` |
| `research_instructions_default` | the "Neue Recherche" procedure, when the followup variant is in use |
| `person_priorities` | contact categories in the required order |
| `include_private` | login sources the owner allows (`linkedin.com`, `xing.com`, `dnbhoovers.com`, `leadfeeder.com`, `rocketreach.com`) |
| `writeback_contract` | `record_ids`, `min_independent_sources` (2) |

## 2a. The app's research procedure is the assignment (steps 0..x)

`payload.research_instructions` carries the procedure the operator maintains **inside the app** (Kampagnen-Einstellungen → „Prompt: Neue Recherche" / „Prompt: Nachrecherche"). It is numbered, usually 0..7, and it is the actual order of work for this campaign. Read it first, follow it step by step, and let it decide what comes before what — which register first, which portal for the address, when Sellify counts, when a source needs a login.

- **This is why the assignment is one sentence.** The procedure can be long; it belongs in the command payload, not in the chat message. Never ask the operator to paste it, never repeat it back into the chat, and never treat its absence from the prompt as its absence from the assignment.
- **Load it, never assume it.** It lives only in the command payload; there is no copy in this skill and none in the prompt sentence. If `commands inspect` gives you no `research_instructions`, say so in the chat and work §5 as the fallback order — do not invent a procedure.
- **`research_instructions_variant`** tells you which one you got: `followup` means the operator maintains a separate Nachrecherche procedure and you must work that one; `default` means the same procedure applies to both. `research_instructions_default` is the other one, for reference only.
- **Precedence.** The app procedure outranks the source order in §5 and the field order in §3. It never outranks §6 (evidence) or §7 (writeback): a step that says "übernimm den Wert" still needs two independent sources, and results still leave through the command bus.
- **Cover every step.** Work the steps in their order, and only skip one with a reason you can state (source down, field not requested, country not applicable). In your closing chat message, say which steps you completed and which you could not, with the reason.
- **A step naming a source names a tool.** "Handelsregister", "Northdata", "Impressum", "LinkedIn/XING by name search" map to `ctox web search --source`, `ctox web read`, `ctox scrape execute --target-key`, or the login path in §4. `ctox web sources list --country <iso>` gives the registered source ids.
- **The procedure can also demand behavior**, not just sources: two independent sources per field, no Google snippet as proof, mark conflicts instead of resolving them, no Sellify handover in this run. Those sentences are rules for you, not prose.

## 2b. The app's agent areas (fixed contract — read these, nothing else)

An app never explains itself to you through its source code, its UI or its collections. It deposits agent-facing information in exactly three defined places. Read those, and only those.

**1. The assignment** — `ctox business-os commands inspect <command-id>`: everything that is specific to this run (§2).

**2. The app policy target** — the standing procedure and settings, written by the app when the operator saves them:

```bash
ctox scrape show-target --target-key outbound-lead-generation-policy
```

Contract: `target_kind = app-policy`, `config.policy_contract = ctox.outbound.research_policy.v1`, and in `config`: `research_instructions` (steps 0..x), `followup_instructions`, `fields`, `person_priorities`, `min_independent_sources`, `source_policy` (enabled sources, validation-only sources, credential requirements), `policy_version`, `updated_at_ms`. Every app uses `<app-id>-policy`, so the same read works for another app's assignment.

**3. The source targets** — one scrape target per source the app registered:

```bash
ctox scrape list-targets                       # which sources have an adapter at all
ctox scrape show-target --target-key <source>  # tier, country_hints, access_mode, allowed_domains, challenge_detection, heal_mode
ctox scrape show-api    --target-key <source>  # how to query what it already collected
ctox scrape show-latest --target-key <source> --limit 20
ctox web sources list --country <DE|AT|CH>     # registered source modules with tier and credential requirement
```

Rules for these three:

- The assignment wins over the policy target when they differ (it is the newer, per-run copy); say so in the chat instead of silently choosing.
- No policy target and no `research_instructions` in the assignment: work §5 as the fallback order and report that the procedure was missing. Never reconstruct one from the app's UI or code.
- Respect a source target's settings: `access_mode: public_native_api` means query the API, not hand-scraping; `heal_mode` and `challenge_detection` decide what happens on drift or a block (§9).
- Look at records a target already holds before scraping it again. A fresh record is a source; a stale one is a lead, not evidence.
- A source named in the procedure without a target is a finding for the closing message, not a reason to stop.
- You read these areas. Writing them is the app's job (policy) or an explicit adapter task (§4) — never a side effect of a research run.

## 3. The 32 fields

Company (21): `firma_name`, `firma_fruehere_namen`, `firma_aktivitaetsstatus`, `firma_anschrift`, `firma_besucheranschrift`, `firma_postanschrift`, `firma_postfach`, `firma_plz`, `firma_ort`, `firma_land`, `firma_email`, `firma_domain`, `firma_telefon`, `firma_fax`, `firma_geschaeftstaetigkeit`, `firma_homepage_fact_sheet`, `firma_geschaeftsfuehrung`, `firma_prokura`, `wz_code`, `umsatz`, `mitarbeiter`.

Person (11, per contact, keyed by a stable `person_key`): `person_geschlecht`, `person_titel`, `person_vorname`, `person_nachname`, `person_funktion`, `person_position`, `person_email`, `person_email_validation`, `person_telefon`, `person_linkedin`, `person_xing`.

Contacts: at least one per category, in this order — Geschäftsführung/Gesamtverantwortung, Prokura, Leitung Finanzen, Einkauf, Supply Chain Management, Operations, Technik, Entwicklung. Sellify contacts are kept under their `person_key` and completed.

**Every category is worked, not only the one that is easy.** The register gives you two for free: Geschäftsführung **and Prokura** — a name in `firma_prokura` is a person, so write it as a person record with `person_funktion` "Prokura", never only as a company field. The operational categories (Finanzen, Einkauf, Supply Chain, Operations, Technik, Entwicklung) come from the company website (Team, Über uns, Kontakt, Presse, Karriere pages, press releases, Impressum) and from LinkedIn/XING name search. Work them in order; a category ends `no_match` only after a documented search and two documented reads for it. Report per category what you found, in the closing message.

**E-mail and its validation belong to every person.** For each person with a name and a known company domain: derive the address from the pattern the company demonstrably uses (take it from a published address on the site or from a Sellify address of the same company, never from a guessed convention alone), then validate it with the configured validation source (`experte.de`, MailTester) and write the outcome to `person_email_validation`. Validation is a source of technical deliverability only: it never counts as the second independent source for the person's identity or employment. Without a demonstrable pattern the field is `no_match` with that reason — never a guessed address.

## 4. The CLI you work with

### Search, read, sources, adapter batch

```bash
ctox web sources list [--country <DE|AT|CH>] [--tier <P|S|C>]... [--field <field-key>]
ctox web sources info --id <source-id>
ctox web search --query <text> [--domain <host>]... [--source <id>]... [--country <DE|AT|CH>] [--context-size <low|medium|high>] [--cached] [--include-sources]
ctox web read --url <url> [--query <text>] [--find <text>]... [--workspace <path>] [--country <DE|AT|CH>]
ctox web person-research --company <name> --country <DE|AT|CH> --mode <new_record|update_firm|update_person|update_inventory_general|have_data> [--field <field-key>]... [--include-private <source-id>]... [--workspace <path>] [--no-workspace]
```

`person-research` is the adapter batch: it plans sources per (mode, country, field), runs search+read per source and returns an envelope `fields{field:{value, confidence, source_id, source_url, candidates}}`, `plan`, `search_runs`, `read_runs`, `scrape_runs`. Run it once at the start with the requested fields; take every `high`/`medium` value with its source; treat the rest as open. It is a helper, not the research.

Search engines rate-limit. When a search answers "rate limit" or "low relevance", switch the engine (`--source html.duckduckgo.com`, `--source bing`, `--domain <host>` pinning) and go on; never close a field because one engine was tired.

### Browser

```bash
ctox web browser-capture --url <url> [--dir <path>] [--out-dir <path>] [--timeout-ms <n>]
ctox web browser-automation [--dir <path>] [--timeout-ms <n>] [--script-file <path>] < script.js
ctox web unlock <list-probes|list-vectors|baseline|history|add-vector|set-vector-status> [...]
```

`browser-capture` renders a page in the real browser (JavaScript sites, screenshots, DOM text). `browser-automation` runs a Playwright script you write (navigate, click, fill, extract) in the owner's persistent browser. `unlock` is the stealth registry when bot detection blocks you — see the `web-unlock` skill before touching it.

### Login sources and the human in the loop

```bash
ctox business-os web-stack auth-assist-request --source-id <id> [--target-url <url>] [--credential-ref <ctox-secret://scope/name>] [--login-hint <hint>] [--task-id <id>]
ctox business-os web-stack auth-assist-status --session-id <id>
ctox business-os web-stack context-capture --session-id <id> [--source-id <id>] [--task-id <id>] [--no-handoff]
ctox business-os web-stack context-extract --session-id <id> [--source-id <id>] [--capture-script <id>] [--task-id <id>]
ctox business-os web-stack source-capture --source-id <dnbhoovers.com|leadfeeder.com|rocketreach.com|xing.com> --company <name> [--country <DE|AT|CH>] [--session-id <id>] [--credential-ref <ctox-secret://scope/name>] [--timeout-ms <n>]
ctox business-os web-stack authenticated-automation --source-id <id> --target-url <url> --credential-ref <ctox-secret://scope/name> [--login-hint <hint>] [--task-id <id>] [--timeout-ms <n>]
```

Unblocking with continuation, in this order:

1. `auth-assist-request --source-id <id> --task-id <your command id>` — opens the owner's streamed browser on that source and returns the browser `session_id`; the human signs in or solves the challenge in the stream.
2. `auth-assist-status --session-id <id>` — poll until the session reports authenticated; do not proceed on a pending session.
3. Continue **in the same session**: `ctox web browser-automation --session-id <id> --script-file <path>` for your own navigation and extraction, `source-capture --source-id <id> --session-id <id> --company <name>` for the built-in extractors of dnbhoovers.com, leadfeeder.com, rocketreach.com and xing.com, `context-capture --session-id <id>` / `context-extract --session-id <id>` for a page the human positioned for you.

Never type credentials yourself; never guess what a login source would have said. If the human does not complete the login within the turn, the field ends `action_required` with the `session_id` and your command id as reference.

### Scraping pipeline (scripts and records live in SQLite)

```bash
ctox scrape list-targets
ctox scrape show-target --target-key <key>
ctox scrape show-api --target-key <key>
ctox scrape show-latest --target-key <key> [--limit <n>]
ctox scrape query-records --target-key <key> [--where field=value]... [--limit <n>]
ctox scrape semantic-search --target-key <key> --query <text> [--limit <n>]
ctox scrape upsert-target --input <json-path>
ctox scrape register-script --target-key <key> --script-file <path> [--language <lang>] [--change-reason <text>] [--notes <text>]
ctox scrape register-source-module --target-key <key> --source-key <key> --module-file <path> [--language <lang>] [--change-reason <text>] [--notes <text>]
ctox scrape execute --target-key <key> [--trigger-kind <manual|scheduled|repair>] [--timeout-seconds <n>] [--allow-heal] [--thread-key <key>] [--queue-priority <urgent|high|normal|low>]
ctox scrape record-template-example --target-key <key> --template-key <template> --script-file <path> [--language <lang>] [--result-count <n>] [--challenge-score <n>] [--reason <text>]
ctox scrape promote-template --template-key <template> --script-file <path> [--language <lang>] --reason <text>
ctox web scrape --target-key <key> --mode <latest|semantic> [--query <text>] [--limit <n>]
```

Where things are: `ctox.sqlite3` holds `scrape_target` (key, start URL, `target_kind`, config, output schema), `scrape_script_revision` (revision number, script body, sha256, change reason), `scrape_source_revision` (per-source extractor modules), `scrape_run` (status, classification, timing), `scrape_record_latest` (the extracted records). Working files (inputs, outputs, artifacts) live under `~/.local/state/ctox/scraping/targets/<target-key>/`. Registered targets today: `handelsregister-de`, `northdata-de`, `bundesanzeiger-de`, `companyhouse-de` (`target_kind = prospect-research`).

When to write a script: a source you will hit again for many leads (register lists, company directories) or one whose page needs structured extraction. Look at `show-api`/`show-target` first; if the target exists, `execute --allow-heal`; if the run classifies `portal_drift`, the repair task is already queued — record it and move on, do not retry the same source in this run. If no target exists and the source will recur, write the script (`universal-scraping` skill explains authoring, fixtures and `upsert-target`), register it, run it. For a one-off page, just read or capture it.

## 5. Source order (DE default; `research_instructions` overrides the order)

1. Identity and register: Handelsregister, Northdata, Bundesanzeiger, CompanyHouse (AT: Firmenbuch/JustizOnline, FirmenABC; CH: Zefix, SHAB, Moneyhouse). Fields: `firma_name`, `firma_fruehere_namen`, `firma_aktivitaetsstatus`, `firma_geschaeftsfuehrung`, `firma_prokura`.
2. Website, address, communication: find `firma_domain` first, then the Impressum for `firma_anschrift`, `firma_besucheranschrift`, `firma_postanschrift`, `firma_postfach`, `firma_plz`, `firma_ort`, `firma_land`, `firma_email`, `firma_telefon`, `firma_fax`. Fallback FirmenABC (AT), Zefix (CH), D&B Hoovers (DE/CH), Northdata last.
3. Figures: `wz_code`, `umsatz`, `mitarbeiter` from D&B Hoovers or Leadfeeder (DE also Bundesanzeiger); `firma_geschaeftstaetigkeit` and `firma_homepage_fact_sheet` from the homepage.
4. Persons: register and Impressum first, homepage team pages, then LinkedIn/XING **by name search** (never from clicked search hits), RocketReach; derive the e-mail pattern from known addresses and validate it (MailTester) → `person_email_validation`.

## 6. Evidence rules

- A field is `verified` only with a value and **two independent sources on different hosts**; each source has `source_id`, `url` and a verbatim `quote`. Two pages of one host are one source. Sellify alone proves nothing, but counts as one source.
- **Citing Sellify.** A value from `sellify_company` or `known_person_records` is cited as a source with `source_id: "sellify"`, `url: "sellify://company/<contact_id>"` (company fields) or `url: "sellify://person/<sellify_person_id>"` (person fields, with that person's `person_key`), and as `quote` the stored Sellify value verbatim. The server checks the citation against the record your assignment carried and drops it when the quote is not that record's value for the field. It is one source, never enough on its own — not even for a self-reported field such as a phone number, which otherwise needs only one source.
- `no_match` only after at least one documented search and two documented page reads for that field.
- `action_required` only for a login or approval you could not get: reference the auth-assist (`source_id` and your command id) or a source with `requires_credential=true`. Also for conflicts: keep both candidates with their sources, leave the value empty, reason `conflict`.
- `unsupported` only when the field cannot be researched under this contract (e.g. AT-only field on a DE lead).
- A blocked or temporarily unreachable source proves nothing, neither the value nor its absence.
- Keep a running checkpoint in your workspace: `gap_closure/field_status.json` after every attempt and one file per attempt under `gap_closure/attempts/<field>/<n>.json` (`kind`, `query_or_url`, `result`, `artifact_path`, `at`). A follow-up turn resumes from it.

## 7. Writeback — the only way results reach the lead

One writeback per lead, sent through the **MCP tool `business_os.execute_writeback`** of the
`ctox-business-os` server. The server binds `module` and `research_command_id` to your task
itself, validates the payload with the native handler, writes the lead collection, and the UI
updates through replication. Nothing else counts: **do not** run `ctox business-os commands
dispatch`, any other CLI, shell, SQLite or direct SQL for the writeback — those paths are
forbidden by the task contract (`forbidden_mechanisms`), fail in the sandbox, and the daemon
rejects the task as incomplete when no successful `execute_writeback` receipt exists for your
lead and your research command. Never edit collections directly, never report results as chat
text only.

Call:

```json
business_os.execute_writeback({
  "record_id": "<lead-id>",
  "payload": {
    "field_status": {
      "<field>": {
        "status": "verified|no_match|unsupported|action_required",
        "value": "...",
        "reason": "...",
        "sources": [{"source_id": "...", "url": "...", "quote": "...", "person_key": null, "requires_credential": false, "task_id": "", "command_id": ""}],
        "attempts": [{"kind": "web_search|web_read|browser_capture|scrape", "query_or_url": "...", "result": "...", "artifact_path": "...", "at": "<iso>"}]
      }
    },
    "result": {
      "fields": { "<field>": {"value": "...", "sources": [ ... ]} },
      "person_records": [ {"person_key": "...", "person_vorname": "...", "person_nachname": "...", "person_funktion": "...", "person_position": "...", "person_email": "...", "sources": [ ... ]} ],
      "evidence": [ {"field_key": "<field>", "source_id": "...", "url": "...", "quote": "...", "person_key": null} ]
    }
  }
})
```

Do not add `module`, `research_command_id`, `command_id` or `gap_task_id` yourself — the server
sets them from the signed task session. Read the tool's returned `status`: `accepted` or
`completed` means the writeback landed; anything else means it did not, and the task is not done
until a retry succeeds.

### Validation rules the daemon enforces (get them right on the first attempt)

Build the writeback in this order, then send it once: (1) collect the terminal status per field, (2) copy each verified value **identically** into `result.fields`, (3) attach sources with absolute URLs, (4) give every person value and its evidence the same `person_key`, (5) check that the key set equals `payload.fields`. These five are the reasons live runs were rejected.

- `field_status` covers **every field of `payload.fields`** (normally all 32) with a terminal status; a missing or extra field is rejected. Reporting only the fields you worked on is not accepted — an untouched field is `no_match` (with its evidence) or keeps the status it had.
- Every `field_status` entry is an object with `status`; `value` only for `verified`.
- For a `verified` field the value in `result.fields.<field>.value` must be **identical** to `field_status.<field>.value` — same string, no reformatting, no added prefix.
- Every populated `person_*` value, in `result.fields` and in `person_records`, needs the `person_key` of the person it belongs to. Keep the `person_key` from `known_person_records` for a Sellify person; invent a stable one only for a new person.
- `result.fields` holds objects (`{"value": …, "sources": [...]}`), never bare strings.
- Every evidence entry needs `field_key`, `source_id`, `url`; person evidence also `person_key`, and it must be **the same `person_key`** the value in `result.fields` carries.
- Every `url` — in sources and in evidence — is an absolute `http(s)://` URL. A file path, a note or an empty string is rejected.
- A **non-verified** field (`no_match`, `unsupported`, `action_required`) must NOT carry a populated `value`. State the reason instead.
- Person fields describe the priority contact(s) you actually found: when you report persons in `person_records`, set the matching `person_*` fields `verified` with their `person_key` instead of `no_match`. `no_match` on a person field means you found no such person at all.
- Person fields carry a `person_key`; `result.fields` holds structured objects only, never free text.
- **Exactly one writeback call per lead.** Never dispatch read commands (`outbound.task.readback`, `outbound.lead.read`, `outbound.queue_task.read`, `outbound.lead.show` or anything similar) to check your own result, and never enqueue Business OS actions (`business_os.execute_action`, `business_os.propose_action`) for the writeback — they are not the writeback and are rejected outside the task contract. Every dispatched command becomes its own queue task and its own agent turn — twelve such reads once blocked a whole campaign for three hours.
- Done means `business_os.execute_writeback` returned status `accepted` or `completed`. Report the counts (verified / no_match / action_required / unsupported) and the persons found in one short chat message.

## 8. Unblocking across turns (login, captcha, MFA)

The human is not always at the keyboard, and your turn is bounded. The system therefore has two paths — use both correctly:

**Same turn (preferred, fastest).** You raised `auth-assist-request`; the owner's browser opens streamed. Poll `auth-assist-status --session-id <id>` a few times while you work other fields. If it reports authenticated, continue in that session (`browser-automation --session-id`, `source-capture --session-id`) and finish the field normally.

**Later (the human confirms after your turn ended).** When the owner presses "Anmeldung bestätigt" in the Browser app, the daemon does three things by itself: it cancels the pending auth-assist task, it reopens your research task if it was blocked or failed, and — if your task had already finished — it creates a follow-up task **"Fortsetzen: Nachrecherche: &lt;Firma&gt;"** in the same thread, with the same workspace, the same skill and the original assignment plus a note naming the source and the browser session. So:

- Finish your turn even when a login is still open. Write the affected fields as `action_required` with the `source_id` and the browser `session_id`, and write back. Do not idle-wait for the human, and never leave the task without a writeback.
- When you receive a "Fortsetzen" assignment: read the same command with `commands inspect` (the id is in the original sentence, and `business_os_command_id` is in the task metadata), read the lead's current `field_status`, and work **only** the fields that are `action_required` or still open. Re-verify nothing that is already `verified` unless the new session contradicts it.
- The continuation carries the browser `session_id` in its prompt and metadata. Use exactly that session; do not open a second one.
- Then write back again with the **complete** 32-field `field_status` (the fields you did not touch keep their previous status and value). The daemon replaces the lead's field status, so a partial map would silently drop earlier results.

## 9. Edge cases (each one has already happened)

| Situation | What you do |
| --- | --- |
| Search engine answers "rate limit" or "low relevance" | Switch engine (`--source html.duckduckgo.com`, `--source bing`) or pin the domain (`--domain handelsregister.de`). A tired engine never justifies `no_match`. |
| Source blocks you (captcha, Cloudflare, 403) | It proves nothing — not the value and not its absence. Try `browser-capture`, then a different source. Only if the source is the only one that can hold the field: `auth-assist-request` and `action_required`. |
| Login source without an owner session | `auth-assist-request --source-id <id> --task-id <your command id>`, keep working other fields, finish the turn with `action_required`. Never enter credentials, never guess the content behind the login. |
| Scrape target exists but the run classifies `portal_drift` | The repair is queued automatically by `--allow-heal`. Record it, use another source, do not retry the same target in this run. |
| Scrape target classifies `temporary_unreachable` / `blocked` | Fall back to another source; do not queue a repair. |
| No adapter for a recurring source | Write the extraction script, `register-script`, `execute --allow-heal`. For a one-off page use `web read` or `browser-capture` instead — do not build a target for a single lead. |
| Two sources contradict each other | Leave the value empty, keep both in `candidates` with their sources, status `action_required`, reason `conflict`. Never average, never pick the prettier one. |
| Sellify already holds a value | It is the starting value and counts as one source (cite it as `sellify://…`, see §6). Confirm it with one independent source (then `verified`), or contradict it with two (then take the new value and say so in `reason`). A value found only in Sellify and one external page is `verified`, not `no_match`. |
| A Sellify person is outdated (left the company) | Keep the `person_key`, set the function to the documented state (for example "Geschäftsführung (ausgeschieden)"), and add the current holder as a new person. Never delete a Sellify person. |
| Two persons look like one (same name, different profile) | Distinct `person_key` each; only merge with a document that shows they are the same person. |
| A profile URL as `person_key` | Do not do it. `person_key` is a stable key (Sellify id or a key you keep for this lead), not a URL — profile URLs change and produce duplicates. |
| Company is a subsidiary / renamed / merged | Research the entity the lead names. Put former names into `firma_fruehere_namen`, note the parent in `firma_geschaeftstaetigkeit`, and never silently replace the lead with the parent company. |
| Register shows the company as inactive | `firma_aktivitaetsstatus` verified with the register entry; keep researching the remaining fields, the lead is still a record. |
| Country is AT or CH | Use the country's register first (Firmenbuch/JustizOnline, Zefix/SHAB/Moneyhouse). A DE-only field on a foreign lead is `unsupported`, not `no_match`. |
| The writeback is rejected | Read the error, fix exactly that (see §7 rules), call `business_os.execute_writeback` again. Three rejected attempts in a row: write back what is valid, mark the rest `action_required` with the error as reason, and report it. |
| Your turn budget runs out | `field_status.json` is your checkpoint; the follow-up turn continues from it. Better a complete writeback with honest `no_match` than a half one. |
| You found nothing at all for a field | `no_match` — but only with the documented search and two reads. `no_match` without evidence is a false statement, not a result. |

## 10. Stop conditions

- One turn is bounded; keep `field_status.json` current so a follow-up continues instead of restarting.
- Never fabricate a value, a person, an e-mail address or a source. An empty verified field is better than a plausible one.
- If the CLI itself fails (sandbox, relay, daemon), report the exact command and error; do not work around it with curl, raw HTTP or a private browser.
