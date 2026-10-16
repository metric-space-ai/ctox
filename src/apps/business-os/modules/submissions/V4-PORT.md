# submissions – Port auf App-API 4.0 (Strang UB2, Welle B)

Stand: ABGESCHLOSSEN (15.10.2026, Berlin 14:16 Start → 14:19 grün, gemessen mit `TZ=Europe/Berlin date`).

## Messwerte

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app submissions` | vorher 29 Stellen (CSS 60 ungescoped, 19 mit Shell-Vorfahr) → **0 Stellen**, Exit 0 |
| CSS-Scope | `node v4/tools/port/scope-css.mjs submissions modules/submissions/index.css --check` | gescoped |
| Löschlisten-Wächter | `node v4/tools/guards/guard-v4.mjs --json --app modules/submissions` | 0 Treffer außerhalb `handlers/` |
| v4-Test gegen Kern-Stub (Pin 66edb98) | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test src/apps/business-os/modules/submissions/tests/v4-api.test.mjs` | **10/10 grün** |
| Bestehende Suite (auf v4 umgestellt) + Kernlogik | `node --test modules/submissions/tests/submissions.test.mjs modules/submissions/core/*.test.mjs` | 14/14 grün |
| Screenshots | `tests/shots/submissions-1280.png`, `tests/shots/submissions-420.png` | selbst angesehen; 420 gestapelt, kein horizontaler Überlauf (im Test geprüft) |

v4-Test deckt: mount (Daten, Titel, 1 Abo, Bänder, Grammatik selbst verdrahtet) · `sim.push` live/Löschung · Band
„Erledigt“/Suche · leere Collection ohne Text/Zone · `api.put` → `accepted`, live · `ats.submission.present`:
Dokument sofort mit unveränderter Nutzlast, `done` still + Formular geleert; blockiert (Doppel-Vorstellung mit
`conflicting_submission_id`) → Zone `submissions.present.blocked` (warn), Eingabe bleibt; `sim.failCommand` → genau ein
Zoneneintrag; fehlende Pflicht-IDs → Zone, kein Befehl · snapshot → structuredClone → Remount erhält Kontakt-Entwurf,
„Weitere Angaben“ offen, Statusfilter, offene Ablage, Listenansicht, Auswahl · unmount: 0 Abos, root leer · keine `pageerror`.

## Stellen und Entscheidungen

- Gleiches Gerüst wie placements (gleiche App-Familie; je App eigener Code, nichts in `shared/`).
- **R3**: `api.query('submissions', {})`, lokale Sortierung wie vorher (`updated_at_ms || sent_at_ms || created_at_ms`);
  Import `upsert` → `api.put`.
- **R5** `ats.submission.present`: `api.command`, Nutzlast unverändert; nie warten. Ergebnisform `outcome.result ?? outcome`
  wie vorher (`presentBlockers`, gleiche Blockade-Regel inkl. `allowed:false`, Konflikt-ID über `blockerText`).
  Erfolg still (die Vorstellung erscheint live), zurück in den Anlege-Modus nur, wenn seither nichts getippt wurde.
- **R6/R11**: `ctx.notifications.info(exportDone)`, Gate-Callout (HTML + CSS), „Vorgestellt.“, „Import abgeschlossen“,
  „Offline: …“, Leer-/Sync-Texte (auch Locales `empty`) entfernt. „ID kopiert“ (lokale Kopier-Aktion) → Zone
  `submissions.id.copied` (info). Weitere Codes: `submissions.form.ids_required` (warn),
  `submissions.command.ats.submission.present` (error), `submissions.import.invalid|partial`.
- **R8**: Locale live + `t('submissions.<key>')`; `title` = „Vorstellungen“ / „Candidate presentations“ (wörtlich aus
  `locales/*.json`) → `api.shell.setTitle`.
- **R10/R14/R16/R17**: wie placements (kein `?v=`, kein `list-state.js`, Grammatik dynamisch im `mount`, CSS gescoped,
  v4-Rahmen-Zonen entfernt, Container am App-root, `schema.js` gelöscht – tief gleich, keine Migrationen).

## Befehlstypen – Einstufung (§16.13)

| Befehlstyp | Rust-Treffer | Einstufung | Ziel |
|---|---:|---|---|
| `ats.submission.present` | 2 (`store.rs:24287`, `store_ats_commands.rs:616`) | Fachlogik | JS-Handler (Handler-Strang): Doppel-Vorstellungs- und Consent-Prüfung, Datensatz `submissions` → `{ok, submission_id}` bzw. `blockers[{reason, conflicting_submission_id}]` |

## OFFEN

1. Container je Fenster und Spaltenteiler wie intake OFFEN-1/2; Werkbank-Kopf 74px wie placements OFFEN-2.
2. Die Kopier-Bestätigung „ID kopiert“ belegt die einzeilige Zone; ob lokale Quittungen dorthin gehören oder still
   bleiben sollen, entscheidet der Shell-Strang (Zonenrichtlinie).
