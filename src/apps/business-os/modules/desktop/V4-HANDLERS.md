# desktop – v4-Handler (Strang H8, Shell v4)

Stand 15.10.2026. Quelle der Einstufung: `~/.local/state/shell-v4/analysis/E-befehlstypen.md` (§2 Tabelle, §3 Capabilities),
Trennregel Architektur §16.13, Handler-Vertrag §16.16. Rust nur gelesen: `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/`.
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`; Fehler `throw {code, message}`
(`invalid|forbidden|not_found|conflict|revoked|expired|quota|unavailable`). Tests: `node --test handlers/tests/*.test.mjs`
(In-Memory-ctx `handlers/tests/_ctx.mjs`: eigene Collections = v4-Manifest `collections`, Fremdzugriff = Manifest
`foreign_collections` + hier beantragte Ergänzungen; Capabilities protokolliert und je Test beantwortbar).

**Stand: 0 von 4 Befehlstypen als JS-Handler, 0 über `ctx.cap.legacy.<command_type>`, 4 PLATTFORM (nur dokumentiert). Tests: – grün.**

## Befehlstypen

| # | Befehlstyp | Einstufung | Rust-Ort | Ziel | Stand |
|---|---|---|---|---|---|
| 1 | `ctox.module.save` | PLATTFORM | `store.rs:17773` | Kern (App-Lebenszyklus) | nur dokumentiert |
| 2 | `ctox.business_os.app.create` | PLATTFORM | `store.rs:6753` (Rechte-Vorprüfung `:6790-6794`) | Kern (Queue) | nur dokumentiert |
| 3 | `ctox.business_os.app.modify` | PLATTFORM | `store.rs:6753` | Kern (Queue) | nur dokumentiert |
| 4 | `business_os.chat.task` | PLATTFORM | `store.rs:6753` | Kern (Queue) | nur dokumentiert |

Für die eigenen Collections (`desktop_icons`, `desktop_layout`, `desktop_notifications`) gibt es keine native
Fachlogik (C1 §14 Nr. 10: keine Treffer); Symbol-Layout und -Ziehen liegen bereits in Browser-JS (`iconLayout.js`,
`iconDrag.js`, `layout-authority.js`). Dateiebene (`desktop_files.rs`, `rxdb_peer_desktop_files.rs`) ist Fähigkeit
`cap.transfer`/`cap.blob` und gehört „ctox sync v4“. Kein Handler.

## SKL-D: Kontext-Chat über den Baustein `ctox:shared/context-chat.js` (15.10.2026)

Symbol-Menü „Frage / Mit Daten arbeiten / App ändern“ und Flächenmenü „Mit CTOX chatten“: vorher Sprung `api.shell.navigate("ctox", {chat})` mit Entwurf, den die ctox-App nie las (OFFEN-4); jetzt Text über `api.shell.dialog({kind:"prompt"})` im App-Fenster, dann `sendContextChat` (App ändern zielt mit `source_app` auf die App hinter dem Symbol), Sprung in den ctox-Chat.

### Manifest-Wünsche (Kern §16.40, Sender-Seite)

```json
{ "commands_out": [{ "app": "ctox", "type": "ctox.chat.task" }, { "app": "ctox", "type": "ctox.app.modify.request" }] }
```
Gegenstück in `modules/ctox/V4-HANDLERS.md` (`commands_in` mit `from:'*'`). Zusätzlich braucht der Nutzer das
Befehlsrecht der Ziel-App ctox (`ctox.app.modify.request`: `apps.source.edit` bzw. owner/admin/chef, im Handler geprüft).
