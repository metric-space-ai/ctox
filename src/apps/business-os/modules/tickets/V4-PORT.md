# tickets – Port auf App-API 4.0 (Strang UC1, Welle C)

Fortschritt (Berlin-Zeit; „ca.“ = geschätzt, sonst gemessen mit `TZ=Europe/Berlin date`):
- ca. 14:17 Start; Scanner vorher: **56 Stellen**, CSS 99 Selektoren ungescoped (20 mit Shell-Vorfahr). V4-HANDLERS.md (Strang H8) gelesen.
- ca. 14:21 Port index.js/index.html/locales/index.css, schema.js gelöscht; Bestandstests auf v4 umgestellt.
- 14:23 (gemessen) v4-api.test.mjs gegen Stub-Pin 67229d2: 11/11 grün; Screenshots 1280/420 angesehen.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app tickets` | 56 → **0 Stellen**, Exit 0 |
| CSS-Scope | `node v4/tools/port/scope-css.mjs tickets modules/tickets/index.css --check` | gescoped |
| guard-v4 | Treffer in `modules/tickets/` außer `handlers/` | **0** |
| v4-Test gegen Kern-Stub | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/67229d2 node --test src/apps/business-os/modules/tickets/tests/v4-api.test.mjs` | **11/11 grün** |
| Bestandssuite | `cd modules/tickets && node --test tickets.test.mjs && node tests/tickets-module-smoke.mjs` | 15/15 grün + Smoke OK (esbuild/RxDB-Attrappen ersetzt durch Node-Auflösung `ctox:shared/`) |
| Screenshots | `tests/shots/tickets-1280.png`, `tickets-420.png` (drei Panes gestapelt), `tickets-420-detail.png` (Aktionen-Pane zu) | angesehen: kein horizontaler Überlauf, Köpfe vollständig |

Abgedeckt vom v4-Test: mount zeigt Live-Daten, Band-Zählung, erstes Ticket ausgewählt (Fachverhalten), Verlauf und
auto-offene Aktionen-Pane (Fall `approval_pending`) · `sim.push` live + Löschung · leere Collections: Liste, Bühne,
Aktionen leer, kein Zustandskasten, keine Zone · `ctox.ticket.approve` legt `business_commands`-Dokument an (`type`,
`app_id`, `record_id`=case_id über opts, Payload `case_id/status` + `client_context.actor` aus `api.actor`),
`sim.failCommand` → genau ein Zoneneintrag · Prompt-Aktionen über `api.shell.dialog` (Stub-Default → kein Befehl;
mit Antwort → `ctox.ticket.local.create`) · Deep-Link über `api.shell.onRoute` (`record: case_1` fokussiert Ticket +
Fall; unbekannt → Zone `tickets.route.unknown_record`) · Import ungültig → Zone, gültig → `local.create` · `api.put` →
accepted, live · snapshot/restored: Auswahl, Suche, offenes Filterfach · unmount löst alle 14 Abos, leert root.

## Was sich geändert hat

- **Einstieg** `mount(root, api, {restored})` → `{unmount, snapshot}`; Abos je Montage.
- **Daten**: 12 `api.query(name, {})` + zwei begrenzte Crew-Abfragen (`ctox_queue_tasks` `{ticket_key:{$gt:''}}`
  limit 200, `ctox_crew_members` `{archived:false}` limit 64 – gleiche Selektoren/Grenzen wie vorher) statt
  `ctx.db…find().exec()`, `$.subscribe({invalidateOnly})`, `collectionReadiness`/`subscribeCollectionReadiness`,
  `startCollection('business_commands')`, `loading`-Zustand, `resolveTicketListState`.
- **Befehle**: `api.command(type, {...Handler-Payload, client_context:{source, module_id, actor}}, {record_id})`, nie
  warten (vorher `commandBus.dispatch(…, {until:'accepted'})` + Neuladen). `client_context.actor` kommt aus
  `api.actor` (`id`, `display_name`, `role`; das frühere `is_admin` aus der Sitzung gibt es in 4.0 nicht – entfällt).
  `reviewed_by` = `api.actor.id`. Crew-Zuweisung `ctox.crew.assign`: Erfolg still, Fehler → Zone + Auswahl wieder frei.
- **Dialoge**: `showBusinessPrompt` (shared/dialogs.js, Shell-Ebene) → `api.shell.dialog({kind:'prompt', title, text,
  default, confirm_label, cancel_label})` im App-Fenster (Form offen, OFFEN-A); `null` = Abbruch wie bisher.
- **Rechte**: `canUseBusinessPermission({session, governance, CrewManage})` → Spiegel aus `api.actor`:
  `permissions` enthält `ctox.crew.manage` oder Rolle `owner|admin|chef` (entspricht `roleCanManage`). Entscheidung im Core.
- **Deep-Link**: `ctx.args` + DOM-Ereignis `ctox-business-os-app-launch` → `api.shell.onRoute` (`params.record |
  record_id | case_id | return_thread_id`). Unbekanntes Ziel → Zone (statt Statuszeile, R13).
- **Zustandstexte** raus: Statuszeile `[data-ticket-command-status]` („Aktion wird ausgeführt…“, „Aktion
  abgeschlossen.“, „Verknüpftes Ticket geöffnet.“, „Importiert: n“, Fehlertexte), „Tickets werden geladen…“,
  „… synchronisiert“, Leer-/Auswahlhinweise in Liste und Bühne, Leerzeilen der Abschnitte („Noch keine Ereignisse.“,
  „Kein Vorgang …“, „Noch kein Crew-Auftrag …“, „Keine offenen Rückfragen.“, „Keine Arbeitsanleitungen.“ – leere
  Abschnitte entfallen jetzt ganz), „Zugewiesen.“. In die Zone: `tickets.command.<type>`, `tickets.action.failed`,
  `tickets.route.unknown_record`, `tickets.import.invalid|empty`, `tickets.export.failed`.
- **Wortwahl wegen guard-v4-Vokabular**: Status-Filter `pending` „Wartet“ → „Wartend“ (wie das Band); Crew-Sätze
  „wartet auf Rückmeldung“ → „Rückmeldung offen“, „wartet auf <x>“ → „abhängig von <x>“, „wartet in der Queue“ →
  „in der Queue“ (EN: „reply outstanding“, „depends on“, „queued“). Statuslogik unverändert.
- **i18n**: `loadModuleMessages` (HTTP) → eingebettete Texte (locales/*.json eingemischt, JSON gewann vorher) +
  `api.shell.locale` live + `t('tickets.<key>')`; Zustandsschlüssel auch aus `locales/*.json` entfernt.
- **Imports** `ctox:shared/crew-renderer.js`, `ctox:shared/crew-motion.js`, `ctox:shared/pane-grammar.js` (ohne `?v=`);
  `shared/i18n.js`, `shared/dialogs.js`, `shared/permissions.js` entfallen. `startCrewMotion(root)` statt `(document)`.
- **Markup/CSS**: `fetch(new URL('./index.html', import.meta.url))` ohne `?v=`; `ensureStyles()` entfernt; index.css
  unter `[data-app="tickets"]` (99 Selektoren, 20 Shell-Vorfahren ersetzt).
- **schema.js gelöscht**: identisch zum v3-Stand und dort gegen `collections.schema.json` verglichen: 12/12
  Collections, gleiche `version`/`primaryKey`/`required`/Properties, keine Migrationen → 0 Abweichungen.
- **Snapshot** `{v:1, selectedId, focusedCaseId, opsMode, search, view, band, status, trayOpen, listScrollTop}`.
- **Tests** `tickets.test.mjs` und `tests/tickets-module-smoke.mjs` auf v4 umgestellt (Readiness-/RxDB-Demand-Loader-
  Prüfungen → v4-Prüfungen: keine Readiness, leere Liste `''`, begrenzte Crew-Abfragen, Abmeldung im unmount; Statuszeile
  → Zone).

## Befehlstypen der UI – Einstufung (§16.13)

Handler-Einstufung: `V4-HANDLERS.md` (Strang H8: alle 12 `ctox.ticket.*` als JS-Handler mit eigenem
Fall-Zustandsautomaten seit Strang TICKAPP, ohne Kern-Fähigkeit). Rust-Treffer `grep -rn '"<type>"' src/core --include=*.rs`:
`approve` 2, `execute` 2, `verify` 2, `request_clarification` 2, `publish_clarification` 1, `resolve_clarification` 2,
`writeback_comment` 2, `close` 2, `local.create` 6. Zusätzlich sendet die UI `ctox.crew.assign` (5 Treffer): Fähigkeit
`cap.agent.crew`, gehört der ctox-App (V4-HANDLERS).

## OFFEN (nummeriert)

1. **Fachlogik außerhalb**: Fall-Zustandsmaschine, Ticket-DB und Projektion bleiben im Kern (`cap.ctox.ticket`);
   die UI ist nur auf `api.command`/`api.query` umgestellt.
2. **Rückmeldung an den öffnenden Thread** (`ctox-business-os-record-focus`, `record_focused|unavailable` +
   `returnThreadId`): 4.0 hat dafür keine API. Das Ereignis wird weiter vom App-root aus (bubbelnd) gesendet; ob die
   v4-Shell es hört oder ein `api.shell`-Weg kommt, entscheidet der Shell-Strang.
3. **Crew-Stylesheet**: `startCrewMotion` (shared) installiert das gemeinsame Crew-Stylesheet in `document.head` – außerhalb
   des App-roots; gehört dem Shell-/shared-Strang (Manifest-`styles`).
4. **Mehrere Fenster**: Modulzustand modulweit (wie knowledge OFFEN-4).
5. **Manifest** (nicht geändert): `ctox_queue_tasks`, `ctox_crew_members`, `business_commands` sind Fremd-/System-
   Collections → `foreign_collections`; Fähigkeiten `cap.ctox.ticket`, `cap.agent.crew`.
6. **Gestapelte Ansicht 420 px**: bei offener Aktionen-Pane bekommt die Liste nur ihr Minimum (v3-Raster
   `minmax(180px,1fr) / 1.4fr / 1fr`, unverändert übernommen); mit offenem Filterfach bleibt eine Zeile sichtbar.
   Gestaltungsentscheidung, kein Port-Defekt – Geometrie-Labor.
7. **Fremde Verweise**: `src/core/rxdb/tools/browser_rust_smoke.js:8032` lädt `/modules/tickets/schema.js` (v3-Rauchtest,
   nicht in meinem Schreibbereich) – bricht nach der Löschung.
8. Shell-Lücken wie support OFFEN-5 (Resizer, Container `business-app-window`, v2-Iconzone).

## Wortlaut zurückgedreht (Strang LBL, 15.10.2026 15:45)

- Zurück (DE `index.js` + `locales/de.json`): Statusfilter `pending` „Wartend“ → **„Wartet“** (Band `bandPending` und HTML-Option waren upstream schon „Wartend“, unverändert); Crew-Sätze „Rückmeldung offen“ → **„wartet auf Rückmeldung“**, „abhängig von“ → **„wartet auf“**, „in der Queue“ → **„wartet in der Queue“** (auch eingebettete Rückfalltexte in `crewHoldReason`/`crewStatusText`). EN (`index.js` + `locales/en.json`): „reply outstanding“ → **„waiting for a reply“**, „depends on“ → **„waiting for“**, „queued“ → **„waiting in the queue“**.
- Original = `upstream-main`. Markierung `v4-allow: label` (JS: Kommentar in derselben Zeile, HTML: `<!-- v4-allow: label -->` davor, JSON: Markerschlüssel `"//<schlüssel>": "v4-allow: label"` davor) nur an Zeilen, die der Wächter sonst meldet. Prüfung: v4-api-Test gegen Stub-Pin cff5ba5 grün, `scan.mjs --app <id>` unverändert, guard-v4 ohne Treffer außerhalb `handlers/` (11/11).
