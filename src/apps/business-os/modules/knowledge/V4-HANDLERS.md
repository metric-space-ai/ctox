# knowledge – Handler-Notizen Shell v4 (Hauptdokument: V4-PORT.md)

## SKL-D: Kontext-Chat über den Baustein `ctox:shared/context-chat.js` (15.10.2026)

„App modifizieren“ im Kontextmenü (`dispatchContextChat`): vorher `ctox.business_os.app.modify`; jetzt `sendContextChat` (Modus app) über `dispatchKnowledgeCommand`. „Mit Daten arbeiten“ bleibt der eigene Befehl `ctox.knowledge.chat` (SKL-B2).

### Manifest-Wünsche (Kern §16.40, Sender-Seite)

```json
{ "commands_out": [{ "app": "ctox", "type": "ctox.app.modify.request" }] }
```
Gegenstück in `modules/ctox/V4-HANDLERS.md` (`commands_in` mit `from:'*'`). Zusätzlich braucht der Nutzer das
Befehlsrecht der Ziel-App ctox (`ctox.app.modify.request`: `apps.source.edit` bzw. owner/admin/chef, im Handler geprüft).
