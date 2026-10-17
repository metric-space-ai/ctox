---
name: person-research
description: Phase A of the person/company research for the Business OS "Outbound Lead Generation" app — build a source plan, collect field candidates with verbatim evidence from registers, directories, the company site and the registered source adapters, group the persons, and return ONE JSON block in the final answer; the app evaluates it when the task completes (callback web_stack.person_research.done) and hands open fields to the gap closure. Trigger on "# Personen-/Firmenrecherche Phase A".
cluster: research
---

# Outbound Lead Generation · Personenrecherche Phase A

<!-- App-Paket outbound-lead-generation (Strang OLG4, 15.10.2026). Ersetzt die Core-Recherchemaschine
     src/tools/web-stack/src/person_research.rs + sources/ (Rust-Stand 4794af03d, ~17 000 Zeilen), die nicht portiert wird:
     Suchplan, Quellenmodule und Personen-Gruppierung sind hier Arbeitsanweisung. Die Bewertung (verified/offen, Lückenschluss,
     Lead-Projektion) bleibt in der App (handlers/_lib/person-research.js). -->

## 1. Purpose and boundary

You collect **candidates** for the requested fields of one company and its persons. You do not decide `verified`/`no_match`
and you do not write anything back: the app reads exactly one JSON block from your final answer (§5), counts independent
sources per field, and queues a gap-closure task for every field that stays open. Model calls cost ~20 s each; don't explore
the system. The task text (company, country, mode, fields, owner instructions, sources, persons) is the authority.

Tools: `ctox_web_scrape` (`mode: "execute"` on a `target_key` from the registered sources, `timeout_seconds` ≤ 420, 400 for
login sources), `ctox_web_search` `{"query", "domains"}`, `ctox_web_read` `{"url", "query", "find"}` (without `query` a read is
no evidence), `ctox_browser_automation` only for a page no adapter and no read can open, `update_plan`.
**Forbidden:** the `ctox` CLI, `curl`/`wget`, `sqlite3`, reading CTOX runtime files, any `business_os.*` tool, dispatching
commands, typing credentials, `ctox_web_read` on `linkedin.com`/`xing.com`.

## 2. Source plan (first `update_plan` step, completed at once)

<!-- ref: person_research.rs:530 build_person_research_plan, :588 is_source_opted_in, sources/mod.rs privacy_opt_in_required -->
- **Fields wanted** = the requested fields of the task. For each field list the sources that are authoritative for it in the
  lead's country; one plan entry per source with its `target_fields`.
- **Order by tier P → S → C**, inside a tier by source id:
  - **P (primary registers / own statements):** `handelsregister.de`, `bundesanzeiger.de` (DE), `zefix.ch` (CH), `impressum`
    (company site Impressum/Kontakt/Team; needs `firma_domain`, so find the domain early).
  - **S (secondary directories):** `northdata.de`, `companyhouse.de` (DE), `firmenabc.at` (AT), `moneyhouse.ch` (CH),
    `google.de` / `maps.google.com` (address, phone, domain), `experte.de` (e-mail check), `person-discovery` (search
    snippets naming persons; only when listed in `include_private`).
  - **C (commercial / login sources):** `dnbhoovers.com`, `leadfeeder.com`, `linkedin.com`, `xing.com`, `rocketreach.com` —
    **only when the source id or an alias is listed in `include_private`**; otherwise they are not in the plan at all.
- Registered runtime sources (`source_policy.sources`, each `id`, `target_key`, `field_keys`) join the plan for their
  `field_keys` with their own tier; run their adapter (`ctox_web_scrape` execute) **before** any search for that source.
- Mode `update_firm`/`update_person`: known Sellify data is the start, complete it; `new_record`: from scratch. (Modes
  `have_data` and `update_inventory_general` never reach you: the app skips them.)
- Owner `research_instructions` decide order and which source serves which field inside this frame.

## 3. Per source: adapter → search → read

<!-- ref: person_research.rs:150-400 (scrape target, shape_query, pinned search, extract_from_hits, MAX_HITS_PER_SOURCE 4,
     MAX_READS_PER_SOURCE 2), :2107 url_belongs_to_source, :2178 search_hit_evidence_eligible, :2005 company_identity_matches,
     :2356 find_terms_for_fields, :1556 confidence_rank, :643 browser_assist_tasks_from_source_failures -->
1. **Adapter first.** If the source has a registered `target_key`, run it once (`execute`). Take its records as candidates
   with `via: "scrape_target"`. A field the adapter delivered is not collected again from the same source. On `portal_drift`,
   `blocked` or `temporary_unreachable` fall through to steps 2–3 as a safety net; never rerun the adapter.
2. **Search pinned to the source.** One `ctox_web_search` with the company name, sharpened for the source (`site:` / domain,
   role words for person sources), restricted to the source's host, country as location. Keep only hits whose URL belongs to
   that source's host **and** whose title/snippet names the company (all distinctive name tokens, legal form ignored). Snippet
   sources (person discovery) take their candidates straight from the hit snippets (`via: "search_snippet"`) and read nothing.
3. **Read.** From the first 4 eligible hits read at most 2 pages per source, each `ctox_web_read` with `query` naming the
   field facts and `find` terms for the fields (e.g. Firma/Firmierung, Umsatz/Umsatzerlöse, Mitarbeiter/Arbeitnehmer,
   Geschäftsführer, E-Mail, Telefon, Anschrift). A read value counts only if the page clearly concerns this company.
4. Every source once per subject; no repeat after `authorization_required`, `blocked`, `provider_account_inactive`. A login
   or approval a source needs becomes an entry in `browser_assist_tasks` (`source_id`, `reason`, `url`); never wait for a human.
5. Confidence per candidate: `high` (register, the company's own site, adapter record with exact match), `medium`
   (directory), `low` (snippet, indirect). Evidence must be from this run (no cached or remembered values).

## 4. Persons

<!-- ref: person_research.rs:759 PERSON_RECORD_FIELDS, :786 grouped_person_records, :900 coalesce_name_only_person_records,
     :1059 aggregate_fields (best person), :1140 aggregate_candidate_eligible, person_ranking.rs:156 compare_person_records -->
- Person fields: `person_vorname`, `person_nachname`, `person_funktion`, `person_position`, `person_titel`,
  `person_geschlecht`, `person_email`, `person_email_validation`, `person_telefon`, `person_xing`, `person_linkedin`.
- Every person candidate carries a `person_key`: a Sellify person keeps `sellify_person_id`; otherwise the profile URL
  (XING/LinkedIn), else the page URL the person was found on, else the normalized name. Candidates of the same person
  (same key, or a name-only candidate matching a keyed person) are merged into one record.
- A record needs an identity (`person_vorname`/`person_nachname`, `person_email`, `person_xing` or `person_linkedin`);
  otherwise drop it. Never take articles or role words as names (der, die, das, ein, eine, Geschäftsführer(in),
  Geschäftsführung, Vorstand, Inhaber(in), vertretungsberechtigt). Only current officers; former ones are no candidates.
- Sort records by the task's person priorities (role category), then by number of evidence entries, then by name. The
  lead-level `person_*` fields come from the **first** record only.

## 5. Result — one JSON block in the final answer

<!-- ref: person_research.rs:479 payload, :1059 aggregate_fields; App: research-outcome-patch.js researchOutcomePatch,
     research-writeback-sanitize.js phaseAAttemptedSources / phaseAVerifiedSources -->
After a short summary (fields found, persons, sources with status), end with exactly one fenced block:

```json
{"plan": [{"source_id": "northdata.de", "tier": "S", "target_fields": ["firma_name", "umsatz"]}],
 "fields": {
  "firma_name": {"value": "ACME GmbH", "confidence": "high", "source_id": "handelsregister.de", "source_url": "https://…",
    "tier": "P", "note": "…", "candidates": [
      {"value": "ACME GmbH", "confidence": "high", "source_id": "handelsregister.de", "source_url": "https://…", "tier": "P",
       "via": "web_read", "quote": "ACME GmbH, Hamburg, HRB 1234"}]},
  "umsatz": {"value": null, "confidence": "missing", "candidates": []},
  "person_vorname": {"value": "Erika", "confidence": "high", "source_id": "impressum", "source_url": "https://…",
    "person_key": "https://www.xing.com/profile/…", "candidates": [ … ]}},
 "person_records": [{"person_key": "…", "person_vorname": "Erika", "person_nachname": "Muster",
   "person_funktion": "Geschäftsführerin", "source_id": "impressum", "source_url": "https://…", "evidence_count": 2,
   "evidence": [{"field": "person_nachname", "value": "Muster", "source_id": "impressum", "source_url": "https://…", "quote": "…"}]}],
 "search_runs": [{"source_id": "northdata.de", "query": "…", "ok": true, "hit_count": 3}],
 "read_runs": [{"source_id": "northdata.de", "url": "https://…", "ok": true}],
 "scrape_runs": [{"source_id": "dnbhoovers.com", "target_key": "dnbhoovers-com", "classification": "succeeded", "record_count": 4}],
 "browser_assist_tasks": []}
```

- Only the requested fields; a field without a candidate is `{"value": null, "confidence": "missing", "candidates": []}`.
- The top-level `value` of a field is its best candidate (highest confidence); all candidates stay in `candidates`, each
  with `source_id`, absolute `source_url` and a verbatim `quote` (≤ 200 chars) — a candidate without them proves nothing.
- Every source you asked appears in `search_runs`/`read_runs`/`scrape_runs` with its status, also when it failed: the
  gap closure starts from these attempts.
- Plain arrays, never `{"item": […]}`. Never add `record_id`, `module`, `command_id` or status verdicts: the app takes
  them from the task. Never fabricate values, persons or sources.

## 6. Finishing

Close the plan (`update_plan`, every step `completed`), then the final answer; always end with a chat message, never on a
tool call. Counts in the summary come from the JSON block.
