# intake – Port auf App-API 4.0 (Strang UB2, Welle B)

Stand: ABGESCHLOSSEN (15.10.2026, Berlin 13:23 Start → 13:34 grün, gemessen mit `TZ=Europe/Berlin date`).

## Messwerte

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app intake` | vorher 24 Stellen (CSS 74 Selektoren ungescoped) → **0 Stellen**, Exit 0 |
| CSS-Scope | `node v4/tools/port/scope-css.mjs intake modules/intake/index.css --check` | gescoped |
| Löschlisten-Wächter | `node v4/tools/guards/guard-v4.mjs --json --app modules/intake` | 0 Treffer außerhalb `handlers/` (1 Treffer `handlers/_base.js:44` gehört dem Handler-Strang) |
| v4-Test gegen Kern-Stub (Pin 66edb98) | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test src/apps/business-os/modules/intake/tests/v4-api.test.mjs` | **10/10 grün** (Chromium headless) |
| Bestehende Suite (auf v4 umgestellt) | `node --test modules/intake/tests/intake.test.mjs` | 15/15 grün |
| Kernlogik | `node --test modules/intake/core/application.test.mjs` | 4/4 grün |
| Screenshots | `tests/shots/intake-1280.png`, `tests/shots/intake-420.png` | selbst angesehen; 420: gestapelt, kein horizontaler Überlauf (im Test geprüft) |

v4-Test deckt: mount zeigt Daten, Titel über `api.shell.setTitle`, 1 Abo · `sim.push` erscheint live, Löschung
verschwindet · Band/Suche über die selbst verdrahtete Spalten-Grammatik · leere Collection: kein Text, keine Zone ·
`api.put` → `accepted`, live sichtbar · Erfassen legt `business_commands`-Dokument **sofort** an (kein Warten),
`done` → still, Formular geleert · `sim.failCommand` → genau ein Zoneneintrag, Eingabe bleibt · blockiertes Ergebnis
→ Zone `intake.capture.blocked` mit Gründen · snapshot → structuredClone → Remount mit `restored` erhält Telefon-Entwurf,
Auswahl, Statusfilter, offene Filterablage · unmount: 0 Abos, root leer · keine `pageerror`.

## Stellen und Entscheidungen

- **R1/R2 mount**: `mount(root, api, {restored})` → `{unmount, snapshot}`. Zustand liegt komplett in der Montage-Closure
  (Abos je Montage lokal in `disposers`); modulweit sind nur `text`/`locale` für die reinen Helfer.
- **R3 Daten**: `api.query('applications', {})` + lokale Sortierung wie vorher (`received_at_ms || created_at_ms` absteigend).
  Import schreibt je Datensatz `api.put` (vorher `col.upsert`), sichtbar über das Live-Abo.
- **R4 Sync**: `collectionReadiness`/`subscribeCollectionReadiness` ersatzlos entfernt.
- **R5 Befehl** `ats.intake.capture`: `api.command(type, {name, email, phone, vacancy_id, channel})` – Nutzlast unverändert
  (passt zu `store_ats_commands.rs:429`). Kein Warten, kein gesperrter Knopf. Status-Abo: `failed` → Zone
  `intake.command.ats.intake.capture` (error); `done` → Ergebnis prüfen (`captureBlockers`, gleiche Blockade-Logik wie
  vorher) → blockiert: Zone `intake.capture.blocked` (warn) mit Gründen, Eingabe bleibt; sonst Formular leeren (nur wenn
  seit dem Absenden nichts Neues getippt wurde).
- **R11 Zustandstexte** entfernt: Gate-Callout (`[data-ats-gate]`, „Bewerbung erfasst …“, „Offline: …“), Leer-/Filter-leer-Texte,
  `ctox-syncing`; Locales ohne `empty`/`syncing`. In die Zone: `intake.capture.name_required` (warn),
  `intake.capture.blocked` (warn), `intake.command.ats.intake.capture` (error), `intake.import.invalid` (error),
  `intake.import.empty` (warn), `intake.import.partial` (warn, nur wenn Zeilen lokal abgelehnt wurden). Erfolg bleibt still.
- **R8 i18n**: `api.shell.locale` (live) + `api.shell.t('intake.<key>')` übersteuert eingebettete Texte; `subtitle`
  (vorher `ctx.manifest.description`) und `title` als eingebettete Texte (wörtlich aus `module.json`/Locales).
- **R10**: `ensureStyles` und `?v=`-Buster entfernt; `fetch(new URL('./index.html', import.meta.url))`.
- **R14**: Spalten-Grammatik verdrahtet die App selbst: `await import('ctox:shared/pane-grammar.js')` im `mount`
  (dynamisch, damit `tests/intake.test.mjs` die reinen Helfer weiter direkt in Node importieren kann).
- **R16 CSS** gescoped unter `[data-app="intake"]`. Layoutkorrekturen für den v4-Rahmen (`v4/frame/frame.css`: Icon und
  Fenstersteuerung liegen in eigener Chrome-Zeile über `root`): v2-Freihaltezonen (Icon 80px links, Steuerung 70px rechts,
  auch gestapelt) entfernt. App-root trägt `container: business-app-window / inline-size`, weil der v4-Rahmen keinen
  Container je Fenster setzt (nur `v4f` am Desktop) – sonst greifen weder die Kit-Stapelregeln aus `base.css` noch die
  App-Regel bei 768px.
- **R17**: `schema.js` gelöscht – gleichwertig zu `collections.schema.json` (Version 0, gleiche Properties,
  `required [id, channel, updated_at_ms]`, keine Migrationen).
- **Snapshot** `{v:1, selectedId, userCollapsed, search, view, band, status, trayOpen, form{5 Felder}, moreOpen, listScrollTop}`.
  Formularentwurf wird direkt zurückgespielt (nicht von der Auswahl überschrieben), Scroll nach dem ersten Datenstand.
- Bestehende Suite `tests/intake.test.mjs` umgestellt (nicht gelöscht): Schema-Test liest `collections.schema.json`;
  Import-Test prüft `api.put`; Readiness-Test → „leere Liste rendert nichts, keine v3-Pfade“; neu `captureBlockers`.

## Befehlstypen – Einstufung (§16.13)

| Befehlstyp | Rust-Treffer `grep -rn '"<type>"' src/core --include='*.rs'` | Einstufung | Ziel |
|---|---:|---|---|
| `ats.intake.capture` | 4 (`store.rs:24284`, `store.rs:32299/32487` Tests, `store_ats_commands.rs:429`) | Fachlogik | JS-Handler `handlers/ats.intake.capture.js` (Handler-Strang): Datensatz `applications` anlegen (`appl_<now>`, `dedupe_key` email:/name:, `status:new`) + Governance-Ereignis `business_os.ats.intake_captured`; Ergebnis `{ok, application_id, dedupe_key}` |

## OFFEN

1. **Container je Fenster**: Der v4-Rahmen sollte jedem Fenster-root `container: business-app-window / inline-size` geben
   (wie Shell v2); bis dahin trägt die App ihn selbst. Gilt für alle Apps mit `@container business-app-window`.
2. **Spaltenteiler** (`.ctox-column-resizer[data-resizer]`) wurde in v2 von der Shell verdrahtet; in v4 verdrahtet ihn
   niemand (kein `shared`-Helfer im Port erlaubt) → er ist sichtbar, aber ohne Wirkung. Klärung Shell-Strang.
3. **Manifest** (`module.json`, nicht geändert): `business_commands` steht nicht in `collections` (gut); `handlers`,
   `styles` (index.css lädt die Shell) fehlen – Strang v4/manifests.
4. **Ergebnisform des Befehls**: Die App liest `status.result` wie vorher das Dispatch-Ergebnis (`ok/blockers/gate/decision`);
   der JS-Handler muss diese Form liefern.
