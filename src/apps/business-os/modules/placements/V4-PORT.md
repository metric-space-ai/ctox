# placements – Port auf App-API 4.0 (Strang UB2, Welle B)

Stand: ABGESCHLOSSEN (15.10.2026, Berlin 14:14 Start → 14:16 grün, gemessen mit `TZ=Europe/Berlin date`).

## Messwerte

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app placements` | vorher 30 Stellen (CSS 54 ungescoped, 17 mit Shell-Vorfahr) → **0 Stellen**, Exit 0 |
| CSS-Scope | `node v4/tools/port/scope-css.mjs placements modules/placements/index.css --check` | gescoped |
| Löschlisten-Wächter | `node v4/tools/guards/guard-v4.mjs --json --app modules/placements` | 0 Treffer außerhalb `handlers/` |
| v4-Test gegen Kern-Stub (Pin 66edb98) | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test src/apps/business-os/modules/placements/tests/v4-api.test.mjs` | **10/10 grün** |
| Bestehende Suite (auf v4 umgestellt) + Kernlogik | `node --test modules/placements/tests/placements.test.mjs modules/placements/core/*.test.mjs` | 20/20 grün |
| Screenshots | `tests/shots/placements-1280.png`, `tests/shots/placements-420.png` | selbst angesehen; 420 gestapelt, kein horizontaler Überlauf (im Test geprüft) |

v4-Test deckt: mount (Daten, Titel, 1 Abo, Bänder, Grammatik selbst verdrahtet) · `sim.push` live/Löschung · Band
„Beendet“/Suche · leere Collection ohne Text/Zone · `api.put` → `accepted`, live · `ats.placement.create`: Dokument
sofort mit unveränderter Nutzlast, `done` still + Formular geleert, blockiert → Zone `placements.create.blocked` (warn),
Eingabe bleibt · `ats.placement.early_leave`: `sim.failCommand` → genau ein Zoneneintrag, `done` → Zone info mit
Clawback und Gutschrift · snapshot → structuredClone → Remount erhält Pflicht-Qualifikationen-Entwurf, „Weitere Angaben“
offen, Art-Filter, offene Ablage, Listenansicht, Auswahl · unmount: 0 Abos, root leer · keine `pageerror`.

## Stellen und Entscheidungen

- **R1/R2**: `mount(host, api, {restored})` → `{unmount, snapshot}`; Abos je Montage in `disposers`; Kopf-Knöpfe werden
  beim unmount abgemeldet (vorher nicht).
- **R3**: `api.query('placements', {})`, lokale Sortierung wie vorher (`updated_at_ms` absteigend); Import `upsert` → `api.put`.
- **R5**: `ats.placement.create` und `ats.placement.early_leave` über `api.command`, Nutzlast unverändert
  (`required_types` fehlt wie vorher, wenn leer). Nie warten. `failed` → Zone `placements.command.<type>` (error).
  Anlegen: Ergebnis blockiert (`blockedReasons`, gleiche Logik wie das frühere `renderBlocked`) → Zone warn; sonst still –
  die neue Vermittlung samt `fee_invoice_id` erscheint live am Datensatz; Formular nur leeren, wenn seither nichts
  getippt wurde. Frühausstieg: Status und Storno-Gutschrift erscheinen live am Datensatz, der Clawback-Betrag nur im
  Ergebnis → Zone `placements.early_leave.result` (info).
- **R6**: `ctx.notifications.info(exportDone)` entfernt. **R11**: Gate-Callout (HTML + CSS), „Placement angelegt“,
  „Import abgeschlossen“, „Offline: …“, Leer-/Sync-Texte (auch Locales `empty`) entfernt. Zone zusätzlich:
  `placements.form.candidate_required` (warn), `placements.import.invalid` (error), `placements.import.partial` (warn).
  Der Werkbank-Fußtext „Eintrag wählen oder neue Vermittlung anlegen.“ ist Bedienhinweis, kein Zustand → bleibt.
- **R8**: Locale live + `t('placements.<key>')`; `title` eingebettet → `api.shell.setTitle`.
- **R10/R14**: `ensureStyles`/`?v=` entfernt; `shared/list-state.js` entfällt; `ctox:shared/pane-grammar.js` dynamisch im `mount`.
- **R16 CSS**: gescoped; v4-Rahmen: `--shell-v2-icon-size: 0px` und `container: business-app-window / inline-size` am
  App-root, 74px-Steuerungszone am Werkbank-Kopf und gestapelt am Listenkopf entfernt. Die gleich hohen Köpfe (2 × 37px)
  bleiben (Vertrag h1, gleiche Kopfhöhen).
- **R17**: `schema.js` gelöscht – tief gleich zu `collections.schema.json` (`offers`, `placements`, keine Migrationen).

## Befehlstypen – Einstufung (§16.13)

| Befehlstyp | Rust-Treffer | Einstufung | Ziel |
|---|---:|---|---|
| `ats.placement.create` | 2 (`store.rs:24285`, `store_ats_commands.rs:475`) | Fachlogik | JS-Handler (Handler-Strang): Einsatz-Gate bei ANÜ, Placement + Honorar-Rechnung → `{ok, placement_id, fee_invoice_id}` bzw. `blockers` |
| `ats.placement.early_leave` | 2 (`store.rs:24286`, `store_ats_commands.rs:1055`) | Fachlogik | JS-Handler: Status `early_leave`, Clawback und Storno-Gutschrift → `{credit_note_id, clawback}` |

## OFFEN

1. Container je Fenster und Spaltenteiler wie intake OFFEN-1/2 (Rahmen-Strang).
2. Werkbank-Kopf 74px hoch (zwei v2-Bänder) bei nur einer Titelzeile – im v4-Rahmen Leerraum; gleiche Kopfhöhen sind
   v2-Vertrag. Entscheidung Shell-Strang, ob v4 den Vertrag h1 behält.
3. `offers` steht im Schema, wird von der UI nicht gelesen (wie vorher).
