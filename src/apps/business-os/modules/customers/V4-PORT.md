# customers – Port auf App-API 4.0 (Strang UD6, Welle D)

Status: **abgeschlossen** (15.10.2026, gemessen 13:48 Start → 14:16 grün; nach Abbruch des Strangs ~14:40 erneut geprüft
gegen Stub-Pin `~/.local/state/shell-v4/stub-pins/9fa9948`, App-API unverändert gegenüber 67229d2/cff5ba5).
Fachlogik (20 Befehlstypen) liegt in `handlers/` (Strang H4, `V4-HANDLERS.md`), dort nichts geändert.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app customers` | **0 Stellen**, Exit 0 (vorher 38 Stellen, 181 Selektoren ungescoped, davon 31 mit Shell-Vorfahr) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs customers modules/customers/index.css --check` | gescoped |
| guard-v4 | `node v4/tools/guards/guard-v4.mjs --json` | 0 Treffer in `modules/customers/` (vorher 29); Testtreffer nur `handlers/tests/_ctx.mjs` (Handler-Strang) |
| v4-Test | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/9fa9948 node --test src/apps/business-os/modules/customers/tests/v4-api.test.mjs` | **12/12 grün** (67229d2 zweimal, cff5ba5 und 9fa9948 je einmal) |
| Bestehende Suite | `node modules/customers/customers.test.mjs` (aus Kopie mit node_modules-Link, Klon hat keine node_modules) | grün („customers schema and IA smoke OK“) |
| Handler (unverändert) | `cd modules/customers/handlers && node --test 'tests/*.test.mjs'` | 71/71 grün |
| Screenshots | `tests/shots/customers-1280.png`, `tests/shots/customers-420.png` (im v4-Test erzeugt, Shell-CSS geladen) | selbst angesehen: 1280 zweispaltig, 420 gestapelt, kein waagrechter Überlauf |

v4-Test deckt ab: mount zeigt Live-Daten, Titel `Kunden` über `api.shell.setTitle`, genau ein Abo je Collection, kein
`business_commands`-Abo · `sim.push` erscheint, `_deleted` und `is_deleted:true` verschwinden · leere Instanz: Liste
und Arbeitsbereich ohne Text, keine Zustandskästen, keine Zone · `api.put` → `accepted`, live sichtbar · Archivieren:
Befehlsdokument `{type, payload:{account_id}, record_id}` ohne `module`, `sim.failCommand` → genau ein Zoneneintrag
`customers.command.customers.account.archive` (level error, `ref` = Befehls-ID) · „Neuer Kunde“: Formular schließt
synchron, Befehl `customers.account.create` endet `done`, keine Zone · Übernahme aus Outbound trägt `account_id` und
`import_batch_id` im payload (U1) · Formular-Validierung → Zone `customers.form.invalid`, kein Kasten im Inhalt ·
snapshot → structured clone → Remount erhält offenes Bearbeiten-Formular (Eingabe) und Auswahl; Live-Änderung während
des Tippens verwirft die Eingabe nicht · App-Sprung über `api.shell.navigate('documents', params)`, `location.hash`
unverändert · Screenshots 1280/420 · unmount: alle Abos 0, root leer, Zone leer, keine `pageerror`.
**Nicht** abgedeckt: JSON-Import/-Export-Dateidialog, Locale-Wechsel live, Geometrie-Labor, Hot-Plug-Skript.

## Stellen und Entscheidungen

- **R1/R2 Einstieg**: `mount(root, api, {restored})` → `{unmount, snapshot}`. `root.dataset.app` nur falls leer.
  `ctx.left/right` entfallen (die App nutzte sie nur zum Leeren). Alle `state.ctx.host`-Zugriffe → `state.root`.
  Abos je Montage lokal (PORTING §4 Falle 1); Rückrufe einer abgelösten Montage prüfen `state.api === api`.
- **R3 Daten**: `readCollection`/`find().exec()` + `$.subscribe`-Neuladen ersetzt durch je ein
  `api.query(name, {sort:[{updated_at_ms:'desc'}]}).subscribe` für 12 eigene + 2 Outbound- + 5 Verknüpfungs-Collections;
  `is_deleted:true` wird wie in v3 ausgefiltert; Neuzeichnen entprellt 16 ms. Fremd-Collections, deren `query` mit
  `forbidden` wirft, bleiben leer und melden `customers.linked.limited` (warn) in die Zone (vorher Badge im Inhalt).
  `business_commands` wird nicht mehr abonniert (nur das Debug-Befehlsprotokoll las es, siehe unten).
  JSON-Import (`importCustomersJsonFile`): `upsertLocalDoc` → `api.get(...).get()` + `api.patch` (bestehend, `created_at_ms`
  bleibt) bzw. `api.put`, je Dokument `await h.local` (lokal, kein Netz).
- **R4**: Readiness-Gate (`subscribeCollectionReadiness`, `collectionReadiness`, `dataEmptyShowsSyncing`,
  `renderSyncingState`), `startCollection('business_commands')`, `buildSyncRows` entfernt.
- **R5 Befehle**: `dispatchCommand` → `api.command(type, payload, {record_id})` ohne Warten. Die 18 Builder behalten ihre
  v3-Form (Legacy-Test prüft sie); `module`, `command_type`, `client_context` gehen nicht ins Dokument. `record_id` als
  Befehlsoption (§16.19 Nr. 8, Stub `cmdOpts.record_id`), weil die Handler (`_lib.js recordIdOf`) es lesen und es nicht
  in `payload` gehört (Handler kopieren `payload` in Aktivitäten). Status: `failed`/`h.local`-Ablehnung →
  `api.status.report({level:'error', code:'customers.command.<type>', ref:h.id})`, `done` → `clear`. Abo endet beim
  Endzustand bzw. mit `unmount`. Alle `await refreshData()` nach Befehlen → `refreshView()` (lokal neu zeichnen).
- **U1 (V4-HANDLERS.md)**: `customers.import.from_outbound` sendet zusätzlich `account_id` und `import_batch_id` =
  Firmen-ID (Rust leitete beide aus `record_id` = Firmen-ID ab). Alle anderen Update/Archiv/Close/Dedupe-Befehle hatten
  die Ziel-ID schon im payload (`account_id`, `contact_id`, `opportunity_id`, `task_id`, `note_id`, `candidate_id`).
- **R6/R13 Shell**: Presence (`ctx.presence`) entfällt (keine 4.0-API, OFFEN-1). `ctx.notifications.notify` → Zone
  (`customers.import.json`). `ctx.getActionIcon` entfällt, die App-eigenen Glyphen waren schon Rückfall (OFFEN-D).
  `openLinkedApp`: `document.querySelector('[data-module=…]').click()` / `location.hash` → `api.shell.navigate(appId,
  params)` mit den Parametern aus dem bisherigen `#app?…`-Verweis; der Klick-Handler für `[data-customers-app-link]`
  ruft jetzt `preventDefault()`, damit kein Hash-Sprung mehr passiert. Kein Deep-Link-Eingang (v3 hatte keinen).
- **R8 Sprache**: `loadModuleMessages` (HTTP) entfällt. Eingebettete `labels` sind Rückfall, `api.shell.t('customers.<key>')`
  übersteuert, `api.shell.locale.subscribe` schaltet live um. Die fünf Schlüssel, die nur in `locales/*.json` standen
  (`customerList`, `entries`, `invalidImport`, `emptyImport`, `importComplete`), sind jetzt eingebettet, damit die
  sichtbaren Texte gleich bleiben.
- **R9 Rechte**: `canMutateCustomersContext` liest `{role, permissions}` aus `api.actor.role` und
  `api.actor.permissions` (Live). `apps.source.edit` ersetzt `ctx.canModifyModule()`; Rollenliste unverändert.
  Abweichung: v3 erlaubte auch bei fehlender Rolle bzw. `is_admin/is_owner`-Flag; 4.0 liefert immer eine Rolle und kein
  `is_admin` am Actor. `ctx.permissions.canWriteCollection` entfällt (Entscheidung im Core).
- **R10**: `ensureStyles` + `?v=`-Cache-Buster entfernt; `index.html` per `fetch(new URL('./index.html', import.meta.url))`.
- **R11 Zustandstexte** (Inhalt → leer bzw. Zone): Leerzustände aller Listen und Teil-Listen (`renderDataEmptyState`,
  `emptyMiniRow`, Inspector-/Apps-Leertexte, `index.html` `data-customers-account-empty`/`-syncing`), Badges
  „Aktualisiert…“/Fehler/„Verknüpfte Daten eingeschränkt“, Kasten „Nur Lesen“ (→ Zone `customers.permission.readonly`,
  info), `customers-command-state` im Formular (Validierung → `customers.form.invalid` warn, Rechte →
  `customers.permission.denied` warn, Aktionsfehler → `customers.action.failed` error). „Änderung gespeichert: …“ entfällt.
  Das Befehlsprotokoll (`renderCommandAuditPanel`, nur bei `ctx.debug` + Admin sichtbar) entfällt; die reinen Helfer
  (`filterCustomerCommands`, `summarizeCustomerCommands`, `commandMatchesContext`, `commandStatusTone`) bleiben (Test).
  Gelöschte Label-Schlüssel (index.js und locales): 23 Zustands-/Leertexte inkl. `commandUnavailable`.
- **R12**: Debounce `setTimeout` 16 ms (Neuzeichnen), sonst keine Timer.
- **R14**: `import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js'`, Aufruf im `mount` (vorher Shell-Heuristik).
  `shared/i18n.js` und `shared/universal-importer.js` entfallen: `openCustomerImporter` und `importCustomerRowsFromPayload`
  waren **toter Code** (nirgends aufgerufen; der Knopf „Importieren“ öffnet die JSON-Dateiauswahl) und sind gelöscht.
- **R16 CSS**: `scope-css.mjs --write`, 31 Shell-Vorfahren ersetzt. Neu: `[data-app="customers"] { container-type:
  inline-size; container-name: business-app-window; }` – die Breitenregeln (`@container business-app-window (max-width:
  768px)`) brauchten in v3 den Container am Shell-Fenster (`app.css:1718/5350`); ohne ihn lief der Arbeitsbereich bei
  420 px aus dem Fenster (gemessen im ersten Screenshot).
- **R17**: `schema.js` gelöscht. Vergleich mit `collections.schema.json`: gleiche 12 Collections, Version je 0, gleiche
  `primaryKey`, `required`, `indexes`, `additionalProperties`; `properties` gleich bis auf Schlüsselreihenfolge;
  Migrationen beide leer.
- **Snapshot**: Ansicht (Filter, Suchen, Sortierungen, Tabs, Auswahl, Formularmodus) + `formDraft` (FormData des
  offenen Formulars) + Scroll der Kontenliste. Die wiederhergestellte Auswahl bleibt stehen, bis Kundendaten da sind.
  Zusätzlich behält ein Neuzeichnen durch Live-Daten die Formulareingaben (v3 hat sie bei jedem Realtime-Refresh verworfen).
- **Bestehende Suite** `customers.test.mjs` auf v4 umgestellt (nicht gelöscht): Schema aus `collections.schema.json`,
  esbuild-Plugin für `ctox:shared/`, Katalogprüfung gegen `v4/manifests/customers.manifest.json` statt `registry.json`,
  Rechte-Spiegel `{role, permissions}`, Import-payload mit Ziel-IDs, Readiness-Prüfung invertiert (kein Sync-Platzhalter).

## Befehlstypen – Einstufung (§16.13)

Rust-Bezüge gemessen mit `grep -rn '"<type>"' src/core --include='*.rs' | wc -l` im Klon. Einstufung und Ziel laut
`V4-HANDLERS.md` (Strang H4): alle **Fachlogik**, Handler `handlers/<type>.js` portiert, 0 × `ctx.cap.legacy`.

| Befehlstyp | Rust | Einstufung | UI-Sender |
|---|---:|---|---|
| customers.account.create | 9 | Fachlogik | Formular Neuer Kunde |
| customers.account.update | 2 | Fachlogik | Formular Bearbeiten |
| customers.account.archive | 2 | Fachlogik | Kopf-Aktion |
| customers.contact.create / .update / .archive | 3 / 2 / 2 | Fachlogik | Formulare, Aktion |
| customers.opportunity.create / .update | 3 / 2 | Fachlogik | Formulare |
| customers.opportunity.move_stage | 3 | Fachlogik | Board-Auswahl |
| customers.opportunity.close_won / .close_lost | 2 / 2 | Fachlogik | Aktionen |
| customers.task.create / .update / .complete | 3 / 2 / 3 | Fachlogik | Formulare, Aktion |
| customers.note.create / .update | 2 / 2 | Fachlogik | Formulare |
| customers.import.from_outbound | 4 | Fachlogik | Übergabe-Tab (mit Ziel-IDs, U1) |
| customers.dedupe.resolve | 3 | Fachlogik | Dubletten-Tab |
| customers.view.save | 2 | Fachlogik | Ansicht speichern |
| customers.activity.record | 2 | Fachlogik | kein UI-Sender |

## OFFEN (nummeriert)

1. **Presence**: v3 zeigte „X bearbeitet gerade“ und veröffentlichte die eigene Auswahl (`ctx.presence`). App-API 4.0
   hat dafür keine Schnittstelle → entfällt, bis der Kern eine anbietet (Wunsch: `api.presence.set/subscribe`).
2. **record_id als dritter Parameter** von `api.command` steht im Stub und in §16.19 Nr. 8 / §16.21, aber nicht in
   `app-api.d.ts` (`command(type, payload)`). Bei Wegfall greift der Rückfall über die Ziel-IDs im payload; nur
   `customers.import.from_outbound` brauchte dafür neue Felder (U1).
3. **Container `business-app-window`**: die App setzt ihn selbst am root. Falls die v4-Shell ihn am Fenster ebenfalls
   setzt, misst die App weiterhin richtig (nächster Container gewinnt). Shell-Strang informieren.
4. **Manifest** (nicht in diesem Strang): `module.json` unverändert; `v4/manifests/customers.manifest.json` führt noch
   `schema.js` in `files` und hat `handlers: []` (V4-HANDLERS W3). `styles: ['index.css']` fehlt.
5. **Mehrere Fenster**: Modulzustand ist modulweit (wie knowledge OFFEN-4); die zweite Montage übernimmt die Darstellung,
   die erste meldet ihre eigenen Abos sauber ab.
6. **Befehlsprotokoll/Debug-Ansicht** entfällt ersatzlos; wer Befehlsstatus sehen will, sieht ihn in der Zustandszone.
   Falls der Owner das Protokoll im Inhalt zurück will, ist das eine Produktentscheidung (Zustandstext-Regel).
7. **`src/core/rxdb/tools/browser_rust_smoke.js`** (v3-Werkzeug, nicht in diesem Strang) importiert noch
   `/modules/customers/schema.js` (Zeilen 3555, 11872); nach dem Löschen von `schema.js` (R17) läuft dieser Smoke für
   customers nicht mehr. Gehört auf die Löschliste bzw. auf `collections.schema.json` umgestellt.
8. Kein git benutzt.
