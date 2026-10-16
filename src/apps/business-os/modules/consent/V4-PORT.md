# consent – Port auf App-API 4.0 (Strang UB1, Welle B)

Stand 15.10.2026 (Berlin). Vorlage: `v4/tools/port/PORTING.md`, Pilot knowledge. `handlers/` und `V4-HANDLERS.md`
gehören dem Handler-Strang H8 und sind unverändert.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app consent` | **0 Stellen**, Exit 0 (vorher 30, CSS 65 ungescoped, 21 mit Shell-Vorfahr) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs consent modules/consent/index.css --check` | gescoped |
| v4-Test (Stub-Pin 66edb98) | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test src/apps/business-os/modules/consent/tests/v4-api.test.mjs` | 8/8 grün |
| Bestehende Suiten | `node --test modules/consent/tests/consent.test.mjs` · `node --test modules/consent/core/consent.test.mjs` | 15/15 · 5/5 grün (Schema-, Sperr-, Befehls- und Readiness-Wächter auf v4 umgestellt; `ctox:shared/` per `registerHooks` wie der SW) |
| guard-v4 (modules/consent ohne handlers/) | gefiltert | vorher 33 (inkl. handlers-fremder Treffer), jetzt **0** |
| Screenshots | `tests/shots/consent-1280.png`, `consent-420.png` | angesehen: 1280 Liste + Detail + Prüfformular; 420 gestapelt, kein Überlauf |

v4-Test: mount zeigt Live-Daten · `sim.push` erscheint/verschwindet · leere Instanz: Liste leer, kein Kasten, keine
Zone · Befehl `ats.consent.check` legt `business_commands`-Dokument an, `done`-Ergebnis erscheint im Prüfbereich,
`sim.failCommand` → genau ein Zoneneintrag `consent.command.ats.consent.check` · Import → `api.put`, Outbox leer,
`consent.import.done` in der Zone · snapshot → Remount erhält Auswahl, Detail und offene Formulareingabe · unmount 0 Abos.

## Was sich geändert hat

- `mount(host, api, {restored})` → `{unmount, snapshot}`; ein Live-Abo `api.query('business_consents', {})` statt
  `find().$` + `load()` + Readiness. Nach Art. 17 kein Nachladen mehr – die Liste folgt dem Abo.
- **Befehle**: `ctx.commandBus.dispatch` (await Ergebnis) → `api.command(type, {...payload, record_id, client_context})`.
  Das Ergebnis kommt gepusht (`status: done`, `result`) und wird wie bisher im Prüfbereich (`[data-ats-gate]`)
  dargestellt – das ist die fachliche Antwort (erlaubt/verweigert, Auskunftsdaten, Löschbilanz), kein Zustandstext.
  `failed`/lokal ungültig → Zustandszone `consent.command.<type>`. Payloads unverändert (`{subject_id, purpose}`,
  `{subject_id}`); `module`/`command_type` fallen weg (Stub legt `type`, `app_id` an).
- **Zustandstexte** raus aus dem Inhalt: Sync-Kasten, Leertexte (`entriesEmpty`, `emptyFiltered`, locale `empty`),
  `databaseOffline`, `commandOffline`, `dispatchOffline` gelöscht; Sperrkasten → `consent.data.locked` (bei
  `CtoxError forbidden` aus `api.query`); Pflichtfeld → `consent.form.subject_required`; Import →
  `consent.import.invalid|empty|done`.
- Titel `api.shell.setTitle`, Kicker-Tooltip aus eingebettetem `subtitle` (früher `ctx.manifest.description`);
  Locale live über `api.shell.locale`, `t('consent.<key>')` übersteuert.
- `ctox:shared/pane-grammar.js` (`autoWirePaneGrammar(host)`); `ensureStyles` und `?v=` entfernt; CSS gescoped,
  App-root ist Größencontainer.
- **schema.js gelöscht**: gleich zu `collections.schema.json` (1 Collection, Version, `required`, Felder, keine
  Migrationen; Vergleich per Skript).

## Befehlstypen (Einstufung siehe `V4-HANDLERS.md`, Strang H8)

| Befehlstyp | Rust (`"<type>"`, .rs) | UI-Sender | Einstufung |
|---|---:|---|---|
| `ats.consent.check` | 2 | Prüfformular | Fachlogik → `handlers/ats.consent.check.js` (H8) |
| `ats.subject.export` | 2 | Rechte-Formular, Detail-Aktion | Fachlogik → Handler (H8) |
| `ats.subject.erase` | 2 | Rechte-Formular, Detail-Aktion | Fachlogik → Handler (H8) |

## OFFEN

1. **Mehrere Fenster**: `text`/`locale` sind modulweit (zwei Montagen mit verschiedener Sprache teilen sie).
2. **Art. 17 ohne Bestätigung**: Löschen läuft (wie vorher) ohne Rückfrage; `api.shell.dialog` wäre der Ort – Fachentscheidung.
3. **Manifest** (Strang v4/manifests): `styles`, `handlers`, `foreign_collections` (siehe V4-HANDLERS.md) nicht in `module.json` geändert.
