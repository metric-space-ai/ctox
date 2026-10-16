---
name: support-workflow
description: Use when the Support app asks CTOX to work on a support conversation ("CTOX fragen") - analyse the conversation, assess risk and SLA, draft a reply and propose the next action. Returns structured suggestions that the Support app stores as reviewable proposals; nothing is sent to the customer and no conversation state is changed by the agent.
---

# Support-Workflow (Support-App)

Herkunft: bisher ein Phantom-Verweis `business-os-support-workflow` (`modules/support/support-commands.mjs`, Strang
SKL-A1 hat ihn ersetzt). Inhalt abgeleitet aus dem heutigen Verhalten: Anweisung `buildAgentInstruction`
(`modules/support/index.js`), Rückschreibvertrag `support.agent.writeback` (`support.rs:1229-1278`), Vorschlagsarten
`SUPPORT_AGENT_SUGGESTION_KINDS` und die Regel `required_human_action: review`.

## Auftrag

Die Support-App schickt eine Konversation (ID, Status, Priorität) und einen Datensatz-Schnappschuss: Konversation,
Thread-Verknüpfungen, letzte Nachrichten, interne Notizen, Ereignisse, frühere Vorschläge, Kundenkonto/-kontakt und
ein verknüpftes Ticket, soweit vorhanden. Der vollständige Schnappschuss steht zusätzlich in der Task-Payload
(`record_snapshot`); die Anweisung enthält ihn gekürzt.

Liefere:
1. eine knappe Zusammenfassung des Anliegens (`summary`),
2. eine Risiko-/SLA-Einschätzung (`classification`, z. B. Dringlichkeit, Stimmung, Fristrisiko),
3. einen Antwortentwurf an den Kunden (`draft_reply`),
4. die nächste Aktion für das Team (`next_action`),
5. optional ein Kunden-Update (`customer_update`) oder eine Ticket-Aktion (`ticket_action`), wenn der Verlauf das
   nahelegt.

## Grenzen

- Du schlägst nur vor. Ein Mensch prüft jeden Vorschlag in der Support-App und übernimmt oder verwirft ihn.
- Sende keine Nachricht an Kunden, ändere keinen Status, keine Zuweisung, keine Priorität, lege kein Ticket an.
- Rufe keine CTOX-Befehle, keine CLI und kein MCP auf, um Ergebnisse zurückzuschreiben: das Ergebnis kommt über
  deine Antwort zur Support-App zurück.
- Erfinde keine Fakten. Was im Schnappschuss fehlt, benennst du als offene Frage im Vorschlag.
- Schreibe in der Sprache der Konversation (im Zweifel Deutsch). Keine personenbezogenen Daten über das hinaus, was
  der Schnappschuss schon enthält.

## Ergebnis

Beende die Arbeit mit genau einem JSON-Objekt als letzter Ausgabe (gern als ```json-Block):

```json
{
  "suggestions": [
    {
      "suggestion_kind": "summary",
      "summary": "Ein Satz, der den Vorschlag benennt",
      "confidence": 0.8,
      "required_human_action": "review",
      "payload": { "text": "…" }
    }
  ]
}
```

- `suggestion_kind`: eine von `summary`, `draft_reply`, `classification`, `next_action`, `customer_update`,
  `ticket_action`. Andere Arten verwirft die Support-App.
- `summary`: kurze Überschrift, höchstens ein Satz.
- `confidence`: Zahl zwischen 0 und 1.
- `required_human_action`: `review` (Standard) oder `approve`, wenn eine Freigabe nötig ist.
- `payload`: Inhalt des Vorschlags, z. B. `{"text": "…"}` für Zusammenfassung, Entwurf und Aktion,
  `{"priority": "high", "sla_risk": "…", "sentiment": "…"}` für die Einschätzung, `{"action": "…", "reason": "…"}`
  für eine Ticket-Aktion.

Kannst du die Konversation nicht bearbeiten (Schnappschuss leer, Inhalt unlesbar), antworte mit
`{"error": "<knappe Ursache>"}`; die Support-App zeigt den Fehlschlag an.
