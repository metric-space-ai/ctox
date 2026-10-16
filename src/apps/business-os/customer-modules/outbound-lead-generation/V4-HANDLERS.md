# V4-HANDLERS – outbound-lead-generation (Strang OLGSCR, 15.10.2026)

Ergänzt `V4-PORT.md` (Handler-Port OLG/OLG2). Dieser Strang verlagert die Scrape-Registry und die
Feld-Status-Zeugen aus dem Rust-Core in die App (Owner-Regel: keine App-Logik im Core). Die v4-Fassung
der App liegt unter `customer-modules/outbound-lead-generation/` (es gibt kein `modules/outbound-lead-generation/`;
`v4/tools/manifest/gen-manifest.mjs` scannt `APP_ROOTS = ['modules', 'customer-modules']`). Schema:
`v4/schemas/outbound-lead-generation.collections.schema.json`. `module.json`/`collections.schema.json` im
Modulordner (Kundensystem) sind unverändert.

Rust-Referenz: Klon-Stand 4794af03d `src/core/capabilities/scrape/{mod.rs,registry.rs,classify.rs,account_state.rs,execute.rs}`,
`~/.local/state/shell-v4-src/4794af03d/src/core/business_os/{store_outbound_commands.rs,outbound_update_digest.rs,outbound_field_review.rs,store.rs}`.

## Verlagerte Core-Fähigkeiten → App-Logik

| vorher (Core) | jetzt (App) | Datei |
|---|---|---|
| scrape list_targets | `listTargets` über `outbound_lead_generation_scrape_targets` (registry.rs:18) | `_lib/scrape-registry.js` |
| scrape registered_target_summary | `registeredTargetSummary` (registry.rs:34) | `_lib/scrape-registry.js` |
| scrape target_script_registration | `targetScriptRegistration` – Revision + Hash des files-Inhalts (registry.rs:66) | `_lib/scrape-registry.js` |
| scrape upsert_target | `upsertTarget` (registry.rs:103, Slug, Ziel-ID `scrape_target-<sha16>`, Quellen-Normalisierung mod.rs:557) | `_lib/scrape-registry.js` |
| scrape latest_script | `latestScript` (store_outbound_commands.rs:1420, 150-KB-Deckel + Marker) | `_lib/scrape-registry.js` |
| scrape last_runs | `lastRuns` (:1357, Texte ≤ 300 Zeichen) | `_lib/scrape-registry.js` |
| scrape account_states | `accountStates` (:1306) | `_lib/scrape-registry.js` |
| scrape registry_runs | `registryRuns` (outbound_update_digest.rs:362) | `_lib/scrape-registry.js` |
| scrape execute_test / reprobe_keep_script / run_file | `executeScrape` / `reprobeKeepScript` / `runFileFacts` über `process.run` + `files` (execute.rs, classify.rs, account_state.rs) | `_lib/scrape-runner.js`, `outbound.research_source.reprobe.js` |
| outbound field_status_witnesses read / issue | `readCurrentWitnesses` / `persistWritebackWitnesses` (outbound_field_review.rs:26-276, store.rs:12652) – Ausstellung jetzt in derselben `ctx.transaction` wie der Lead (wie Rust; die frühere ABWEICHUNG entfällt) | `_lib/field-status-witnesses.js`, `_lib/research-writeback.js` |
| agent.task enqueue_scrape_repair | generisches `agent.task.enqueue` (flache Form, idempotency_key; Skill/Anweisung = App-Daten) | `_lib/research-source-adapter.js` |

`grep -rnE 'scrape\.[a-z_]+\(|cap\.scrape|cap\.outbound|field_status_witnesses\.(read|issue)' handlers --include=*.js` → 0 Treffer.

## Neue Collections (Präfix `outbound_lead_generation_`, keine Migration nötig – alle neu)

- `outbound_lead_generation_scrape_targets` – id = target_key; Ziel-Manifest inkl. `config.sources`, aktive Revision, `script_registered`, `workspace_dir` (Arbeitsbereich-Schlüssel für `process.run`).
- `outbound_lead_generation_scrape_scripts` – id = `<target_key>:r<revision_no>`; Text inhaltsadressiert in `files` (`script_ref`), `script_sha256` über den getrimmten Text.
- `outbound_lead_generation_scrape_runs` – id = run_id; Status, `result` (records_found, failure_mode, detail, reason, …), Run-Manifest und Artefakte als `files`-Refs.
- `outbound_lead_generation_scrape_account_states` – id = target_key; Generation, Lease, Backoff 6 h / 24 h (account_state.rs).
- `outbound_lead_generation_field_status_witnesses` – id = record_id; `witnesses: [{scope, person_key, field, status}]`. Privat (kein Export), nur der Writeback-Handler schreibt.

## Neue Befehle der App

| Typ | Payload | Ergebnis |
|---|---|---|
| `outbound.scrape.target.upsert` | `{manifest:{target_key?, display_name?, start_url, target_kind?, status?, schedule_hint?, config?, output_schema?}}` | `{ok, target_key, target_id, manifest_path, script_registered}` |
| `outbound.scrape.script.register` | `{target_key, script_text, language?, change_reason?, notes?, entry_command?}` | `{ok, target_key, revision_no, script_sha256, script_ref, deduplicated}` |
| `outbound.scrape.execute` | `{target_key, timeout_seconds? (1..900, Std. 120), input?, trigger_kind?}` | ScrapeExecutionOutcome (`run_id, status, records_found, fields_extracted, reason, error, run_manifest_path, …`) |
| `outbound.research.adapters.reconcile.done` (CAPSIG2) | Kern-Rückruf `{task_id, status, result, status_note?}` (on_complete/on_failed) | Ergebnis von Phase 2 des Abgleichs bzw. `{ok, task_id, duplicate:true}` |

Der Generator-Agent (Prompt in `queueResearchScraperGeneration`) registriert und testet jetzt über diese
Befehle statt `ctox scrape register-script|execute`.

## Angebot an den Strang outbound (scrape.registry.read, scrape.target.registration, scrape.target.upsert)

- **exports** (für `foreign_collections` der App outbound): `outbound_lead_generation_scrape_targets: ['read','watch']`,
  `outbound_lead_generation_scrape_runs: ['read','watch']`, `outbound_lead_generation_scrape_account_states: ['read']`,
  `outbound_lead_generation_scrape_scripts: ['read']`. Witnesses werden nicht exportiert.
- **scrape.registry.read** → foreign read der drei Collections (Ziel-Liste sortiert updated_at DESC, target_key ASC; letzter Lauf / letzter Erfolg je `target_id`; Konto-Zustand je `target_id`).
- **scrape.target.registration** → foreign read des Ziel-Dokuments: `script_registered`, `latest_script_revision_no`, `latest_script_sha256`, `workspace_dir` (von `outbound.scrape.script.register` gepflegt).
- **scrape.target.upsert** → `ctx.command('outbound.scrape.target.upsert', {manifest})` (Routing (app_id, type) an OLG).
- Eintrag in `DOC_FOREIGN.outbound.wishes` von `gen-manifest.mjs` (nicht meine Datei) nötig, damit der Generator die exports erzeugt.

## Capabilities (nur OS-Primitive; Manifest-Generator liest diesen Abschnitt)

- `cap.files.put`, `cap.files.get`, `cap.files.materialize` – Skripttexte, Eingabe-Umschlag, Run-Manifest, Artefakte.
- `cap.process.run` – Programme laut Allowlist: `node`, `tsx`, `bash`, `sh` (aus `entry_command`, mod.rs:1849); `cwd_ref` = `workspace_dir` des Ziels.
- `cap.agent.task.enqueue` – Generator-/Reparaturauftrag (flach: title, instruction, skill, thread_key, priority, idempotency_key, payload).
- unverändert aus OLG/OLG2: `cap.secret.scope`, `cap.secret.exists`, `cap.browser.auth_assist.request`, `cap.fence.claim`, `cap.fence.release`, `cap.command.get`, `cap.runtime_state.read`, `cap.runtime_state.write`, `cap.mail.send`.
- CAPSIG2: `cap.config.get` (Instanz-Absender `CTO_EMAIL_ADDRESS`), `cap.agent.task.{get, find, update, set_metadata, cancel, workspace.read}` (flach §16.30). Entfallen: `cap.mail.default_sender`, `cap.agent.task.workspace_file`.
- OLG3: entfallen `cap.mail.validate_contacts` (→ App-Befehl `outbound.contacts.email_validate`, Prüflauf über `cap.process.run`/`cap.files.*` der App-Registry) und `cap.web.capture` (→ `cap.browser.capture`, CATALOG §3, **Kernlücke: kein Kernvertrag**). Neu: ~~`cap.browser.capture`~~ (entfällt seit OLG4, Phase A ist ein Agentenauftrag), Trigger `every: PT1H` (`outbound.contacts.email_validation_sweep`). `cap.mail.send` bleibt (jetzt mit App-Buchführung).

Befehlsrechte: `outbound.research_source.test` und `outbound.research_source.reprobe` tragen die Probe-Freigabe
(Rust :4123 `outbound_source_test_probe_grant`); im v4-Vertrag prüft der Core `command_rights` vor dem Handler –
Vorschlag: nur Rollen mit App-Schreibrecht. `outbound.scrape.execute` läuft ohne Probe-Freigabe (ein inaktives Konto
wird erst nach Ablauf des Backoffs erneut geprüft).

## Kern-Import (einmalig, liefert der Kern; Quelle `crate::paths::core_db(root)`)

| Quelle (Core-SQLite) | Ziel-Collection | Feldabbildung |
|---|---|---|
| `scrape_target` | `outbound_lead_generation_scrape_targets` | id=target_key; target_id, target_key, display_name, start_url, target_kind, status, schedule_hint; config=JSON(config_json); output_schema=JSON(output_schema_json); workspace_dir=`outbound-lead-generation/scrape/targets/<target_key>`; latest_script_revision_no, latest_script_sha256; script_registered=Ergebnis von `target_script_registration` (registry.rs:66) zum Importzeitpunkt; created_at, updated_at; updated_at_ms=ms(updated_at) |
| `scrape_script_revision` | `outbound_lead_generation_scrape_scripts` | id=`<target_key>:r<revision_no>` (target_key per Join); target_id, revision_no, language, entry_command=JSON(entry_command_json), script_sha256, change_reason, notes, created_at; script_body → `files.put` (owner_app OLG) → script_ref, size_bytes; nur Zeilen mit sha256(trim(body)) = script_sha256 |
| `scrape_run` (+ `scrape_artifact`) | `outbound_lead_generation_scrape_runs` | id=run_id; target_id, target_key (Join), trigger_kind, started_at, finished_at, status, script_revision_no, script_sha256; result=JSON(result_json); artifacts=[{artifact_kind, path→files-Ref der Datei, content_sha256, record_count}]; run_manifest_ref=files-Ref von `<output_dir>/run.json` falls lesbar, sonst null; created_at_ms=ms(created_at) |
| `scrape_account_state` | `outbound_lead_generation_scrape_account_states` | id=target_key (Join über target_id); alle Spalten 1:1 |
| `outbound_native_field_status_witnesses` | `outbound_lead_generation_field_status_witnesses` | je record_id ein Dokument: id=record_id, witnesses=[{scope, person_key, field, status=JSON(status_json)}] |

Nicht importiert (von OLG-Handlern nicht gelesen): `scrape_source_revision`, `scrape_template_*`, `scrape_record_latest`, `scrape_semantic_record`.

## Invarianten und Mutationsbeleg (`node tests/mutation-proof-scr.mjs`, alle rot → zurück grün)

| Inv. | Regel | Test | Mutation |
|---|---|---|---|
| INV-SCR1 | Testbeleg nur gültig, wenn der Artefakt-Hash dem erklärten Hash entspricht | research_source.test „tampered records“ | sha-Vergleich entfernt |
| INV-SCR2 | Zeuge nur für unveränderten oder in diesem Writeback ausgestellten Status; mehrdeutige Identität speichert weder Lead noch Zeugen | research_writeback „INV-SCR2“ ×2 | `kept \|\| newlyIssued` entfernt; Mehrdeutigkeitsprüfung entfernt |
| INV-SCR3 | Inaktives Konto: kein Anbieteraufruf vor Fälligkeit; Backoff 6 h, danach 24 h | scrape-registry „suppresses“, reprobe „still inactive“ | Admission-Prüfung entfernt; Backoff konstant 6 h |
| INV-SCR4 | `probe_in_progress` nur bei nicht abgelaufener Lease | scrape-registry „INV-SCR4“ (vorher registry_read, s. CAPSIG2) | `>` → `>=` |
| INV-SCR5 | Revisions-Dedupe per sha256; Registrierung unbrauchbar bei geändertem Dateiinhalt | scrape-registry „INV-SCR5“ ×2 | Dedupe entfernt; Hash-Prüfung entfernt |
| INV-SCR6 | Klassifikationsreihenfolge (Exit ≠ 0 → portal_drift vor leerem Ergebnis) | scrape-registry „INV-SCR6“ | Exit-Prüfung entfernt |

## Abweichungen (bewusst, mit Grund)

- **Kein HTTP-Vorabtest** des Portals (classify.rs Probe-Zweige 401/403/404/429/5xx/unerreichbar): das wäre `net.http` vor jedem Lauf; der Status kommt aus `failure_mode`, Exit, Zeitüberschreitung und Fehlertext. Wunsch an Kern/Folgestrang: optional `cap.net.http.fetch` vor dem Lauf.
- **query_completion**: Vorbedingungen (query_completion.rs:38-80) portiert; die Hash-Prüfung der Belegdatei im Laufverzeichnis entfällt, weil `process.run` keine Ausgabedateien liefert.
- **process.run ohne env**: Eingabe als JSON-Umschlag `{ctox_scrape:{target_key, run_id, trigger_kind, command_id, start_url, manifest, sources, input}}` auf stdin statt `CTOX_SCRAPE_*`-Variablen. Bestehende Skripte müssen stdin lesen. Wunsch an den Kern: `process.run({…, env})` (generisch).
- **Konto-Zustand ohne Zugangsdaten-Version** (account_state.rs `credential_changed`): eine geänderte Zugangsdatei löst keine vorzeitige Prüfung aus; autorisierte Prüfung (Test/Reprobe) und fälliger Backoff schon.
- **manifest_path** in `research_policy.publish` und `scrape_registry_effect.target_manifest` nennt jetzt `outbound_lead_generation_scrape_targets/<target_key>` statt eines Dateipfads.

## CAPSIG2 (15.10.2026) – Handler auf Kernform (check-caps --calls)

Ausgang 11 ABWEICHUNG in dieser App, Stand jetzt 4 (alle unten unter „Offen“ begründet).

| Aufruf vorher | jetzt (Kernform) | Datei |
|---|---|---|
| `mail.default_sender()` | Absender = App-Einstellung `sender_email`, sonst `config.get({key:'CTO_EMAIL_ADDRESS'}) → {value}` (CATALOG §14; ref outbound_update_digest.rs:1350) | `outbound.update_digest.send_now.js` |
| `mail.send({kind:'policy_report', sender_email, body, body_html, report_key, policy_summary, …})` | `mail.send({account_id, to, subject, text, html, idempotency_key})` (caps/mail.rs:129). Die Policy-Report-Buchführung des Kerns (`record_and_send_policy_report_email`, `policy_summary`) entfällt; Versandstand weiter in der Statuszeile der App. | `outbound.update_digest.send_now.js` |
| `agent.task.enqueue({suggested_skill, command_id, reply_to_handler})` + Wiederaufruf mit `agent_reply` | flach `{title, instruction, skill:'universal-scraping', priority, idempotency_key:'<type>:<command.id>', payload:{command}, on_complete, on_failed}`; Rückruf = **neuer Befehl `outbound.research.adapters.reconcile.done`** (liest den Ausgangsbefehl über `agent.task.get({task_id})`, completed → Phase 2 mit `{task_id, text}`, failed/blocked/cancelled → Fehlerprojektion; idempotent je task_id über `reconciliation_task_id` der Richtlinie) | `outbound.research.adapters.reconcile.js`, `outbound.research.adapters.reconcile.done.js` |
| `agent.task.get({command_id})` | `agent.task.find({command_id})` (§16.30) | `_lib/research-writeback.js` |
| `agent.task.workspace_file({task_id, path})` ×2 | `agent.task.workspace.read({task_id, path}) → {text, sha256}`; „direkt unter `gap_closure/attempts/<feld>/`“ prüft die App am normalisierten Pfad. ABWEICHUNG: Symlink im Versuchsordner auf eine andere Datei im Workspace nicht erkennbar (kein kanonischer Pfad im Ergebnis); Ausbrüche weist der Kern ab. | `_lib/research-writeback-sanitize.js` |
| `agent.task.enqueue({prompt, workspace_root_rel, suggested_skill, parent_message_key, extra_metadata})` | flach `{title, instruction, skill, thread_key, priority, idempotency_key, payload:{workspace_root_rel, person_research_gap_closure}}` (Rückschrieb weiter über `outbound.lead.research_writeback`) | `_lib/person-research.js` |
| Befehl `outbound.research_source.registry_read` | **gelöscht** (Handler + Test). Die UI liest die Registry live (OLG2, `index.js` ~1072); kein Handler/keine UI setzt den Befehl ab (grep über customer-modules/, modules/, v4/: nur Doku/Kommentare und der v4-api-Test „kein registry_read“; `modules/outbound` hat einen eigenen, unberührten Handler). Die Bibliothekstests (INV-SCR4 `accountStates`, `latestScript`, von `index.js` benutzt) stehen jetzt in `tests/scrape-registry.test.mjs`, die INV-SCR4-Mutation in dessen Gruppe von `mutation-proof-scr.mjs`. **Manifest:** `outbound.research_source.registry_read` aus der Befehlsliste und neu `outbound.research.adapters.reconcile.done` aufnehmen – übernimmt der Manifest-Lauf. |

## Offen (Orchestrator-Ergänzung 15.10., nicht in diesem Strang erledigt)

- ~~`mail.default_sender`, `mail.send`, `agent.task.*`, `workspace_file`~~ → erledigt CAPSIG2 (oben).
- ~~**offen (2 ABWEICHUNG)** `mail.validate_contacts`~~ → erledigt OLG3 (Scheibe 1, unten). Alt: (`_lib/research-writeback.js:237`, `_lib/person-research.js:126`) → OLG-Fachlogik
  in die App. Umfang: Rust `contact_email_validation.rs` ~1300 Zeilen (Kandidatenwahl `emails_needing_validation`, Prüflauf über
  das Scrape-Ziel `experte-de`, Urteilsanwendung `apply_email_verdicts`/`promote_pattern_email`, Belegquittungen, Sweep alle
  6 h). Baustein vorhanden: Scrape-Läufer `_lib/scrape-runner.js` (process.run). Eigener Strang nötig; nicht im 50-min-Rahmen.
- ~~**offen (2 ABWEICHUNG)** `web.capture(…)`~~ → OLG3 Scheibe 3 (unten): App-Anteil portiert, Erfassung über `browser.capture` (Kernlücke). Alt: `web.capture({kind:'person_research', phase:'research'|'sources'})` (`_lib/person-research.js:456/458`)
  → laut CATALOG §3 `browser.capture({query|urls, auth_profile?, extract_schema}) → {captures[]}`. Phase A der Personenrecherche
  (Suchplan, Quellenwahl, Feldextraktion) ist heute Kern-Engine hinter web.capture; sie muss als App-Logik über browser.capture/
  web.fetch neu gebaut werden. Zusätzlich **Kernlücke**: `browser.*` hat keinen Kernvertrag (CAP-1/2, core-contract.mjs);
  ein bloßes Umbenennen auf `browser.capture` würde die Abweichung nur nach OHNE_VERTRAG verschieben.


## OLG3 (15.10.2026) – Kontaktprüfung, Policy-Report-Buchführung, Personenrecherche Phase A als App-Logik

Rust-Referenz: Snapshot 4794af03d `src/core/business_os/{contact_email_validation.rs, person_research_gap_closure.rs:1552-1636,
store.rs:12742, outbound_update_digest.rs:1442-1462, person_research_command.rs:2181-2409}`, Klon `src/core/mission/channels/outbound_review.rs:632-735,1332-1350,1795-1895,2343-2440`.

### Scheibe 1 (grün): `mail.validate_contacts` → App-Logik

| vorher (Core) | jetzt (App) | Datei |
|---|---|---|
| `mail.validate_contacts({lead_id})` (spawn_contact_email_validation :1290, Hintergrund-Thread) | `spawnContactEmailValidation` → `ctx.command('outbound.contacts.email_validate', {record_id})` | `_lib/research-writeback.js`, `_lib/person-research.js` |
| validate_lead_contacts :1200 (Kandidaten :231, Prüflauf :1141/:1160, Urteil :264, Anwendung :296/:431/:493, Quittungen :548, Altbestand :612, Abschluss gap_closure.rs:1552/:1567, CAS+Zeugen Preserve store.rs:12742) | `validateLeadContacts` | `_lib/contact-email-validation.js` |
| in_flight je Lead :1192 | Fence `email-validation:<record_id>`; zweiter Auftrag endet mit `{in_flight:true}` ohne Lauf | `outbound.contacts.email_validate.js` |
| sweep_unchecked_leads :1340 (stündlich, Wiederholung nach 6 h :1318) | Trigger-Handler `outbound.contacts.email_validation_sweep` (`every: PT1H`), je fälligem Lead ein `outbound.contacts.email_validate` | `outbound.contacts.email_validation_sweep.js` |
| Prüflauf `execute_scrape_with_outcome(experte-de)` + `outputs/result.json` | `executeScrape` der App-Registry (OS-Primitiv `process.run`, Ziel `experte-de`, 180 s, `input:{email}`), Records aus dem Artefakt `result_json` des Laufdokuments | `_lib/contact-email-validation.js` `runValidation` |

Neue Befehle: `outbound.contacts.email_validate` `{record_id}` → `{ok, record_id, changed, in_flight?}`;
Trigger `outbound.contacts.email_validation_sweep` → `{ok, due}`. **Manifest:** beide Befehle + Trigger
`{handler:'handlers/outbound.contacts.email_validation_sweep.js', every:'PT1H'}` aufnehmen (Manifest-Lauf).

ABWEICHUNGEN (bewusst): (a) kein Warten bis 900 s auf den Recherche-Fence (kein Schlafen im Handler) – belegter Lead →
Fehler `lead <id> stayed busy; N verdicts not applied` (Rust-Text), der nächste Sweep wiederholt (ohne Pass-Vermerk sofort
fällig); (b) kein Wiederholen bei „already has an active run“ (der App-Läufer hat keine Laufsperre); (c) Altbestands-
Reparatur akzeptiert neben `scrape_run-…` (importierte Läufe) auch `run-…` (Laufkennung des App-Läufers);
(d) Netzzugriff des Prüflaufs liegt im registrierten Skript `experte-de` (process.run), nicht in web.fetch/browser –
das Skript fährt die Angular-Seite `experte.de/email-pruefen` (echtes Rendern nötig, `web.fetch` reicht nicht).

### Scheibe 2 (grün): Policy-Report-Buchführung des Update-Verteilers zurück (App-Logik)

`outbound.update_digest.send_now` → `recordAndSendPolicyReportEmail` (`_lib/policy-report.js`, ref outbound_review.rs:2364):
Prüfungen und Fehlertexte wörtlich (Absender, Empfänger, Betreff/Text), Textprüfung auf Header-Zeilen/interne Begriffe für
Text und HTML (`founderBodyTextGate`, :659, Fehlertexte wörtlich), Freigabe-Digest exakt wie :1332
(sha256(action_json ‖ 0x00 ‖ sha256(trim(text))), action_json `{thread_key, subject, to, cc, attachments}` in
Einfügereihenfolge, serde_json preserve_order), `review_summary` = Richtlinien-Satz aus outbound_update_digest.rs:1460,
nur unverbrauchte Freigabe sendet (:1795, Text wörtlich), danach `sent_at` + `send_result` (:1881). Versand über die
Kernform `mail.send` mit getrimmtem Text/Betreff und normalisierten Empfängern (wie Rust). Nicht übernommen (Kern-Sache):
`acquire_reviewed_founder_send_lock`, `enforce_reviewed_founder_send_core_transition`.

Neue Collection `outbound_lead_generation_policy_report_reviews` (id = approval_key `policy-report:<report_key>:<action_digest>`;
approval_key, inbound_message_key, action_digest, action_json, body_sha256, reviewer, review_summary, account_key,
report_key, approved_at, sent_at, send_result). Neu, keine Migration.

### Kern-Import (OLG3)

| Quelle (Core-SQLite) | Ziel-Collection | Feldabbildung |
|---|---|---|
| `communication_founder_reply_reviews WHERE reviewer='policy-report'` (channel-DB) | `outbound_lead_generation_policy_report_reviews` | id=approval_key; approval_key, inbound_message_key, action_digest, action_json, body_sha256, reviewer, review_summary, approved_at, sent_at; send_result=JSON(send_result_json); report_key=inbound_message_key ohne Präfix `policy-report:`; account_key entfällt (Spalte fehlt in der Quelle) |

Kontaktprüfung braucht keinen Import: Urteile, Quittungen (`payload.email_validation_receipts`, `payload.email_validation_pass`)
liegen schon im Lead (RxDB); experte-de-Läufe kommen mit dem OLGSCR-Import (`scrape_run`).

### Invarianten und Mutationsbeleg OLG3 (`node tests/mutation-proof-scr.mjs`, Gruppen contact-email-validation + send_now)

| Inv. | Regel | Test | Mutation |
|---|---|---|---|
| INV-MAIL1 | Ein Urteil gilt nur für die angefragte Adresse | „INV-MAIL1 (2)“ (Rust :1432) | Adressvergleich entfernt |
| INV-MAIL2 | „Unbekannt“ (Catch-all) ist geprüft, aber nie verifiziert/ungültig | „INV-MAIL2 (9)“ (Rust :1715) | Status immer `verified` |
| INV-MAIL3 | Musteradresse wird nur bei zustellbarem Urteil zur verifizierten Personenadresse | „INV-MAIL3 (5)“ (Rust :1505 + Zusatzfall) | Gültig-Prüfung entfernt |
| INV-MAIL4 | Kein Patch, solange der Lead belegt ist (Recherche-Fence) | „INV-MAIL4 handler“ | Fence-Prüfung entfernt |
| INV-MAIL5 | Abschluss nach Prüfung nur ohne weitere ungeprüfte Adresse | „INV-MAIL5 (11)“ (Rust :1839) | Prüfung entfernt |
| INV-MAIL6 | Altbestand nur mit gültiger Laufkennung der Quittung | „INV-MAIL6 (12)“ (Rust :1901) | Laufkennungsprüfung entfernt |
| INV-PR1 | Eine verbrauchte Freigabe sendet denselben Slot/Text nie zweimal | „INV-PR1“ | Verbrauchsprüfung entfernt |
| INV-PR2 | Header/interne Begriffe im Text verhindern Freigabe und Versand | „INV-PR2“ | Textprüfung entfernt |

| INV-PRC1 | Authentifizierte Erfassung ohne verifizierten Besitzer erreicht die Erfassung nie (:2224) | „INV-PRC1“ | Besitzerprüfung entfernt |
| INV-PRC2 | Ungültiger Laufzeitquellen-Bezeichner bricht nach Phase A, vor der Quellen-Erfassung ab (:2265) | „INV-PRC2“ | Bezeichnerprüfung entfernt |

### Scheibe 3 (grün, mit Kernlücke): Personenrecherche Phase A (`web.capture`) → App-Anteil + `browser.capture`

`_lib/person-research-capture.js` (ref person_research_command.rs execute :2181-2409; web-stack sources/mod.rs:60/:102):
in der App jetzt Payload-Prüfung mit Rust-Texten (`invalid web_stack.person_research payload`, `company is required`,
`unsupported country …`, `unsupported research mode …`), Country/Mode-Normalisierung, Besitzerpflicht vor jeder
Erfassung bei `auto_browser_capture` (:2140/:2224), Bezeichnerprüfung der Laufzeitquellen (:2132/:2265, nach Phase A wie
Rust), `research_instructions_len` (Bytes), `workspace_root`, Zusammenfassungstext (:2367-2401). Die Erfassung selbst
läuft über `ctx.cap.browser.capture({query: company, extract_schema: {kind:'person_research', phase:'research'|'sources',
command_id, request, client_context, result?}}) → {captures:[result]}` (CATALOG §3: „browser“ deckt `web.capture` und
`research.person`; Extraktionsschema = App-Daten).

**Kernlücke (gemeldet, nicht erfunden):** `browser.*` hat keinen Kernvertrag (core-contract.mjs, CAP-1/2) → check-caps
`OHNE_VERTRAG`. Der Kern muss `browser.capture` mit `extract_schema.kind='person_research'` liefern (Antwortform
`{captures:[<Rust-Ergebnis von run_ctox_person_research_tool bzw. der Quellen-Phase>]}`).

**Erledigt durch OLG4 (unten), nicht portiert:** die Recherche-Engine selbst ist noch Kern-Code hinter der
Erfassung – Suchplan `build_person_research_plan`, Quellenmodule `sources/*` mit `shape_query`/`extract_from_hits`,
Treffer-/Lese-Kaskade, Personen-Gruppierung `rebuild_grouped_person_records`/`aggregate_fields`
(`src/tools/web-stack/src/person_research.rs` 3 512 Zeilen + `sources/` 13 594 Zeilen), Laufzeitquellen-Läufe
(`run_via_runtime_target` → `ctox scrape execute`, Beleg-Gate `runtime_record_to_field_evidence`, `merge_runtime_scrape_result`),
authentifizierte Erfassung (`source-capture` als Befehlsbesitzer, `merge_person_research_source_records`) und `repersist`.
Netzzugriffe davon wären `web.fetch` (Seitenlektüre) + `browser.*` (Rendern/Login); der Suchanbieter hat noch kein OS-Primitiv.
**Befund für den Folgestrang:** Laufzeitquellen verweisen auf Scrape-Ziele, die seit OLGSCR in der App-Registry liegen –
der Kern findet nach dem Import nur noch seine Altkopie; die Quellen-Phase gehört deshalb als Nächstes in die App
(Bausteine vorhanden: `executeScrape`, Beleg-Gate lt. scrape_bridge.rs:516-1190).

### Stand OLG3 (15.10.2026, Node 26)

`node --test handlers/tests/*.test.mjs` → 124 pass, 0 fail; `node --test tests/*.test.mjs` → 12 pass;
`node handlers/tests/mutation-proof-scr.mjs` → 7/7 Gruppen „vollständig“ (davon neu: contact-email-validation 6,
send_now 2, person-research-capture 2); `node handlers/tests/mutation-proof.mjs` → vollständig;
`check-caps --calls` → 0 ABWEICHUNG gesamt (OLG vorher 4); `check-caps` → OK (Ratchet-Baseline); `check-schemas` → 0 rot;
`v4/debt/ratchet.mjs` → OK (nicht gestiegen).
`grep -rnE "cap\.(mail\.validate_contacts|web\.capture)" handlers` → 0 Treffer.

## SKL-C (15.10.2026) – Skill, Templates und Agenten-Starts im Zielbild (Kern §16.30/§16.34)

Quelle: Bestandsaufnahme `F-automatisierung-skills.md` §1 #22/#23/#24/#26/#27, §5 Zeile OLG.

### Auslöser vorher → nachher

| # | Auslöser | vorher | nachher |
|---|---|---|---|
| 22 | Lead-Nach-/Neurecherche (`researchLead`) | `business_os.chat.task` ohne Handler; Satz im UI, Core-Prompt mit Sonderblock (store.rs:27037-27056), Skills per `required_skills`, Rückweg MCP `business_os.execute_writeback` (R5) | Befehl `outbound.lead.research` → `prompts/lead-research.md` bzw. `lead-research-continuation.md` → `agent.task.enqueue({skill:'outbound-lead-generation/research', idempotency_key, on_complete:'outbound.lead.research.done', on_failed:'outbound.lead.research.failed'})`; Lead `running` mit echter Task-ID |
| 23 | Kampagnenrecherche | Agent als Planer (`campaignResearchPrompt`, MCP `web_stack.person_research` je Lead) – im UI seit dem Je-Lead-Start schon toter Code; der Browser schickte je Lead einen Chat-Auftrag | Befehl `outbound.campaign.research` `{campaign, run_id, scope, shared, leads:[{lead_id, payload}]}`: der Handler fächert selbst je Lead einen `outbound.lead.research` auf (idempotency_key `outbound.campaign.research:<run_id>:<lead_id>` → Doppelzustellung legt keinen zweiten Task an). Sellify-Vorabgleich bleibt je Lead im Browser (`researchLead(…, {collect})`) |
| 24 | Lückenschluss (`web_stack.person_research`) | enqueue ohne Callback, Skill `outbound-lead-generation-research` (System), Abschluss per `ctox business-os commands dispatch` | `prompts/gap-closure.md`, Skill `outbound-lead-generation/research`, `on_complete/on_failed` wie #22 (der `.done`-Handler setzt `gap_task_id` aus dem Task) |
| 26 | Adapter-Erzeugung (`queueResearchScraperGeneration`) | enqueue ohne Callback (Stand nur über die Scrape-Registry) | `on_complete/on_failed: 'outbound.research_source.generation.done'`: Adapter `generation_queued` → `adapter_ready/registered` (Skript registriert) bzw. `adapter_failed/generation_failed` mit Grund |
| 27 | Sellify-Vermerke prüfen | `business_os.chat.task`, Antwort im Browser nach Status-Polling geparst (`leseFreitextAntwort`, `verarbeiteFreitextAntworten`) | Befehl `outbound.sellify.remarks.check` → `prompts/sellify-remarks.md` (wörtlich), Task ohne Skill, Callback `outbound.sellify.remarks.complete`/`.failed` schreibt Urteil bzw. Prüffehler in den Lead; UI liest nur `payload.sellify_freitext_urteil` |

`grep -c business_os.chat.task index.js` → 0. Entfernt aus `index.js`: `campaignResearchPrompt`, `openCampaignResearchChat`,
`freitextPruefungPrompt`, `freitextAntwortText`, `leseFreitextAntwort`, `ersterJsonBlock`, `freitextEintrag`, `baueFreitextUrteil`,
`verarbeiteFreitextAntworten`, `vorgangNochOffen`, `chatResearchTaskLeadPatch` und der Chat-Task-Abgleich in
`reconcileResearchCommands`/`researchCommandForLead`/`researchCommandLeadPatch` (Lead-Stand schreiben jetzt die Rückruf-Handler).

### Paket

- `skills/research/SKILL.md` – Kopie von `src/skills/system/research/outbound-lead-generation-research/SKILL.md`, zugeschnitten:
  Name `research`, Rückweg = ein JSON-Block in der Schlussantwort (§7), keine `business_os.*`-Werkzeuge mehr
  (execute_writeback/get_record/get_command_status), Folgeversuch liest `stored_field_status` aus dem Payload.
- `prompts/lead-research.md`, `prompts/lead-research-continuation.md` (Satz aus `researchLead`, :9541 alt), `prompts/gap-closure.md`
  (`buildGapClosurePrompt`, ref gap_closure.rs:655), `prompts/sellify-remarks.md` (`freitextPruefungPrompt`, wörtlich).
- `universal-scraping`, `web-unlock`: generische System-Skills, bleiben ohne Präfix im Core (Beleg in
  `v4/tools/agent-sim/README.md`, Abschnitt „System-Skills des Core“).

### Template-Füllung (ref store.rs:27034-27066 command_prompt)

`_lib/lead-research.js leadResearchInstruction`: `research_instructions` als eigener Block mit der Rust-Überschrift
„Rechercheablauf des Owners (research_instructions, wörtlich, unverändert, verbindlich):“, aus der Payload-Vorschau entfernt
(samt `research_instructions_default`, `instruction`, `prompt`), sensible Schlüssel raus (:27126), Vorschau ≤ 32 000 Zeichen
(:205/:27708). Neu in der Vorschau: `stored_field_status` (ersetzt den MCP-Lesezugriff des Agenten). D&B-Hinweise wörtlich aus
`researchLead` (jetzt im Handler, Lead-Belege aus der Collection).

### Neue Befehle (Manifest-Lauf übernimmt sie)

`outbound.lead.research`, `outbound.lead.research.done`, `outbound.lead.research.failed`, `outbound.campaign.research`,
`outbound.sellify.remarks.check`, `outbound.sellify.remarks.complete`, `outbound.sellify.remarks.failed`,
`outbound.research_source.generation.done`. Manifest: `skills:[{name:'research', path:'skills/research/SKILL.md'}]`,
`prompts:` die vier Dateien (Generator liest die Ordner). `validateOriginalResearchCommand` akzeptiert jetzt auch
`outbound.lead.research` als Recherchebefehl (Bindung des Writebacks an Befehl und Lead unverändert).

### Invarianten und Mutationsbeleg (`node handlers/tests/mutation-proof-scr.mjs`, Gruppe skl-research, 7/7 rot → grün)

| Inv. | Regel | Mutation |
|---|---|---|
| INV-SKL1 | Rechercheablauf nur als eigener Block, nie (abgeschnitten) in der Payload-Vorschau | Entfernen aus der Vorschau gestrichen |
| INV-SKL2 | Ein Rückruf wird höchstens einmal angewandt (Revision `research-callback:<task_id>`) | Duplikatprüfung entfernt |
| INV-SKL3 | Fehlschlag/Abbruch ändert den Lead nur, solange er an diesem Task hängt | Prüfung „superseded“ entfernt |
| INV-SKL4 | Kampagne: je Lauf und Lead genau ein Task | idempotency_key im Fan-out entfernt |
| INV-SKL5 | Vermerk-Urteil nur vollständig (Firma und jede geprüfte Person) | Personenprüfung entfernt |
| INV-SKL6 | Abbruch des Lückentasks beendet die Lückenphase | `research_phase = null` entfernt |
| INV-SKL7 | Adapter nur mit registriertem Skript `adapter_ready` | Registrierungsprüfung entfernt |

Testhilfe: `_ctx.mjs` hat `asset()` (Paketdateien); `mutation-proof.mjs` kopiert `prompts/`+`skills/` mit und startet die
Kindprozesse mit `--import v4/tools/node/ctox-shared.mjs`.

### Abweichungen (bewusst)

- Alt-Prompt der Kampagne (`campaignResearchPrompt`) nicht übernommen: Phase-1/2-Planerzeilen sind durch den Handler-Fan-out
  ersetzt, die Validierungsregeln („eine Quelle genügt“) widersprechen der Owner-Regel 07.10. (zwei Quellen, eine nur im Notfall),
  die der Skill trägt.
- Rückweg-Zeilen in `gap-closure.md` und Skill §7 geändert (Schlussantwort statt Dispatch/MCP) – Folge des Vertrags.
- Ein Rückruf kommt höchstens einmal: ein abgelehnter Writeback wird als Lead-Fehlerstand (`research_error` bzw.
  `research_last_failure`) festgehalten, nicht wiederholt.
- Alt-Leads, die noch an einem `business_os.chat.task` hängen, werden nicht mehr über den Befehlsstatus abgeglichen; sie fallen
  nach der 24-h-Grenze (`RESEARCH_RUNNING_MAX_MS`) auf „nicht zurückgemeldet“ und können neu gestartet werden.

### Kern-Abbau-Hinweise (jetzt tot für OLG)

- store.rs:27037-27056 Outbound-Sonderblock in `command_prompt` und `owner_research_instructions` (:27095).
- `suggested_skill_for_command`/`required_skill_names` für OLG-Recherchen; `src/skills/system/research/outbound-lead-generation-research/`
  (Quelle der Wahrheit ist das Paket).
- MCP-Rückschreibweg `business_os.execute_writeback` für `outbound.lead.research_writeback` und dessen `writeback_contract`-Aktivierung im Harness.
- Chat-Projektion (R7) für OLG-Aufträge (`business_commands`-Status als Ergebnisquelle der Vermerkprüfung).

### Stand SKL-C

`node --import <bos>/v4/tools/node/ctox-shared.mjs --test handlers/tests/*.test.mjs` → 131 pass, 0 fail; `tests/*.test.mjs`
(v4-api, Stub 7706796) → 12 pass; `mutation-proof-scr.mjs` → 8/8 Gruppen vollständig (neu skl-research 7);
`mutation-proof.mjs` → vollständig; `check-skills --no-baseline` → 0 Befunde (Baseline auf `[]` gesenkt);
`check-caps --calls` → ABWEICHUNG 0; `guard-v4` Exit 0.

## Strang OLG4 – Personenrecherche Phase A als Agentenauftrag (15.10.2026)

Entscheidung (Orchestrator): `web_stack.person_research` läuft nicht mehr über die Core-Recherchemaschine (web-stack,
~17 000 Zeilen Rust, wird nicht portiert), sondern wie `outbound.lead.research` als Agentenauftrag. Die beiden Aufrufe
`ctx.cap.browser.capture({query, extract_schema})` (vorher `_lib/person-research-capture.js:119/:140`) entfallen.

| Befehl | Handler | Wirkung |
|---|---|---|
| `web_stack.person_research` | `handlers/web_stack.person_research.js` | Prüfungen (Payload, Land, Modus, Eigentümer, Feldliste, Laufzeitquellen – jetzt alle VOR dem Auftrag) → `fence.claim({key:'outbound-research:<lead>', ttl_ms: 3 h})` → Lead `running` → Template `prompts/person-research.md` → `agent.task.enqueue` (Skill `outbound-lead-generation/person-research`, `idempotency_key` `person-research:<cmd>`, payload `person_research_phase_a` = Befehlsschnappschuss ohne client_context + Prüfergebnis + Fence-Token) → Lead `task_id`/`payload.person_research_task_id`. Ergebnis `{ok, status:'running', worker_started:true, task_id}`. `have_data`/`update_inventory_general`: kein Agent, `empty_plan_response` (web-stack :2078) direkt ausgewertet. |
| `web_stack.person_research.done` | `handlers/web_stack.person_research.done.js` | Rückruf on_complete: ein JSON-Block (`fields`, `person_records`, `plan`, `*_runs`, `browser_assist_tasks`) → `normalizePhaseAOutcome` (nur angefragte Felder, fehlende = missing) → wie vorher: Sellify-Grundlinie :2430, Zusammenfassung :2367, Lückenaufgabe :755 (Skill `outbound-lead-generation/research`, Rückruf `outbound.lead.research.done`), Lead-Projektion :402/:463 → Fence frei. Ohne Block: Lead `failed` („Die Personenrecherche endete ohne JSON-Ergebnisblock; nichts gespeichert.“). |
| `web_stack.person_research.failed` | `handlers/web_stack.person_research.failed.js` | Rückruf on_failed (auch `cancelled` → „Personenrecherche abgebrochen.“): Lead `failed` (:2075) → Fence frei. |

Beide Rückrufe sind idempotent je task_id (Marke `payload.person_research_callback_task_id`) und wirkungslos, wenn der Lead
an einem neueren Auftrag hängt (`payload.person_research_task_id` ≠ task_id; Fence wird trotzdem freigegeben).
Skill `skills/person-research/SKILL.md`: Suchplan (Stufen P → S → C, private Quellen nur per `include_private`,
Laufzeitquellen vor der Suche; ref person_research.rs:530/:588), Kaskade Adapter → Suche je Quelle → ≤ 2 Lektüren aus
≤ 4 Treffern (ref :150-400, :2107, :2178, :2005, :2356), Personen-Gruppierung (ref :786/:900/:1059/:1140,
person_ranking.rs:156) und Antwortform (ref :479) als Arbeitsanweisung. Eigenständiger Skill, weil Phase A nur Kandidaten
sammelt; die Bewertung (verified/offen) bleibt in der App, der Research-Skill (Lückenschluss) liefert field_status.

ABWEICHUNG (eigene App-Datei): `enqueueGapClosureIfNeeded` ruft `agent.task.update` jetzt flach mit `instruction` statt
`prompt` und ohne `workspace_root_rel` (steht schon im payload; §16.30 / agent-sim UPDATE_KEYS).
UI (gezielte Ersetzung `index.js` `researchCommandLeadPatch`): ein abgeschlossener `web_stack.person_research`-Befehl mit
Ergebnis `status:'running'` (Übergabe an den Agentenauftrag) wird im Browser-Abgleich nicht mehr als Recherche-Ergebnis
auf den Lead geschrieben; den Lead projizieren die Rückrufe.
`recover_once` (:106) entfällt: der Kern stellt den Rückruf zu, ein verlorener Auftrag gibt den Fence nach 3 h frei.

Capabilities: entfällt `cap.browser.capture`; `cap.fence.claim` jetzt mit `ttl_ms`; neu genutzt `cap.agent.task.enqueue`
für Phase A. Mutationsbeleg `tests/mutation-proof-scr.mjs` Gruppe `web_stack.person_research` (INV-PRA1..7).

### Kern-Abbau-Hinweise OLG4 (jetzt tot für OLG)

- `src/tools/web-stack/src/person_research.rs` (`run_ctox_person_research_tool`, Plan, Kaskade, Gruppierung, `aggregate_fields`,
  `merge_runtime_scrape_result`, `merge_person_research_source_records`, `persist_person_research_workspace`) und
  `person_ranking.rs`; `sources/*` (Quellenmodule inkl. `scrape_bridge.rs`, `runtime_targets.rs`), soweit nur von der
  Personenrecherche benutzt; das CLI-/Tool-Binding `ctox_person_research`.
- Core-Seite von `web.capture`/`browser.capture` mit `extract_schema.kind='person_research'` (Phasen research/sources,
  authentifizierte `source-capture`, `repersist`) – die Kernlücke aus OLG3 entfällt, kein Kernvertrag nötig.
- `person_research_command.rs` execute :2181-2409 und spawn_worker/recover_once (:106/:271) samt Worker-Thread und
  ActiveResearchCommandGuard; die App hält den Fence selbst.

### Stand OLG4

`handlers/tests/*.test.mjs` + `tests/v4-api.test.mjs` (Stub 68b83a3) → 149 pass, 0 fail; `mutation-proof-scr.mjs` →
alle Gruppen vollständig (neu web_stack.person_research 7); `mutation-proof.mjs` → vollständig;
`check-caps --calls` → ABWEICHUNG 0 (vorher 2: browser.capture); `check-skills --no-baseline` → 0 Befunde; `guard-v4` Exit 0.
