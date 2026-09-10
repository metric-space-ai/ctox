---
name: proxy-model-workers
description: Delegate implementation packages to parallel Codex Desktop workers using Grok, GLM, Kimi or OpenAI, with per-task providers and a PR lifecycle.
---

# Codex Desktop workers

Ein Haupttask analysiert und verteilt unabhängige Arbeitspakete parallel.
Der kurze Auftrag beschreibt Problem, Ergebnis, Grenzen und Erfolgskriterien.
Der Worker entscheidet die Umsetzung. Der Haupttask bündelt Korrekturen,
prüft die Arbeit und archiviert den Worker nach dem Merge seines PRs.
Die gemeinsamen Regeln stehen in GLOBAL-INSTRUCTIONS.md; nicht jedem Auftrag beilegen.

Für die konkrete Bedienung nur den benötigten Abschnitt in [PROTOCOL.md](PROTOCOL.md) lesen:

- Modellwahl und Anmeldung: **Model routing**. Grok, GLM und Kimi nutzen
  `cli_proxy` mit Reasoning `high`; OpenAI behält seine direkte Verbindung.
  Erfahrung: `~/.codex/proxy-workers/MODEL-EXPERIENCE.md`.
  Verfügbarkeit: `scripts/worker.py availability`; Quoten separat zurückstellen.
- Worker anlegen: **Prepare and dispatch**. Der Helper setzt Provider, Projekt,
  Titel und 256k-Kontext ohne Initialisierungsturn. Dann den echten Auftrag senden.
- Korrektur oder Kompaktierung: **Context continuity and publication**.
- PR zuordnen, Review festhalten, archivieren: **PR and merge lifecycle**.

Worktrees und temporäre Daten gehören auf `/Volumes/tmp`; der gemeinsame
Ressourcenwächter bleibt verbindlich. Technische Referenzen und private
Koordination gehören nicht in öffentliche Issues oder PRs.

[ACCEPTANCE.md](ACCEPTANCE.md) ist für Tests der Worker-Integration gedacht,
nicht als Pflichtprogramm für jede Implementierungsaufgabe.
