Erstelle das Word-Dokument "{{title}}".
Nutzerauftrag: {{prompt}}

Nutze den systematic-research Skill, die CTOX Report-Pipeline und den doc/Documents Word-Produktionsskill.
{{knowledge_context}}
Der Skill strukturiert die Ableitung, ist aber keine Primaerquelle. Lies fuer faktische Aussagen die darin referenzierten Originalquellen und zitiere diese, nicht den Skill als Ersatzquelle.
Halte die Knowledge-Lookup-Pflicht aus dem systematic-research Skill ein und verwende immer den neuesten verfuegbaren Knowledge-Stand.
{{report_type}}
Erzeuge ein solides .docx-Dokument mit sauberer Struktur, Quellen/Evidenz, Tabellen und sinnvollen Abbildungen/Diagrammen, wo sie fachlich tragen.
Wende den Documents-Skill-Workflow an: Design-Preset, echte Word-Styles, echte Listen/Tabellengeometrie, DOCX-Render/QA soweit die lokale Runtime es erlaubt.
Erzeuge kein Markdown als Endartefakt. Das finale Artefakt muss ein Word-Dokument sein.
Speichere das finale DOCX im Workspace unter {{output_path}}.
Der Business-OS Writeback registriert genau diese DOCX-Datei danach automatisch im Documents-Modul.

Runbook "{{runbook_title}}": {{runbook_prompt}}

Abschluss: Schreibe nichts selbst in Business-OS-Daten. Antworte zum Schluss mit genau einem JSON-Objekt {"output_path": "{{output_path}}", "knowledge_version_id": "<geprüfter Knowledge-Stand>", "source_receipt_links": [{"kind": "source_receipt", "id": "<source_id>", "table_id": "<Knowledge-Tabelle>", "snapshot_hash": "sha256:<hex>"}]}. Die Documents-App übernimmt die DOCX nur mit geprüften Quellbelegen.
