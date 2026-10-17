Führe die systematische Recherche für "{{task_title}}" mit dem System-Skill systematic-research fort.
Research Task ID: {{task_id}}
Research Run ID: {{research_run_id}}
Research Command ID: {{research_command_id}}
Requested Discovery Depth: {{discovery_depth}}
Target Verified Sources: {{target_verified_sources}}
Minimum Candidate Sources: {{minimum_candidate_sources}}
Minimum Discovery Rounds: {{minimum_discovery_rounds}}
Minimum Scholarly Rounds: {{minimum_scholarly_rounds}}
Knowledge domain: {{knowledge_domain}}
{{task_prompt_block}}{{criteria_block}}Scoring-Modell:
{{scoring_model}}
Portfolio axes: x={{axis_x}}, y={{axis_y}}
Bekannte Quellen: {{known_source_count}}; davon bereits evidence-eligible: {{verified_source_count}}. Die vollständige kanonische Exclude-Liste steht in web_stack_plan.exclude_urls.
Arbeite iterativ mit den typisierten Web-Stack-Werkzeugen, folge bei wissenschaftlichen Quellen den Referenzen und führe zwei orthogonale Nulltreffer-Runden durch. Behandle Discovery nur als Kandidatenmenge. Evidence, Knowledge, Graph und Reports dürfen ausschließlich aus gelesenen, gesnapshotpten und vom Evidence-Gate zugelassenen Originalquellen entstehen.
Nutze die vom System materialisierte Skill-Anleitung und den serverseitigen Writeback-Vertrag. Erzeuge keine parallelen Tabellen, schreibe nicht direkt in Business-OS-Datenbanken und starte keine Child Agents.
