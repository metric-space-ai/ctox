# Personen-/Firmenrecherche Phase A

Firma: {{company}}
Land: {{country}}
Modus: {{mode}}
Lead/Record: {{record_id}}
Recherche-Kommando: {{research_command_id}}

## Angefragte Felder mit Definition
{{field_lines}}

## Owner-research_instructions (wörtlich, unverändert)
--- BEGIN OWNER INSTRUCTIONS ---
{{research_instructions}}
--- END OWNER INSTRUCTIONS ---

## Freigegebene private Quellen (include_private)
{{include_private}}

## Registrierte Laufzeitquellen (source_policy.sources)
{{runtime_sources}}

## Personen-Prioritäten
{{person_priorities}}

## Bereits bekannte Sellify-Personen
{{known_person_records}}

## Auftrag
- Stelle zuerst den Suchplan auf (Skill §2): je angefragtem Feld die zuständigen Quellen, Reihenfolge P → S → C, private
  Quellen nur, wenn sie oben freigegeben sind; registrierte Laufzeitquellen laufen als Adapter vor der Websuche.
- Sammle je Feld Kandidaten mit Quelle, URL und wörtlichem Beleg (Skill §3); entscheide nicht über verified/no_match –
  das macht die App nach dem Rückruf und gibt offene Felder an den Lückenschluss weiter.
- Gruppiere Personen nach Skill §4 und sortiere sie nach den Personen-Prioritäten.
- Antworte mit einer kurzen Zusammenfassung und genau EINEM ```json-Block nach Skill §5, nur mit diesen Feldern:
{{requested_fields}}
- Keine Rückschreibbefehle, kein `ctox`-CLI, kein Business-OS-Werkzeug: die App liest das Ergebnis aus deiner Schlussantwort.
