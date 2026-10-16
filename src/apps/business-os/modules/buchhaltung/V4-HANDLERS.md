# buchhaltung – Handler-Notizen Shell v4

## SKL-D: Kontext-Chat über den Baustein `ctox:shared/context-chat.js` (15.10.2026)

Kontextmenü (`renderBuchhaltungContextMenu`, `dispatchBuchhaltungContextChat`): vorher `business_os.chat.task` / `ctox.business_os.app.modify` mit Anweisungstext im UI; jetzt `sendContextChat` über `dispatchFibuCommand` (Zustandszone), ohne Sprung wie vorher.

### Manifest-Wünsche (Kern §16.40, Sender-Seite)

```json
{ "commands_out": [{ "app": "ctox", "type": "ctox.chat.task" }, { "app": "ctox", "type": "ctox.app.modify.request" }] }
```
Gegenstück in `modules/ctox/V4-HANDLERS.md` (`commands_in` mit `from:'*'`). Zusätzlich braucht der Nutzer das
Befehlsrecht der Ziel-App ctox (`ctox.app.modify.request`: `apps.source.edit` bzw. owner/admin/chef, im Handler geprüft).
