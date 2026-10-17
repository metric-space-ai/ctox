# Zammad API – Zugang und Rückschrieb-Endpunkte

Quelle: `src/skills/packs/vendor/zammad-rest/references/api-access.md`, ergänzt um die Rückschrieb-Endpunkte aus
`src/core/mission/ticket_zammad_native.rs`.

Bevorzugte Authentifizierung:
- `Authorization: Token token=<token>`

Rückfall:
- HTTP Basic mit `ZAMMAD_USER` und `ZAMMAD_PASSWORD`

Immer `Accept: application/json` und bei Schreibzugriffen `Content-Type: application/json`.

Endpunkte für den Rückschrieb:
- `GET /api/v1/tickets/<id>` – Ticket lesen (Zugangs- und Nummernprüfung)
- `GET /api/v1/ticket_articles/by_ticket/<id>` – Artikel eines Tickets (Doppelanlage vermeiden)
- `POST /api/v1/ticket_articles` – Kommentar/Notiz anlegen; Antwort enthält `id`
- `PUT /api/v1/tickets/<id>` mit `{"state": "<name>"}` – Status setzen

Beispiel (curl, Token aus der Umgebung, nie ausgeben):

```sh
curl -sS -X POST "$ZAMMAD_BASE_URL/api/v1/ticket_articles" \
  -H "Authorization: Token token=$ZAMMAD_API_TOKEN" -H 'Accept: application/json' -H 'Content-Type: application/json' \
  -d '{"ticket_id": 4711, "subject": "CTOX update", "body": "…", "content_type": "text/plain", "type": "note", "internal": true, "sender": "Agent"}'
```
