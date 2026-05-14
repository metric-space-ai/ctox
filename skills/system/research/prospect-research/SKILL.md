---
name: prospect-research
description: Use when the operator asks to research one or more companies / Ansprechpartner for sales pipeline work — fill the Sellify / Master-AT field set (Firmierung, Anschrift, Register-ID, Status, Tel/Fax/E-Mail/Website, USt-Id, Branche WZ, Geschäftsführung, Gegenstand, Tickers, Finanzkennzahlen, Mitarbeiter, etc.) using the CTOX web-stack source modules (zefix.ch, northdata.de, firmenabc.at, bundesanzeiger.de, handelsregister.de, companyhouse.de, optional Tier-C: linkedin.com / xing.com / dnbhoovers.com / leadfeeder.com). Trigger phrases (DE/EN) — "recherchier (mir) Firma X", "Stammdaten zu …", "Master-AT für …", "Sellify-Eintrag bauen", "Ansprechpartner finden für …", "Nachrecherche …", "Neurecherche …", "company research", "fill the prospect record for …".
cluster: research
---

# Prospect / Company Research

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.

## When to use this skill

Trigger this skill when the operator asks to recherchier one or more companies (and optionally their Ansprechpartner) and wants the Sellify-/Master-AT-typical field set populated. This is **not** the right skill for free-form market studies, decision briefs, or feasibility reports — those go to [`deep-research`](../deep-research/SKILL.md). This skill is for the structured *prospect record*: a typed bundle of fields per company that another system (Sellify, Business OS Sales pipeline, CSV export) can consume.

## Field vocabulary

The canonical field set is the union of the Thesen Nachrecherche fields and the Master-AT Excel schema:

| Group | Fields |
| --- | --- |
| Firma stammdaten | `firma_name`, `firma_rechtsform`, `firma_anschrift`, `firma_plz`, `firma_ort`, `firma_land`, `firma_email`, `firma_telefon`, `firma_fax`, `firma_domain`, `firma_status` |
| Register | `hr_amtsgericht`, `register_id`, `ust_id` |
| Branche & Geschäft | `wz_code`, `gegenstand`, `tickers` |
| Geschäftsführung | `ges_vertreter_1`, `ges_vertreter_2`, `ges_vertreter_3` (Liste der Vorstände / GFs, **nicht** nur ein Ansprechpartner) |
| Person (Sales-Kontakt) | `person_geschlecht`, `person_titel`, `person_vorname`, `person_nachname`, `person_funktion`, `person_position`, `person_email`, `person_telefon`, `person_linkedin`, `person_xing` |
| Finanzkennzahlen (Roh) | `finanzkennzahlen_datum`, `stamm_grundkapital_eur`, `bilanzsumme_eur`, `gewinn_eur`, `umsatz_eur`, `eigenkapital_eur`, `mitarbeiter`, `steuern_eur`, `kassenbestand_eur`, `forderungen_eur`, `verbindlichkeiten_eur`, `umsatz_pro_mitarbeiter_eur` |
| Finanzkennzahlen (Berechnet) | `gewinn_cagr_pct`, `umsatz_cagr_pct`, `umsatzrendite_pct`, `ek_quote_pct`, `ek_rendite_pct`, `steuer_quote_pct` |

The web-stack [`FieldKey`](../../../../tools/web-stack/src/sources/mod.rs) enum currently covers a subset; new fields added to this taxonomy must extend that enum before extractors can return them.

## Source priority by country

Pulled from the Thesen Abstimmungs-Excel translation in
[tools/web-stack/src/sources/EXCEL_MATRIX.md](../../../../tools/web-stack/src/sources/EXCEL_MATRIX.md).
Per `(mode, country, field)` the agent should consult sources in tier order
**P → S → C**, stopping when a high-confidence value is captured.

| Country | Tier P | Tier S | Tier C (opt-in via tenant credential) |
| --- | --- | --- | --- |
| AT | (Firmenbuch direkt — TODO adapter), zefix-only-for-CH | `firmenabc.at`, `northdata.de`, `person-discovery` | `linkedin.com`, `xing.com`, `dnbhoovers.com`, `leadfeeder.com` |
| DE | `bundesanzeiger.de`, `handelsregister.de` | `northdata.de`, `companyhouse.de`, `person-discovery` | dito |
| CH | `zefix.ch` | `northdata.de`, `person-discovery` | dito |

### `person-discovery` (public LinkedIn/XING snippet mining)

Tier S, DACH-wide, no credential required. Fans out a Google query of the
form `<company> ("linkedin.com/in/" OR "xing.com/profile/")` and mines the
search-result titles for:

* `person_linkedin` / `person_xing` — canonical profile URL (Confidence::High)
* `person_vorname` / `person_nachname` — split from the title (Confidence::Medium)
* `person_funktion` — role segment between name and company (Confidence::Medium)

Each emitted evidence row carries a `note` like
`network=linkedin;seniority=c_level;google_snippet_title` so the agent can
rank discovered profiles without a separate evaluation pass. Seniority
buckets are derived from the role string:

* `c_level` — CEO/CFO/COO/CTO/Vorstand/Geschäftsführer/Founder/Managing Director
* `senior`  — VP/Direktor/Head of/Bereichsleiter/Leiter/Prokurist/Partner
* `mid`     — Manager/Lead/Principal/Specialist
* `unknown` — anything else

Pinned-source flow (`ctox web search --source person-discovery`) skips the
per-hit `web read` step because LinkedIn and XING profile pages are gated
behind login walls. All evidence comes from the search-engine snippet
titles via [`SourceModule::extract_from_hits`](../../../../tools/web-stack/src/sources/mod.rs).

### Social-media evaluation step

After running person-discovery (or any Tier-C `--include-private` pass),
the agent SHOULD evaluate each discovered profile for sales relevance:

1. **Filter by seniority bucket** from the `note` annotation — c_level
   and senior are typical first-pass targets for sales pipeline work.
2. **De-duplicate** across LinkedIn / XING when the same person appears
   on both networks (match by full name + company).
3. **Cross-validate the role** against [`person-research`](../../../../tools/web-stack/src/person_research.rs)'s
   aggregated `person_funktion` from authoritative sources (Northdata,
   Bundesanzeiger, Zefix) — if a discovered "CEO" title contradicts the
   register, surface BOTH as candidates and flag.
4. **Optionally re-read** the public profile URL via `ctox web read` —
   most LinkedIn/XING pages return Open-Graph `og:title` + `og:description`
   even on a login wall, which can confirm the snippet.

## Tool surface

The agent drives these CLI tools directly. They run as subprocesses; output is JSON on stdout.

### Discovery + read

```bash
# List registered source modules, filter by country / field / tier
ctox web sources list [--country DE|AT|CH] [--tier P|S|C]... [--field <field-key>]
ctox web sources info --id <source-id>

# Pinned-source web search. `--source <id>` engages the source-module
# routing (api fetch_direct OR shape_query+domain pin).
ctox web search --query "<text>" --source <id> [--source <id>]... \
                --country <iso> [--include-sources]

# Read a single URL; auto-runs the source module's extract_fields on the
# response if the URL host matches a registered source.
ctox web read --url <url> [--country <iso>] [--find <pattern>]...
```

### Person research orchestrator (high-level)

```bash
# Build a plan from (mode, country, fields), run search+read per source,
# aggregate typed evidence per field.
ctox web person-research \
    --company "<name>" --country DE|AT|CH \
    --mode new_record | update_firm | update_person | update_inventory_general \
    [--field <field-key>]... \
    [--include-private linkedin] [--include-private dnbhoovers] ... \
    [--workspace runtime/research/person/<slug>] [--no-workspace]
```

The envelope returns `fields: {field: {value, confidence, source_id, source_url, candidates: […]}}`, `plan: [{source_id, tier, target_fields}]`, `search_runs`, `read_runs`, and `scrape_runs` (the per-target scrape executor outcomes).

### Scrape-target pipeline (durable, drift-repair-capable)

When a source module is registered as a scrape target (currently
`northdata-de`, `companyhouse-de`, `handelsregister-de`, `bundesanzeiger-de`),
extraction goes through the universal-scraping pipeline:

```bash
ctox scrape execute --target-key <key> \
    --input-json '{"company":"<name>","country":"<iso>"}' \
    --allow-heal
```

`--allow-heal` enqueues a `universal-scraping`-skill repair task on
`portal_drift` / `partial_output`. The agent should **always** pass
`--allow-heal` so DOM drift gets auto-fixed instead of silently failing.

## Procedure

1. **Classify the mode.** For each company:
   - `new_record` — kein Excel-/Sellify-Eintrag vorhanden, ab Stammdaten
   - `update_firm` — Firmierung/Anschrift potenziell veraltet
   - `update_person` — Firma stabil, Ansprechpartner hat gewechselt
   - `update_inventory_general` — periodischer Refresh (Excel B-Block leer ⇒ skip)

2. **Run person-research for the structured pull.**
   ```bash
   ctox web person-research --company "<name>" --country <iso> --mode <mode> \
       --field <list-of-fields-from-vocabulary>
   ```
   Parse the envelope. For every `field.confidence == high|medium` value, take it. For `missing` fields, plan a fallback pass.

3. **Fallback pass for uncovered fields.** For each `missing`:
   - If field is financial and country=DE: search Bundesanzeiger PDF directly
     (`ctox web search --query "<company> Jahresabschluss filetype:pdf" --domain bundesanzeiger.de` + `ctox web read --url <pdf>`).
   - If field is `register_id`/`hr_amtsgericht` and country=DE: query Handelsregister via `ctox scrape execute --target-key handelsregister-de`.
   - If field is `person_funktion`/`person_linkedin`/`person_xing` and no creds: rely on the always-on `person-discovery` source (already in the default plan for DACH); for credentialled API enrichment add `--include-private linkedin` / `--include-private xing`.
   - If country=AT and `register_id`/`firma_rechtsform` missing: probe Firmenbuch (manual `ctox web search --query "<company> firmenbuch.justiz.gv.at" + read`).

4. **Compute derived fields.** When the raw inputs are present:
   - `umsatzrendite_pct = gewinn_eur / umsatz_eur * 100`
   - `ek_quote_pct = eigenkapital_eur / bilanzsumme_eur * 100`
   - `ek_rendite_pct = gewinn_eur / eigenkapital_eur * 100`
   - `steuer_quote_pct = steuern_eur / (gewinn_eur + steuern_eur) * 100`
   - `umsatz_pro_mitarbeiter_eur = umsatz_eur / mitarbeiter`
   - CAGR-Felder brauchen historische Jahresabschlüsse aus Bundesanzeiger
     (3- bzw. 5-Jahres-Datenreihe).

5. **Verify against existing evidence.** If the operator gave a reference
   (Sellify-Nummer, FN, HRB, USt-Id), check that the consolidated record
   matches. Conflicting values → keep all `candidates` in the envelope and
   flag for user confirmation.

6. **Render the deliverable.** A Markdown table per company, columns =
   field-keys mapped to human labels, plus a per-field `(src, confidence)`
   annotation. Save the raw envelope under
   `runtime/research/person/<timestamp>-<slug>/envelope.json` (the
   `person-research` workspace does this automatically; do not skip with
   `--no-workspace` unless explicitly asked).

## Drift contract

If `scrape_runs[*].classification == "portal_drift"`, the universal-scraping
repair queue is already triggered. **Do not retry the same source in the
same run**; record the drift in the deliverable and move on. The repair
will land in a subsequent run.

If `scrape_runs[*].classification == "temporary_unreachable"` or
`"blocked"`, that source is transiently unavailable — try a fallback
source from the priority list, but do not queue any repair.

If `classification == "credential_missing"` (Tier C), the field this
source uniquely owns (e.g. `person_email` ↔ leadfeeder, `person_funktion`
↔ linkedin) stays `missing` with reason `credential_missing` and is
surfaced to the operator as an opt-in suggestion, not a failure.

## What this skill does NOT do

- Free-form market research, competitor analyses, feasibility reports → use
  [`deep-research`](../deep-research/SKILL.md).
- Sellify or Business-OS writeback — this skill produces the typed
  prospect record; persistence to Postgres / Sellify is the calling
  module's responsibility.
- Adapter / DOM-drift repair — that's the job of the
  [`universal-scraping`](../../communication/universal-scraping/SKILL.md)
  skill, which gets auto-enqueued by `ctox scrape execute --allow-heal`.

## Resources

- [tools/web-stack/src/sources/EXCEL_MATRIX.md](../../../../tools/web-stack/src/sources/EXCEL_MATRIX.md) — the Thesen Abstimmungs-Excel as a `(mode, country, field) → source-priority` table
- [tools/web-stack/src/sources/README.md](../../../../tools/web-stack/src/sources/README.md) — source-module trait + fixture conventions
- [skills/system/communication/universal-scraping/SKILL.md](../../communication/universal-scraping/SKILL.md) — drift-repair sibling skill
