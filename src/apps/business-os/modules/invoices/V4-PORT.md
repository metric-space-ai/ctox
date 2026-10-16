# invoices – Port auf App-API 4.0 (Strang UC1, Welle C)

Fortschritt (Berlin-Zeit; „ca.“ = geschätzt, sonst gemessen mit `TZ=Europe/Berlin date`):
- ca. 13:45 Start; V4-HANDLERS.md (Strang H5) gelesen, Scanner vorher: **28 Stellen**, CSS 64 Selektoren ungescoped (25 mit Shell-Vorfahr).
- ca. 14:05 Port index.js/index.html/locales/index.css, schema.js gelöscht; Node-Bestandssuite auf v4 umgestellt (79/79).
- 14:16 (gemessen) v4-api.test.mjs gegen Stub-Pin 67229d2: 12/12 grün; Screenshots 1280/420 angesehen.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app invoices` | 28 → **0 Stellen**, Exit 0 |
| CSS-Scope | `node v4/tools/port/scope-css.mjs invoices modules/invoices/index.css --check` | gescoped |
| guard-v4 | Treffer in `modules/invoices/` außer `handlers/` | **0** |
| v4-Test gegen Kern-Stub | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/67229d2 node --test src/apps/business-os/modules/invoices/tests/v4-api.test.mjs` | **12/12 grün** |
| Bestandssuite | `cd modules/invoices && node --test tests/*.test.mjs` | 79/79 grün (vorher esbuild-Bündel + ctx-Attrappe, jetzt Node-Auflösung `ctox:shared/` + `createTestApi`; läuft ohne node_modules) |
| Screenshots | `tests/shots/invoices-1280.png`, `invoices-420.png`, `invoices-420-draft.png` | angesehen: kein horizontaler Überlauf des Fensters (Positionstabelle scrollt in ihrem `.ctox-table-wrap`), Köpfe vollständig |

Abgedeckt vom v4-Test: mount zeigt Live-Daten, Band-Zählung (Alle/Offen/Bezahlt/Überfällig) über selbst verdrahtete
Pane-Grammatik, leere Bühne ohne Hinweis/Fehlerstreifen · `sim.push` live + Löschung · leere Collections: Liste und
Bühne leer, keine Zone · **Zahlung zuordnen ohne Zahlungs-ID sendet kein `payment_id`** (mit ID: `pay_42`),
`sim.failCommand` → genau ein Zoneneintrag, `done` räumt ihn · Mahnlauf `invoices.dunning.run` · Entwurf speichern →
`invoices.invoice.update` mit Summen; Buchen → `update`, `post` erst nach `done` des Updates · Löschen nur nach
`api.shell.dialog`-Ja · Import ungültig → Zone, gültig → `invoices.invoice.create` · `api.put` → accepted, live ·
snapshot/restored: offener Entwurf (getippte Position), Auswahl, Suche; eine Live-Änderung am Entwurf überschreibt die
offene Eingabe nicht · unmount löst alle Abos, leert root · keine `pageerror`.
**Nicht** abgedeckt: XRechnung-Download, Export-Download, Locale-Wechsel live, Geometrie-Labor, Hot-Plug-Skript.

## Was sich geändert hat

- **Einstieg** `mount(root, api, {restored})` → `{unmount, snapshot}`; Abos/Listener je Montage.
- **Daten**: 9 `api.query(name, {}).subscribe` (entprellt 80 ms) statt `ctx.db.collection(x).find().exec()` +
  `$.subscribe`; entfernt: `subscribeCollectionReadiness`, `invoiceReadiness`, `shared/list-state.js`
  (`renderListOrState`), `ctx.eventBus.on('customers.account.updated')` (die Kundendaten kommen jetzt live über
  `customer_accounts`), die Modul-Abhängigkeitssperre über `ctx.modules` (`REQUIRED_MODULES`, Karte „Rechnungen benötigt
  weitere Module“ + „Neu laden“) und „ctx.db fehlt“ → Abhängigkeiten deklariert das Manifest (OFFEN-3).
- **Befehle**: `api.command(type, {...Handler-Payload, client_context}, {record_id})`, nie warten. Das frühere
  `STATE.busy` (Buttons gesperrt, „another command is in flight“) und alle `await refresh()` entfallen. Buchen bleibt
  zweistufig: `invoices.invoice.post` wird erst am gepushten `done` des vorangehenden `update` abgesetzt (keine
  Blockade der UI, Reihenfolge wie bisher). Löschen fragt über `api.shell.dialog({kind:'confirm', default:false})`
  statt `window.confirm`.
- **UI-Fehler aus dem Handler-Bericht behoben**: `invoices.payment.allocate` sendet **kein leeres `payment_id`** mehr
  (`buildAllocatePaymentCommand`; ohne Kennung entfällt das Feld, der Handler bildet je Befehl eine eigene
  Zuordnungs-ID statt `alloc__<invoice>` → zweite Zahlung ohne Kennung wird nicht mehr verworfen, V4-HANDLERS I8).
- **Entwurfs-Editor** arbeitet auf einer Kopie des Live-Entwurfs (`STATE.edit {id, dirty, inv}`): Live-Zeilen sind
  Schnappschüsse; ungespeicherte Eingaben überleben Live-Refresh und Remount, nach `done` des Speicherns folgt die
  Kopie wieder dem Live-Stand. Zellen schreiben auf `input` (nicht erst `change`) in die Kopie, damit `snapshot()` die
  offene Eingabe trägt. Fachlogik (Summen, Validierung, Payloads) unverändert.
- **Zustandstexte** raus: Fehlerstreifen `[data-invoices-banner]`/`STATE.lastError`, „Noch keine Rechnungen
  vorhanden.“, „Wähle eine Rechnung …“ (index.js, index.html, locales `empty_hint`/`list_empty`), „Daten werden
  synchronisiert.“, Import-Toast „{count} importiert“. In die Zone: `invoices.command.<type>`,
  `invoices.create.no_customer`, `invoices.post.invalid`, `invoices.import.invalid|empty`, `invoices.export.failed`,
  `invoices.xrechnung.failed`, `invoices.read.<collection>` (Abfrage abgelehnt).
- **Shell**: `setTitle('Rechnungen')`; `autoWirePaneGrammar(root)` ruft die App; Sprache über `api.shell.locale` (live,
  Rahmen wird neu beschriftet) + `t('invoices.<key>')` mit eingebettetem `COPY` als Rückfall.
- **Imports** `ctox:shared/stable-dom.js`, `ctox:shared/pane-grammar.js`. **Markup** über `fetch(new URL('./index.html',
  import.meta.url))` ohne `?v=`, Rückfall programmatischer Rahmen; `ensureModuleStyles()` entfernt.
- **CSS** unter `[data-app="invoices"]` (64 Selektoren, 25 Shell-Vorfahren ersetzt); Regel `.invoices-error-banner`
  entfernt.
- **schema.js gelöscht**: identisch zum v3-Stand (`~/.local/state/shell-v4-src/4794af03d`, `diff` leer) und dort gegen
  `collections.schema.json` verglichen: 22/22 Collections, gleiche `version`/`primaryKey`/`required`/Properties, keine
  Migrationen → 0 Abweichungen. (Im Klon ist `buchhaltung/schema.js` schon entfernt, deshalb Vergleich im v3-Stand.)
- **Snapshot** `{v:1, selectedInvoiceId, userCollapsed, search, view, band, filterType, tab, edit, paymentForm,
  listScrollTop}`.
- **Tests**: neue `tests/_v4-node.mjs` (Node-Auflösung `ctox:shared/`, `createTestApi`, Fake-DOM); `mount-unmount`,
  `editor-validation`, `ui-totals` auf v4 umgestellt (Readiness-Test → „leere Liste rendert `''`“, neu:
  `buildAllocatePaymentCommand` ohne leere ID).

## Befehlstypen der UI – Einstufung (§16.13)

Alle 23 Typen: `V4-HANDLERS.md` (Strang H5, alle Fachlogik, 10 davon treue Stubs). Von der UI gesendet:
`invoices.invoice.create|update|delete|post`, `invoices.payment.allocate`, `invoices.dunning.run` – alle Fachlogik mit
Handler; keine Fähigkeit, kein Legacy.

## OFFEN (nummeriert)

1. **Mahnlauf-Rechnungsfilter**: die UI schickt `filter:{invoice_id}` (gewählte Rechnung). Der Handler
   `invoices.dunning.run` wertet nur `filter.state`/`filter.min_open_cents` aus (`_lib/invoices.js listDueInvoices`) –
   der Lauf mahnt weiter **alle** überfälligen Rechnungen (V4-HANDLERS I7). Payload unverändert gelassen; Unterstützung
   von `filter.invoice_id` (oder `invoice_ids`) gehört in den Handler-Strang.
2. **Schema-Befunde I4–I6** (Handler-Bericht): `accounting_payment_allocations.payment_id` ist Pflichtfeld `string`,
   der Handler schreibt bei fehlender Kennung `null` → unter Schemaprüfung scheitert die Zuordnung ohne Zahlungs-ID
   weiterhin (jetzt sichtbar in der Zone statt still verworfen). Lösung im Handler-/Manifest-Strang.
3. **Abhängigkeiten** `buchhaltung`/`customers`: v3 prüfte `ctx.modules` und zeigte eine Sperrkarte. 4.0 hat keine
   Modulliste; die Fremd-Collections (`customer_accounts`, `accounting_journal_*`, `accounting_number_series`) gehören
   ins Manifest (`foreign_collections`, Strang v4/manifests). Lehnt der Core eine Abfrage ab, meldet die App
   `invoices.read.<collection>` in die Zone.
4. **Mehrere Fenster**: Modulzustand ist modulweit (wie knowledge OFFEN-4).
5. **Debug-Griff** `window.__ctoxInvoicesModule` (mount/inspect) bleibt, weil die Bestandssuite ihn prüft; er liegt
   global auf `window` (nicht im App-root). Entscheidung Shell-Strang, ob er unter v4 entfällt.
6. **Werkzeugbefund** `v4/tools/port/test-api.mjs createTestApi`: `shell` ist `Object.freeze`d, `shell.setTitle`
   schreibt aber `shell._title` → `TypeError: object is not extensible` bei jedem `setTitle`. Umgangen in
   `tests/_v4-node.mjs` (Kopie mit eigenem `setTitle`); Korrektur gehört Strang K.
7. **Fremde Verweise**: `src/core/rxdb/tools/browser_rust_smoke.js:8097, 11928` lädt `/modules/invoices/schema.js`
   (v3-Rauchtest, nicht in meinem Schreibbereich) – bricht nach der Löschung.
8. Shell-Lücken wie support OFFEN-5 (Resizer, Container `business-app-window`, v2-Iconzone im Kopf).

## Wortlaut zurückgedreht (Strang LBL, 15.10.2026 15:45)

- Keine wächterbedingte Umbenennung gefunden: upstream enthielt nur „Daten werden synchronisiert.“/„Syncing data.“ (`syncingHint`, echter Sync-Zustandstext, bleibt entfernt). Keine Datei geändert außer diesem Nachtrag.
- Original = `upstream-main`. Markierung `v4-allow: label` (JS: Kommentar in derselben Zeile, HTML: `<!-- v4-allow: label -->` davor, JSON: Markerschlüssel `"//<schlüssel>": "v4-allow: label"` davor) nur an Zeilen, die der Wächter sonst meldet. Prüfung: v4-api-Test gegen Stub-Pin cff5ba5 grün, `scan.mjs --app <id>` unverändert, guard-v4 ohne Treffer außerhalb `handlers/` (12/12).
