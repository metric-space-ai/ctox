# conversations – Port auf App-API 4.0 (Strang UC3)

Status: **PORTIERT (Strang UC3b, 15.10.2026 16:24 Berlin).** scan 102 → 1 (nur `schema.js`, bewusst belassen),
CSS gescoped, guard-v4 in modules/conversations (ohne handlers) nur `file-schema-js`, Isolation (check-isolation) 0,
`tests/v4-api.test.mjs` gegen Stub-Pin 9fa9948 12/12 grün, `conversations.test.mjs` 10/10 grün, Screenshots
`tests/shots/conversations-1280.png` / `-420.png` angesehen (zwei Layoutfehler behoben, s. u.).

## Scan-Baseline (vor dem Port)

`node v4/tools/port/scan.mjs --app conversations` → **102 Stellen**, CSS 0 (index.css noch ungescoped,
scope-css meldet „nicht vollständig gescoped"). Verteilung:
- R1-mount (4): `mount(ctx)` + `onActivate`-Callbacks → `mount(root, api, {restored}) → {unmount, snapshot}`.
- R3-db (6): `ctx.db.collection(x).find().exec()/findOne().exec()/upsert()/incrementalPatch()` (threads, accounts,
  messages, outbound_messages, outbound_approvals) → `api.query/get/put/patch`.
- R4-sync (3): `ctx.sync.collectionReadiness/subscribeCollectionReadiness/startCollection/diagnostics` → entfällt.
- R5-command (3): `ctx.commandBus.dispatch(command, {until:'local'|timeoutMs})` → `api.command` ohne Warten.
- R6-shell (7): `ctx.openDesktopApp('file-viewer')`, `ctx.contextMenu.show`, `ctx.contextActions.dispatch`,
  `ctx.notifications.show` → `api.shell.openFile/menu/navigate` bzw. `api.status.report`.
- R9-actor (1): `ctx.session.user.id` im Approval-Audit → `api.actor.id`.
- R10-http (2): `?v=`-Cache-Buster auf index.html + index.css (`ensureStyles`/markupHref) → ohne Buster, CSS lädt Shell.
- R11-state (12): `.ctox-syncing`, `conversationEmptyState`-Zustandsmaschine (syncing/sync-starting/projection-missing/
  syncFailure), „Daten werden synchronisiert", „Nachrichten werden synchronisiert" → leere Liste rendert nichts;
  Zustand nur `api.status.report`.
- R12-timer (2): `withTimeout`/`retryRxdbWrite`-Schleifen (10 s/60 s), `scheduleOutboundCommandDispatch`-Retry
  (60×1 s), `waitForOutboundCommandOutcome`-Polling → entfallen, Live-Abo + api.command.
- R13-nav (3): `location.hash = '#ctox?…'` (navigateToFlowview), `location.hash`-Parsing, `hashchange`-Listener →
  `api.shell.navigate('ctox', {task_id,work_id,source})` + `api.shell.onRoute`.
- R17-schema (1): `schema.js` löschen – siehe unten (blockiert durch Fremd-Consumer).

## Port-Plan je Regel (verbindlich für die Fortsetzung, Muster: knowledge/appsec/coding-agents)

1. **mount**: `mount(root, api, {restored})`. `ctx.host` → `root`. Die 3-Spalten-Resizer sind bereits shell-eigen
   (Kommentar im Code: „owned by the shell-global resizer"); `CtoxResizer`-Import (`ctox:shared/resizer.js`) nur
   behalten, falls noch benutzt, sonst entfernen. `ctx.left/right.replaceChildren` entfällt (nur `root`).
2. **Daten (R3/R4)**: im mount je Collection EIN `api.query(coll, {}).subscribe(rows => { view.rows[coll] = rows;
   <rerender> })` aufsetzen (communication_threads/messages/accounts, outbound_campaigns/pipeline_items/engagements/
   messages/approvals, business_commands). Abo-Handles in `cleanups`/`disposers`. Die Loader `loadThreads`,
   `loadAccounts`, `probeMessages`, `loadTimelineForBucket`, `findOneJson`, `markMessageRead` lesen synchron aus
   `view.rows[coll]` (gelöschte sind schon raus, Query liefert sortiert) statt `find().exec()`. Fremd-Collections
   (outbound_*, business_commands, business_apps) brauchen `foreign_collections` im Manifest (v4/manifests).
3. **Befehle (R5/R12)**: `dispatchOutboundCommand` → `api.command(type, {...payload, record_id, module:'conversations',
   client_context})`, NIE warten. `scheduleOutboundCommandDispatch`, `isBusinessCommandsReadyForDispatch`,
   `waitForOutboundCommandOutcome`, `retryRxdbWrite`, `withTimeout` löschen. `followCommand`-Muster: `handle.status`
   abonnieren, `failed` → `api.status.report({code:'conversations.command.<type>'})`, `done` → `clear`.
   **Fachverhalten-Entscheidung**: Die 4 Outbound-Befehlstypen gehören laut Brief zu outbound (Handler dort). Die
   lokale RxDB-Materialisierung `materializeOutboundDecisionLocally` (direktes upsert auf outbound_messages +
   outbound_approvals) ersetzt der Outbound-Handler; die UI feuert nur den Befehl und zeigt das Ergebnis aus dem
   Live-Abo. Das ist der v4-Vertrag, KEINE Verhaltensänderung der Freigabe-Fachlogik (gleiche Felder, gleiche Audit-
   Approval), nur Zuständigkeit UI→Handler. Vor Umsetzung mit dem Outbound-Handler-Strang abgleichen.
4. **Shell (R6)**: `ctx.openDesktopApp('file-viewer', {args})` → `api.shell.openFile(ref)` bzw.
   `api.shell.navigate('file-viewer', params)`; `ctx.contextMenu.show(event, items)` → `api.shell.menu(spec)`
   (Spec-Form OFFEN-A; Items-Mapping prüfen); `ctx.contextActions.dispatch('app'|'data', …)` → generischer
   Befehl/`api.shell`-Pfad; `ctx.notifications.show` → `api.status.report`.
5. **Navigation (R13)**: `navigateToFlowview` → `api.shell.navigate('ctox', {task_id, work_id, source})`;
   `parseConversationDeepLink` liest Route aus `api.shell.onRoute`-Event statt `location.hash`; `hashchange`-Listener
   löschen, Deep-Link-Zustand über `restored`/`onRoute`.
6. **Zustandstexte (R11)**: `conversationEmptyState` auf zwei Fälle reduzieren — `no-results` (aktive Filter) und
   `initial-empty` (keine Konversationen). Alle sync/readiness/projection-Varianten + `.ctox-syncing` +
   `renderTimeline`-Syncing-Zweig + die ~37 Zustandstexte raus; Fehler (Collection-Fehler) → `api.status.report`.
   `buildConversationDataDiagnostics`, `currentDataDiagnostics`, `syncDiagnostics`-Feld + Listener löschen.
7. **i18n (R8)**: `loadModuleMessages(import.meta.url, ctx.locale, FALLBACK_LABELS)` → `api.shell.t(key)` mit
   `FALLBACK_LABELS` als Rückfall; Locale-Wechsel live über `api.shell.locale.subscribe`.
8. **Persistenz (R9-storageScope)**: `ctx.storageScope.get/set` (Spaltenbreiten) → `snapshot()`/`restored`
   (Spaltenbreiten shell-eigen; UI-State = Filter, Auswahl, Kanal-Tab, listView, offener Chat-Entwurf,
   `userCollapsedContext`, Deep-Link). SNAPSHOT_VIEW_KEYS festlegen; offene Eingaben IMMER mitnehmen.
9. **HTTP (R10)**: `ensureStyles()` + `?v=`-Buster auf index.html/index.css entfernen; Markup
   `fetch(new URL('./index.html', import.meta.url))` ohne Buster.
10. **Imports (R14)**: `'../../shared/i18n.js'` entfällt (api.shell.t), `'../../shared/resizer.js?v=…'` →
    `'ctox:shared/resizer.js'` nur falls noch benötigt.
11. **CSS (R16)**: `node v4/tools/port/scope-css.mjs conversations modules/conversations/index.css --write`
    (alles unter `[data-app="conversations"]`).
12. **Titel (R15)**: `api.shell.setTitle('Conversations')`.

## schema.js / outbound-Kopplung (Brief-Kernpunkt)

- `modules/conversations/schema.js:16-19` importiert `collections`/`migrationStrategies` aus `../outbound/schema.js`
  und re-deklariert `outbound_campaigns/pipeline_items/engagements/messages/approvals`; eigene Defs:
  `communication_accounts/threads/messages`.
- **Gemessen**: `collections.schema.json` enthält genau diese 8 Collections; die 5 outbound-Defs sind nach
  Schlüssel-Normalisierung IDENTISCH zu `../outbound/schema.js` (Vergleich 15.10.). Einzige nicht in
  collections.schema.json abgebildete Laufzeitsache: `migrationStrategies.outbound_messages['1']` (Code), nur im
  outbound-Modul relevant; `ctox/schema.js` importiert nur `collections`, nicht `migrationStrategies`.
- **Blocker gegen sofortiges Löschen (R17)**: `modules/ctox/schema.js:1` importiert
  `{ collections as conversationCollections } from '../conversations/schema.js'` — ctox ist ein FREMDER Strang (nicht
  im Whitelist dieses Strangs). Löschen von conversations/schema.js bricht ctox/schema.js beim Import.
  Zusätzlich: `modules/outbound/V4-PORT.md:108` hält fest, dass das Löschen von outbound/schema.js an genau diesem
  conversations-Import hängt.
- **Entscheidung**: Die eigentliche Entkopplung (Import von `../outbound/schema.js` entfernen) ist korrekt erst,
  wenn conversations/schema.js gelöscht wird; das verlangt, dass der ctox-Strang seinen Import von
  conversations/schema.js auf `collections.schema.json` umstellt. Diese Cross-Strang-Abhängigkeit ist als
  Koordinationspunkt an den ctox- und outbound-Strang zu melden (OWNER/Koordination), NICHT einseitig hier gelöst,
  um ctox/schema.js nicht zu brechen. Alternativ als Zwischenschritt: conversations/schema.js die 5 outbound-Defs
  aus collections.schema.json inlinen (normalisiert gleich) und den outbound-Import entfernen — behält
  ctox/schema.js funktionsfähig und macht outbound/schema.js löschbar; kostet aber die Drift-Garantie des
  Konformitätswächters und die outbound_messages-Migration (im outbound-Modul weiterhin vorhanden).

## Port-Ergebnis UC3b (umgesetzt)

**Messwerte**: `node v4/tools/port/scan.mjs --app conversations` → 1 Stelle (`schema.js vorhanden`, R17, bewusst);
`scope-css.mjs … --check` → gescoped; `guard-v4 --json` in modules/conversations → nur `file-schema-js`;
`node v4/tools/isolation/check-isolation.mjs --app conversations` → OK, 0 Treffer; v4-Test 12/12; Unit-Test 10/10.
index.js 3505 → 2825 Zeilen.

Umsetzung je Regel:
- **R1/R2 mount**: `mount(root, api, {restored}) → {unmount, snapshot}`; Markup per `fetch(new URL('./index.html', import.meta.url))`
  ohne Buster in `root`; `root.dataset.app` nur falls fehlend; Abos/Listener je Montage in `disposers`; `unmount` leert `root`.
- **R3 Daten**: Live-Abos `communication_threads` (sort last_message_at desc) und `communication_accounts`; Timeline = EINE
  Abfrage `communication_messages {thread_key: {$in: <Threads der Auswahl>}}`, neu abonniert nur bei geänderter Thread-Menge
  (statt globalem Nachrichtenabo + `find().exec()`). Outbound-Kontext: `outbound_messages`/`outbound_approvals` als Live-Abo
  (erst bei Bedarf), `outbound_engagements/campaigns/pipeline_items` je id über `api.get` (ein Abo je Dokument). Der v3-Deckel
  `limit(250)` entfällt (v4: kein Zeilendeckel). Adressierung über `id` (§16.28, MAIL2: id = *_key); *_key nur als Feld.
- **R4/R11/R12**: Readiness, `startCollection`, Sync-Diagnose, `conversationEmptyState`/`buildConversationDataDiagnostics`,
  `.ctox-syncing`, Account-Statuszeile, alle Lade-/Sync-/Leer-Texte (HTML, JS, locales: je 27 Schlüssel) entfernt; leere Liste,
  leere Timeline, keine Auswahl rendern nichts (Fußzeile zählt). `withTimeout`, `retryRxdbWrite`, `waitForOutboundContextForLink`
  (15 s Poll), `scheduleOutboundContextRetry` (60×500 ms), `scheduleOutboundCommandDispatch` (60×1 s),
  `waitForOutboundCommandOutcome` (60 s Poll), `isBusinessCommandsReadyForDispatch` entfernt → Live-Abos + Mikrotask-Bündelung.
  Fehler (Abfrage wirft, Schreiben lokal abgelehnt, Befehl failed, Import) → `api.status.report` mit stabilen Codes
  `conversations.data.<coll>`, `conversations.write.communication_messages`, `conversations.command.<type>`,
  `conversations.import`, `conversations.route.not_found`.
- **R5 Befehle** (Eigentümer outbound, V4-HANDLERS.md): `api.command(type, {...payload, record_id, client_context:{source_module,
  inbound_channel, deep_link}})`, nie warten; `status` abonniert: `failed` → Zone, `done` → `clear`. Auto-Zuordnung
  `outbound.reply.match` unverändert (Dedupe, bei failed erneut erlaubt).
- **Entscheidung `materializeOutboundDecisionLocally`**: geprüft – `modules/outbound/handlers/outbound.message.approve.js`
  (tx.put outbound_approvals + outbound_messages, approval_status/draft_status/send_status/revision_id) und
  `outbound.message.reject.js` → `_lib/decisions.js` (tx.put outbound_approvals + outbound_messages) schreiben die Entscheidung
  selbst → lokales Upsert ENTFERNT (Manifest erlaubt outbound_* ohnehin nur `read`). Fachlicher Unterschied, den jetzt der Handler
  (Rust-Referenz) bestimmt: v3-UI schrieb bei Ablehnung `draft_status:'changes_requested', send_status:'cancelled'`, der Handler
  setzt `approval_status:'rejected', send_status:'blocked'` (+ engagement `draft_rejected`). Optimistische lokale Kontext-Merges
  (classify/match) entfallen; die Änderung kommt gepusht.
- **Projektion in die eigene Nachricht** (`applyOutboundDecisionToCommunicationMessage`): bleibt, da conversations Eigentümer von
  `communication_messages` ist; jetzt `api.patch('communication_messages', <id = message_key>, {status, folder_hint, metadata_json,
  updated_at})` nach `done` des Befehls (v3: nach dem lokalen Upsert). Felder unverändert.
- **R6/R13 Shell/Navigation**: CTOX-Flow → `api.shell.navigate('ctox', {task_id, work_id, source})`; Outbound →
  `navigate('outbound', {campaign_id, engagement_id, message_id})`; Anhang → `navigate('file-viewer', {messageKey, channel, ref,
  metadata})` (v3-args 1:1); Deep Links (`#conversations?…`) über `api.shell.onRoute` mit derselben Parameter-Semantik; Ziel nicht
  in den lokalen Daten → `conversations.route.not_found` (warn), jede Live-Emission versucht erneut, Treffer löscht den Eintrag.
  `hashchange`/`location.hash` entfernt. Notifications → Zone.
- **Tot entfernt**: `openMessageContextMenu`, `spawnMessageCtoxForm` (ctx.contextActions), `copyToClipboard` hatten keinen Aufrufer
  (grep 15.10.) – kein Fachverhalten betroffen; der v2-Quelltext-Wächter darauf wurde auf „nichts außerhalb des Fensters“ umgestellt.
- **R8 i18n**: `api.shell.t('conversations.<key>')` übersteuert die eingebetteten `FALLBACK_LABELS` (de/en), Locale live über
  `api.shell.locale.subscribe`; `locales/*.json` ohne Zustandstexte (bleiben als Schlüsselquelle, OFFEN-B).
- **R9 Snapshot**: `SNAPSHOT_VIEW_KEYS` (Kanal, Account, Richtung, Zeitraum, Suche, Liste/Karten, Kontext eingeklappt, Auswahl,
  Kanal-Tab, Timeline-Limit, Hervorhebung) + `searchText` (Eingabefeld), `deepLink`, `outboundLink`, `classificationDraft`
  (offene Reply-Klasse vor dem Klick), `timelineScrollTop`. Auswahl wird erst übernommen, wenn sie in den Live-Daten steht.
  Spaltenbreiten: Shell (v4/shell/column-resizers.js über `data-resizer-var`), `ctx.storageScope` entfällt.
- **R10/R14/R15/R16**: `ensureStyles`/`?v=` entfernt; Imports nur `ctox:shared/pane-grammar.js` (`autoWirePaneGrammar(frame)` im
  mount, ersetzt die v2-Shell-Verdrahtung); `CtoxResizer`/`i18n.js` raus; Titel `api.shell.setTitle`; CSS per scope-css.
- **Wächter-Kennzeichnung**: Datenstatus-Beschriftungen `in Warteschlange` / `Wartet auf Freigabe` mit `// v4-allow: label`; in
  `locales/de.json` als Vorzeile `"_statusQueued_note": "v4-allow: label …"` (JSON kennt keine Kommentare).
- **Layout (Screenshots)**: (1) Titel „Conversations“ wurde vom langen Kicker auf 0 gedrückt → Kicker kürzt zuerst; (2) Inspektor-
  Karten (overflow:hidden) schrumpften in `auto`-Zeilen unter ihren Inhalt (Freigeben/Ablehnen unsichtbar) → `grid-auto-rows:
  max-content` an `.conv-right-detail`. Testfenster ist Size-Container `business-app-window` wie `.shell-window-content` (frame.css).

## schema.js (Stand UC3b)

- Orchestrator-Hinweis 15.10. (löschen) vs. Brief UC3b (NICHT löschen, `modules/ctox/schema.js:1` importiert es; zusätzlich lädt das
  alte `app.js:6512` je Modul `schema.js` samt `migrationStrategies`). Umgesetzt ist der Weg, der beides erfüllt, was erfüllbar ist:
  `schema.js` importiert `../outbound/schema.js` NICHT mehr (Isolation R1 = 0); die fünf outbound-Definitionen und die
  `outbound_messages`-Migration v1 sind wörtlich eingebettet (gemessen: JSON-gleich zu outbound/schema.js, Migration liefert
  dasselbe Dokument; `modules/ctox/schema.js` lädt weiter, 29 Collections). `index.js` importiert `schema.js` nicht.
  Restfund: guard `file-schema-js` / scan R17 = 1, bis die Datei zentral mit dem alten app.js entfällt.
- Abweichungen schema.js ↔ v4: `collections.schema.json` des Moduls hat noch `primaryKey` = *_key; `v4/schemas/
  conversations.collections.schema.json` (MAIL2) hat `id` + Migrationen (id ← *_key). Die UI adressiert über `id` und liest *_key nur
  als Feld (Accounts-Map: `account_key || id`).

## OFFEN (nummeriert, Stand UC3b)

1. **schema.js löschen**: erst wenn `modules/ctox/schema.js` nicht mehr importiert und das alte `app.js` entfällt (Fremd-Strang
   ctox / zentrale Löschung). Dann scan/guard = 0.
2. **Befehlsweg über App-Grenzen** (V4-HANDLERS.md): conversations sendet `outbound.*`; Stub legt `app_id = conversations` an –
   Klärung mit „ctox sync v4“, ob der Handler unter dem Eigentümer (outbound) läuft und `command_rights` greifen.
3. **Payload-Durchreichung im Handler**: `outbound.message.approve` übernimmt `objectPayload(command.payload)` ins Approval-Dokument;
   damit landen `record_id` und `client_context` (inkl. Deep Link) im Audit. Handler-Strang entscheidet, ob filtern (hier nicht
   angefasst).
4. **Reject-Status**: v3-UI `send_status:'cancelled'`, Handler `send_status:'blocked'` (Rust-Referenz) – Fachentscheidung outbound.
   Die Projektion in `communication_messages.metadata_json` schreibt weiter v3-Werte (`cancelled`).
5. **Manifest** (v4/manifests, Generator): `foreign_collections` (outbound_* read) passt; `business_commands` liest die App nicht
   mehr direkt (Status kommt über `CommandHandle.status`) → Eintrag kann weg; `collections.schema.json` des Moduls auf `id`
   nachziehen (Eigentum MAIL2/Manifest-Strang).
6. **Projektion nach `done`**: wird das Fenster vor `done` geschlossen, entfällt die metadata-Projektion (v3: lokal sofort). Sauberer
   Ort wäre der outbound-Handler über `cap.mail.message.annotate` (wie reply.match) – Vorschlag an Handler-Strang.
7. **file-viewer-Route**: Parameter `{messageKey, channel, ref, metadata}`; `raw_payload_ref` ist kein `files`-sha256, daher nicht
   `api.shell.openFile` – file-viewer-Strang muss die Route annehmen.
