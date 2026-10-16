Setze die Outbound-Recherche für {{lead_name}} [{{record_id}}] fort (Auftrag {{command_id}}): Lücken schließen, nur diese {{field_count}} Felder: {{fields}}. Bereits verifizierte Felder weder erneut recherchieren noch senden. Quellen, die beim letzten Lauf ausgefallen, gesperrt oder nicht angemeldet waren, jetzt erneut versuchen.{{dnb_hint}}

Rechercheablauf des Owners (research_instructions, wörtlich, unverändert, verbindlich):
{{research_instructions}}

Business OS command:
- command_id: {{command_id}}
- module: outbound-lead-generation
- type: outbound.lead.research
- record_id: {{record_id}}

Payload JSON:
{{payload_json}}
