Nutze den CTOX Skill outbound/campaign-setup.

Aufgabe:
Richte die Outbound Campaign anhand des natuerlich formulierten Briefings ein. Verwende keine lokalen Heuristiken der App und keine HTTP-Datenkanaele. Die Outbound-App ist nur der Task-Produzent.

Nicht verhandelbar:
- Keine Nachricht senden.
- Keine E-Mail, keinen Brief und keine externe HTTP-Integration ausloesen.
- Keine Daten- und App-Aenderungen selbst zurueckschreiben; die Outbound-App schreibt deine Antwort per `outbound.campaign.apply_setup` ein.
- Outbound-Kommunikation bleibt approval-gated.
- Wenn ein Wunsch nicht durch vorhandene Outbound-Settings abbildbar ist, schreibe ihn als konkreten App-Extension-Request in die Antwort.

Campaign:
- campaign_id: {{campaign_id}}
- name: {{campaign_name}}
- command_id_for_writeback: {{command_id}}
- selected_template_id: {{template_id}}
- selected_template_title: {{template_title}}

User-Briefing:
{{briefing}}

Erwartetes Ergebnis:
1. Interpretiere Zielgruppe, Kanal, Angebot, Timing, Landingpage-/Webinar-Wuensche und manuelle Schritte.
2. Erstelle passende Campaign Settings, Research Settings, Outreach-Strategie und Kommunikations-/Approval-Plan.
3. Antworte ausschliesslich mit genau einem JSON-Objekt {"status": "<optional: setup_required|active|paused|closed>", "campaign_payload_patch": {...}}; erlaubte Schluessel im Patch: campaign_setup (Pflicht, mit status), research_settings, active_outreach, communication_strategy, sequence_strategy, landing_page, app_extension_requests.
4. Wenn die App eine benoetigte Faehigkeit noch nicht hat, notiere sie unter `app_extension_requests` statt clientseitig zu improvisieren.
