Starte eine Outbound {{research_label}} für {{lead_name}} [{{record_id}}] (Auftrag {{command_id}}).{{dnb_hint}}

Rechercheablauf des Owners (research_instructions, wörtlich, unverändert, verbindlich):
{{research_instructions}}

Business OS command:
- command_id: {{command_id}}
- module: outbound-lead-generation
- type: outbound.lead.research
- record_id: {{record_id}}

Payload JSON:
{{payload_json}}
