# buchhaltung – Port auf App-API 4.0 (Strang UB1, Welle B)

Stand 15.10.2026 13:41 (Berlin, gemessen `TZ=Europe/Berlin date`). Vorlage: `v4/tools/port/PORTING.md`, Pilot knowledge.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app buchhaltung` | **0 Stellen**, Exit 0 (vorher 107 Stellen, CSS 218 Selektoren ungescoped, 23 mit Shell-Vorfahr) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs buchhaltung modules/buchhaltung/index.css --check` | gescoped |
| v4-Test (Stub-Pin 66edb98) | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test src/apps/business-os/modules/buchhaltung/tests/v4-api.test.mjs` | 8/8 grün |
| Bestehende Suite | `node --test modules/buchhaltung/tests/buchhaltung.test.mjs` | 10/10 grün (Readiness- und v2-Overlay-Wächter auf v4-Vertrag umgestellt, nicht gelöscht) |
| guard-v4 (modules/buchhaltung) | `node v4/tools/guards/guard-v4.mjs --json` gefiltert | vorher 25, jetzt 3 – alle Fachvokabular, siehe OFFEN-5 |
| Screenshots | `tests/shots/buchhaltung-1280.png`, `buchhaltung-420.png` | selbst angesehen: 1280 dreispaltig, 420 gestapelt ohne horizontalen Überlauf des Fensters |

v4-Test deckt ab: mount zeigt Live-Daten (Journal) · `sim.push` erscheint, Löschung verschwindet · leere Instanz:
kein Lade-/Leertext, keine Zone · Befehl `business_os.data.modify` (AfA-Lauf) legt `business_commands`-Dokument an,
`sim.failCommand` → genau ein Zoneneintrag `buchhaltung.command.business_os.data.modify` · Storno fragt über
`api.shell.dialog` (default false → nichts; mit Ja: Stornobuchung + Gegenzeilen + `reversed_by_id` über api.put/patch,
Outbox leer = angenommen, Erfolgsmeldung in der Zone) · snapshot → structured clone → Remount erhält Ansicht und
offene Chat-Eingabe · unmount meldet alle 6 Abos ab und leert root · keine `pageerror`.

## Was sich geändert hat

- **Einstieg**: `mount(root, api, {restored})` → `{unmount, snapshot}`; Abos je Montage (`disposers`).
- **Daten**: 6 Live-Abos `api.query(<accounting_*>, {})` (entprellt 40 ms) statt `$.subscribe` + Readiness-Abos.
  Lese-/Schreibstellen laufen über einen kleinen Adapter `fibuDb()` (`rows/byId/add/change/drop` =
  `api.query().get()/api.get().get()/api.put/api.patch/api.remove`, Schreiben wartet nur auf `h.local`). 46 `insert`,
  10 `findOne().exec()`, 2 `find().exec()`, 9 `doc.patch`, 1 `doc.remove` umgestellt; `templates/skr.js` ebenso.
  RxDB-`insert` (Fehler bei Existenz) → `put` (Upsert); alle Aufrufer prüfen vorher auf Existenz oder vergeben neue IDs.
- **Zustandstexte**: Ladeblock „Daten werden synchronisiert“ (7 Stellen) und `syncingData` (JS + locales) entfernt;
  leere Listen rendern nichts (auch die früheren Leertexte der 7 Tabellen/Listen). Auswahl-/Trefferabhängige Hinweise
  („Keine Belegevidenz …“) bleiben. 17 `alert()` → Zustandszone (`buchhaltung.<bereich>.<fall>`, info/warn/error),
  (UI-Testläufer-Anzeige entfällt: In-App-E2E-Suite `core/ui_e2e_tests.js` samt Ansicht „System & QS“ gelöscht, FIX5), Chat „Nachricht fehlt“ →
  `buchhaltung.chat.message_missing`; Chat-Gate auf `[data-ctox-chat-root]` (Shell-DOM außerhalb root) entfernt.
- **Dialoge**: `showBusinessConfirm` (SKR neu initialisieren, mit `require_text`) und `confirm()` (Storno) →
  `api.shell.dialog({kind:'confirm', …, default:false})`.
- **Befehle**: `ctx.commandBus.dispatch` (Kontext-Chat) und `ctx.contextActions.dispatch('data')` (Anlagegut, AfA-Lauf;
  laut app.js = Befehl `business_os.data.modify`) → `api.command`, nie warten, Status in die Zone.
- **Shell**: `setTitle('Buchhaltung')`, Locale über `api.shell.locale` + `t('buchhaltung.<key>')` mit eingebetteten
  Rückfalltexten; Rechte-Spiegel über `api.actor` (`apps.source.edit` oder Rolle admin/chef).
- **Imports/HTTP**: `shared/i18n.js` und `shared/dialogs.js?v=…` entfallen; `ctox:shared/pane-grammar.js`
  (`autoWirePaneGrammar(root)` im mount); `index.html` ohne `?v=`, `ensureStyles()` entfernt.
- **CSS**: gescoped; das App-root ist jetzt der Größencontainer `business-app-window` (früher das Shell-Fenster),
  sonst greifen die schmalen Layoutregeln nicht (420er-Bild vorher: Mittelspalte abgeschnitten). Ein kaputter
  Kommentar (`/* custom */` im Kommentar) hatte die Regel `.shell-window[…] .fibu-module {overflow:hidden …}` ungültig
  gemacht; repariert, die Regel greift jetzt unter `[data-app="buchhaltung"]`.
- **schema.js gelöscht**: inhaltlich gleich zu `collections.schema.json` (8 Collections, Version 0, gleiche
  `required` und Felder, keine Migrationen; Vergleich per Skript).
- **GoBD-Hooks** (`preSave/preRemove`) entfernt: in 4.0 gibt es keine Collection-Hooks; die Shell-Fassade bot sie auch
  vorher nicht an (gemessen: `preSave` kommt im gesamten business-os-Baum nur in buchhaltung vor, der Aufruf lief ins
  `catch`). Fachverhalten damit unverändert; Sperre gehört in den Handler (OFFEN-2).

## Befehlstypen – Einstufung (§16.13)

Rust-Bezüge `grep -rn '"<type>"' src/core` (nur .rs) im Klon:

| Befehlstyp | Rust | Einstufung |
|---|---:|---|
| `business_os.data.modify` (Anlagegut erfassen, AfA-Lauf) | 2 | Core-generisch (Kontext-Aktion, app-unabhängig) |
| `business_os.chat.task` (Kontext-Chat data/ask) | 154 | Core-generisch |
| `ctox.business_os.app.modify` (Kontext-Chat app, nur mit Recht) | 26 | Core-generisch (App-Authoring) |

Alle Buchungen (Storno, Festschreiben, Bankabgleich, Reisekosten, Fahrtenbuch, Seed, Kontenrahmen) schreibt die App
heute direkt als Dokumente (mehrere Dokumente je Vorgang, siehe OFFEN-1). App-spezifischer Rust-Code: in `src/core`
67 Zeilen mit `accounting_`/`buchhaltung` (u. a. `business_os/store.rs`, `invoices.rs`, `module_lifecycle.rs`), hier nicht
einzeln eingestuft.

## OFFEN

1. **Mehrdokument-Vorgänge ohne Transaktion**: Storno, Festschreiben, Bankabgleich, Reisekosten/Fahrtenbuch schreiben
   Kopf + Zeilen + Status in mehreren `api.put/patch`. PORTING R3: „keine Transaktion über mehrere Dokumente →
   Befehl“. Kandidaten für Handler (`buchhaltung.booking.storno`, `.post`, `.reconcile`, `.travel.create`, …) – nicht
   in diesem Strang (Handler-Strang), Fachverhalten hier unverändert gelassen.
2. **GoBD-Festschreibung**: Unveränderlichkeit festgeschriebener Buchungen (`posted_at`) war clientseitig nie wirksam;
   gehört als Prüfung in den Handler/Core.
3. **Zahlenkreise**: Storno-/Buchungsnummern werden clientseitig gezählt (`J-2026-` + Anzahl), im Screenshot sichtbar
   doppelt (`J-2026-0002`); vorbestehend, `accounting_number_series` ist ungenutzt → Handler.
4. **Mehrere Fenster**: Modulzustand und 28 `window.*`-Handler (inline `onclick`) sind modulweit; 58 Stellen
   `document.getElementById/querySelector` statt `root`. Zwei Montagen gleichzeitig teilen Zustand (wie knowledge
   OFFEN-4); Test montiert deshalb nacheinander.
5. **guard-v4 Rest (3)**: Fachvokabular, kein Sync-Zustand: `matchSuccess` „… abgeglichen“ (geht über
   `api.status.report`, Text steht aber in `labels`), Bankzeilen-Status „Abgeglichen & Verbucht“, Testfall-Status
   „Läuft…“ im UI-Testläufer. Umbenennen wäre Fachänderung → Entscheidung Board.
6. **Seed-Daten**: `seedMockDataIfEmpty` schreibt bei leerer Instanz Demo-Buchungen, `autoInitializeAccounts` den
   Kontenrahmen – unverändert übernommen; in v4 sollte das ein Befehl/Handler sein, kein Seiteneffekt des Öffnens.
7. **Manifest** (Strang v4/manifests): `styles`, `handlers`, Container-Hinweis nicht in `module.json` geändert.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G1: Tabellen hatten `min-width: 760px`; jetzt ohne Mindestbreite, Zellen brechen um, schmale Köpfe ohne Sperrung; ≤ 520 px blendet der Kontenrahmen die USt-Spalte aus.
- Offen: 420 bleibt G1 (680 > 412) – Ursache im Kontenrahmen noch nicht gefunden (Kandidaten: Badges/Monospace-Kontonummern).

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app buchhaltung --shared` (Stub-Pin 9fa9948) → 1280/900 OK, 420 G1 offen; `node --test modules/buchhaltung/tests/v4-api.test.mjs` grün; `scan.mjs --app buchhaltung` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.

## Nachtrag Strang VIS2 (15.10.2026)

- Ursache 420 px G1 (680 > 412): Regel `.fibu-module .ctox-table { min-width: 680px }` im Block `@container business-app-window (max-width: 768px)` hob unterhalb 769 px das VIS-`min-width: 0` wieder auf und traf dabei jede Tabelle der App (auch Inspektor-Tabellen).
- Behoben: die sieben Datentabellen der Hauptansichten (Kontenrahmen, Journal, Belege, Bank, …) stehen in `.fibu-scrollable-container[data-scroll-x]`; nur diese behalten 680 px Spaltenbreite (bei jeder Fensterbreite) und `overflow-wrap: normal`, damit Kontonummern/Beträge nicht mitten im Wort brechen (900 px zeigte „010/0“). Detailtabellen bleiben bei `min-width: 0` und brechen um.

Prüfung: Labor `--shared` (Stub-Pin 9fa9948) → G1 grün bei 1280/900/420 (900/420 „G1 erlaubt: [data-scroll-x] 680>520/412“); offen G4 Kicker „Finanzwesen“ im linken Kopf bei 1280/900 (nicht Teil von VIS2); `node --test modules/buchhaltung/tests/v4-api.test.mjs` 8/8, `buchhaltung.test.mjs` 10/10.
