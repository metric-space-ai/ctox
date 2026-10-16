# App-Port auf App-API 4.0 – Ersetzungsregeln

Vorlage für alle Port-Stränge (Shell v4, S6/S7). Pilot: `modules/knowledge` (Protokoll `modules/knowledge/V4-PORT.md`).

- **Typen**: `v4/tools/port/app-api.d.ts` = wörtlich Teil 1 des Kern-Stubs `~/.local/state/ctox-sync-v4/stub/index.d.ts`
  (`CtoxAppApi`, `LiveQuery`, `LiveDoc`, `Query`/`Selector`/`FieldOperators`, `WriteHandle`, `CommandHandle`,
  `CommandState`, `CtoxError`, `StatusItem`, `ShellApi`, `Actor`, `FilesApi`, `AppMount`, `AppMountHandle`) plus
  OFFEN-Liste (A–G). Nichts darüber hinaus verwenden.
- **Scanner**: `node v4/tools/port/scan.mjs --app <id>` listet jede umzubauende Stelle (Datei:Zeile, Regel, Vorschlag).
  Exit 0 = null Stellen. Baseline aller Apps: `SCAN-BASELINE.md`.
- **CSS**: `node v4/tools/port/scope-css.mjs <id> modules/<id>/index.css --write`, Prüfung mit `--check`.
- **Test**: gegen `createStubKernel` (Stub, read-only) in Chromium; Muster `modules/knowledge/tests/v4-api.test.mjs`.
  Eigene Helfer nur, wo der Stub nicht abdeckt: `countingApi` (Abo-Zähler), `withDialogAnswer` (Dialog-Antwort) aus
  `v4/tools/port/test-api.mjs`.

## 1. Ersetzungstabelle (Regel-ID = Scanner)

| Regel | Heute (ctx, v3) | App-API 4.0 | Hinweis |
|---|---|---|---|
| R1-mount | `export async function mount(ctx) { …; return () => cleanup }`, `onActivate` | `export async function mount(root, api, { restored }) { …; return { unmount, snapshot } }` (`AppMount`) | `mount` binnen 10 s auflösen, sonst Rückfall auf die alte Version (§16.10). Modulzustand bei jedem `mount` neu setzen; Abos **je Montage lokal** halten (siehe §4 Falle 1). |
| R2-host | `ctx.host`, `ctx.left`, `ctx.right` | `root` | Es gibt nur den Inhaltsbereich. `root.dataset.app` setzt die Shell; die App setzt es nur, wenn es fehlt. |
| R3-db | `ctx.db.collection(x).find(q).$.subscribe(cb)`, `.find().exec()`, `.findOne()`, `insert/upsert/incrementalPatch` | `api.query(x, { selector, sort, limit, skip }).subscribe(rows => …)` (`LiveQuery<T>`), `api.get(x, id)` (`LiveDoc<T\|null>`), `api.put/patch/remove` (`WriteHandle`) | Ergebnis ist schon sortiert, Gleichstand nach `id`, **kein Zeilendeckel**. `subscribe` ruft sofort mit dem aktuellen Stand. `doc.toJSON()` entfällt (Zeilen sind Plain Objects). Gelöschte Dokumente erscheinen nicht. Fremd-Collections nur mit `foreign_collections` im Manifest, sonst `forbidden`. Keine Transaktion über mehrere Dokumente → Befehl. |
| R4-sync | `ctx.sync.*`: `startCollection`, `leaseCollection`, `restartCollection(s)`, `stopCollection`, `collectionReadiness`, `subscribeCollectionReadiness`, `collectionFreshness`, `awaitInSync`, `awaitInitialReplication`, `requireRevision`, `requestNative`, `readNativeCollectionDocument`, Bridge-Interna (`bridge.state.*Loader`) | **entfällt ersatzlos** | Keine Ladefunktion, keine Readiness. Was heute über Demand-Loader nachgeladen wird, ist eine gewöhnliche Abfrage mit `skip/limit` (knowledge: `knowledge_table_rows`, OFFEN-1 in V4-PORT.md). |
| R5-command | `ctx.commandBus.dispatch({ command_type, record_id, payload, until })` → wartet auf `accepted` | `const h = api.command(type, { ...payload, record_id, client_context })` → `CommandHandle { id, status, local, result, cancel }` | **Nie warten.** UI arbeitet lokal weiter. `h.status.subscribe(s => …)`: `failed` → `api.status.report({level:'error', code:'<app>.command.<type>', text, ref: h.id})`, `done` → `api.status.clear(code)`. `h.local` lehnt mit `CtoxError{code:'invalid'}` ab → ebenfalls melden. Abo bei `unmount` lösen. `module`/`command_type` gehören nicht mehr ins Dokument (Stub legt `{id, type, payload, status, app_id, actor_id}` an). |
| R6-shell | `ctx.notifications`, `ctx.getActionIcon/getSvgIcon`, `ctx.openLeft/Right/BottomDrawer`, `ctx.closeDrawers`, `ctx.contextMenu`, `ctx.windowManager`, `ctx.eventBus`, `ctx.openDesktopApp`, `ctx.businessChat` … | `api.shell.setTitle`, `registerHeader`, `navigate(appId, params)`/`onRoute(cb)`, `dialog`/`menu`, `window.{size,mode,close}`, `openFile(ref)` | 4.0 hat **keine** Icon- und Drawer-API (OFFEN-D): Glyphen und Drawer bleiben app-lokal, Drawer werden in `root` gehängt (knowledge: `openDrawer(side, body)`). Benachrichtigungen = Zustandszone (`api.status.report`). |
| R7-dialog | `showBusinessConfirm/Prompt/Alert`, `window.confirm/alert/prompt` | `await api.shell.dialog({ kind: 'confirm', title, text, confirm_label, default: false })` | Form von `spec` offen (OFFEN-A); Stub liefert `spec.default ?? null`. Für destruktive Aktionen `default:false`. |
| R8-i18n | `loadModuleMessages(import.meta.url, lang, labels)` (HTTP), `ctx.locale`, `postMessage('ctox-business-os-language')` | `api.shell.locale.get()/.subscribe()`, `api.shell.t(key)` | Schlüsselraum offen (OFFEN-B): eingebettete Texte als Rückfall behalten, `t('<app>.<key>')` übersteuert, wenn es nicht den Schlüssel selbst liefert. Locale-Wechsel live über `subscribe`. |
| R9-actor | `ctx.session.user.role`, `ctx.canModifyModule()`, `ctx.permissions`, `ctx.user`, `ctx.actor` | `api.actor.{id, display_name, role}`, `api.actor.permissions.get()` (`LiveDoc<string[]>`) | Nur Spiegel für die Darstellung; jede Mutation entscheidet der Core. Recht für App-Änderung: `apps.source.edit`. |
| R10-http | `fetch('/api/…')`, absolute URLs, `?v=<build>`-Cache-Buster, `registry.json`, `mobile-apps.json` | kein HTTP-Datenweg | Erlaubt bleibt `fetch(new URL('./index.html', import.meta.url))` u. ä. relativ, **ohne** `?v=` – der Service Worker liefert die Artefakt-Datei versioniert (§16.15). CSS lädt die Shell aus dem Manifest (Entwurf §5 Schritt 3): `ensureStyles()`/`<link>` aus der App entfernen. |
| R11-state | `renderListOrState`, `.ctox-syncing`, „Daten werden synchronisiert.“, „… wird geladen“, „nicht verbunden“, „Command angelegt“, Fehlerkästen im Inhalt | leere Liste rendert **nichts**; Zustand nur `api.status.report({level, code, text, ref})` / `api.status.clear(code)` | Auch Platzhalter im `index.html` („Knowledge wird geladen“) und Texte in `locales/*.json` entfernen. Stabile `code`s je Zustand (`<app>.<bereich>.<fall>`), damit `clear` greift. |
| R12-timer | `setInterval`, Retry-Ketten (`[750, 1500, 3000 …]`), `promiseWithTimeout`, Poll-Schleifen | entfällt – Live-Abo statt Polling | `setTimeout` nur für Entprellung/UI. `setInterval` nur für deklarierte UI-Animation. |
| R13-nav | `location.hash = 'ctox'`, `location.reload`, `sessionStorage`-Übergaben zwischen Apps, `window.addEventListener('message')` | `api.shell.navigate(appId, params)`, `api.shell.onRoute(ev => ev.params)`; Fensterzustand → `snapshot()`/`restored` | `navigate` auf unbekanntes Ziel liefert `false` und meldet selbst in die Zone. Unbekannte Route-Parameter der eigenen App → `api.status.report` (nie stiller Rückgriff auf den ersten Datensatz). |
| R14-import | `import … from '../../shared/x.js?v=…'` | `import … from 'ctox:shared/x.js'` | Nur über den SW auflösbar (OFFEN-F: native Import-Map greift für `ctox:` nicht). Relative Importe innerhalb der App bleiben. Gemeinsame Helfer, die die App selbst verdrahten muss (knowledge: `autoWirePaneGrammar(root)`), im `mount` aufrufen statt auf DOM-Heuristik der Shell zu warten. |
| R15-guard | `wireShellV2ModuleTitle`, `createDynamicAppDataGuard`, `hasReviewedCollectionDataGrant`, `window.CTOX_BUSINESS_OS_APP` | entfällt (Löschliste Entwurf §11) | Titel über `api.shell.setTitle`. |
| R16-css | ungescopte Selektoren, `.shell-window[data-shell-contract="v2"] .x` | alles unter `[data-app="<id>"]` | `scope-css.mjs` ersetzt Shell-Vorfahren (`.shell-window…`) durch den Scope – sie liegen in v4 außerhalb von `root`. Bestehende CSS-Wächter der App weiter grün halten (knowledge: kein `box-shadow: 0…`). |
| R17-schema | `schema.js` (RxDB `addCollections`, `migrationStrategies`) | löschen; einzige Quelle `collections.schema.json` (§16.13) | Vorher vergleichen (Version, `required`, Migrationen); Abweichungen löst der Port. `business_commands` ist System-Collection – im Manifest-Strang klären, ob sie aus `collections` der App fällt. |

## 2. Snapshot-Muster (Live-Remount, §10.2a Nr. 1, Entwurf §5)

```js
const SNAPSHOT_VIEW_KEYS = ['selectedId', 'activeTab', /* Filter, Sortierung, Ansicht … */];
function snapshot() {
  const snap = { v: 1 };
  for (const k of SNAPSHOT_VIEW_KEYS) snap[k] = state[k];          // nur structured-clone-fähige Werte
  snap.flagFilters = [...state.flagFilters];                        // Set → Array
  if (state.editing) snap.markdownDraft = els.editor.value;         // offene Eingaben IMMER mitnehmen
  snap.listScrollTop = els.list?.scrollTop || 0;
  return snap;                                                      // ≤ 1 MiB
}
// mount: Ansicht sofort aus `restored` setzen; Entwürfe erst zurückspielen, wenn die ausgewählten
// Datensätze in den Live-Daten stehen (die erste Emission kann leer sein).
```

- Typ jedes Feldes beim Einlesen gegen den Anfangszustand prüfen (fremde/alte Snapshot-Fassungen tolerieren, `v`).
- Ohne `snapshot()` sichert die Shell nur Formularwerte/Fokus/Scroll generisch – für Editoren mit eigenem Zustand
  reicht das nicht.

## 3. Handler-Einstufung (§16.13)

Je Befehlstyp der App und je app-spezifischer Rust-Funktion genau eine Einstufung, dokumentiert in
`modules/<id>/V4-PORT.md`:

| Einstufung | Kriterium | Ziel |
|---|---|---|
| **Fachlogik** | Datenpflege, Validierung, Ableitung, Projektion, Befehlsablauf | JS-Handler `modules/<id>/handlers/<type>.js`, im Manifest `handlers` |
| **Fähigkeit** | Browser-Engine, Dokument-Rendering, Pentest-Werkzeuge, Mail-Transport, Dateiübertragung, Modellaufruf, Prozess, Dateiformat-Lesen (Parquet …) | `ctx.cap.<name>.*`-Wunsch an „ctox sync v4“: Signatur + Beispielaufruf, app-unabhängig benannt, im Manifest `capabilities_required` |
| **Core-generisch** | schon heute app-unabhängig im Core (z. B. `ctox.business_os.app.modify`) | bleibt; nur dokumentieren |
| **Legacy** | am Mo 05:00 offen | `ctx.cap.legacy.<command_type>`, im Manifest deklariert, auf dem Board gezählt |

Zählweise: Rust-Bezüge je Befehlstyp per `grep -rn '"<type>"' src/core` (nur `.rs`) – 0 Treffer heißt: generischer
Weg (Befehl → Queue-Task), kein app-spezifischer Rust-Code.

## 4. Fallen (gemessen im Pilot)

1. **Zwei Montagen derselben Modulinstanz**: Modulweiter Zustand (`const state = …`) plus `resetState()` im `mount`
   verliert die Abos der ersten Montage → verwaiste Abos (knowledge: 3 gemessen). Abos je Montage lokal festhalten;
   echte Mehrfenster-Fähigkeit braucht Zustand je Instanz (OFFEN-4 in knowledge/V4-PORT.md).
2. **`ctox:shared/` im Test**: native Import-Map greift nicht; Testserver muss `'ctox:shared/` → `'/shared/` wie der
   SW umschreiben, esbuild-Tests brauchen ein `onResolve`-Plugin (knowledge `test.mjs`).
3. **Eingefrorene API**: Stub-Objekte sind `Object.freeze`; Testhüllen über neue Objekte (`delegate`), nicht über
   Prototyp-Zuweisung.
4. **Shell-Vorfahren im CSS** (`.shell-window[data-shell-contract="v2"] …`) treffen nach dem Scoping nichts mehr, wenn
   man nur voranstellt – `scope-css.mjs` ersetzt sie.
5. **Vorhandene Quelltext-Wächter der App** (z. B. `test.mjs` prüft `state.ctx.openBottomDrawer(`) auf den v4-Vertrag
   umstellen, nicht löschen: die Regel dahinter („nichts außerhalb der App“) bleibt.

## 5. Abnahme-Checkliste je App

- [ ] `node v4/tools/port/scan.mjs --app <id>` → `0 Stellen`, Exit 0.
- [ ] `node v4/tools/port/scope-css.mjs <id> modules/<id>/index.css --check` → gescoped.
- [ ] `schema.js` gelöscht, `collections.schema.json` geprüft; Locales ohne Zustandstexte.
- [ ] `tests/v4-api.test.mjs` gegen `createStubKernel` grün, mindestens: mount zeigt Daten · `sim.push` erscheint live
      und Löschung verschwindet · leere Collections rendern keinen Text und keine Zone · Schreiben (`put` → `accepted`)
      · Befehl legt `business_commands`-Dokument an, `sim.failCommand` → genau ein Zoneneintrag · `snapshot()` →
      structured clone → Remount mit `restored` erhält offene Eingabe und Auswahl · `unmount` meldet alle Abos ab
      (`countingApi`) und leert `root` · keine `pageerror`.
- [ ] Bestehende App-Tests grün (auf v4-Vertrag umgestellt, nicht gelöscht).
- [ ] `V4-PORT.md`: Befehlstypen eingestuft (Fachlogik/Fähigkeit/Core-generisch/Legacy) mit gemessenen Rust-Bezügen;
      offene Punkte nummeriert.
- [ ] Danach (nicht in diesem Werkzeug): Hot-Plug-Skript, Geometrie-Labor 3 Breiten, Löschliste-Wächter.
