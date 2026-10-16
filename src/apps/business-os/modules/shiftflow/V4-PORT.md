# shiftflow – Port auf App-API 4.0 (Strang UB3, Welle B)

Stand: abgeschlossen 15.10.2026 14:02 (Berlin, gemessen). Start 13:38.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app shiftflow` | vorher 145 Stellen + 224 CSS-Selektoren ungescoped → **0 Stellen**, Exit 0 |
| CSS-Scope | `node v4/tools/port/scope-css.mjs shiftflow modules/shiftflow/index.css --check` | gescoped |
| Löschlisten-Wächter | `node v4/tools/guards/guard-v4.mjs --json` (Filter `modules/shiftflow/`) | vorher 10 (+ R2-Treffer) → **0** |
| v4-Test gegen Stub-Pin | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test modules/shiftflow/tests/v4-api.test.mjs` | **8/8 grün** |
| Bestehende Suite | `node modules/shiftflow/test.mjs` (über `tests/shiftflow.test.mjs`) | **11/11 grün** (auf v4-Vertrag umgestellt) |
| Fachkern | `node --test modules/shiftflow/core/*.test.mjs` | 5 + 3 + 5 grün (unverändert) |
| Screenshots | `SHIFTFLOW_SHOTS=1 …` | `tests/shots/shiftflow-1280.png`, `shiftflow-420.png` angesehen |

v4-Test deckt ab: mount (4 Abos, Titel „Einsatzplanung“, Liste + Planungsraster) · `sim.push` live, Löschung
verschwindet · leere Collections mit Rückstand: Liste/Konflikte/Zeiterfassung/Abrechnung leer, keine Zone, kein Seeding
· neue Schicht über den Drawer **im App-root** → `api.put`, Drawer schließt · `api.patch` → `accepted`, live ·
Veröffentlichen fragt über `api.shell.dialog` (ohne Ja nichts), mit Ja alle Entwürfe `published`, genau ein
Zoneneintrag `shiftflow.publish`, keine Statuszeile im Inhalt · snapshot → structuredClone → Remount stellt offenes
Schicht-Formular (Notiz-Entwurf), Auswahl und Ansicht wieder her, alte Abos abgemeldet · unmount löst alle Abos, leert
root · keine `pageerror`.

## Was sich geändert hat

- **Einstieg** `mount(root, api, {restored})` → `{unmount, snapshot}`; `ctx` überall durch `api` ersetzt.
- **Daten**: 4 Live-Abos `api.query('planning_*', {})` statt `find().$`; Neuzeichnen einmal je Mikrotask. Alle
  Einmal-Lesungen (`find().exec()`, `findOne().exec()`) → `store.all/one` = `api.query(...).get()` / `api.get(...).get()`
  (lokal, synchron, ohne Zeilendeckel). `insert/incrementalPatch/remove/upsert` → `api.put/patch/remove`
  (`store.put/patch/remove`, `local`-Ablehnung → `shiftflow.write.<collection>` in die Zone). Import-Upsert:
  vorhandene Schicht `patch`, sonst `put` (wie vorher `upsert`/`incrementalPatch`).
- **Readiness raus**: `subscribeCollectionReadiness`, `latestReadiness`, `dataDrivenEmptyHtml` und alle Leertexte
  („Daten werden synchronisiert.“, „Niemand im Dienst“, „Keine Projekte angelegt“, „Keine aktiven Konflikte …“,
  „Alle eingereichten Stundenzettel wurden freigegeben! 🎉“, „Keine Einträge“ …) entfernt: leere Quellen rendern nichts.
- **Rechte-Spiegel** `ctx.permissions.canRead/WriteCollection` entfällt (Core entscheidet, R9).
- **Seeding** (Fachverhalten, Demo-Datensatz bei leerer Mitarbeiter-Collection) bleibt, jetzt nur bei
  `api.metrics.get().backlog === 0` (Entscheidung dieses Strangs, wie notes).
- **Zustandstexte**: Veröffentlichungsstatus (`#publishScheduleStatus`, „Veröffentliche…/… veröffentlicht/
  fehlgeschlagen/bereits veröffentlicht/keine Schichten“) → `shiftflow.publish` in der Zone; `ctx.notifications`
  (Import, Auto-Plan, Konfliktprüfung) → `shiftflow.import`, `shiftflow.import.done`, `shiftflow.autogenerate.done`,
  `shiftflow.conflicts.check`. Locale-Schlüssel `syncingData` und das unbenutzte `aiCheckWarning` entfernt.
- **Dialoge** `confirm` (5×) → `api.shell.dialog({kind:'confirm', default:false})`; `alert` (4×) →
  `api.shell.dialog({kind:'alert'})` ohne Warten.
- **Drawer** `ctx.openLeft/RightDrawer/closeDrawers` → app-eigener Drawer im root (`.shiftflow-app-drawer`, CSS in
  index.css). **Icons** `ctx.getActionIcon` entfällt: die Knöpfe tragen ihre SVG-Glyphen bereits im Markup.
- **Sprache**: `loadModuleMessages` → App-eigene `./locales/<lang>.json` relativ per `fetch(new URL(...))` (R10: Artefakt
  der App, kein ?v=), überlagert von `api.shell.t('shiftflow.<key>')`; Wechsel live über `api.shell.locale.subscribe`.
- **Shell**: `setTitle('Einsatzplanung')`; `autoWirePaneGrammar(root)` ruft die App selbst (`ctox:shared/pane-grammar.js`).
- **Markup/CSS**: `ensureStyles`, `MOD_BUILD`, `?v=` entfernt; CSS unter `[data-app="shiftflow"]`; v2-Freihaltungen
  (Icon-Zone 80 px, Fensterknopf 70 px, gestapelte Einrückungen) entfernt (Chrome liegt in v4 außerhalb von root).
- **Snapshot**: Woche, Ansicht, Timeline-Fokus, Auswahl (Schicht/Mitarbeiter), Abteilungsfilter, Listenzustand
  (Suche/Band/Filter/Ansicht), Abrechnungszeitraum und offenes Drawer-Formular (Art, Datensatz, alle Feldwerte). Das
  Formular wird nach dem Remount geöffnet, sobald die Live-Daten den Datensatz enthalten.
- **schema.js gelöscht**: 5 Collections, Versionen 0, `required` und Feldmengen identisch mit collections.schema.json
  (gemessen per Skript); Migrationen leer auf beiden Seiten. Keine Abweichung.

## Befehlstypen – Einstufung (§16.13)

shiftflow sendet **keine** Befehle (kein `commandBus`, kein `api.command`; `business_commands` steht nur in
`module.json`). Rust-Bezüge auf `planning_*` in `src/core/**/*.rs`: 1 Datei, 3 Treffer, alle in einem Test in `src/core/business_os/rxdb_peer.rs` (Zeilen 17788–17793, gemessen) – kein app-spezifischer Laufzeitcode. Keine Handler nötig.

## OFFEN

1. Modulzustand ist modulweit (`appApi`, `appRoot`, `latest*`): eine zweite Montage übernimmt; nach deren unmount ist
   die erste ohne API (im Test gemessen, Remount nötig). Mehrfenster braucht Zustand je Instanz (wie knowledge OFFEN-4).
2. `globalThis.CTOX_LAST_AGGREGATION` (Rechnungsentwurf-Export liest die letzte Abrechnung aus einer globalen Variable)
   – fensterübergreifend geteilt; sollte Modulzustand je Montage werden. Fachverhalten unverändert gelassen.
3. 420 px: Tab-Leiste und „Dienstplan veröffentlichen“ laufen im Planungskopf über den Rand (Inhalt nicht umgebrochen);
   das ist die bestehende v2-Geometrie der App, nicht durch den Port entstanden → Geometrie-Labor.
4. Die App-CSS hängt an der Container-Query `business-app-window` (Shell setzt den Container um root).
5. Hinweise in der Zone (`info`) bleiben stehen, bis die Shell sie räumt (die App räumt `shiftflow.publish` bei der
   nächsten Veröffentlichung neu); ob `info`-Einträge selbst verfallen, regelt der Shell-Strang.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G1: Wochenraster hatte bei schmalem Fenster `min-width: 784px`; jetzt Container `shiftflow-week` am Planer: kompakte Spalten (`minmax(0,1fr)`), Kurz-Wochentage, Schichtkarten schrumpfen; Aktionsleisten brechen um.
- G4: Mitarbeitername bricht um.
- Offen: 900/420 bleiben 3 px über (513 > 510); bei 420 ragt der Planer rechts aus dem sichtbaren Bereich (So-Spalte, Reiter „Abrechnung“, Knopf abgeschnitten) – Pane-Breite, vom Labor nicht als G1 erfasst.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app shiftflow --shared` (Stub-Pin 9fa9948) → 1280 OK, 900/420 G1 offen (3 px); `node --test modules/shiftflow/tests/v4-api.test.mjs` grün; `scan.mjs --app shiftflow` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.

## Nachtrag Strang VIS2 (15.10.2026)

- 420 px, Planer ragte rechts heraus: `.shiftflow-center-pane` (Kit-`display: grid`) hatte keine Spaltenvorlage, die implizite auto-Spur wuchs auf 461 px Inhaltsbreite. Jetzt `grid-template-columns: minmax(0, 1fr)`.
- Hauptreiter: Kit-`.ctox-pane-tabs` ist `inline-flex` mit `min-width: max-content` (437 px), die vorhandene 2-Spalten-Vorlage griff nie. Schmal (≤ 768) jetzt `display: grid; min-width: 0` → 2×2 in Panebreite.
- Aktionsleiste: umgebrochen schrumpfte sie im Spaltenfluss auf die Kit-Mindesthöhe 36 px, „Dienstplan veröffentlichen“ lag halb unter dem Wochenkopf → `flex-shrink: 0`.
- 3 px Überstand 900/420: „+ Schicht“ (64 px, absolut zentriert) war breiter als die kompakte Tageszelle (47 px). Im Container `shiftflow-week ≤ 760` jetzt ein 26-px-„+“-Knopf (`::before`), der Name „+ Schicht“ bleibt für Bildschirmleser.

Prüfung: Labor `--shared` (Stub-Pin 9fa9948) → 1280/900/420 OK; Messung kein Element rechts aus root; `node --test modules/shiftflow/tests/v4-api.test.mjs` 7/7, `tests/shiftflow.test.mjs` 1/1.
