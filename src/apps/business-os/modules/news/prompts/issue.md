Erstelle die News-Ausgabe für das Thema "{{topic_title}}" mit dem Skill news-aggregation.
Fokus: {{focus}}
Sprache: {{language}}. Höchstens {{max_posts}} Beiträge, Kategorien: {{categories}}.
Quellen ({{source_count}}): {{source_line}}.
Angemeldete Quellen (X, Reddit) über das CTOX-Konto lesen; --task-id {{signed_in_task_id}}.
Vorherige Ausgabe {{previous_issue_date}}: wiederhole keine Meldung aus früheren Ausgaben (covered_urls).
Zeitfenster: nur Meldungen, deren Primärquelle nach {{window_start}} veröffentlicht wurde; ältere Beiträge verwirft die Übernahme.
Liefere das Ergebnis im Vertrag ctox.news.issue.v1; jeder Beitrag braucht mindestens eine Quelle mit Link.
{{run_contract}}

Auftragsdaten (JSON):
{{context_json}}
