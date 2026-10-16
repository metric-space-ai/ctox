# news – UI-Port auf App-API 4.0 (Strang UB3, Welle B)

Stand: abgeschlossen 15.10.2026 14:16 (Berlin, gemessen). Start 14:03. Handler (`handlers/**`, `V4-HANDLERS.md`) gehören
Strang H5 und sind unberührt.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app news` | vorher 31 Stellen + 187 CSS-Selektoren ungescoped (51 mit Shell-Vorfahr) → **0 Stellen**, Exit 0 |
| CSS-Scope | `node v4/tools/port/scope-css.mjs news modules/news/index.css --check` | gescoped |
| Löschlisten-Wächter | `guard-v4.mjs --json` (Filter `modules/news/`, ohne `handlers/`) | vorher 12 → **1** (Fehltreffer, OFFEN-1) |
| v4-Test gegen Stub-Pin | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test modules/news/tests/v4-api.test.mjs` | **8/8 grün** |
| Bestehende Suite | `node --test modules/news/tests/news.test.mjs` | **12/12 grün** (Schema-Test auf collections.schema.json umgestellt) |
| Screenshots | `NEWS_SHOTS=1 …` | `tests/shots/news-1280.png`, `news-420.png` angesehen: 3 Spalten ohne v2-Freiräume, 420 gestapelt |

v4-Test deckt ab: mount (4 Abos, Titel „News“, Themen + Feed) · `sim.push` live, Löschung verschwindet · leere
Collections: Themenliste, Feed und Feed-Titel leer, keine Zone · **Befehl** `news.issue.generate`: Ausgabe lokal
angelegt, `api.command` ohne Warten (Payload mit `topic_id`, `record_id`), `sim.failCommand` → genau ein Zoneneintrag
`news.command.news.issue.generate`, Ausgabe `failed` mit `command_id = h.id`, kein Fehlerkasten im Feed; zweiter Lauf
`done` räumt den Eintrag · Merken per `api.patch` · neues Thema über den Drawer per `api.put` · snapshot →
structuredClone → Remount öffnet den Themen-Drawer mit ungespeicherter Eingabe wieder, Auswahl bleibt, alte Abos
abgemeldet · unmount löst alle Abos, leert root · keine `pageerror`.

## Was sich geändert hat

- **Einstieg** `mount(root, api, {restored})` → `{unmount, snapshot}`; Zustand je mount zurückgesetzt, Abos je Montage.
- **Daten**: 4 Live-Abos `api.query('news_*', {})` statt `find().exec()` + `$.subscribe` + Neuladen der Collection je
  Änderung; Rendern entprellt (60 ms). Schreiben `api.put/patch/remove` (`write()`), Einzellesen `api.get().get()`.
- **Befehle** (`commandBus.dispatch` → `api.command`, nie warten): Umschlag aus `core/news-model.mjs` →
  `api.command(command_type, {...payload, record_id, client_context, sync_collections})`. Status live: `failed` →
  `news.command.<type>` (error), `done` → räumen. „Auftrag an CTOX übergeben“ entfällt. Ausgabe erzeugen: Ausgabe
  lokal `queued`, Befehl, danach `command_id = h.id`; `failed` setzt die Ausgabe auf `failed` (wie vorher bei
  Dispatch-Fehler). Thema löschen: Zeitplan-Löschbefehl ohne Warten, Datensätze lokal entfernen (vorher Abbruch, wenn
  der Dispatch scheiterte).
- **Zustandstexte raus**: Leerzustände (erstes Thema, keine Themen/Beiträge in der Ansicht, noch keine Ausgabe, „Kein
  Thema ausgewählt“), „Ausgabe wird erstellt …“ (→ `news.issue.running`, info), „Ausgabe fehlgeschlagen“ (→
  `news.issue.failed`, error, mit Fehlertext), Quellen-Pille „in Arbeit“ (= laufender Befehl, steht in der Zone).
  Hinweise (`notify`) → Zone mit stabilen Codes (`news.write.<coll>`, `news.topic.invalid`, `news.source.unsaved`,
  `news.import`, `news.import.done`, `news.link.copied`). 13 Locale-Schlüssel entfernt.
- **Navigation**: `sessionStorage` + `location.hash = '#ctox?…'` → `api.shell.navigate('ctox', {task_id, command_id,
  task_status, source:'news', drawer:true})`.
- **Shell**: `setTitle('News')`; Sprache über `api.shell.locale` + `t('news.<key>')` mit eingebetteten Rückfalltexten
  (`shared/i18n.js` entfällt); `autoWirePaneGrammar(root)` ruft die App.
- **Imports** `ctox:shared/stable-dom.js`, `ctox:shared/pane-grammar.js`; `core/news-model.mjs` relativ (unverändert).
- **Markup/CSS**: `ensureStyles`/`?v=` entfernt; CSS unter `[data-app="news"]` (51 Shell-Vorfahren ersetzt);
  v2-Freihaltungen für Icon (80 px links), Fensterknöpfe (`--news-controls`) und Drawer-Versatz unter die Kopfzeilen
  entfernt. Der 64-px-Abstand der Drawer-Fußleiste (Chat-Blase der Shell) bleibt, bis der Shell-Strang die Blase klärt.
- **Snapshot**: Auswahl (Thema/Ausgabe), beide Grammatiken (Suche/Band/Filter), Ansichten, aufgeklappte Beiträge,
  Feed-Scroll und der offene Themen-Drawer (alle Formularwerte, Quellen-Entwürfe, entfernte Quellen, Tab).
- **schema.js gelöscht**: 4 Collections, Versionen, `required`, Feldmengen identisch mit collections.schema.json
  (gemessen); Migrationen leer auf beiden Seiten.

## Befehlstypen – Einstufung (§16.13)

Einstufung und Handler: siehe `V4-HANDLERS.md` (Strang H5; alle 4 GEMISCHT, portiert). UI-Sicht, Rust-Treffer
`grep -rn '"<type>"' src/core --include=*.rs` (gemessen 14:15):

| Befehlstyp | Rust-Treffer | gesendet von |
|---|---:|---|
| `news.issue.generate` | 1 | „Ausgabe erzeugen“ |
| `news.topic.schedule` | 4 | Thema speichern (Zeitplan geändert), Import, Löschen (`deleted:true`) |
| `news.source.adapter` | 1 | Quelle einrichten |
| `news.source.auth_assist` | 4 | Quelle anmelden |

## OFFEN

1. **Wächter-Fehltreffer**: `vocab:en-loading` trifft das HTML-Attribut `loading="lazy"` am Beitragsbild
   (index.js:690). Kein Zustandstext; nicht umgangen. → Vokabelliste des Wächters (Attribut ausnehmen).
2. **Befehls-ID im Anweisungstext**: `buildGenerateCommand` (core/news-model.mjs, nicht in meiner Liste) schreibt eine
   selbst erzeugte `command_id` in die Agent-Anweisung (`--task-id`). In v4 vergibt der Kernel die ID (`h.id`); die
   Ausgabe trägt danach `h.id`, die Anweisung aber die alte. Der Handler-Strang braucht die ID vor dem Befehl
   (V4-HANDLERS „`ctx.command` mit fester ID“) – für UI-Befehle offen: `api.command` mit vorgegebener ID oder
   Anweisung im Handler bauen.
3. **Quellen-Pillen** „Anmeldung nötig/Anmeldung offen/Fehler/wird repariert“ bleiben (Datensatzzustand mit Aktion);
   Owner-Entscheid, ob das unter „keine Status-Pills“ fällt.
4. Modulzustand ist modulweit (zweite Montage übernimmt, wie knowledge OFFEN-4).
5. Kategorie-Chips im Drawer (`aria-pressed`, keine Formularelemente) sind nicht im Snapshot.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G4: Themen-Titel und -Zeile brechen um.
- G3 bleibt: Zone „Ausgabe fehlgeschlagen: <Titel>“ stammt aus einer Fixture-Ausgabe mit Fehlerstatus – korrekte Meldung ohne ID, kein Darstellungsmangel.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app news --shared` (Stub-Pin 9fa9948) → G4 grün; G3 nur die berechtigte Fehlermeldung; `node --test modules/news/tests/v4-api.test.mjs` grün; `scan.mjs --app news` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.
