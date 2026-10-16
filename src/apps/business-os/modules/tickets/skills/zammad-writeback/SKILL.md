---
name: zammad-writeback
description: Use when the Tickets app hands over a writeback to a Zammad helpdesk ticket - a public or internal comment, a clarification question, or a state transition with an optional note. Performs exactly the requested REST writes against the live Zammad instance and returns the created article ids, which the Tickets app records on its writeback row.
---

# Zammad-Rückschrieb (Tickets-App)

Quelle: `src/skills/packs/vendor/zammad-rest` (inhaltlich auf den Rückschrieb der Tickets-App zugeschnitten) und das
frühere Kern-Adapterverhalten `src/core/mission/ticket_zammad_native.rs` (`writeback_comment`, `writeback_transition`,
`create_article`, `update_ticket_state`).

Die Tickets-App hat den Auftrag angelegt. Die Anweisung nennt Ticketsystem, Ticketnummer (`remote_ticket_id`, die
numerische Zammad-ID) und was zu schreiben ist. Dieselben Angaben stehen maschinenlesbar in der Task-Payload:
`operation` (`comment`, `clarification_request` oder `transition`), `remote_ticket_id`, `body`/`internal` bzw.
`state`/`note_body`/`internal_note`, dazu `writeback_id` und `ticket_key`.

Du schreibst nur das, was der Auftrag verlangt. Das Ergebnis meldest du als Antwort zurück; die Tickets-App trägt es
selbst ein (Writeback-Zeile, Fallzustand, Rückfrage). Rufe dafür keine CTOX-Befehle, keine CLI und kein MCP auf.

## Zugang

Bevorzugte Geheimnisquellen (aus dem CTOX-Secret-Store bzw. der Konfiguration der Ticketquelle):
- `ZAMMAD_BASE_URL`
- `ZAMMAD_API_TOKEN`
- Rückfall: `ZAMMAD_USER` und `ZAMMAD_PASSWORD`

Bevorzugte Laufzeitdatei auf dem Zielhost: `runtime/secrets/zammad-admin.env`. Fehlt sie, prüfe, ob die Werte schon
anderswo hinterlegt sind, bevor du aufgibst. Authentifizierung und Endpunkte: `references/api-access.md`.

## Ablauf

1. Basis-URL und Zugang bestätigen (eine authentifizierte Leseanfrage, z. B. `GET /api/v1/tickets/<id>`, muss
   gelingen; so ist auch die Ticketnummer geprüft).
2. Je `operation`:
   - `comment` und `clarification_request`: einen Artikel anlegen
     `POST /api/v1/ticket_articles` mit
     `{"ticket_id": <id als Zahl>, "subject": "CTOX update", "body": "<body getrimmt>", "content_type": "text/plain",
     "type": "note", "internal": <internal>, "sender": "Agent"}`.
   - `transition`: zuerst den Status setzen `PUT /api/v1/tickets/<id>` mit `{"state": "<state getrimmt>"}`. Nur wenn
     `note_body` nicht leer ist, danach einen Artikel wie oben mit `body = note_body` und `internal = internal_note`.
3. Jede Antwort prüfen: ein angelegter Artikel liefert ein Objekt mit `id`. Erst ein erfolgreicher Aufruf zählt.
4. Nichts darüber hinaus ändern (keine Zuweisung, keine Priorität, keine weiteren Artikel). Eine Wiederholung
   desselben Auftrags (gleiche `writeback_id`) legt keinen zweiten Artikel an: vorher
   `GET /api/v1/ticket_articles/by_ticket/<id>` prüfen, ob ein Artikel mit genau diesem Text schon von dir stammt,
   und dessen `id` melden.

## Ergebnis

Beende die Arbeit mit genau einem JSON-Objekt als letzter Ausgabe:

```json
{"remote_event_ids": ["<id des angelegten Artikels>"]}
```

- `comment`/`clarification_request`: genau eine Artikel-ID.
- `transition` ohne Notiz: `{"remote_event_ids": []}` nach erfolgreichem Statuswechsel.
- Ist der Rückschrieb nicht gelungen (Zugang fehlt, Ticket unbekannt, HTTP-Fehler), melde stattdessen
  `{"error": "<knappe Ursache mit Endpunkt und HTTP-Status>"}`. Die Tickets-App markiert den Rückschrieb dann als
  fehlgeschlagen; behaupte nie einen Erfolg ohne angelegten Artikel bzw. bestätigten Statuswechsel.

## Leitplanken

- Geheimnisse nie in Ausgaben, Logs oder das Ergebnis schreiben.
- Nicht aus dem Gedächtnis antworten, wenn die Live-API abfragbar ist.
- Keine Tokens anlegen oder rotieren; fehlt der Zugang, das als `error` melden.
