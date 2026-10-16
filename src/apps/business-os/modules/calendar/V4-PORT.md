# calendar – Port auf App-API 4.0 (Strang UB1, Welle B)

Stand 15.10.2026 14:15 (Berlin, gemessen). Vorlage: `v4/tools/port/PORTING.md`, Pilot knowledge.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app calendar` | **0 Stellen**, Exit 0 (vorher 64, CSS 155 ungescoped, 29 mit Shell-Vorfahr) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs calendar modules/calendar/index.css --check` | gescoped |
| v4-Test (Stub-Pin 66edb98) | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test src/apps/business-os/modules/calendar/tests/v4-api.test.mjs` | 9/9 grün |
| Bestehende Suite | `node --test modules/calendar/calendar.test.mjs` (aus Scratch-Kopie mit node_modules-Link, Klon hat kein esbuild) | 16/16 grün; esbuild-Bündel mit `ctox:shared/`-Plugin, Readiness-Test auf v4-Vertrag umgestellt |
| guard-v4 (modules/calendar) | gefiltert | vorher 20, jetzt **0** |
| Screenshots | `tests/shots/calendar-1280.png`, `calendar-420.png` | angesehen: 1280 Liste + Monatsansicht mit Termin, Zähler Monat/Woche/Tag (1); 420 gestapelt, kein horizontaler Überlauf |

v4-Test deckt ab: mount zeigt Kalender + Termin · `sim.push` Kalender erscheint/verschwindet · leere Instanz: kein
Lade-/Leertext, keine Zone (Standardkalender werden wie bisher angelegt) · neuer Kalender über das Formular →
`api.put`, live sichtbar, Outbox leer · Löschen fragt über `api.shell.dialog` (default false → bleibt; mit Ja →
`api.remove`) · ungültiger Import → genau ein Zoneneintrag `calendar.import.invalid` (früher `ctx.notifications`) ·
snapshot → Remount erhält Monat/Woche-Ansicht und offenes Kalenderformular mit Eingabe · unmount: 0 Abos, root leer.
Calendar sendet **keine Befehle** (kein `commandBus` im Bestand) – daher kein Befehlstest.

## Was sich geändert hat

- `mount(root, api, {restored})` → `{unmount, snapshot}`; 5 Live-Abos (`calendar_calendars/events/booking_pages/
  booking_holds/bookings`) statt `$.subscribe` + Readiness; Formular-Kalenderliste ebenfalls `api.query(...).subscribe`.
- Adapter `calendarDb()` (`rows/byId/add/change/drop` = api.*): 14 `insert/upsert`, 10 `findOne`, 8 `find`, 1 `count`,
  `doc.patch/remove` umgestellt; `toJSON()` entfällt.
- Zustandstexte: Ladeblock + `syncingData`, linker Leerzustand („Keine Einträge …“, index.html), Badge „Keine lokale
  Datenbank verbunden“ und `importUnavailable` entfernt; Import-Schreibfehler → `calendar.import.failed`.
  `ctx.notifications` → Zustandszone (`calendar.import.invalid|empty|done|failed`).
- `confirm()` (4×: Termin löschen/stornieren, Buchungsseite, Kalender) → `api.shell.dialog({kind:'confirm', default:false})`.
- Locales: die 38 Texte, die nur in `locales/*.json` standen (HTTP-Laden entfällt), sind jetzt in `labels` eingebettet;
  `api.shell.t('calendar.<key>')` übersteuert. Live-Sprachwechsel über `api.shell.locale.subscribe`.
- Icons: `ctx.getActionIcon` entfällt (OFFEN-D), app-lokale Glyphen waren schon Rückfall.
- Imports `ctox:shared/stable-dom.js`, `ctox:shared/pane-grammar.js` (`autoWirePaneGrammar(root)` im mount);
  `index.html` ohne `?v=`; Modul-CSS lädt die Shell. Vendor EventCalendar/RRule (JS + CSS aus `/vendor/`) lädt die App
  weiterhin selbst (kein Datenweg).
- CSS gescoped; App-root ist Größencontainer `business-app-window`.
- **schema.js gelöscht**: gleich zu `collections.schema.json` (8 Collections, Version, `required`, Felder, keine
  Migrationen; Vergleich per Skript).

## Befehlstypen

Keine. Rust-Bezüge auf `calendar_*`-Collections in `src/core` (.rs): 4 Zeilen (nicht einzeln eingestuft).

## OFFEN

1. **Mehrdokument-Vorgänge**: Kalender löschen (alle Termine + Kalender), Buchung stornieren (Buchung + Termin),
   Buchungsseite + Verfügbarkeitsregel schreiben mehrere Dokumente ohne Transaktion → Handler-Kandidaten.
2. **Präsenz** („X bearbeitet gerade“, `ctx.presence`): in App-API 4.0 nicht vorhanden; Veröffentlichung und Hinweis
   entfallen (Funktion `updateEventFormPresenceHint` bleibt, sieht aber keine Gegenstellen). Wunsch an „ctox sync v4“.
3. **Mehrere Fenster**: Modulzustand modulweit (wie knowledge OFFEN-4); `document`-Keydown-Listener für Escape.
4. **Vendor-CSS** (`vendor/event-calendar/event-calendar.min.css`) sollte ins Manifest (`styles`), statt per `<link>`
   in `document.head` (Manifest-Strang).
5. **Bild 420/1280**: EventCalendar-Kopfknöpfe (rechts oben) ohne Beschriftung; Kontextspalte „Buchungen“ bei ≤1024 px
   immer sichtbar (bewusste Regel im CSS) mit gekürztem Kopf, weil die Shell-V2-Kopfzonen (Icon 80 px, Fenster-
   Steuerung 74 px) noch reserviert werden – ob die v4-Shell diese Zonen weiter überlagert, entscheidet der Shell-Strang.
6. Seed-Standarddaten beim ersten Öffnen (unverändert) sollten in v4 ein Befehl/Handler sein.

## VENDOR (Strang VENDOR, 15.10.2026)
- event-calendar und rrule liegen jetzt in `modules/calendar/vendor/` (verschoben aus `business-os/vendor/`); index.js lädt `./vendor/…`.
- **Manifest-Bedarf `styles`:** `vendor/event-calendar/event-calendar.min.css` gehört in die `styles`-Liste des calendar-Manifests (heute noch per `<link>` aus `loadStylesheetOnce`, index.js:445). Nachziehen im Manifest-Strang (gen-manifest), dann den `<link>`-Weg streichen.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- Termine kaum lesbar: `--ec-event-text-color` zeigte auf das nirgends definierte `--accent-contrast` → dunkle Schrift auf satter Terminfarbe. Jetzt Rückfall Weiß und je Termin `textColor` aus der Leuchtdichte der Terminfarbe (`eventTextColor` in calendar-view-adapter.js).
- G4: Woche/Tag/Liste brechen Titel um; im Monatsraster bleibt eine Zeile mit Ellipse (fachlich gewollt, feste Tageszelle) mit `data-v4-allow-ellipsis` + Tooltip; „+N more“ → „+N weitere“.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app calendar --shared` (Stub-Pin 9fa9948) → alle Läufe OK; `node --test modules/calendar/tests/v4-api.test.mjs` grün; `scan.mjs --app calendar` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.
