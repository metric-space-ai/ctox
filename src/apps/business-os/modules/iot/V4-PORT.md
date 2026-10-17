# iot – Port auf App-API 4.0 (Strang UC2, Welle C)

Stub-Pin cff5ba5 (Erstport gegen 67229d2). Start 14:12, fertig 14:17 (Berlin, gemessen; Vorarbeit Lesen ab 14:11).

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app iot` | 37 → 0 Stellen, Exit 0 (CSS vorher 128 ungescoped, 19 mit Shell-Vorfahr) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs iot modules/iot/index.css --check` | gescoped |
| Wächter | `node v4/tools/guards/guard-v4.mjs --app modules/iot` | 16 → 0 Treffer (ein zwischenzeitlicher Treffer in `handlers/_base.js` gehört dem Handler-Strang, beim Schlusslauf 0) |
| v4-Test | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/cff5ba5 node --test src/apps/business-os/modules/iot/tests/v4-api.test.mjs` | 10/10 grün |
| Legacy-Suite | `node modules/iot/iot.test.mjs` (aus Kopie mit node_modules-Link) | 11/11 grün (+ Modulvertrag) |
| Screenshots | `tests/shots/iot-1280.png`, `iot-420.png` | angesehen: Baum + Dashboard bzw. gestapelt, kein Überlauf |

v4-Test deckt ab: mount (Baum live, Titel, Abo-Zähler) · `sim.push` erscheint/verschwindet · leere Collections: kein
Text, keine Zone · `api.put` → `accepted`, live im Baum · Pausieren über `api.shell.menu` → `ctox.iot.widget.pause`,
`failCommand` → genau ein Zoneneintrag, nichts auf Dokumentebene · Löschen fragt über `api.shell.dialog`, ohne Ja
kein Befehl · Webhook: Ergebnis (`sim.onCommand`) erscheint als In-Fenster-Dialog mit Token/Pfad · Editor liegt im
Fenster, snapshot → Remount mit `restored` erhält Editor-Entwurf und Auswahl · unmount löst alle 7 Abos, leert root.

## Entscheidungen

- **Einstieg/Daten**: `mount(root, api, {restored})`; 7 `api.query(<iot_*>, {})`-Abos statt `find().exec()`-Reload mit
  In-Flight-/Queue-Logik, Readiness-Abos und Syncing-Hülle. Entprellt 60 ms.
- **Zustandstexte entfernt**: „Daten werden synchronisiert.“, „Noch keine Assets“, „Kein Asset passt zum Filter“,
  „Wähle links ein Asset oder Signal“, „Noch keine Aufträge“ (inkl. Leer-Kasten mit Knopf; „Auftrag anlegen“ bleibt
  im Dashboard-Kopf und als Karte), Alert „Importiert: N“. Leerer Baum zeigt beim Anlegen das Formular (vorher
  blieb es hinter dem Leertext unerreichbar – einzige Verhaltensänderung, bewusst).
- **Dialoge**: `showBusinessPrompt/Confirm` → `api.shell.dialog({kind:'prompt'|'confirm', title, text,
  confirm_label, default})` (Form OFFEN-A). `showBusinessAlert` für Hinweise → Zone (`iot.auftrag.no_signal`,
  `iot.auftrag.signal_unknown`, `iot.import.invalid`, `iot.import.empty`, warn); Webhook-Ergebnis (Inhalt, kein
  Zustand) → `api.shell.dialog({kind:'alert'})` erst bei Befehlsstatus `done` mit `result`.
- **Kontextmenü**: `createContextMenu({host: document.body})` (Popover auf Dokumentebene) → `api.shell.menu({items:[{id,
  label, icon}], at:{x,y}})`, Auswahl per `id`.
- **Editor**: `.ctox-modal` hängt in `[data-iot-root]`; root `position:relative` + `container: business-app-window`
  (die Breitenabfragen 768 griffen sonst nicht); Entwurf aller drei Felder überlebt Tab-Wechsel und Remount.
- **Befehle**: `api.command(type, {...payload, module:'iot', record_id, inbound_channel:'business_os.iot',
  client_context}, {record_id})`, nie warten (vorher `await` in Editor, Neu-Auftrag, Import-Schleife, ensureDashboard).
  Zone-Code `iot.command.<type>`; „tolerante“ Befehle (compile_trigger/generate_render) wie vorher nur Konsole.
- **Shell**: `ctx.getActionIcon` entfällt (lokale Pfade bleiben); Titel `setTitle('IoT')`; Theme der Render-Iframes
  aus `getComputedStyle(root)` statt `document.documentElement`; `autoWirePaneGrammar(root)` ruft die App.
- **i18n**: Locales per JSON-Modul-Import, `api.shell.t('iot.<key>')` übersteuert, Locale live; 33 ungenutzte bzw.
  Zustands-Schlüssel entfernt; `ed.triggerNote` „läuft im Backend“ → „im Backend pro Messwert ausgewertet“.
- **schema.js gelöscht**: 11 Collections, Felder/required/indexes/version 0 gleich `collections.schema.json` (nur
  Schlüsselreihenfolge), Migrationsstrategien waren leer. Verweise nur noch in Kommentaren
  `src/core/iot/projector.rs:27,48` und `src/core/rxdb/tools/browser_rust_smoke.js:11962` (Smoke lädt
  `/modules/iot/schema.js` → OFFEN-2).

## Collections

Eigene: alle 11 `iot_*` (gelesen werden 7: realms, assets, attributes, datapoints, alarms, dashboards, widgets).
Fremd: `business_commands` (System, nur über `api.command`).

## Befehlstypen – Einstufung (§16.13)

| Befehlstyp | Rust-Treffer | Handler vorhanden | Einstufung |
|---|---:|---|---|
| `ctox.iot.asset.upsert` | 7 | ja | Fachlogik |
| `ctox.iot.dashboard.upsert` | 5 | ja | Fachlogik |
| `ctox.iot.widget.upsert` | 7 | ja | Fachlogik |
| `ctox.iot.widget.delete` | 2 | ja | Fachlogik |
| `ctox.iot.widget.pause` | 3 | ja | Fachlogik |
| `ctox.iot.widget.compile_trigger` | 3 | ja | Fachlogik + Fähigkeit (Modellaufruf) |
| `ctox.iot.widget.generate_render` | 1 | ja | Fachlogik + Fähigkeit (Modellaufruf) |
| `ctox.iot.webhook.register` | 2 | ja | Fachlogik + Fähigkeit (Token/Secret-Store, Ingest-Endpunkt) |

Handler liegen in `handlers/` (Handler-Strang, nicht angefasst).

## OFFEN

1. **Dialogform** (OFFEN-A): `prompt` mit `default`, `alert` – Stub liefert `spec.default ?? null`; echte Shell muss
   `kind:'prompt'` mit Texteingabe und `kind:'alert'` können. Die Signalwahl bleibt (wie vorher) eine Freitext-Eingabe.
2. **browser_rust_smoke.js** lädt `/modules/iot/schema.js` (gelöscht) – Smoke-Werkzeug im Core, nicht in diesem Strang.
3. **Menüform** (OFFEN-A): `api.shell.menu({items, at})`, Antwort `id` oder `{id}`.
4. **Mehrere Fenster**: Modulzustand modulweit (wie knowledge OFFEN-4).
5. **module.json** unverändert (Generator).
6. Kein git benutzt.

## Nachprüfung gegen Stub-Pin cff5ba5 (Wiederaufnahme 15.10., bis 15:10 Berlin, gemessen)

Nach dem Sitzungsabbruch ~14:40: keine Datei im Schreibbereich nach 14:25 geändert, keine halben Edits. Unverändert
gegen den neuen Pin neu gefahren:

- `scan.mjs --app iot`: 0 Stellen, CSS ungescoped 0, Exit 0; `scope-css.mjs … --check`: gescoped.
- `guard-v4.mjs --json`: 0 Treffer unter `modules/iot/` (nur getrennt gezählte Testdatei-Treffer).
- v4-Test gegen cff5ba5: 10/10 grün (mit `CTOX_APP_API_STUB` und ohne – Rückfallpfad im Test jetzt cff5ba5).
- Legacy-Suite (Kopie mit node_modules-Link): 11/11 grün.
- Screenshots 1280/420 vom Testlauf neu geschrieben und angesehen: unverändert gegenüber Erstport, kein Überlauf.
