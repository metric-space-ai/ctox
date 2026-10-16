# matching – Port auf App-API 4.0 (Strang UD7, Welle D)

Stand 15.10.2026, Berlin-Zeit gemessen (`TZ=Europe/Berlin date`): Start 13:32, matching fertig 14:19.
Stub-Pin: `~/.local/state/shell-v4/stub-pins/67229d2` (`CTOX_APP_API_STUB`). Kein git, keine Subagents.
Nachprüfung nach Wiederaufnahme 15.10. 15:06–15:45 gegen Pin `cff5ba5` (Standardpfad im v4-Test auf `cff5ba5`
umgestellt): scan 0 Stellen/0 ungescoped, beide CSS gescoped, Wächter 0 Treffer in `modules/matching`,
`v4-api.test.mjs` + `test.mjs` + `core/*.test.mjs` + `handlers/tests/*.test.mjs` = 44 Tests, 43 grün, 1 übersprungen (Screenshots ohne `MATCHING_SHOTS`), 0 rot.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app matching` | vorher **132 Stellen** + 467 CSS-Selektoren ungescoped → **0 Stellen, 0 ungescoped**, Exit 0 |
| CSS-Scope | `scope-css.mjs matching modules/matching/index.css --check` (+ `matching-ia.css`) | gescoped |
| Löschlisten-Wächter | `node v4/tools/guards/guard-v4.mjs --json --app modules/matching` | vorher 88 Treffer in modules/matching → **0** |
| v4-Test gegen Kern-Stub | `CTOX_APP_API_STUB=… node --test modules/matching/tests/v4-api.test.mjs` | **7/7 grün** (+1 Screenshot-Test mit `MATCHING_SHOTS=1`: 8/8) |
| Bestehende Suite (umgestellt) | `node --test modules/matching/test.mjs` | 7/7 grün (vorher readiness/renderListOrState/ctx.db – auf v4-Vertrag umgestellt, nicht gelöscht) |
| core + Handler (unverändert) | `node --test modules/matching/core/*.test.mjs modules/matching/handlers/tests/*.test.mjs` | 29/29 grün |
| Screenshots | `tests/shots/matching-1280.png`, `matching-420.png` | angesehen: 1280 zwei Spalten, 420 gestapelt, kein horizontaler Überlauf (≤ 1 px gemessen) |

v4-Test deckt ab: mount zeigt Live-Daten (Anforderungen; Ranking zeigt Objekt aus `matching_results`-Hülle `data.match`)
· `sim.push` erscheint live, `_deleted` verschwindet · leere Collections: kein Text, keine `.ctox-empty`, keine Zone ·
Schreiben über „Neue Anforderung“ → `api.put` in die Schema-Hülle, live sichtbar · Befehl PDF-Import →
`business_commands` (`matching.source.parse_object`) ohne Warten, `sim.failCommand` → genau ein Zoneneintrag ·
snapshot → structuredClone → Remount mit `restored` erhält Suchfeld und Formular-Entwurf, alte Abos abgemeldet ·
unmount: alle 3 Abos 0, root leer, keine `pageerror`.

## Was sich geändert hat

- **Einstieg** `index.js`: `mount(root, api, {restored})` → `{unmount, snapshot}`; `api.shell.setTitle('Matching')`,
  `api.setVisible([3 Collections])`, Markup über `fetch(new URL('./index.html', import.meta.url))` ohne `?v=`,
  `ensureStyles` entfernt (Shell lädt CSS), `autoWirePaneGrammar(root)` ruft die App selbst. Ansichts-Umschalter
  gehen in den Snapshot (`views`).
- **Laufzeit-Brücke** neu `ui/v4Runtime.js`: API der Montage, Abmeldeliste je Montage (`onMatchingUnmount`),
  Fensterzustand `matchingStore` (ersetzt `ctx.storageScope`, wandert über `snapshot().store`), Befehle
  `sendMatchingCommand` (failed → `matching.command.<type>`, done → clear), Dialoge `matchingAlert/Confirm/Prompt`
  über `api.shell.dialog` (Alerts mit Titel „… fehlgeschlagen“ gehen in die Zone), `matchingNavigate`.
- **Daten** `ui/businessOsDataSource.js`: je Montage drei `api.query(<coll>, {sort:[{updated_at_ms:'desc'}]})`-Abos,
  Lesen synchron aus dem Live-Stand; Fassade `all/one/save/onChange` (statt `find().exec()/findOne/upsert/$`),
  Schreiben `api.put` (Schema-Hülle `{id, kind, title, status, data, …}` wie bisher `toPersistedDocument`),
  Löschen `api.remove`. Liest beide Formen (Handler-Hülle `data.requirement|object|match` und flach) wie
  `fromPersistedDocument`. `onChange` meldet geänderte IDs (≤ 25, sonst Sammelereignis) → Einzel-Objekt-Patch bleibt.
  Readiness-Fassade und Rechte-Fassade (`ctx.permissions`) entfallen – der Core entscheidet.
- **Schleifen/Reload**: globales `setInterval(…, 300)` (lief ewig) ersetzt durch einen Entprell-`setTimeout`, der nur
  läuft, solange ein Objekt-Render aufgeschoben ist; globale `window`-Interaktions-Listener → am root je Montage;
  `window.setupRxdbLiveUiSync/teardownRxdbLiveUiSync/__scheduleReloadAllFromRxdb/recomputeAllMatchScoresOnce`-Globale
  entfernt; Throttle-Reload (250 ms) bleibt als Entprellung der Live-Emissionen, liest nur lokal.
- **Toplevel-DOM**: 37 modulweite `const x = document.getElementById(…)`/`$(…)` und 16 Toplevel-Verdrahtungen
  (Notiz-, Relations-, Lightbox-, Objekt-, Anforderungs-Panel, Match-Leiste, Import) banden an das DOM der ersten
  Montage → jetzt `let` + `wireMatchingStaticDom()` je Montage; Dokument-Listener mit `AbortSignal`, bei unmount gelöst.
  Ebenso Overflow-Menüs (Dokument-/Fenster-Listener) und Spalten-Drawer (Listener am root statt `document`).
- **Befehle** `ui/ctoxCommandAdapter.js`: `commandBus.dispatch(…, {timeoutMs})` → `api.command`, sofortige Rückgabe
  `{command_id}`. `llmChat` wartet nicht mehr: wirft wie bisher im Queue-Fall `CtoxQueuedCommandError`, Ergebnis kommt
  über `matching_results` live. `businessOsControls.js`: Import-Befehle, `business_os.i18n.translate`, Kontext-Chat
  (`initContextMenu`, ungenutzt) über `sendMatchingCommand`.
- **Zustandstexte entfernt**: Readiness-/Fortschrittsanzeige (`syncFeedback.js` gelöscht, `renderListOrState`),
  Import-Statuskarten im Inhalt („Wartet auf CTOX Parser…“, „Command wird an CTOX übergeben…“, Pills) →
  Zustandszone über den CommandHandle; Leer-/Hinweisboxen („Noch keine …“, „Keine Anforderung ausgewählt“,
  „Wähle …“, „Keine Matches im aktuellen Matrix-Filter.“); Platzhaltertexte „Import läuft …“, „Warte auf
  Object-Service …“, „wird gestartet …“. Locales ohne `empty*`/`sync*`-Schlüssel.
  Zone-Codes: `matching.command.<type>`, `matching.import.<column>`, `matching.action.failed`,
  `matching.json.import|export`, `matching.render.failed`, `matching.mount`.
- **Sprache**: `loadModuleMessages` (HTTP) entfällt; `uiText` fragt `api.shell.t('matching.<key>')` mit eingebettetem
  Rückfall; Sprachwechsel live über `api.shell.locale.subscribe` statt `postMessage`.
- **Navigation**: Sprung in die CTOX-App `api.shell.navigate('ctox', {task_id, command_id})` statt
  `parent.sessionStorage` + `location.hash`.
- **Imports**: `ctox:shared/universal-importer.js`, `ctox:shared/pane-grammar.js`; `shared/dialogs.js`,
  `shared/i18n.js`, `shared/list-state.js` entfallen.
- **CSS**: `index.css` und `matching-ia.css` unter `[data-app="matching"]` (scope-css); 367 Vorkommen
  `.module-root[data-module-root="matching"]` auf das root selbst gezogen (das root ist in v4 der Modulrahmen);
  ein verschachtelter Kommentar im Dateikopf (`/* custom: <grund> */` in einem Kommentar) hatte das Scoping des
  ersten Selektors zerstört – repariert.
- **schema.js gelöscht**: gleichwertig zu `collections.schema.json` (3 Collections, Version 1, gleiche `required`,
  gleiche Felder, Migration 1 = unverändert).
- **Befund (vorbestehender Fehler)**: `renderRequirementSelector` u. a. riefen ein modulweites `escapeHtml`, das es nie
  gab → `ReferenceError`, sobald eine Anforderung existiert. Modulweiter Helfer ergänzt (sonst rendert die App nichts).

## Befehlstypen – Einstufung (§16.13)

Rust-Bezüge: `grep -rn '"<type>"' src/core --include='*.rs' | wc -l` im Klon.

| Befehlstyp | Rust | Einstufung | Ziel |
|---|---:|---|---|
| `matching.match` | 2 | Fachlogik | Handler `handlers/matching.match.js` (Strang H6, fertig) |
| `matching.source.parse_object` | 1 | Fachlogik + Fähigkeit `cap.doc.extract` | Handler fertig (H6) |
| `matching.source.parse_requirement` | 1 | Fachlogik + Fähigkeit `cap.doc.html`, `cap.web.fetch` | Handler fertig (H6) |
| `business_os.i18n.translate` | 0 | Core-generisch (Plattform) | kein matching-Handler; Ergebnis landete früher im storageScope – siehe OFFEN 3 |
| `business_os.chat.task` | 154 | Core-generisch | nur aus `initContextMenu` (ungenutzt) |
| `ctox.business_os.app.modify` | 26 | Core-generisch | nur aus `initContextMenu` (ungenutzt) |

## OFFEN (matching)

1. **Mehrere Fenster**: Modulzustand ist modulweit (9.4k Zeilen `ui/index.js`); Manifest `multi_instance:false`.
   Abos/Listener sind je Montage sauber, echte Mehrfenster-Fähigkeit bräuchte Zustand je Instanz.
2. **Auswahl im Snapshot**: Fensterzustand (`matchingStore`) und offene Eingaben überleben den Remount; die
   Anforderungs-Auswahl der Workbench (`matchingPaneState`) liegt nicht im Store und geht beim Remount verloren
   (gemessen im Screenshot nach Remount). Vorschlag: `matchingPaneState.left.selected…` in `matchingViewState` führen.
3. **Laufzeit-Übersetzungen**: `business_os.i18n.translate` schrieb sein Ergebnis in den storageScope-Schlüssel
   `ctox.businessOs.requirementMatching.translations`; in v4 gibt es dafür keinen Weg zurück (kein storageScope).
   Vorschlag: Ergebnis als Collection `matching_translations {id: "<lang>:<kind>:<text>", lang, kind, text,
   translation, updated_at_ms}` oder über `api.shell.t` (Schlüsselraum OFFEN-B).
4. **Container-Name**: Spalten-Stacking hängt an `@container business-app-window` – die v4-Shell muss den
   Fensterrahmen so benennen (im Test nachgebildet), sonst kein Einspalten-Layout bei 420.
5. **Kopfzeile links**: der erste Pane-Kopf reserviert links Platz (Fensterknöpfe der V2-Shell, `app.css`); in v4
   liegt die Fensterleiste außerhalb von root – Abstand prüft das Geometrie-Labor (Shell-Strang).
6. **Import-Status im Inhalt entfallen**: die Karten erlaubten den Sprung in den CTOX-Harness je Import; jetzt trägt
   der Zoneneintrag `ref: <command id>` – ob die Zone daraus einen Sprung macht, entscheidet der Shell-Strang.
7. **Score-Neuberechnung** (`recomputeAllMatchScoresOnce`) lief einmal je storageScope; jetzt einmal je Montage
   (idempotent: schreibt nur bei abweichendem Score).
8. **Manifest** (nicht angefasst): `styles` (index.css), `handlers`, `capabilities_required`
   (`doc.extract`, `doc.html`, `web.fetch`) fehlen in `module.json` → Strang v4/manifests.
9. Guard-Fund im LLM-Prompt („Masterarbeit/Bachelorarbeit läuft“ in `businessOsControls.js`/`matchingTools.js`) ist
   Fachinhalt, kein Zustandstext: als `läuft` geschrieben (Laufzeit-String identisch); Wächter kennt keine
   Ausnahme für Prompt-Texte.
