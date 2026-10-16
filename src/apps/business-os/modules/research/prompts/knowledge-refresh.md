Baue oder aktualisiere die Knowledge Base fuer das abgeschlossene Research "{{task_title}}".
Research Task ID: {{task_id}}
Research Run ID: {{research_run_id}}
Research Command ID: {{research_command_id}}
Knowledge domain: {{knowledge_domain}}

Erzeuge bzw. aktualisiere einen fachlichen Skill/Skillbook und die dazugehoerigen Runbooks und Ressourcen.
Verwende die bestehenden stabilen IDs und aktualisiere vorhandene Elemente per Upsert; erzeuge keine parallelen Kopien derselben Knowledge Base.
Der Skill ist der Wissens- und Arbeits-Hub, aber keine Ersatzquelle: Jede faktische Aussage muss auf die originalen source_id/source_url-Eintraege aus source_catalog und evidence_points zurueckverweisen.
Uebernimm keine unbelegten Aussagen. Halte Quellen, Evidenz, Tabellen und Ableitungen getrennt nachvollziehbar.
Erzeuge Runbooks fuer wiederkehrende Analysen und Dokumenttypen, die dieses Knowledge und bei Bedarf die Originalquellen erneut lesen.
Bewahre die Verbindung zu Research Task, Research Run und Knowledge-Tabellen, damit spaetere Research-Laeufe dieselben Elemente aktualisieren koennen.
