# Lückenschluss Personen-/Firmenrecherche

Firma: {{company}}
Lead/Record: {{record_id}}
Modul: {{module}}
Phase-A-Kommando: {{research_command_id}}

## Offene Felder mit Definition
{{field_lines}}

## Bereits versuchte Quellen aus Phase A
{{attempted_sources}}

## Bereits terminale Phase-A-Felder (unverändert mitführen)
{{terminal_fields}}

## Owner-research_instructions (wörtlich, unverändert)
--- BEGIN OWNER INSTRUCTIONS ---
{{research_instructions}}
--- END OWNER INSTRUCTIONS ---

## Personen-Prioritäten
{{person_priorities}}

## Bereits bekannte Sellify-Personen
{{known_person_records}}

## Verbindlicher Feldvertrag für Phase B
- Bearbeite jedes offene Feld einzeln mit `ctox web search` und `ctox web read`; `ctox web browser-capture` ist optional.
- Vor `no_match` sind mindestens 1 dokumentierte Websuche und mindestens 2 dokumentierte Seitenlektüren (`web_read` oder `browser_capture`) Pflicht.
- Schreibe JEDEN Versuch als JSON-Datei unter `gap_closure/attempts/<feld>/<n>.json` in diesem Workspace. Jeder Versuch enthält kind, query_or_url, result, artifact_path und at.
- Halte den fortlaufenden Sammelstand nach jedem Versuch in `gap_closure/field_status.json` fest. Diese Datei ist der Checkpoint für einen Folgeturn.
- Terminale Feldstatus sind ausschließlich `verified`, `no_match`, `unsupported` und `action_required`.
- `verified` verlangt einen Wert und Belege mit source_id, URL und wörtlichem Belegtext.
- Zwei unabhängige Hosts sind Pflicht für Angaben, die Dritte prüfen können: Firmenname, Anschrift, PLZ, Ort, Land, Aktivitätsstatus, frühere Namen, Geschäftstätigkeit, Geschäftsführung, Prokura, WZ-Code, Umsatz, Mitarbeiter.
- EIN Beleg genügt bei Selbstauskünften, für die es keine zweite unabhängige Quelle geben kann: firma_domain, firma_email, firma_telefon, firma_fax, firma_postfach, firma_besucheranschrift, firma_postanschrift, firma_homepage_fact_sheet sowie alle person_-Felder. Belege sie von der Unternehmensseite bzw. dem Profil selbst und trage den Wert ein, statt ihn als no_match zu verwerfen.
- Listen sind reine JSON-Listen: `"sources": [ {...}, {...} ]`. NIEMALS ein Traegerobjekt wie `{"item": [...]}`, weder bei `sources` und `attempts` noch bei `result.person_records` und `result.evidence`.
- E-Mail-Pruefung (`person_email_validation`) uebernimmt der Daemon selbst: nach jedem Rueckschreiben prueft er jede gelieferte Kontaktadresse ueber experte.de und haengt das Ergebnis dem Kontakt an. Liefere die Adresse als `person_email` mit Beleg und dem `person_key` der Person. Setze `person_email_validation` NICHT auf `no_match`, nur weil du die Pruefung nicht selbst ausfuehren kannst.
- Personenbezogene Ergebnisse und Belege tragen einen stabilen `person_key`.
- Status je Person: Bei mehreren Personen gehoert der Status eines person_-Feldes in `result.person_field_status` als `{"<person_key>": {"person_email": {"status": ..., "reason": ..., "sources": [...]}}}`. Das Lead-weite `field_status` fasst pro Feld nur EINEN Eintrag; der Status einer zweiten Person ueberschrieb dort den der ersten. Ein fehlerhafter Eintrag einer Person verwirft nur diesen Eintrag.
- Schreibe in `result.fields` nur strukturierte Feldobjekte, keine freien Texte.
- `action_required` ist ausschließlich für Login/Freigabe zulässig und verweist auf einen Auth-Assist (source_id plus Task-/Command-ID) oder eine Quelle mit `requires_credential=true`.
- Abschluss erfolgt AUSSCHLIESSLICH über deine Schlussantwort: genau ein ```json-Block mit `field_status` und `result`; die App schreibt ihn beim Abschluss des Tasks über den typisierten Befehl `outbound.lead.research_writeback` zurück.
- Der Task darf sich erst als fertig melden, wenn `gap_closure/field_status.json` und alle Versuchsdateien geschrieben sind.

## Writeback-Vertrag
{{writeback_contract}}

Die Felder `record_id`, `module`, `research_command_id` und `gap_task_id` setzt die App aus dem Task; dein JSON-Block enthält nur `field_status` und `result`. `field_status` muss ALLE angeforderten Felder abdecken; bereits terminale Phase-A-Felder werden unverändert mitgeführt.