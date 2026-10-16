# V4-PORT – outbound-lead-generation (Kunden-App, Strang OLG „Shell v4“)

Stand 15.10.2026, Klon `~/Documents/ctox-shell-v4` (Zweig `shell-v4/prototype`). App-Dateien byte-gleich mit
origin/main `4794af03d` (sha256 `module.json` 2be4554a…, `collections.schema.json` bc6c7f13…, `index.js` 1a9a2bfe…).
Rust (nur gelesen): `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/`. Pfade unten relativ zur App bzw. zu
`core/business_os/`. Messregeln wie C1 (`~/.local/state/shell-v4/analysis/C1-apps-teil1.md`, Abschnitt Messregeln).
Die App fehlte in C1/C2, weil dort nur `modules/` aufgenommen wurde; derselbe blinde Fleck steckt im Manifest-Generator
(scannt nur `modules/`, siehe §16 Fallen).

## Übersicht (Zeile im Format von C1)

| App | Zeilen (Dateien) | Collections (eigen) | Sync-Kopplung verboten / sonst. | Befehle: Typen / Dispatch-Stellen / Warten | HTTP-Ladevorgänge | Schleifen (wiederk.) | Zustandstexte ≈ | Rust app-spez. (grob) | Aufwand |
|---|---|---|---|---|---|---|---|---|---|
| outbound-lead-generation | 18 118 (14) | 5 (+4 fremd gelesen, +2 fremd geschrieben über Handler) | 20 / 5 | 12 eigene + 5 Plattform / 9 `sendeBefehl` + 2 direkt `dispatch` + 3 `submitTask` + 1 `cancel` + 3 `requestNative` / ja, 9× accepted/terminal (bis 120 s) | 5 (+1 dyn. Vendor) | 5 setInterval + 1 Sweep | 122 (Wächter) | ~20 k | L |

CI-Wächter (`v4/tools/guards/guard-v4.mjs --app customer-modules/outbound-lead-generation`): **168 Treffer** (bestätigt),
davon 122 Zustandsvokabular, 46 Löschlisten-Regeln. **Stand OLG2: 179 Treffer** – die 11 neuen stammen aus `handlers/`:
10× `_lib/update-digest.js` (Mail-Text des Update-Verteilers wörtlich aus Rust: „warten auf Prüfung“, „nicht erreichbar“)
und 1× `outbound.research.adapters.reconcile.js:258` (Rust-Fehlertext „…definition is unavailable“). Das ist Mail-/Fehlerinhalt,
kein UI-Zustandstext; Text bleibt wegen Treue. Vorschlag an den Wächter-Strang: Vokabelregeln nicht auf `handlers/` anwenden.

## 1. Umfang

18 118 Zeilen in 14 Dateien (ohne Tests, ohne Locales/JSON/Bilder): `index.js` 14 498, `index.css` 1 921,
`xlsx-export.js` 561, `import-preview-groups.js` 348, `lead-list-loader.mjs` 194, `startup-cache.mjs` 121,
`collection-reloader.mjs` 89, `schema.js` 84, `lead-revision-loader.mjs` 72, `field-catalog.mjs` 70,
`current-state-export.mjs` 53, `in-flight-lead-sweep.mjs` 44, `read-error-grace.mjs` 38, `reconcile-basis.mjs` 16,
`required-field-selection.mjs` 8, `index.html` 1. Einstieg `module.json.entry = local-modules/outbound-lead-generation/index.html`,
`export async function mount(ctx)` `index.js:672` (v1-Vertrag, nicht `mount(root, api, {restored})`).
Benutzte ctx-Felder: `host` (23×), `sync` (11×: startCollection, restartCollection, requestNative, collectionReadiness,
subscribeCollectionReadiness), `db` (7×, `collection(name)` `:682-695,816`), `commandBus` (dispatch, cancel, getStatus),
`businessChat` (submitTask 3 Stellen), `session` (user/userId), `locale`.

## 2. Shell/Global

Globals: `globalThis.__olgDialogZiel` (`index.js:21,676`, Dialogziel), `globalThis.__olgLaenderkorrektur` (`:9483-9525`,
Diagnose), `document.documentElement.dataset.olgBoot`/`olgLaenderkorrektur` (Boot-/Diagnosezustand). `localStorage` 10×
(u. a. `ctox.olg.registry.v1` `:9342,9373`), `sessionStorage` 1×.
Importe: `../../shared/universal-importer.js?v=20261006-import-preview-groups-v1` (`:1-7`, Cache-Buster),
`../../shared/dialogs.js` (`:9-13`), `../../shared/i18n.js` loadModuleMessages (`:27`), 9 lokale Module.

## 3. Sync-Kopplung (verboten 20, sonst. 5)

- startCollection 4: `index.js:864, 1001, 7154, 9209`
- restartCollection 2: `index.js:803, 815` (`business_commands`)
- leaseCollection 2: `lead-list-loader.mjs:146, 150`
- collectionReadiness 4: `index.js:889, 1132-1134`; subscribeCollectionReadiness 1 (`:1170`)
- requireRevision 5: `index.js:11122`, `lead-list-loader.mjs:85, 124`, `lead-revision-loader.mjs:15, 51`
- requestNative 3: `index.js:9225, 9227, 9239` (`ctox.outbound.sellify_lookup.v1`)
- sonst.: `localReplicaComplete` 1 (`lead-list-loader.mjs:46`), awaitInSync 2, awaitInitialReplication 2.
Eigene Lade-/Neulade-Maschinerie: `collection-reloader.mjs`, `lead-list-loader.mjs`, `lead-revision-loader.mjs`,
`startup-cache.mjs`, `read-error-grace.mjs` — in v4 ersatzlos (Live-Abonnements `api.query`).

## 4. Befehle (mit Warten)

Wrapper `sendeBefehl` `index.js:7148-7180` (startet vorher `business_commands` per `startCollection`, Frist
`timeoutMs + 10 s`). Warte-Stellen (Wächter `until-accepted-terminal` 9): `:2944` update_digest (terminal 45/120 s),
`:7309` secret.delete (terminal 60 s), `:7342` secret.list (terminal 30 s), `:8861` research_policy.publish
(accepted 45 s, direkt `dispatch`), `:9281` sellify.lookup (terminal), `:9321` und `:9368` registry_read (terminal 45 s),
`:12233` external_sql.write (terminal 90 s), `:13132` `bus.cancel` (terminal). Nur `:7269` secret.put nutzt `until:'local'`.
Agentenaufträge über `businessChat.submitTask` (`:5229` Kampagnenrecherche, `:5632` Freitextantworten, `:8036`
Adapter-Befehl, `:9112` Adapter-Abgleich). Status-Polling über `commandBus.getStatus` (`:11032`).

## 5. HTTP

`loadModuleMessages` (`:679`, fetch locales), `<link>` CSS mit `?v=${STYLE_BUILD}` (`:1177-1182`), statischer Import
`universal-importer.js?v=…` (`:7`), dynamischer `import('./xlsx-export.js')` (`:13196`), darin `import(options.jszipUrl)`
(`xlsx-export.js:328`, Vendor JSZip). `index.html` wird nicht gefetcht (Markup im Code).

## 6. Schleifen

`setInterval` 5 (Wächter): `:734` Kampagnen-Personen-Pflege 20 s, `:738` Befehlsabgleich 3×reconcile (Takt im Code),
`:1529` Freitext-Antworten 20 s, `:4431` und `:9563` Sekundenuhren (UI). Dazu `IN_FLIGHT_SWEEP_INTERVAL_MS = 60 000`
(`in-flight-lead-sweep.mjs:6`). 36 `setTimeout`/`setInterval` gesamt in `index.js`.

## 7. Zustandstexte (122 laut Wächter)

Beispiele wörtlich: „Netzwerk wieder verfügbar. Daten werden synchronisiert“ (`:712`), „Daten werden synchronisiert“
(`:959`), „CTOX-Verbindung wird hergestellt“ (`:984`), „CTOX ist gerade nicht erreichbar. Erneuter Versuch läuft.“ (`:985`),
„Datenverbindung konnte nicht hergestellt werden.“ (`:2249`), „Prompt wird geladen – Synchronisierung läuft …“ (`:2665`),
„Der CTOX-Befehlskanal ist in dieser Sitzung nicht verfügbar.“ (`:7150`), „Direkter Sellify-Weg nicht verfügbar.“
(`:9225`), „Sellify hat den Schreibvorgang bestaetigt. Die Ruecksynchronisierung in die lokale Kopie steht noch aus.“
(`:12263`). Größte Klassen: de-fortschritt-wird 24, de-laeuft 20, de-nicht-verfuegbar 19, de-geladen 10.

## 8. Collections und Schema-Quelle

Eigene (alle in `module.json.collections` und `collections.schema.json`, module_id `outbound-lead-generation`):

| Collection | Version | Besonderheit |
|---|---|---|
| outbound_lead_generation_sources | 0 | |
| outbound_lead_generation_adapters | 2 | Migrationen 1, 2 (`set_from_first_truthy last_task_id`) in JSON **und** schema.js; **Namensgleich in modules/browser/collections.schema.json mit ABWEICHENDEM Schema** (gleiche Version 2; OLG hat last_command_id/last_task_id, browser hat 14 andere Felder; required verschieden) |
| outbound_lead_generation_imports | 0 | |
| outbound_lead_generation_research_policies | 0 | |
| outbound_lead_generation_leads | 0 | JSON-Eintrag ist Hülle `{schema, syncProfile:'eager'}`; `module.json.record_fields` ergänzt contacts/selected_contact_ids/evidence |

`schema.js` und `collections.schema.json` stimmen für alle 5 Collections kanonisch überein (geprüft); `schema.js`
entfällt (§16.13). Fremd: `sellify_companies` (lokal gelesen `index.js:695,11601`), `sellify_people`, `sellify_campaigns`
(über den Lookup), `business_commands` (Status), geschrieben über Handler/Legacy: `outbound_research_policies`
(nur nativ `business_records`, in keinem Manifest), `outbound_research_adapters` (Eigentümer outbound).

## 9. Assets

`icon.svg` (738 B) = `assets/icon/outbound-lead-generation.svg` (byte-gleich). `module.json.layout.icon_asset` =
`local-modules/outbound-lead-generation/assets/icon/outbound-lead-generation.svg`, `module.json.icon = "icon.svg"`;
**kein `icon_svg` in module.json auf main** – das v4-Manifest übernimmt `icon_asset` **und** ein inline `icon_svg` aus
`icon.svg` (Owner-Auflage: Logo erhalten). `locales/de.json`/`en.json` sind leer (`{}`); Texte stehen im Code.

## 10. Rust (app-spezifisch, ~20 k Zeilen)

| Datei:Zeile | Funktion | Zeilen | Einstufung |
|---|---|---|---|
| store_outbound_commands.rs:30-48 | `handle_outbound_active_command` Modulprüfung (OLG darf reprobe/policy.publish) | 19 | Routing |
| store_outbound_commands.rs:1171-1190 | Routing der OLG-Typen | 20 | Routing |
| store_outbound_commands.rs:1194-1273 | `outbound_handle_sellify_lookup`, `…_research_source_registry_read` | 83 | Fachlogik → **portiert** |
| store_outbound_commands.rs:1277-1481 | Registry: target_list (portiert), account_states, last_runs, latest_script (SQL/Datei → cap.scrape) | 205 | gemischt |
| store_outbound_commands.rs:1483-2083 | Sellify-Lookup komplett (spec, field_rows, groups, snapshot, batch, projection) | 600 | Fachlogik → **portiert** |
| store_outbound_commands.rs:2085-2705 | Adapter-Abgleich `apply_outbound_adapter_reconciliation_reply` 509, `mark_…_failed` 103 | 620 | Fachlogik (Queue-Writeback) → legacy |
| store_outbound_commands.rs:2776-3100 | `outbound_handle_research_source_adapter` 261 + writeback 64 | 325 | gemischt → legacy |
| store_outbound_commands.rs:3101-4253 | Scrape-Wirkung, Testbelege, Generator-Queue, Credential-Status, Registrierung, Ausführung | ~1 150 | Fähigkeit (cap.scrape/cap.browser.adapter) |
| store_outbound_commands.rs:4090-4139 | `outbound_handle_source_reprobe`, Probe-Grant | 50 | gemischt → **portiert** (Grant = Core-Policy) |
| store_outbound_commands.rs:4529-4601 | `outbound_handle_research_policy_publish` | 73 | Fachlogik + cap.scrape → **portiert** |
| person_research_gap_closure.rs (gesamt) | Writeback, Lückenaufgaben, Belegregeln, CRM-Abgleich | 7 305 | Fachlogik (L) → legacy |
| person_research_command.rs (gesamt) | `start` :59, Recovery :106, Outcome-Patch :555, Belegzählung :1640-1765 | 4 418 | gemischt (Web-Stack-Erfassung = cap) → legacy |
| outbound_update_digest.rs (gesamt) | Konfig, Bericht :1290, `tick` :1469, `send_now` :1552 | 2 126 | gemischt (cap.mail.send) → legacy |
| outbound_field_review.rs | Peer-Schreibwächter `peer_lead_write_allowed` :88, Review-Sicht RPC `ctox.outbound.field_review_view.v1` :336 | 1 919 | Fachlogik im Sync-Pfad (offen, §16) |
| outbound_lead_import.rs | MCP-Aktion `outbound.leads.import_and_research` :21/:257 | 824 | Fachlogik → legacy (MCP-Strang) |
| outbound_lead_readback.rs | kompakte Lead-Rücklese für MCP/Mail :95-535 | 576 | Fachlogik (Projektion) → offen |
| rxdb_peer.rs:708, 3113-3135 | RPC `ctox.outbound.sellify_lookup.v1` (DataRead-Prüfung auf `outbound_lead_generation_leads`) | 23 | entfällt → Handler `outbound.sellify.lookup` |
| mcp_channel.rs:5490-5496 u. a. (76 Treffer) | MCP-Aktionsdeskriptoren, Person-Research-Gate :9244 | – | MCP-Strang |
| mcp_app_authority.rs:98, 379 | App-Autorität/Installation | – | Plattform |
| external_sql_sync.rs:396 | `external_sql.write` (Sellify-Übergabe) | – | Fähigkeit cap.sql.external |

## 11. Aufwand: L

Größte Sync-Kopplung aller Kunden-Apps (eigene Lade-/Revisions-Maschinerie, Leases, requireRevision, requestNative),
122 Zustandstexte, 9 wartende Befehlsstellen und ~20 k Zeilen Rust, davon ~12 k Fachlogik der Recherche (Writeback,
Lückenschluss, Belegregeln), die bis zum Port als `cap.legacy.*` weiterlaufen muss.

## 12. Befehlstypen der App (Einstufung, Ziel, Stand)

| Befehlstyp | Sender (index.js) | Rust | Einstufung | Ziel v4 | Stand |
|---|---|---|---|---|---|
| outbound.research_policy.publish | :8842 | store_outbound_commands.rs:4529 | FACHLOGIK + Fähigkeit | Handler + `cap.scrape.upsert_target` | **portiert, 3 Tests** |
| outbound.research_source.registry_read | :9316, :9359 | :1207 | FACHLOGIK über Fähigkeit | Handler + `cap.scrape.{list_targets,last_runs,account_states,latest_script}` | **portiert, 4 Tests** |
| outbound.research_source.reprobe | (MCP-Aktion mcp_channel.rs:5491) | :4090 | GEMISCHT | Handler + `cap.scrape.reprobe_keep_script` | **portiert, 3 Tests** |
| outbound.sellify.lookup (+ RPC ctox.outbound.sellify_lookup.v1) | :9267, RPC :9227 | :1194 → :1673 | FACHLOGIK (E: FAEHIGKEIT/cap.http.fetch – **Korrektur**: liest die replizierten Sellify-Collections, kein HTTP) | Handler über `foreign_collections` | **portiert, 8 Tests** |
| outbound.research_source.upsert | (über :7978 `commandType`) | :2776 next_status active | GEMISCHT | Handler (`_lib/research-source-adapter.js`) + `cap.secret.{scope,exists}` | **portiert** (OLG2) |
| outbound.research_source.generate_adapter | :7978 | :2776 adapter_requested | GEMISCHT | Handler + `cap.scrape.{target_script_registration,upsert_target}`, `cap.agent.task.enqueue_scrape_repair` | **portiert** (OLG2) |
| outbound.research_source.test | :7978 | :2776 test_requested | GEMISCHT | Handler + `cap.scrape.{registered_target_summary,execute_test,run_file}` | **portiert** (OLG2) |
| outbound.research_source.auth_assist | :7978 | :2776 auth_requested | GEMISCHT | Handler + `cap.browser.auth_assist.request` | **portiert** (OLG2) |
| outbound.update_digest.send_now | :2935 | outbound_update_digest.rs:1552 | GEMISCHT | Handler (`_lib/update-digest.js`: Bericht Text+HTML, Korrektur, Status) + `cap.mail.{send,default_sender}`, `cap.scrape.registry_runs`, `cap.runtime_state.{read,write}` | **portiert** (OLG2); `tick` bleibt nativ (kein Befehlstyp) |
| outbound.lead.research_writeback | Vertrag :10519, :10582 (Agent schreibt über MCP) | person_research_gap_closure.rs:1745 | FACHLOGIK | Handler (L) + Capabilities laut §15a | **portiert** (OLG3; `_lib/research-writeback.js` + Teilschritte 1–7, 34 Tests) |
| web_stack.person_research | :5201 (Vertrag) | person_research_command.rs:59 | GEMISCHT | Handler + `cap.web.capture` + Lückenaufgabe (`person_research_gap_closure.rs:755`) | **portiert** (OLG3; `_lib/person-research.js`, 4 Tests; Web-Erfassung von `execute` :2181 über `cap.web.capture` in zwei Phasen, Sellify-Grundlinie in JS). **OLG4:** Phase A als Agentenauftrag (`prompts/person-research.md`, Skill `outbound-lead-generation/person-research`, Rückrufe `web_stack.person_research.done/.failed`), keine Erfassung über den Kern mehr – siehe V4-HANDLERS.md „Strang OLG4“ |
| outbound.research.adapters.reconcile | :9124 (submitTask) | :2085/:2093 (Queue-Antwort), :2602 | FACHLOGIK (Writeback) | Handler (2 Phasen: Auftrag, `command.agent_reply` → Projektion/Fehlerprojektion) + `cap.agent.task.enqueue`, `cap.secret.{scope,exists}` | **portiert** (OLG2) |
| outbound.leads.import_and_research | MCP-Aktion | outbound_lead_import.rs:257 | FACHLOGIK | MCP-Strang (kein Business-Command) | offen, nicht im Manifest |
| ctox.secret.put / .list / .delete | :7257, :7333, :7301 | store.rs:17408/17385/17452 | PLATTFORM | `cap.secret` (Shell/Core) | Plattform, kein Handler |
| business_os.chat.task | :5190, :5641 | Queue (store.rs record_command) | PLATTFORM | `cap.agent.task` | Plattform |
| external_sql.write | :12212 | external_sql_sync.rs:396 | FAEHIGKEIT | `cap.sql.external.write` | Plattform-Fähigkeit |

Zählung: 12 Handler im Manifest = **12 portiert** + **0 legacy** (OLG2 15.10.2026: Legacy 8 → 2; OLG3 15.10.2026:
Legacy 2 → 0). In `cap.web.capture` bleibt nur Web-Erfassung (Web-Stack, Laufzeitquellen, authentifizierte Erfassung);
die Sellify-Grundlinie :2430 läuft in JS zwischen den beiden Phasen. 5 Plattform-/Fähigkeitstypen ohne App-Handler, 1 MCP-Aktion offen.

## 13. Abnahme Nicht-Admin (Owner-Auflage 15.10.2026)

Lehre: ein Umbau der installierten `local-modules/outbound-lead-generation/module.json` (icon_svg entfernt,
`sellify_companies` aus `collections` gestrichen) zerstörte das Logo und sperrte alle Nicht-Admin-Konten mit
„Datenzugriff fehlt“ (`app.js:7699`, `renderModulePermissionDeniedState`). Deshalb: **module.json und
collections.schema.json bleiben unverändert**; das v4-Manifest übernimmt `icon_asset` + `icon_svg` und alle heutigen
Collections inkl. `sellify_companies`. Hinweis: die main-Fassung von `module.json` nennt `sellify_companies` **nicht**;
die installierte Kundenfassung tut es laut Owner-Befund → vor Aktivierung gegen die installierte Datei auf thesen
abgleichen (Diff sichern, nichts überschreiben).

Rechtemodell heute (`policy.rs:312-320` parse_role, `:385-396` role_may_read_collection, `:450-458` DataRead/DataWrite): Rollen `owner|chef → Chef`, `admin`, `founder`, sonst `User`.
`DataRead` = Chef/Admin **oder** Modul dem Akteur zugewiesen (`scope.assigned_to_actor`); `DataWrite` zusätzlich
Datensatz-Besitz. `role_may_read_collection`: User darf alles außer `ADMIN_ONLY_COLLECTIONS` (business_users,
business_module_acl, business_credentials, business_module_source_*, business_module_commits, ctox_runtime_settings)
und Cockpit-Projektionen. `chef` ist in `permissions.js:94` Admin gleichgestellt.

Prüfliste je Rolle (`chef` und `user` mit zugewiesenem Modul; zusätzlich `user` ohne Zuweisung als Negativprobe):

| Prüfpunkt | Nötiges Recht | chef | user (zugewiesen) |
|---|---|---|---|
| Katalogeintrag sichtbar, startbar | kein Recht: lokales Modul ist immer sichtbar (`shared/app-lifecycle.js:343 isLocalModule`); `AppsView` (`policy.rs:439`, User = nein) betrifft Quellansicht/Ändern, nicht den Katalog | ja | ja |
| Icon | `layout.icon_svg` inline **und** `icon_asset` auflösbar (kein Recht nötig; Asset im Artefakt `files`) | sichtbar | sichtbar |
| outbound_lead_generation_sources | DataRead + DataWrite; für `user` nur mit Modulzuweisung (`scope.assigned_to_actor`), sonst „Datenzugriff fehlt“ | r/w | r/w |
| outbound_lead_generation_adapters | read, write (Adapterzustand) | r/w | r/w |
| outbound_lead_generation_imports | read, write (Import) | r/w | r/w |
| outbound_lead_generation_research_policies | read, write (Prompt/Policy) | r/w | r/w |
| outbound_lead_generation_leads | read, write; **DataRead hier ist auch das Tor für den Sellify-Lookup** (rxdb_peer.rs:3119-3124) | r/w | r/w |
| sellify_companies | read (lokale Dublettensuche `index.js:11601`, Lookup) | r | r |
| sellify_people, sellify_campaigns | read (Lookup-Handler) | r | r |
| business_commands | read (Befehlsstatus) | r | r |
| outbound_research_policies | write **durch den Handler** (Core prüft Akteur des Befehls) | w | w (offen: heute nur nativ ohne Rollenprüfung) |
| outbound_research_adapters | write durch die Legacy-Typen | w | w |
| ctox.secret.put/list/delete | `SecretsManage` (nur Chef/Admin!) | ja | **nein** – Zugangsverwaltung der Quellen ist für User heute gesperrt; Owner-Entscheid, ob das so bleiben soll |

Automatische Abnahme (Vorschlag): Stub-Kernel mit Akteur `{role:'chef'}` und `{role:'user', assigned:['outbound-lead-generation']}`:
(1) Katalog enthält App, `layout.icon_svg` nicht leer, `icon_asset` in `files`; (2) `api.query` auf alle 5 eigenen und 3
Sellify-Collections liefert ohne `forbidden`; (3) je Handler ein Aufruf als user → kein `forbidden`; (4) user ohne
Zuweisung → `forbidden` bei (2). Beleg je Rolle als Bildschirmfoto der geöffneten App.

## 14. Capability-Wünsche (an „ctox sync v4“)

| Name | Signatur | Beispielaufruf | Nutzer |
|---|---|---|---|
| `ctx.cap.scrape.upsert_target` | `({manifest, manifest_dir, manifest_file}) → {ok, manifest_path}` – schreibt `runtime/scraping/<dir>/<file>` und führt `scrape upsert-target --input` aus | `await ctx.cap.scrape.upsert_target({manifest, manifest_dir:'app-policy', manifest_file:'outbound-lead-generation-policy.json'})` | research_policy.publish |
| `ctx.cap.scrape.list_targets` | `() → {ok:true, targets:[{target_key, target_id, display_name, status, target_kind, start_url, latest_script_revision_no}]}` (= `scrape list-targets`) | `await ctx.cap.scrape.list_targets()` | registry_read |
| `ctx.cap.scrape.last_runs` | `() → {[target_id]: {last_run:{run_id,status,started_at,finished_at,failure_mode,detail}, last_successful_run|null}}` (SQL aus :1357-1418; Texte auf 300 Zeichen) | `await ctx.cap.scrape.last_runs()` | registry_read, update_digest |
| `ctx.cap.scrape.account_states` | `() → {[target_id]: {status:'provider_account_inactive', reason, causal_run_id, last_probe_run_id, failed_probes, next_probe_at_ms, next_probe_at, probe_in_progress, updated_at_ms}}` (:1306-1355; fehlende Tabelle → `{}`) | `await ctx.cap.scrape.account_states()` | registry_read |
| `ctx.cap.scrape.latest_script` | `({target_key, max_bytes}) → {target_key, available, revision_no?, language?, script_sha256?, created_at?, truncated?, text?, error?}` (:1420-1481) | `await ctx.cap.scrape.latest_script({target_key:'xing', max_bytes:150000})` | registry_read |
| `ctx.cap.scrape.reprobe_keep_script` | `({target_key, probe_input, timeout_s, command_id}) → native Ergebnis` – Probe-Grant aus Befehls-Autorisierung (:4123) im Core | `await ctx.cap.scrape.reprobe_keep_script({target_key, probe_input, timeout_s:90, command_id: command.id})` | reprobe |
| ~~`ctx.cap.scrape.register_target`~~ | **ersetzt (OLG2)**: Registrierung = `cap.scrape.upsert_target` mit `manifest_dir:'outbound-adapters/<key>'`, `manifest_file:'target.json'`; Statusübergänge sind Handler-Logik | – | – |
| `ctx.cap.scrape.registered_target_summary` | `({target_key}) → target | null` (= `scrape::registered_target_summary`) | `await ctx.cap.scrape.registered_target_summary({target_key:'xing'})` | research_source.test |
| `ctx.cap.scrape.target_script_registration` | `({target_key}) → {script_registered, workspace_dir, …} | null` (= `scrape::target_script_registration`) | `await ctx.cap.scrape.target_script_registration({target_key})` | research_source.generate_adapter/test |
| `ctx.cap.scrape.execute_test` | `({target_key, trigger_kind:'manual', timeout_seconds, input, command_id}) → ScrapeExecutionOutcome` (`{run_id, status, records_found, fields_extracted, latency_ms, reason, error, query_completion, run_manifest_path, …}`); Probe-Grant aus der Befehlsautorisierung im Core (:4123) | `await ctx.cap.scrape.execute_test({target_key, trigger_kind:'manual', timeout_seconds:120, input, command_id: command.id})` | research_source.test |
| `ctx.cap.scrape.run_file` | `({path}) → {is_file, sha256, json}` – nur Dateifakten (Run-Manifest, Artefakte); die Belegprüfung (:3562/:3658) liegt im Handler | `await ctx.cap.scrape.run_file({path: outcome.run_manifest_path})` | research_source.test |
| `ctx.cap.scrape.registry_runs` | `() → {last:[{target_key,status,started_at,result_json}], ok:[{target_key,started_at}]}` (die zwei SQL-Abfragen aus outbound_update_digest.rs:362) | `await ctx.cap.scrape.registry_runs()` | update_digest.send_now |
| `ctx.cap.agent.task.enqueue_scrape_repair` | `({channel:'scrape', target_key, title, prompt, thread_key, workspace_root, priority, suggested_skill, parent_message_key, extra_metadata}) → {message_key, route_status}` (= `channels::create_scrape_repair_queue_task`) | siehe `_lib/research-source-adapter.js` | research_source.generate_adapter/test |
| `ctx.cap.browser.auth_assist.request` | wie E §3: `({source_id, start_url, credential_ref?, auth_assist?, requesting_task_id, owner_user_id}) → {request_id, …}` | `await ctx.cap.browser.auth_assist.request({source_id, start_url:url, credential_ref})` | research_source.auth_assist |
| `ctx.cap.secret.scope` / `.exists` | `() → {scope}` (= `credential_scope()`), `({scope, name}) → {exists}` (= `secret_exists`, Fehler → false im Handler); ersetzt den Wunsch `cap.secret.ref` – die Referenz `ctox-secret://<scope>/<name>` baut der Handler | `await ctx.cap.secret.exists({scope, name})` | research_source.*, adapters.reconcile |
| `ctx.cap.mail.send` | wie E §3, hier `kind:'policy_report'` = `channels::record_and_send_policy_report_email` (`{sender_email, to, subject, body, body_html, report_key, policy_summary, idempotency_key}`) → `{status, …}` | `await ctx.cap.mail.send({kind:'policy_report', sender_email, to, subject, body, body_html, report_key, policy_summary, idempotency_key: report_key})` | update_digest.send_now |
| `ctx.cap.mail.default_sender` | `() → {address|null}` (Operator-Umgebung `CTO_EMAIL_ADDRESS`) | `await ctx.cap.mail.default_sender()` | update_digest.send_now |
| `ctx.cap.runtime_state.read` / `.write` | `({name}) → value|null`, `({name, value})` – app-gebundene JSON-Zustandsdatei unter `runtime/` (hier `outbound-update-digest-state.json`, geteilt mit dem nativen `tick`) | `await ctx.cap.runtime_state.read({name:'outbound-update-digest-state.json'})` | update_digest.send_now |
| `ctx.cap.web.capture` | hier zwei Phasen von `execute`: `({kind:'person_research', phase:'research', command_id, payload, client_context}) → result` (= :2181-2244 Payload-/Besitzerprüfung, `run_ctox_person_research_tool`, `research_instructions_len`, `workspace_root`) und `({…, phase:'sources', result}) → result` (= :2259-2409 Laufzeitquellen über `scrape_bridge`, authentifizierte Erfassung :2157 als Befehlsbesitzer, Zusammenfassungstext, `repersist_augmented_person_research`); dazwischen Sellify-Grundlinie in JS | `await ctx.cap.web.capture({kind:'person_research', phase:'research', command_id: command.id, payload, client_context})` | web_stack.person_research |
| `ctx.cap.agent.task.enqueue` | `({instruction, title, priority, suggested_skill, command_id, reply_to_handler}) → {task_id}`; **Rückweg**: der Core ruft denselben Handler erneut mit `command.agent_reply = {task_id, text}` (Antwort) bzw. `{task_id, error}` (Queue-Fehler/Frist, store.rs:15080) | `await ctx.cap.agent.task.enqueue({instruction, suggested_skill:'universal-scraping', command_id: command.id, reply_to_handler: command.type})` | adapters.reconcile; später research_writeback (Lückenaufgaben) |
| `ctx.cap.command.get` | `({command_id}) → {command_type, module, record_id, payload} \| null` (Zeile aus `business_commands`) | `await ctx.cap.command.get({command_id: request.research_command_id})` | research_writeback |
| `ctx.cap.agent.task.get` | `({task_id} \| {command_id}) → {attempt, metadata, workspace_root_rel, route_status_terminal} \| null` (= `channels::load_queue_task` / `load_queue_task_for_business_os_command`; `workspace_root_rel` relativ zum CTOX-Root) | `await ctx.cap.agent.task.get({task_id: request.gap_task_id})` | research_writeback, person_research |
| `ctx.cap.agent.task.workspace_file` | `({task_id, path}) → {canonical, is_file, len}`; `canonical` = kanonischer Pfad relativ zum kanonischen Task-Workspace, `null` außerhalb; Fehler = fehlt | `await ctx.cap.agent.task.workspace_file({task_id, path:'gap_closure/attempts/umsatz/1.json'})` | research_writeback (:3514) |
| `ctx.cap.agent.task.find` / `.enqueue` / `.set_metadata` / `.update` / `.cancel` | `({idempotency_key}) → task\|null`; `({title, prompt, thread_key, workspace_root_rel, priority, suggested_skill, parent_message_key, extra_metadata}) → {task_id, …}` (legt den Workspace an); `({task_id, key, value})`; `({task_id, title, prompt, thread_key, workspace_root_rel, priority}) → {task_id, workspace_root_rel, metadata}`; `({task_id, status_note})` → route_status `cancelled` | siehe `_lib/person-research.js` | person_research (Lückenaufgabe :755, :2121) |
| `ctx.cap.fence.claim` / `.release` | `({key}) → {token} \| null`, `({token})` – Datensatz-Fence statt `ActiveResearchCommandGuard` (prozessweit je record_id) | `await ctx.cap.fence.claim({key:'outbound-research:'+record_id})` | research_writeback, person_research |
| `ctx.cap.outbound.field_status_witnesses.read` / `.issue` | `({record_id}) → [{scope, person_key, field, status}]`; `({record_id, revision, keys:[[scope,person,field]]})` = `NativeFieldStatusWitnesses::persist` mit `Writeback`-Ausgabe gegen den gespeicherten Lead. **Wunsch:** im Core an den Commit von `ctx.transaction` binden (Rust: dieselbe SQLite-Transaktion) | siehe `_lib/research-writeback.js` | research_writeback |
| `ctx.cap.mail.validate_contacts` | `({lead_id})` – startet `spawn_contact_email_validation` (experte.de je Adresse, nativ) | `await ctx.cap.mail.validate_contacts({lead_id})` | research_writeback, person_research |
| `ctx.cap.legacy.<type>` | `(command) → native Ergebnis` – **0 Typen** seit OLG3; `_lib/legacy.js` bleibt als Werkzeug | – | – |

## 15. Handler-Port: Abweichungen und Treue-Notizen

- **Sellify-Lookup**: Rust sucht exakt/ids/contains auch in gelöschten RxDB-Zeilen (`deleted IN (0,1)` bzw. kein Filter,
  store.rs `rxdb_string_field_lookup_sql`); `ctx.read/get` liefern keine Tombstones → gelöschte Zeilen fehlen
  (Sellify-eigenes `is_deleted` bleibt sichtbar). Contains-Vorfilter ist Mango `$regex`+`i`, Nachfilter emuliert SQLite-LIKE
  (nur ASCII-Faltung); numerische Feldwerte (Rust `CAST AS TEXT`) erfasst der Vorfilter nicht. Exakt-Treffer liest alle
  Übereinstimmungen und sortiert/kappt in JS (Rust: SQL `LIMIT`). Gruppierung: nicht-textuelle Gruppenwerte werfen
  (Rust: rusqlite-Typfehler). Ein Lesetest `limit:1` je Collection ersetzt den „required read snapshot“, damit eine
  fehlende Projektion ein Fehler bleibt und nie ein leeres Ergebnis.
- **reprobe**: Die Probe-Grant-Prüfung lag in Rust vor der Payload-Prüfung; in v4 prüft der Core die Capability unter
  `command.actor` → Payloadfehler erscheinen jetzt vor Rechtefehlern.
- **policy.publish**: `_rev`/`_deleted` setzt der v4-Store; `client_context`-Redaktion (store.rs:27329) ist Aufgabe des
  Core-Schreib-Threads. Der Rust-Modulwächter (:36-45, module outbound oder OLG) entfällt – Routing über das Manifest.
- **registry_read**: rein lesend, Reihenfolge der nativen Lesevorgänge und Fehlermeldungen (`.context`) beibehalten.
- Kein Handler verwendet `ctx.transaction` (Schreibvorgänge in Rust-Reihenfolge nach der externen Wirkung; Rust hat dort
  ebenfalls keine Transaktion über business_records + RxDB).
- **research_source.\*** (OLG2, `_lib/research-source-adapter.js`): `upsert_business_record` + `upsert_rxdb_collection_record`
  = ein `ctx.put`. Laden nur über `ctx.get` (Rust: business_records, dann RxDB). ABWEICHUNG v4-Vertrag: v4-Befehle tragen
  kein `client_context`; ein optionales `command.client_context` wird gelesen, sonst stehen `command.actor.id` (Owner der
  Anmeldehilfe) und `command.app_id` (Quellmodul des Writebacks, `source_module` der Anmeldehilfe – Rust nutzte
  `command.module`, die UI sendet dort `'outbound'`) ein; `campaign_id` aus `client_context` entfällt dann. Latenzen über
  `ctx.now()`-Differenzen (Rust: `Instant`). RUST-EIGENHEITEN markiert: `swap_remove` von `task_id` im Generator-Prompt
  (serde_json `preserve_order` → letzter Schlüssel rückt nach), unerreichbarer `!has_script`-Zweig (:3290), Debug-Format
  `["a", "b"]` in `last_error`, ungetrimmte `requesting_task_id`.
- **adapters.reconcile** (OLG2): zwei Phasen in einem Handler (Auftrag / `command.agent_reply`). Validierung vor dem ersten
  Schreiben wie Rust; Adapter-`target_key`/`id` werden wie in Rust erst in der Projektionsschleife geprüft (Teilschreiben
  möglich, dann Fehlerprojektion). Fehlerprojektion bei Queue-Fehlern wirft danach den Queue-Fehler.
- **update_digest.send_now** (OLG2, `_lib/update-digest.js`): Zeitzonen über `Intl.DateTimeFormat` (Rust chrono-tz;
  IANA-Namen in Intl ohne Groß/Klein-Unterscheidung), RFC-3339 per Regex + `Date.parse`, Sortierung in Code-Point-Ordnung
  (= Rust-Bytefolge). Leseordnung der Datensätze wie `ctx.read` liefert (Rust: SQLite-Zeilenfolge) – wirkt nur auf
  Gleichstände nach stabiler Sortierung. Der geplante Versand `tick` (outbound_update_digest.rs:1469) ist **kein**
  Befehlstyp, bleibt nativ und teilt die Zustandsdatei über `cap.runtime_state` → Folgekarte: als `every: PT1M`-Trigger
  (§16.20) portieren.

### 15a. Restport research_writeback + web_stack.person_research (gekoppelt, offen)

**OWNER-REGEL-Befund (Kundeninstanz):** Der Rust-Stand 4794af03d setzt `required_independent_sources(field) = 1` für **jedes**
Feld (person_research_command.rs:1670, Kommentar „Owner rule 23.09.2026: one matching, cited source is enough for every
field“). Die Formulierung im Auftrag „Ziel 2 Quellen je Feld, 1 nur im Notfall“ (Owner-Regel 07.10.2026) steht **nicht**
im Code dieses Stands. Portiert ist wörtlich der Rust-Stand (1); Klärung beim Owner/Recherche-Strang, ob die Regel vom
07.10. noch umgesetzt werden muss – nicht Teil dieses Umbaus (§16.19 Nr. 9).

Portiert (Teilschritte mit Tests): 1 Belegregeln `_lib/research-evidence.js` (Quellenzählung je Anbieter, Sellify allein = 0,
Zitatprüfung Zahl/E-Mail/Name, Deduplizierung, Abgleich Wert+Person+Zitat); 2 Prüfregeln `_lib/research-validation.js`
(CRM-Abgleich auf Token-Ebene, Quell-URL, reservierte Hosts, no_match-Versuche, Versuchsform); 3 tolerantes Einlesen
`_lib/research-writeback-payload.js` (Träger entpacken, Hoist, lenient_*, serde-Fehlertexte „missing/unknown field“,
`FieldKey::from_str` aus dem Klon `src/tools/web-stack/src/sources/mod.rs`, `swap_remove`-Reihenfolge); 4a Vorprüfungen
`_lib/research-writeback-checks.js` (derive_result_fields, evidence-free-Ablehnung, Feldschlüssel exakt, result-Form,
Konsistenz result↔field_status, Füllstatus); 4b `CrmBaseline` in `_lib/research-validation.js` (Sellify-Zitatprüfung,
zustimmende Quelle, Hinweistext).

**Befund (Folgekarte, nicht gefixt):** `hoist_top_level_field_entries` behandelt `result.custom_fields` als
Streuschlüssel und schiebt es nach `result.fields.custom_fields`; `request.result.custom_fields` ist damit immer `{}` und
`apply_custom_field_results` (:2189) sieht im echten Writeback nie ein Feld (der Rust-Test ruft die Funktion direkt).
Ein `custom_fields` auf oberster Ebene scheitert an `deny_unknown_fields`. Treu portiert und markiert.

Stand OLG3 (15.10.2026, alle Teilschritte mit Tests, `node --test handlers/tests/*.test.mjs` 81/81 grün):
4. ~~Rest Bereinigung/Prüfungen~~ – erledigt: `_lib/research-writeback-sanitize.js` (sanitize :2434 Zeile für Zeile,
   validate_terminal_field :3096, validate_artifact_path :3514 über `cap.agent.task.workspace_file`,
   action_required_has_auth_reference :3562, add_field_status_evidence :3765, phase_a_* :3850-3970, Prüfungen
   :2357/:2395, task_workspace :3950 relativ zum Root), 5 Tests.
5. ~~Lead-Ergebnispatch~~ – erledigt: `_lib/research-outcome-patch.js` (:555-2005 ohne Belegregeln aus Nr. 1; inkl.
   `role_is_valid` aus web-stack `person_ranking.rs` und `is_email_verdict`), 4 Tests.
6. ~~Feldstatus-Zusammenführung~~ – erledigt: `_lib/research-field-status.js` (:948-1534, `native_review_view_from_witnesses`,
   `is_refuted_no_match`, E-Mail-Urteile invalidate/restore/align aus contact_email_validation.rs), 4 Tests.
7. ~~Handler `outbound.lead.research_writeback`~~ – erledigt: `_lib/research-writeback.js`; optimistisches Schreiben in
   `ctx.transaction` (aktueller Lead == `expected_master`, sonst `conflict`), Zeugen-Ausgabe danach über
   `cap.outbound.field_status_witnesses.issue`, Fence je record_id, 4 Tests.
8. ~~`web_stack.person_research`~~ – erledigt: `_lib/person-research.js` (start/spawn_worker/persist/projection,
   Lückenaufgabe :755 mit wörtlichem Prompt :655 – per Skript byte-gleich zum Rust-Formatstring geprüft), 4 Tests.
8b. ~~Sellify-Grundlinie~~ – erledigt: `merge_sellify_baseline_evidence` :2430, `sellify_fuzzy_company_probe` :2511,
   `inject_sellify_candidates` :2535 in JS über `_lib/sellify-lookup.js`, zwischen den zwei `cap.web.capture`-Phasen
   (Rust-Reihenfolge: nach `workspace_root`, vor den Laufzeitquellen).

Offen:
9. `recover_once` :106 (Wiederaufnahme nach Daemon-Neustart, abgeschlossene Phase A ohne Lückenaufgabe) und
   `complete_after_native_email_validation` :1552 (Abschluss nach nativer E-Mail-Prüfung) bleiben nativ → Trigger-Karte.

Treue-Notizen OLG3 (RUST-EIGENHEITEN markiert im Code):
- person_records-Belege übernehmen `field_key` nur aus `field` (:725), nie aus `field_key`.
- `invalidate_changed_email_addresses` legt per IndexMut `person_field_status: null` an, wenn eine Adresse wechselte.
- Gap-Prompt und Writeback-Vertrag nennen „zwei unabhängige Hosts“, der Code verlangt 1 (Owner-Regel) – Text unverändert.
- Ohne Fence startet person_research keinen Worker und endet mit dem „running“-Ergebnis (`worker_started:false`).
- `merge_research_into_known_contact`: ein gesetzter Alias (`phone`, `email`, `role`, …) verhindert das Füllen des
  `person_*`-Felds des Sellify-Kontakts (:1089).
- Zusammenfassungstext „Recherche für … abgeschlossen“ (:2395) bleibt in Phase `sources`, weil `repersist` ihn mitschreibt.
ABWEICHUNGEN v4: kein Hintergrund-Thread (Kernel-Status „running“ bis Handler-Ende); Workspace-Pfade relativ zum
CTOX-Root; `native_review_view` legt fehlende Schlüssel nicht als null an (reine Ansicht); Zeugen-Ausgabe nach dem
Commit statt in derselben Transaktion (fail-safe, Wunsch §14).

## 16. Fallen und offene Punkte

1. **Generator löscht dieses Manifest**: `gen-manifest.mjs` ohne `--check` entfernt jede `*.manifest.json`, die er nicht
   selbst erzeugt (`gen-manifest.mjs:784-788`), also auch `outbound-lead-generation.manifest.json`. Vor dem nächsten
   Generatorlauf `customer-modules/` in den Generator aufnehmen (anderer Strang) oder das Manifest sichern.
2. **Kollision outbound_lead_generation_adapters**: browser und OLG definieren dieselbe Collection, gleiche Version 2,
   unterschiedliches Schema; `browser.manifest.json` führt sie heute als browser-eigen. Eigentümer-Entscheid nötig
   (Vorschlag: OLG, browser als `foreign_collections`).
3. **Sellify-Collections ohne Eigentümer-App**: Projektion der externen SQL-Quelle (external_sql_sync.rs); `owner_app: null`.
4. **outbound_research_policies** existiert nur nativ in `business_records` – kein Schema, kein Eigentümer.
5. **Peer-Schreibwächter** `outbound_field_review.rs:88 peer_lead_write_allowed` filtert Browser-Schreibvorgänge auf
   Leads (native field_status, kein `failed` über recherchierte Leads). In v4 gibt es dafür keinen Ort; App-Logik im
   Sync-Pfad ist nach §16.13 verboten → Wunsch: deklarativer Schreibwächter je Collection im Manifest **oder** alle
   Lead-Schreibvorgänge der UI als Befehle. Bis dahin blockierend für die Lead-Collection.
6. RPC `ctox.outbound.field_review_view.v1` (outbound_field_review.rs:336-345) wird von der App heute nicht genutzt (kein
   Treffer in App-Code); Nutzer klären, sonst streichen.
7. Zwei Apps senden dieselben Typen (`outbound.research_source.*` mit module `outbound`); Handler-Routing je Typ muss
   eindeutig sein.

### 16a. Nötige Manifest-Änderung (nicht in diesem Strang – `module.json`/Manifest außerhalb der Schreibfreigabe)

`v4/manifests/outbound-lead-generation.manifest.json` führt noch alle 8 `cap.legacy.*`. Für den Stand OLG2 (+ OLG3 unten):
- `capabilities_required`: entfernen `cap.legacy.outbound.research_source.{upsert,generate_adapter,test,auth_assist}`,
  `cap.legacy.outbound.update_digest.send_now`, `cap.legacy.outbound.research.adapters.reconcile`; ergänzen
  `cap.scrape.{registered_target_summary,target_script_registration,execute_test,run_file,registry_runs}`,
  `cap.agent.task.{enqueue,enqueue_scrape_repair}`, `cap.secret.{scope,exists}`, `cap.browser.auth_assist.request`,
  `cap.mail.{send,default_sender}`, `cap.runtime_state.{read,write}`; `_legacy_open` auf die 2 offenen Typen kürzen.
- `foreign_collections.outbound_research_adapters.ops`: `['read','write']` (Handler liest den Datensatz vor dem Schreiben).
- Die Handler-Pfade bleiben unverändert (gleiche Dateinamen).
- OLG3: zusätzlich `cap.legacy.outbound.lead.research_writeback` und `cap.legacy.web_stack.person_research` entfernen,
  `_legacy_open` leeren; ergänzen `cap.command.get`, `cap.agent.task.{get,find,set_metadata,update,cancel,workspace_file}`,
  `cap.fence.{claim,release}`, `cap.outbound.field_status_witnesses.{read,issue}`, `cap.mail.validate_contacts`,
  `cap.web.capture`; `outbound_lead_generation_leads` wird per `ctx.transaction` geschrieben (eigene Collection).

## 17. Dateien dieses Strangs

- `handlers/_lib/olg.js`, `handlers/_lib/legacy.js`, `handlers/_lib/sellify-lookup.js`,
  OLG2: `handlers/_lib/research-source-adapter.js`, `handlers/_lib/update-digest.js`, `handlers/_lib/research-evidence.js`,
  `handlers/_lib/research-validation.js`, `handlers/_lib/research-writeback-payload.js`, `handlers/_lib/research-writeback-checks.js`
- portiert: `handlers/outbound.research_policy.publish.js`, `handlers/outbound.research_source.registry_read.js`,
  `handlers/outbound.research_source.reprobe.js`, `handlers/outbound.sellify.lookup.js`,
  OLG2: `handlers/outbound.research_source.{upsert,generate_adapter,test,auth_assist}.js`,
  `handlers/outbound.update_digest.send_now.js`, `handlers/outbound.research.adapters.reconcile.js`
- OLG3: `handlers/_lib/research-writeback-sanitize.js`, `handlers/_lib/research-outcome-patch.js`,
  `handlers/_lib/research-field-status.js`, `handlers/_lib/research-writeback.js`, `handlers/_lib/person-research.js`;
  portiert `handlers/outbound.lead.research_writeback.js`, `handlers/web_stack.person_research.js`
- legacy: keine (0)
- Tests: `handlers/tests/_ctx.mjs` (In-Memory-ctx, + `$regex`), 18 Testdateien, 81 Tests (OLG3):
  `node --test src/apps/business-os/customer-modules/outbound-lead-generation/handlers/tests/*.test.mjs`
- Manifest: `src/apps/business-os/v4/manifests/outbound-lead-generation.manifest.json`

## 18. UI-Port (Strang OLGUI, 15.10.2026) – abgeschlossen

**Messwerte** (Klon `~/Documents/ctox-shell-v4`, Stub-Pin `~/.local/state/shell-v4/stub-pins/9fa9948`):

| Prüfung | vorher | nachher |
|---|---|---|
| `node v4/tools/port/scan.mjs --modules customer-modules --app outbound-lead-generation` | 332 Stellen, CSS 661 ungescoped | **0 Stellen, CSS 0** |
| `node v4/tools/guards/guard-v4.mjs --json --app customer-modules/outbound-lead-generation` | 168 Treffer (UI) | **0** (handlers/ eingeschlossen) |
| `scope-css.mjs … index.css --check` | – | gescoped |
| `CTOX_APP_API_STUB=… node --test …/tests/v4-api.test.mjs` | – | **9/9 grün** |
| `node --test handlers/tests/*.test.mjs` (unberührt) | – | 97/97 grün |
| Bildschirmfotos `tests/shots/olg-1280.png`, `olg-420.png` | – | angesehen: 1280 dreispaltig, 420 gestapelt ohne Querüberlauf |

**Umbau**
- `v4-port.mjs` (neu, app-lokal): `daten(name)` mit `eins/alle/jetzt/schreibe/aendere/entferne/abonniere`; gelesene Zeilen
  sind Kopien (App-API-Zeilen sind tief eingefroren, der Fachcode mutiert Leads) mit unsichtbarem Collection-Bezug, damit
  `aendereDok(doc, patch)` die Stelle von `doc.incrementalPatch` einnimmt. **Entscheidung:** `aendere` = flaches
  Zusammenführen + `api.put` (Semantik von RxDB `incrementalPatch`), **nicht** `api.patch` (RFC 7396 mischt tief und löscht
  bei `null` – hätte `payload` der Leads verändert). `befehl()` = `api.command(type, {...payload, command_id, record_id,
  client_context, module}, {record_id})`, Status gepusht, `failed` → genau ein Eintrag `outbound-lead-generation.command.<typ>`,
  `done` räumt ihn. `{ergebnis:true}` (Lookups, Secret-Katalog, Registry, Sellify-Schreiben, Update-Vorschau) wartet auf den
  gepushten Endstand – ohne Frist und ohne Polling. `auftrag()` ersetzt `businessChat.submitTask`; `abbrechen()` nutzt den
  Plattformbefehl `ctox.command.cancel {target_command_id, reason}` (wie shared/command-bus.js). Dialoge über `api.shell.dialog`.
- index.js: `mount(root, api, {restored})` → `{unmount, snapshot}`; Titel `api.shell.setTitle`; Locale live über
  `api.shell.locale`; je Collection ein Live-Abo (`bindCollections`), Kopie nur geänderter Zeilen (Referenzvergleich).
  Entfallen: Start-/Readiness-/Lease-/Revisions-/Startcache-Maschinerie (synchronizeInitialData, retryInitialSync,
  flushReplicatedCollection, Kanal-Selbstheilung, requestNative-Direktweg, `withTimeout`, Sync-Fußzeile, `ensureStyles`).
  Startpflege (seed, dedupe, repair, reconcile, Länderkorrektur, Sperrvermerke) läuft einmal nach dem ersten Stand.
  Die 5-s-Abgleiche und der 20-s-Freitexttakt laufen jetzt auf Änderungen an `business_commands` (entprellt 5 s),
  die Kampagnenpersonen-Pflege 20 s nach der letzten Lead-Änderung. Befehlsstatus lokal über `business_commands`
  (`befehlsDokument`, Suche nach eigener ID oder `payload.command_id`; Statusabbildung done→completed).
- Zustandstexte: Lade-/Leer-/Synchronisierungstexte entfernt (leere Collections rendern nichts); `zeigeHinweis` und
  Aktionsfehler gehen in die Zustandszone (`outbound-lead-generation.hinweis`, `….action.<aktion>`). Fachstatus-Beschriftungen
  (Recherche „Läuft/Wartet“, Übergabe, Belege, Filterreiter, Agenten-Prompts, Excel-Zellen) mit `// v4-allow: label` markiert.
- Browserspeicher (Sperrvermerk-Urteile, Vorabgleich-Versuche, Registry, Detailreiter) → Zustand der Montage bzw.
  `snapshot()`; `sessionStorage`+`location.hash` → `api.shell.navigate('ctox', {task_id, command_id, …})`.
  Browser-Anmeldung → `api.shell.navigate('browser', {title, mode, args})`, Spreadsheet-Öffnen → `navigate('spreadsheets', …)`.
- CSS: `scope-css.mjs --write`, danach Wurzelselektoren auf das Root selbst gezogen
  (`[data-app="outbound-lead-generation"].outbound-lead-generation`, 99 Stellen) – sonst fehlten Raster und Container
  `business-app-window` (gemessen: Layout gestapelt bei 1280). Schmale Leadlisten-Aktionen einzeilig scrollbar.
- Importe: `ctox:shared/universal-importer.js` (ohne `?v=`); jszip über `./vendor/jszip/jszip.mjs` (liegt vor).
- Gelöscht: `collection-reloader.mjs`, `lead-list-loader.mjs`, `lead-revision-loader.mjs`, `startup-cache.mjs`,
  `read-error-grace.mjs`, `schema.js` (§8: mit collections.schema.json kanonisch gleich).
- Fachstand main c9a99316e (#775) übernommen: Detail-Scroll im Stapel-Layout (`stacked-detail-reveal.mjs`),
  Doppel-Adapter entfernen (`adapter-dedupe.mjs`, Startpflege), „Quelle entfernen“ für alle Quellen (mitgeliefert →
  `payload.removed`, seedSources überspringt), Adapter-Einzellöschung und Sammelfeld Empfängersperren entfernt, CSS
  Namenszelle. Die client-first-Leselogik aus #775 ist durch die Live-Abos ohnehin erfüllt.
- Schreib-Bestätigung beim Löschen einer eigenen Quelle: statt `flushReplicatedCollection` wartet der Zugangs-Löschschritt
  auf `WriteHandle.result` (Core-Annahme), sonst Zoneneintrag `outbound-lead-generation.source.delete`.

**OFFEN (UI-Port)**
1. ~~`scripts/tests/outbound-*` importieren gelöschte v3-Module~~ → **erledigt OLG2 (§19)**: 5 gelöscht, 4 umgeschrieben,
   1 neu, 22/22 grün.
2. `v4/manifests/outbound-lead-generation.manifest.json` (`files`) nennt noch `schema.js`/Loader und nicht `v4-port.mjs`,
   `stacked-detail-reveal.mjs`, `adapter-dedupe.mjs`, `tests/` → Manifest-Strang.
3. `shared/universal-importer.js` ist noch v3 (`ctx.host/db/commandBus`); OLG übergibt nur `{host: root}` mit
   `dispatch:false`, `showFileExplorer:false` → Shared-Strang.
4. Kein Vorab-Browserfenster mehr bei `auth_assist` (4.0 hat keine Fensterliste/kein gezieltes Schließen): die Browser-App
   öffnet erst mit der Sitzung aus dem Befehlsergebnis. ~~Parameterform von `navigate` abgleichen~~ → **erledigt OLG2 (§19)**;
   die toten Stummel `preopenBrowserWindow`/`closePreopenedBrowserWindow` samt `preopened`-Durchreichung gelöscht.
5. Befehle ohne Frist: ein nie beantworteter Lookup bleibt offen (früher Fehler nach 20–120 s). Bewusst (R5/R12); falls
   nötig, Abbruchwunsch an den Kern (CommandState `expired`).
6. Modulzustand ist modulweit (wie knowledge OFFEN-4): zwei gleichzeitige Fenster teilen `state`; `unmount` räumt nur die
   eigenen Abos (Port je Montage). **Manifest-Wunsch (an den Manifest-Strang): `layout.multi_instance: false`** – die Shell
   darf OLG nur einmal öffnen; ein zweites `navigate('outbound-lead-generation', …)` fokussiert das bestehende Fenster.
   Stand 15.10.: `v4/manifests/outbound-lead-generation.manifest.json` trägt `layout.presentation.multi_instance: false`
   (aus module.json übernommen); der Manifest-Strang entscheidet, ob der Schlüssel dort bleibt oder nach `layout.multi_instance`
   wandert – maßgeblich ist, dass die Shell ihn auswertet.
7. ~~Registry über `registry_read`~~ → **erledigt OLG2 (§19)**: Live-Lesen der `outbound_lead_generation_scrape_*`-Collections.
8. Im Stub liefert `outbound.sellify.lookup` kein Handler-Ergebnis → Konsolenhinweis „Sellify lieferte kein gültiges
   Ergebnis“ im Test (erwartet; Handler-Strang hat eigene Tests).

## 19. Strang OLG2 (15.10.2026): Alt-Tests, Registry live, navigate-Form, Modulzustand

**Messwerte** (Klon `~/Documents/ctox-shell-v4`, Stub-Pin `~/.local/state/shell-v4/stub-pins/9fa9948`):

| Prüfung | vorher | nachher |
|---|---|---|
| `node --test scripts/tests/outbound-*.test.mjs` | 22 Tests, 15 grün, 7 rot (13 Dateien) | **22/22 grün** (9 Dateien) |
| `CTOX_APP_API_STUB=… node --test …/tests/v4-api.test.mjs` | 9/9 | **12/12** (+ Registry live, navigate Browser, navigate Spreadsheets) |
| `node --test handlers/tests/*.test.mjs` (unberührt) | 97/97 | 97/97 |
| `scan.mjs --modules customer-modules --app outbound-lead-generation` | 0 | **0** |
| `guard-v4.mjs --json --app customer-modules/outbound-lead-generation` | 0 | **0** |

**1. `scripts/tests/outbound-*.test.mjs`**

| Test | Ergebnis | Grund |
|---|---|---|
| `outbound-lead-list-loader` | gelöscht | Modul `lead-list-loader.mjs` gelöscht (OLGUI); Projektion/200er-Fenster/Lease-Autorität gibt es in 4.0 nicht – `api.query` ist lokal und vollständig |
| `outbound-lead-revision-loader` | gelöscht | Modul gelöscht; Revisions-Manifeste ersetzt durch Live-Abo (`bindCollections`, Referenzvergleich) |
| `outbound-read-error-grace` | gelöscht | Modul gelöscht; Lesefehler-Schonfrist gehörte zur Sync-Fußzeile (entfallen, R11/R13) |
| `outbound-startup-cache` | gelöscht | Modul gelöscht; Startcache entfällt, der lokale Stand ist in 4.0 sofort da |
| `outbound-scoped-reload` | gelöscht, Fachteile umgeschrieben | 18 von 22 Fällen prüften `collection-reloader.mjs`/Kanal-Wiederherstellung/Lease (gelöscht); Vorab-Browserfenster-Test entfällt mit dem Vorab-Fenster. Fachregeln weiter geprüft: Pflichtfeld-Häkchen und 139-Lead-Export → neu `outbound-app-actions.test.mjs`; Sync-Profil/Schema v0 → `outbound-lead-sync-profile` |
| `outbound-app-actions` | neu (aus scoped-reload) | echte index.js: Pflichtfeld-Häkchen/abgelehntes Speichern/Neuladen; Export ohne Befehl, Schnappschuss vor dem await, Download vor Übergabe, `api.files.put` → `navigate('spreadsheets', {file_ref,…})`, Empfängerfehler verwirft den Download nicht |
| `outbound-import-app` | umgeschrieben | Fixture kopiert die v4-App, lenkt `ctox:shared/` um; Schreibsonden auf Port-Datenobjekten (`schreibe/aendere/entferne`); Fachregeln (Gruppenwahl, 5.000-Grenze, keine Schreibung vor Prüfung, Dedupe, Sellify-Zeilen) unverändert |
| `outbound-lead-sync-profile` | umgeschrieben | `schema.js` gelöscht → prüft `collections.schema.json` (eager, Profil neben dem Schema, Schema v0 = Fixture) |
| `outbound-current-export` | umgeschrieben (letzter Fall) | `openResearchSnapshot` neue Aktionen `{ablegen, navigate}` statt `openApp`; übrige 3 Fälle unverändert grün |
| `outbound-field-catalog`, `outbound-import-groups`, `outbound-in-flight-lead-sweep`, `outbound-reconcile-basis`, `outbound-required-fields` | unverändert grün | prüfen weiter geltende app-lokale Fachmodule |

**2. Registry direkt lesen.** `ladeQuellenRegistry` faltet jetzt `outbound_lead_generation_scrape_targets/_runs/_account_states`
lokal (Live-Abo in `bindCollections`, Mikrotask-entprellt, Nachlauf bei Änderung während des Lesens) zum selben Eintrag wie
`handlers/outbound.research_source.registry_read.js handle()`; der Skript-Inspektor liest `_scrape_scripts` + `api.files.get`
(`latestScript`, 150-KB-Kappung samt Kürzungsvermerk) und lädt bei Änderungen an `_scrape_scripts` neu. Die Lese-Sichten
(`listTargets`, `lastRuns`, `accountStates`, `latestScript`) importiert die UI aus `handlers/_lib/scrape-registry.js` über einen
App-API-Kontext (`registryKontext`) – eine Fachlogik für Handler und UI. Das 5-Minuten-Nachladen beim Öffnen der Quellen entfällt.
**Befehl:** Die App setzt `outbound.research_source.registry_read` nicht mehr ab; kein anderer OLG-Handler ruft ihn
(geprüft: `grep registry_read handlers/` = nur der Handler selbst + seine Tests). Löschen von Handler, Tests, `mutation-proof-scr.mjs`-
Eintrag und Manifest-`handlers`-Eintrag gehört dem Handler-/Manifest-Strang (außerhalb dieser Schreibfreigabe); `modules/outbound`
hat einen eigenen Handler gleichen Typs (Routing-Doppel, `_todo` im Manifest).

**3. navigate-Form.**
- Browser: `modules/browser/index.js` `browserRouteArgs(params)` nimmt `params.args` oder `params` selbst, `browserAuthRequestFromArgs`
  braucht `session_id` (`browser_session_…`), `purpose: 'web_stack_auth'`, `target_url`. OLG schickt jetzt die Startargumente flach
  (`navigate('browser', args)`); `title`/`mode` waren v3-Fensterhinweise. Vorher funktionierte es nur über den Träger-Rückfall.
- Spreadsheets: `modules/spreadsheets/index.js handleRoute` erwartet `file_ref|ref` (+ `name`, `mime`) und die Herkunft
  (`source_kind`, `open_purpose`, `report_snapshot`) **flach**. OLG schickte `{openFile: {file: File, …}}` → **echter Fehler in OLG**:
  Spreadsheets ignorierte den Aufruf (kein `file_ref`), und ein `File` ist kein Routenparameter. Jetzt: `api.files.put(file)` →
  `ref`, dann `navigate('spreadsheets', {file_ref, name, mime, source_kind, open_purpose, report_snapshot})`; `file_sha256` passt
  zu den abgelegten Bytes (`assertSpreadsheetSnapshotBytes`). Ziel-Apps unverändert, dort kein Fehler gefunden.
- `navigate('ctox', …)` (Aufgabenfokus) nicht Teil dieses Auftrags, ungeprüft.

**4. Modulzustand:** Manifest-Wunsch `layout.multi_instance: false` in OFFEN 6 eingetragen.

**Geänderte Dateien:** `index.js` (gezielte Edits: Registry-Import/-Lesen/-Abo, Browser-navigate, Export-Übergabe,
Vorab-Fenster-Stummel gelöscht, Test-Hooks `openSourceAuthorization`/`ladeQuellenRegistry`/`ladeAdapterSkript`),
`current-state-export.mjs`, `tests/v4-api.test.mjs`, `README.md` (Regressionsliste), `V4-PORT.md`;
`scripts/tests/outbound-{import-app,lead-sync-profile,current-export}.test.mjs` umgeschrieben, `outbound-app-actions.test.mjs` neu,
5 Alt-Tests gelöscht. Nicht angefasst: `handlers/`, `module.json`, `collections.schema.json`.
**Offen außerhalb der Freigabe:** `v4/debt/LOESCHPLAN.md:486` nennt noch `outbound-scoped-reload.test.mjs`.

