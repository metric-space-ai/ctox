Werte die Freitextvermerke aus dem CRM Sellify semantisch aus: Darf das Unternehmen bzw. die Person angeschrieben werden (Serienbrief, E-Mail, Anruf)?
Urteile nach dem Sinn des Vermerks, nicht nach Stichworten. Vermerke sind oft Gesprächsnotizen mit Datum; der jüngste Stand zählt.
- "gesperrt": Der Vermerk besagt, dass nicht (mehr) angeschrieben oder kontaktiert werden soll, oder dass die Person nicht mehr erreichbar bzw. nicht mehr im Unternehmen ist (ausgeschieden, Ruhestand, verstorben).
- "pruefen": Der Vermerk wirft eine Frage auf, die ein Mensch vor der Ansprache klären sollte (z. B. angekündigter Wechsel oder Nachfolge, unklare Zuständigkeit, widersprüchliche Angaben).
- "frei": Der Vermerk enthält nichts, was gegen eine Ansprache spricht.
"firma" gilt für das ganze Unternehmen und für Kontakte ohne eigenes Urteil. Setze "firma" nur auf "gesperrt", wenn der Vermerk das ganze Unternehmen betrifft; betrifft er nur eine Person, bewerte diese Person.
"personen": ein Urteil für JEDE aufgeführte person_id, auch ohne eigenen Vermerk (dann das, was der Firmenvermerk für diese Person bedeutet).
"namen": optional, für Personen, die ein Vermerk namentlich nennt, die aber keine person_id haben (z. B. ein genannter Nachfolger), Schlüssel "Vorname Nachname".
Zu jedem Urteil außer "frei" gehören "grund" (kurz, deutsch) und "zitat" (die wörtliche Stelle aus dem Vermerk).
Ablaufhinweis, kein Teil des Ergebnisses: Führe den vom System verlangten Plan mit genau einem Schritt „Vermerke beurteilen“ und setze ihn auf erledigt, bevor du antwortest. Sonst benutze keine Werkzeuge, recherchiere nicht und ändere keine Datensätze.
Das Ergebnis ist ausschließlich genau ein JSON-Block in diesem Format; geprüft wird nur, ob die Urteile zu den Vermerken passen:
```json
{"leads":{"<lead_id>":{"firma":{"status":"frei|pruefen|gesperrt","grund":"","zitat":""},"personen":{"<person_id>":{"status":"frei|pruefen|gesperrt","grund":"","zitat":""}},"namen":{}}}}
```

Die Leads mit ihren Vermerken stehen vollständig unten im Payload JSON unter "freitext_leads". Urteile genau über diese Leads.

Payload JSON:
{{payload_json}}
