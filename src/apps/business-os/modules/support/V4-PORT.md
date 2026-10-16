# support – Port auf App-API 4.0 (Strang UC1, Welle C)

Fortschritt (Berlin-Zeit, gemessen mit `TZ=Europe/Berlin date`):
- 13:25 Start; Brief, PORTING.md, app-api.d.ts, Stub-Pin 67229d2, knowledge-Pilot, V4-HANDLERS.md gelesen.
- 13:25 Scanner vorher: **36 Stellen**, CSS 79 Selektoren ungescoped (12 mit Shell-Vorfahr).
- ca. 13:41 Port index.js/index.html/locales, CSS gescoped, schema.js gelöscht; Bestandssuite auf v4 umgestellt 14/14.
- 13:43 v4-api.test.mjs gegen Stub-Pin 67229d2: 11/11 grün; Screenshots 1280/420 angesehen.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app support` | 36 → **0 Stellen**, Exit 0 |
| CSS-Scope | `node v4/tools/port/scope-css.mjs support modules/support/index.css --check` | gescoped |
| guard-v4 | `node v4/tools/guards/guard-v4.mjs --json` (Treffer in `modules/support/` außer `handlers/`) | **0** |
| v4-Test gegen Kern-Stub | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/67229d2 node --test src/apps/business-os/modules/support/tests/v4-api.test.mjs` | **11/11 grün** |
| Bestandssuite | `node --test src/apps/business-os/modules/support/tests/support.test.mjs` | 14/14 grün (auf v4-Vertrag umgestellt, nicht gelöscht) |
| Screenshots | `tests/shots/support-1280.png`, `support-420.png` (Kontext offen = ganze Fläche), `support-420-queue.png` (gestapelt) | angesehen: kein horizontaler Überlauf (Test misst `scrollWidth-clientWidth ≤ 1`), keine Überlappung |

Abgedeckt vom v4-Test: mount zeigt Live-Daten, Band-Zählung über selbst verdrahtete Pane-Grammatik · Auswahl zeigt
Timeline (Nachricht + Notiz) und Kontext · `sim.push` erscheint live, Löschung verschwindet · leere Collections: Liste,
Timeline, Kontext und Titel leer, kein `.ctox-syncing/.ctox-empty/[role=status]`, keine Zone · Befehl `claim` legt
`business_commands`-Dokument an (`type`, `app_id`, `record_id` über opts, Handler-Payload `conversation_id`,
`client_context`), `sim.failCommand` → genau ein Zoneneintrag, optimistische Zuordnung zurückgenommen · Notiz →
`support.note.create` lokal sofort sichtbar, `done` ohne Zone · Import: ungültiges JSON → Zone `support.import.invalid`,
gültig → `open_from_thread` · `api.put` → accepted, live · snapshot → structured clone → Remount mit `restored` erhält
Notiz-Entwurf, Auswahl und Suche (Filter greift) · unmount löst alle Abos, leert root · keine `pageerror`.
**Nicht** abgedeckt: Locale-Wechsel live, Export-Download, `askCtox`, Geometrie-Labor, Hot-Plug-Skript.

## Was sich geändert hat

- **Einstieg** `mount(root, api, {restored})` → `{unmount, snapshot}`; Modulzustand je mount neu (`initialState()`),
  Abos je Montage lokal (PORTING §4 Falle 1).
- **Daten**: 29 Collections je `api.query(name, {}).subscribe` (entprellt 80 ms) statt `ctx.db…find().exec()` +
  `$.subscribe`; entfernt: `startCollection`-Kette mit 8-s-Timeout und 80-ms-Staffel, `collectionReadiness`/
  `subscribeCollectionReadiness`, `withTimeout`, `loading`-Zustand, `resolveConversationListState`. Die
  Mount-Rechteprüfung `ctx.permissions.canReadCollection` + absichtlich werfendes `ctx.db.collection(x).find()`
  entfällt: die Entscheidung trifft der Core (fremde Collections → `forbidden`, OFFEN-4).
- **Zustandstexte** raus (index.js, index.html, locales): „Support wird synchronisiert“, Leerzustände von Liste/Timeline/
  Kontext, „Keine Support-Konversation ausgewählt“ (Titel bleibt leer), „Befehl wurde an CTOX übergeben.“ (8×),
  „Befehl konnte nicht übergeben werden.“, Toasts „Queue exportiert.“/„… aus Threads geöffnet.“, die
  `[data-support-status]`-Zeile im Kontext, „N Nachrichten lokal geladen“ → „N Nachrichten“. In die Zone gehen:
  `support.command.<type>` (failed oder lokal ungültig), `support.import.invalid`, `support.import.empty`,
  `support.export.failed`.
- **Wortwahl wegen guard-v4-Vokabular** (`de-warten`): Titel „Warteschlangen“ → „Queues“ (DE wie EN), Filteroption
  Status `waiting` „Wartet“ → „Wartend“. Fachwerte unverändert.
- **Befehle**: `api.command(type, {...Handler-Payload, client_context}, {record_id})`, nie warten; Optimistik
  (Overrides, lokale Notiz) bleibt und wird bei `failed`/lokal ungültig zurückgenommen. `module`/`command_type`/
  `inbound_channel` stehen nicht mehr im Dokument. `businessCommandsFor` liest v4-Dokumente (`type`, `app_id`,
  `record_id`, `created_at`) und weiterhin projizierte Alt-Zeilen.
- **Shell**: `setTitle('Support')`; `autoWirePaneGrammar(root)` ruft die App selbst; Sprache über
  `api.shell.locale` (live) + `t('support.<key>')` mit eingebetteten Rückfalltexten (Werte aus `locales/*.json`
  eingemischt, JSON gewann vorher über HTTP); Nutzer-ID über `api.actor.id`; `ctx.notifications` → Zone.
- **Imports** `ctox:shared/stable-dom.js`, `ctox:shared/pane-grammar.js`; `shared/i18n.js` entfällt.
- **Markup/CSS**: `fetch(new URL('./index.html', import.meta.url))` ohne `?v=`; `ensureStyles()` entfernt; index.css
  unter `[data-app="support"]` (79 Selektoren, 12 Shell-Vorfahren ersetzt).
- **schema.js gelöscht**: gegen `collections.schema.json` verglichen (29/29 Collections, gleiche `version`,
  `primaryKey`, `required`, Property-Schlüssel; keine `migrationStrategies`) → 0 Abweichungen.
- **Snapshot** `{v:1, selectedId, queue, viewMode, search, filters, contextCollapsed, trayOpen, noteDraft,
  listScrollTop}`; Ansicht (Suche/Band/Filter/Ansicht/Tray) wird vor `autoWirePaneGrammar` ins Markup geschrieben,
  Auswahl + Entwurf erst, wenn die Konversation in den Live-Daten steht.
- **Bestandssuite** `tests/support.test.mjs`: `registerHooks` bildet `ctox:shared/` auf `shared/` ab; Import-Prüfung
  auf `importSupportText`; Readiness-Test ersetzt durch v4-Prüfung (kein `ctox-syncing`, keine Sync-/commandBus-/
  ctx.host-Bezüge, leere Liste rendert `''`).

## Befehlstypen der UI – Einstufung (§16.13)

Handler-Einstufung aller 34 `support.*`-Typen: `V4-HANDLERS.md` (Strang H3; alle Fachlogik, `ticket.create_from_conversation`
seit TICKAPP per `ctx.command` an die Tickets-App). Von der UI gesendet: `note.create`, `conversation.assign|snooze|claim|resolve|
reopen|status|priority|open_from_thread`, `ticket.create_from_conversation`, `agent.apply_suggestion|reject_suggestion`.
Zusätzlich sendet die UI `business_os.chat.task` („CTOX fragen“): **Core-generisch** (154 Treffer
`grep -rn '"business_os.chat.task"' src/core --include=*.rs`), kein support-Handler.

## OFFEN (nummeriert)

1. **JSON-Import `open_from_thread` ohne `conversation_id`** (V4-HANDLERS U1): die UI sendet weiter nur
   `{thread_key, inbox_id}`; Handler und Rust antworten `invalid: conversation_id is required` → der Fehler erscheint
   jetzt in der Zone (`support.command.support.conversation.open_from_thread`). Nicht erfunden; Owner-Entscheidung
   (z. B. `support_conv_<uuidv5(URL, thread_key)>` wie der Intake).
2. **„CTOX fragen“ ohne Chat-API**: v3 nutzte `ctx.businessChat.submitTask({…, open:true})` (öffnete den Chat). 4.0 hat
   keine Chat-API; der bisherige Rückfall `business_os.chat.task` ist jetzt der einzige Weg, ein Chat-Fenster öffnet
   sich nicht. Ob `api.shell.navigate('ctox', {command_id})` gewünscht ist (wie knowledge), entscheidet der Shell-Strang.
3. **Mehrere Fenster**: Modulzustand ist modulweit (wie knowledge OFFEN-4); eine zweite Montage übernimmt die
   Darstellung, die erste muss neu montiert werden (im Test nachgebildet).
4. **Manifest** (nicht geändert, Strang v4/manifests): `business_commands`, `ctox_queue_tasks`, `business_chats`,
   `communication_*`, `ctox_ticket_cases`, `customer_*` sind Fremd-/System-Collections → `foreign_collections`
   (V4-HANDLERS W5); `desktop_files`/`desktop_file_chunks` stehen in `collections`, werden von der UI nicht gelesen.
5. **Shell-Lücken**: Spaltenresizer (`data-resizer`) und der Fenster-Container `business-app-window` (alle
   `@container`-Regeln) kamen von der v3-Shell; 4.0 nennt beides nicht. Der Test stellt den Container wie die Shell.
   Die Kopfzeilen halten weiter die v2-Iconzone (80×74) frei.
6. **Fremde Verweise auf gelöschte schema.js**: keine für support (`src/core/rxdb/tools/browser_rust_smoke.js` lädt
   nur tickets/invoices).

## Wortlaut zurückgedreht (Strang LBL, 15.10.2026 15:45)

- Zurück: Titel/Navigation „Queues“ → **„Warteschlangen“** (DE: `index.js` queueTitle, `index.html` h2 + aria-label „Support-Warteschlangen“ + View-Switch-aria-label, `locales/de.json`; EN bleibt „Queues“ wie upstream); Statusfilter `waiting` „Wartend“ → **„Wartet“**.
- Original = `upstream-main`. Markierung `v4-allow: label` (JS: Kommentar in derselben Zeile, HTML: `<!-- v4-allow: label -->` davor, JSON: Markerschlüssel `"//<schlüssel>": "v4-allow: label"` davor) nur an Zeilen, die der Wächter sonst meldet. Prüfung: v4-api-Test gegen Stub-Pin cff5ba5 grün, `scan.mjs --app <id>` unverändert, guard-v4 ohne Treffer außerhalb `handlers/` (11/11).


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G4: Zeilentitel (Karten- und Listenansicht) brechen um statt Ellipse.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app support --shared` (Stub-Pin 9fa9948) → alle Läufe OK; `node --test modules/support/tests/v4-api.test.mjs` grün; `scan.mjs --app support` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.

## Nachtrag Strang SKL-A1 (App-Skills, 15.10.2026)

- „CTOX fragen“ sendet jetzt den App-Befehl `support.agent.task` (`buildSupportAgentTaskCommand`, Payload
  `{conversation_id, request_kind, title, status, priority, record_snapshot}`) statt `business_os.chat.task`;
  `buildAgentInstruction` und das tote `supportWritebackContract` sind aus `index.js` entfernt (Template
  `prompts/agent-task.md`, Skill `skills/support-workflow`, Handler siehe V4-HANDLERS.md §6). OFFEN-2 ist damit
  entschieden: kein Chat-Fenster; Vorschläge erscheinen live in der Timeline (`support_agent_suggestions`).
- Zone: `reportAgentRequests` meldet Aufträge (`support_agent_requests`), die während der Montage failed/cancelled
  werden (`support.agent.request.<id>`), completed räumt den Eintrag.
- Prüfung: `tests/support.test.mjs` 14/14, `tests/v4-api.test.mjs` 11/11 (Stub-Pin 7706796), `scan.mjs --app support` 0.
