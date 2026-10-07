---
name: outbound-lead-generation-research
description: Research a company and its contacts for the Business OS "Outbound Lead Generation" app with the CTOX web stack — browser, search, source adapters and the scraping pipeline — and return the result to the lead through the command bus. Trigger on "Starte eine Outbound Nachrecherche für <Firma> [<lead-id>] (Auftrag <command-id>)" or "Starte eine Outbound Neurecherche für …".
cluster: research
---

# Outbound Lead Generation · Recherche über den CTOX Web-Stack

## 1. Purpose

You research one lead (company and contacts) and store the result with `business_os.execute_writeback`. You decide; the registered source adapters are your tools, and you maintain them. Model calls cost ~20 s each; don't explore the system.

## 2. Inputs — the payload in the task text is the authority

The task text carries the command id, `record_id` (= `lead_id`) and the payload; never search for it elsewhere. Only if it says `truncated` and a needed key is missing, call `business_os.get_command_status` once.

- `company`, `country` (`DE`/`AT`/`CH`), `fields` (requested keys, normally 32; cover exactly these).
- `mode`: `update_firm` = company in Sellify (Nachrecherche), Sellify values are the start; `new_record` = not in Sellify, from scratch.
- `lead_snapshot`, `sellify_company`, `known_person_records`, `crm_knowledge`: known data; complete it, never re-guess it.
- `research_instructions`: operator procedure (steps 0..x), binding order of work; `person_priorities`; `include_private`; `source_policy.sources[]` (`id`, `target_key`, `field_keys`).

**Precedence.** `research_instructions` decides order and which source serves which field; it never overrides §3–§7. Login steps are covered by the adapters' own login handoff (§4); never wait for a human. No procedure: register → website/address → figures → persons.

## 3. Tools — closed list

- `ctox_web_scrape` `{"target_key": "<source.target_key>", "mode": "execute", "timeout_seconds": 180, "input": {…}}`; modes `test`, `register_script`, `upsert_target` in §4a.
- `ctox_web_search` `{"query": "…", "domains": ["host"]}`.
- `ctox_web_read` `{"url": "https://…", "query": "<fact to prove>", "find": ["…"]}` — without `query` a read is no evidence.
- `ctox_browser_automation` — priority 2 only (§4a).
- `business_os.execute_writeback` (§7); `business_os.get_record` `{"collection": "outbound_lead_generation_leads", "record_id": "<lead_id>"}` (§8); `business_os.get_command_status` `{"command_id": "…"}`; `update_plan`.

**Adapter calls.** `execute` and `test` accept only `target_key`s from `source_policy.sources`. The server fills `source_id`, `task_id`, `company`, `country` from your task: omit them (passed values must match exactly). Add `city`; `domain` (`impressum` needs it); `"person": {"first_name", "last_name"}` or `"profile_url"` for `linkedin-com`/`xing-com`; `"email"` for `experte-de`/`mailtester-com`. `timeout_seconds` max 420; 400 for `linkedin-com` and the login sources (`dnbhoovers-com`, `leadfeeder-com`, `xing-com`, `rocketreach-com`), which sign in with the stored credential themselves.

The result carries `status`, `reason`, `auto_reauthorization` and `records_preview` `{shown, total, records: [{field, value, source_url, note, …}]}` (≤ 40 records): take the values from there. A record is a source: `source_id` = the source's `id`, `url` = `source_url`, `quote` = value or note.

**Forbidden:** reading CTOX runtime or run files (`latest_records.json`, `runs/…`; sole exception §4a); the `ctox` CLI in the shell; `curl`/`wget`; running node/python/Playwright yourself (drafts run only via `test`); `sqlite3`; `ctox_web_read` on `linkedin.com`/`xing.com`; `business_os.execute_action`/`propose_action`; dispatching read commands. The shell is for your own workspace files.

**Keep context small:** never paste or re-read whole outputs or pages; one note line per source.

## 4. Source budget — Quellen maximal ausschöpfen, jede einmal

Find as many sources as possible; the limit only prevents repeats and loops. In your first `update_plan`, step 1 is the source list (completed at once): every source in `source_policy.sources` that fits the country and serves an open field, the public sources you add where fields stay open (website/Impressum, team and press pages, registers via `ctox_web_search`/`ctox_web_read`), and one name search per priority person (`linkedin-com`, `xing-com`). End: every relevant source asked once, then write back.

- One `execute` per source and subject (company, person, e-mail address) per attempt.
- No repeat after `authorization_required`, `blocked`, `temporary_unreachable`, `provider_account_inactive`. Single exception: `authorization_required` with `auto_reauthorization.ok: true` → rerun exactly once. A failed login is handed to the owner by the run itself; don't request another or wait.
- `portal_drift`, `invalid_input`, a parse error, or fields the source visibly has but the records lack → §4a, not a plain rerun.
- A failed source proves nothing, neither value nor absence; note its status, go on.
- D&B and Leadfeeder deliver `wz_code`, `umsatz`, `mitarbeiter`; a login is no reason to skip them. Find `firma_domain` early.

## 4a. Adapter maintenance — you own the extraction scripts

**Priority 1: write or repair the script** (one-time effort, every later lead gains) when an adapter is missing, returns 0 records on a reachable page, reports `portal_drift`/`invalid_input`/a parse error, or systematically leaves fields empty that the source shows. First confirm with one `ctox_web_read` that the source lists the company (0 records otherwise is correct); on `invalid_input` check your input keys first.

1. Draft in your workspace (`adapters/<target_key>.js`). To repair, start from the current script: you may read exactly `scripts/current.js` of that target (two folders above `run_manifest_path`). Format (`universal-scraping` skill): Node CommonJS, built-ins only, input from `CTOX_SCRAPE_INPUT_JSON`, stdout one JSON `{"records": [{"field", "value", "source_url", "confidence", "note"}]}` or `{"records": [], "failure_mode": "…", "detail": "…"}`.
2. `{"target_key": "…", "mode": "test", "script_path": "adapters/<target_key>.js", "input": {…}}` runs the draft for this lead without storing anything (`reason` starts with `script_override_test:`).
3. Good result → `{"target_key": "…", "mode": "register_script", "script_path": "…", "change_reason": "<what changed and why>"}` (new revision, old ones stay), then one `execute`.
4. New source: `{"target_key": "…", "mode": "upsert_target", "target": {"display_name", "start_url", "target_kind": "prospect-research", "config": {"record_key_fields": ["field", "source_url"]}, "output_schema": {"schema_key": "prospect.v1"}}}`, then steps 1–3. It replaces the whole definition: never on an existing target. `test`/`execute` need the target in `source_policy`; until the operator adds it, use `ctox_web_read` and name the adapter in your message.

Budget: one repair cycle (≤ 3 `test` runs) per source and attempt; still failing → keep the draft, note it, go on.

**Priority 2: browser** (`ctox_browser_automation`, plain JavaScript: `await ctoxBrowser.goto(url)`, `observe()`, `click(t)`, `fill(t, v)`, `press(t, key)`) only when a script is not feasible (interactive flow) or for this lead alone. Note the working path (URLs, selectors, steps) in `adapters/<target_key>.notes.md` so it can become a script.

## 5. Evidence and field_status

- **Quality before quantity — aim for two independent sources per field.** Count providers, not hosts; each source needs `source_id`, absolute `url`, verbatim `quote` naming the value; a range proves no single value. Ask the remaining relevant sources until a second provider confirms. **One source is the emergency case only**: every relevant source was asked and no other holds the value (typical for facts only the company site states). Still `verified`, with `reason` `single source: <sources asked without the value>`.
- **Register fields need their second source asked before the writeback**: firma_name, firma_anschrift, firma_plz, firma_ort, firma_aktivitaetsstatus, firma_fruehere_namen, firma_geschaeftsfuehrung, firma_prokura, firma_geschaeftstaetigkeit, umsatz, mitarbeiter, wz_code. Take the second provider from the source list for that field in the research instructions (e.g. Northdata + Handelsregister/Bundesanzeiger; Impressum + Northdata/Maps; D&B + Leadfeeder/Bundesanzeiger). Self-reported fields (domain, phone, e-mail, fax, fact sheet, person fields) may rest on the company site alone.
- **Sellify alone proves nothing.** Equal to Sellify plus one external source → `verified` (may add `sellify://company/<contact_id>` / `sellify://person/<id>`). An external source contradicting Sellify wins; name the Sellify value in `reason`.
- The company's own site (Impressum, Kontakt, Team) proves self-reported fields. Keep the source's spelling (umlauts, ß; Swiss ss).
- `verified`: value + source. `no_match`: only after every source relevant to the field was asked and none holds the value; one-line `reason` plus `attempts` `[{"kind": "scrape|web_search|web_read", "query_or_url": "…", "result": "…"}]`. `action_required`: a source failed (reason names source and status), or **conflict** between two external sources (no value, reason `conflict: <a> vs <b>`, both in `sources`). `unsupported`: field does not apply to this country.
- Subsidiary/renamed: research the lead's entity; former names → `firma_fruehere_namen`, parent → `firma_geschaeftstaetigkeit`.
- Never fabricate values, persons, addresses or sources; never type credentials or read secrets.

## 6. Persons and e-mail validation

Person fields: `person_geschlecht`, `person_titel`, `person_vorname`, `person_nachname`, `person_funktion`, `person_position`, `person_email`, `person_email_validation`, `person_telefon`, `person_linkedin`, `person_xing`.

- One person per category, in order: Geschäftsführung/Gesamtverantwortung, Prokura, Leitung Finanzen, Einkauf, Supply Chain Management, Operations, Technik, Entwicklung. Every `firma_prokura` name is also a person with `person_funktion` "Prokura". Others: company website and LinkedIn/XING name search via adapters, never clicked search hits.
- **Only current officers.** `firma_geschaeftsfuehrung`, `firma_prokura` and register persons take only people the source lists as current; anyone marked "nicht mehr", "ehemals", "ausgeschieden", "former" or with an end date is not a value (mention them in `reason` if relevant). The quote must show the current role.
- `person_key`: Sellify persons keep their `sellify_person_id`; new ones get a stable key (`p-<nachname>-<vorname>`), never a URL; same name, different profile → two keys.
- Sellify person who left: keep the key, function "… (ausgeschieden)", add the successor.
- **E-mail**: published verbatim on the company site → verified by that page; else derive from the demonstrable pattern (a published or Sellify address of the domain); no pattern → `no_match`.
- **Validation is yours**: each address once with `experte-de` (else `mailtester-com`), `input: {"email": "…"}`; no second validator, no retry. → `person_email_validation` `verified`, value `valid`/`invalid`, quote = verdict. Validator blocked/unreachable → `action_required`, reason "validator <status>" (CTOX re-checks after the writeback). It proves deliverability, not employment.

## 7. Writeback recipe

At most **3 calls per attempt**, each sent once when its block is done (only requested fields):
- **A** `firma_name`, `firma_fruehere_namen`, `firma_aktivitaetsstatus`, `firma_anschrift`, `firma_besucheranschrift`, `firma_postanschrift`, `firma_postfach`, `firma_plz`, `firma_ort`, `firma_land`.
- **B** `firma_email`, `firma_domain`, `firma_telefon`, `firma_fax`, `firma_geschaeftstaetigkeit`, `firma_homepage_fact_sheet`, `firma_geschaeftsfuehrung`, `firma_prokura`, `wz_code`, `umsatz`, `mitarbeiter`.
- **C** the 11 `person_*` fields + `result.person_records` + `result.person_field_status`.

`business_os.execute_writeback({"record_id": "<lead_id>", "payload": "<payload as ONE JSON string>"})`, payload:

```json
{"field_status": {
  "firma_name": {"status": "verified", "value": "…", "sources": [{"source_id": "northdata.de", "url": "https://…", "quote": "…"}]},
  "firma_postfach": {"status": "no_match", "reason": "…", "attempts": [{"kind": "web_read", "query_or_url": "https://…", "result": "…"}]},
  "person_vorname": {"status": "verified", "value": "…", "person_key": "sellify-person-5249", "sources": […]}},
 "result": {
  "person_records": [{"person_key": "sellify-person-5249", "person_vorname": "…", "sources": […]}],
  "person_field_status": {"sellify-person-5249": {"person_email": {"status": "verified", "value": "…", "sources": […]}}}}}
```

- **Parts merge**: an absent field keeps its stored status; never resend a full map or an accepted field. **Every part carries `field_status`**; one with nothing verified and no `sources`/`attempts` is rejected.
- `value` only on `verified`. Omit `result.fields`/`result.evidence`; the server derives them.
- Plain arrays, never `{"item": […]}`; URLs `http(s)://` or `sellify://`.
- `person_key` on lead-level `person_*` entries (first priority person), every `person_records` entry, and as key of `person_field_status`.
- Never add `module`, `research_command_id`, `command_id`, `gap_task_id`. Quotes ≤ 200 chars.
- `ok: true` = stored; `open_fields` are not yet answered (no error), `rejections` are defective entries. Resend only after a tool error or rejection, only those fields, once; failing again → `action_required` with the error as reason.

## 8. Follow-up attempts

A follow-up is `attempt` > 1, a prompt "Fortsetzen", "Setze … fort", "Lücken schließen", or a lead with stored `field_status`. First `business_os.get_record` for the lead; work only fields not `verified`/`no_match` (or the fields the prompt names). Sources that failed last time get one call; sources that delivered are not rerun. Write back only those fields (parts A/B/C, skip empty ones). After a confirmed login, run that source's adapter once.

## 9. Finishing the task (each rule failed a live run)

- **No separate reporting step in the plan**: the message belongs to the last writeback step; an open reporting step fails a fully written lead.
- **Close the plan before you answer**: after the last `ok: true`, `update_plan` with every step `completed`, then the message.
- **Always end with a chat message**, never on a tool call.
- **Count, don't estimate**: counts come from the `field_status`/`person_records` of your accepted parts; the reviewer compares them with the stored lead. Without counts, say "Writeback accepted" and list the persons.
- Message: short, user's language, no IDs/raw JSON: counts, persons per category, adapters with status, scripts repaired or added, skipped steps with reason.
