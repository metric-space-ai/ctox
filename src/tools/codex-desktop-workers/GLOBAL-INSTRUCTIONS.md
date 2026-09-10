# Zusammenarbeit mit Workern

Der Haupttask versteht das Problem, nutzt bei Bedarf Analyse-Subagents und zerlegt
das Ziel in sinnvolle Arbeitspakete. Er vergibt unabhängige Pakete parallel;
es gibt keine pauschale Begrenzung der Worker-Anzahl.

Ein Auftrag beantwortet kurz vier Fragen:
- Welches Problem lösen wir?
- Welches Ergebnis wird erwartet?
- Welche Grenzen gelten?
- Woran erkennen wir Erfolg?

Nur hilfreiche Fundstellen ergänzen. Keine vorweggenommene Implementierung,
Befehlsketten oder wiederholten Prozessregeln. Der Worker erkundet den Code,
entscheidet die Umsetzung und liefert das Ergebnis einschließlich nötiger Tests.
Zusammengehörige Arbeit bleibt in einem Paket.

Der Haupttask prüft das Ergebnis und gibt nötige Korrekturen gesammelt an denselben
Worker. Normale Nacharbeit braucht keine Supervisor-Freigabe. Nachrichten sind
verständlich und knapp; technische Kennungen stehen nur bei Bedarf separat.

Jeder Worker liefert einen PR. Vor Veröffentlichung prüft der Haupttask Änderungen
und Text auf private Informationen und Geheimnisse. Nach erfolgreichem Review und
den erforderlichen Tests hält er die Modellerfahrung kurz fest, merged und
archiviert den Worker. PR-Verwaltung dient der erledigten Arbeit.

Worker melden Ergebnisse oder echte Blockaden an ihren Haupttask und beenden den
Turn. Korrekturen starten danach einen neuen Turn im selben Worker. Der Haupttask
meldet relevante Ergebnisse oder ungelöste Blockaden dem Supervisor, ohne auf eine
Empfangsbestätigung zu warten. Keine Warteschleifen oder unveränderten Statusmeldungen.

Auftrag und Zwischenstand dauerhaft sichern und nach Kompaktierung fortsetzen.
Kein READY- oder Initialisierungsdialog. Worktrees und Builddaten liegen auf der
tmp-Platte; vorhandene Ressourcenregeln gelten auch bei parallelen Workern.

Modelle nach Erfahrung und Verfügbarkeit wählen. Quoten sind vorübergehende
Blockaden, keine Modellschwäche. OpenAI bleibt direkt angebunden; andere Modelle
nutzen ihren eigenen Provider. Technische Bedienung bei Bedarf im Skill
`proxy-model-workers` nachschlagen, nicht in jeden Auftrag kopieren.
