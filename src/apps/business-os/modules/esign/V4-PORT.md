# esign – Port auf App-API 4.0 (Strang UB1, Welle B)

Stand 15.10.2026 14:22 (Berlin, gemessen). Vorlage: `v4/tools/port/PORTING.md`, Pilot knowledge. `handlers/` gehört
dem Handler-Strang und ist unverändert.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app esign` | **0 Stellen**, Exit 0 (vorher 30, CSS 74 ungescoped, 22 mit Shell-Vorfahr) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs esign modules/esign/index.css --check` | gescoped |
| v4-Test (Stub-Pin 66edb98) | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test src/apps/business-os/modules/esign/tests/v4-api.test.mjs` | 9/9 grün |
| Bestehende Suiten | `node --test modules/esign/tests/esign.test.mjs` · `node --test modules/esign/core/signature.test.mjs` | 11/11 · 4/4 grün (Schema-, Import- und Readiness-Wächter auf v4 umgestellt; `ctox:shared/` per `registerHooks`) |
| guard-v4 (ohne handlers/) | gefiltert | vorher 23 (inkl. handlers), jetzt **0** |
| Screenshots | `tests/shots/esign-1280.png`, `esign-420.png` | angesehen: 1280 Liste + Formular + Ergebnis + Detail; 420 gestapelt, kein Überlauf |

v4-Test: mount zeigt Live-Daten · `sim.push` erscheint/verschwindet · leere Instanz: Liste leer, kein Kasten, keine
Zone · `ats.signature.request` legt `business_commands`-Dokument an (Payload unverändert `{document_id, subject_kind,
signers}`), `done`-Ergebnis im Prüfbereich, `sim.failCommand` → genau ein Zoneneintrag
`esign.command.ats.signature.request` · Import → `api.put`, Outbox leer, `esign.import.done` · snapshot → Remount
erhält Auswahl, Detail und offene Eingabe · `ats.signature.sign` aus dem Detail → `done` · unmount 0 Abos.

## Was sich geändert hat

- `mount(host, api, {restored})` → `{unmount, snapshot}`; Live-Abo `api.query('signature_requests', {})` statt
  `find().$` + `refresh()` + Readiness; nach Befehlen kein Nachladen (Abo).
- **Befehle** `ats.signature.request|sign`: `api.command`, nie warten; `done`-Ergebnis (bzw. Blocker) im Prüfbereich
  wie bisher (fachliche Antwort), `failed` → Zone. Payload unverändert, plus `record_id`/`client_context`.
- **Zustandstexte** raus: Sync-Kasten, Leertext (`empty`, auch locales), `offlineService`, `offlineSend`;
  Pflichtfeld → `esign.form.document_required`; Datei unlesbar → `esign.import.invalid`; Import/Export →
  `esign.import.done` / `esign.export.done` (früher `ctx.notifications.info`).
- Fenstertitel `api.shell.setTitle('E-Signatur'|'E-Signature')` (früher Manifest-Titel); Locale live über
  `api.shell.locale`, `t('esign.<key>')` übersteuert.
- `ctox:shared/pane-grammar.js` (`autoWirePaneGrammar(host)`), `ensureStyles` und `?v=` entfernt, CSS gescoped,
  App-root Größencontainer.
- **schema.js gelöscht**: gleich zu `collections.schema.json` (1 Collection, Version, `required`, Felder, keine
  Migrationen; Vergleich per Skript).

## Befehlstypen

| Befehlstyp | Rust (`"<type>"`, .rs) | Einstufung |
|---|---:|---|
| `ats.signature.request` | 2 | Fachlogik → `handlers/ats.signature.request.js` (Handler-Strang) |
| `ats.signature.sign` | 3 | Fachlogik → `handlers/ats.signature.sign.js` (Handler-Strang) |

## OFFEN

1. **Mehrere Fenster**: `text`/`dateLocale` sind modulweit.
2. Formular wird beim Absenden sofort geleert (vorher erst nach Annahme) – Folge von „nie warten“; bei `failed`
   steht die Eingabe nicht mehr im Formular (Zoneneintrag nennt den Fehler). Entscheidung Board, ob der Entwurf bis
   `done` gehalten werden soll.
3. **Manifest** (Strang v4/manifests): `styles`, `handlers` nicht in `module.json` geändert.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G4: Metazeilen `.esign-shard-meta` brechen um.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app esign --shared` (Stub-Pin 9fa9948) → alle Läufe OK; `node --test modules/esign/tests/v4-api.test.mjs` grün; `scan.mjs --app esign` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.
