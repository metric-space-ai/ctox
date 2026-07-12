# CTOX Harness Hardening — Remediation-Ledger 2026-07-10

Dieses Ledger ergänzt das historische Review vom 10.07.2026. Das Review selbst
bleibt unverändert. Statuswerte sind `open`, `partial`, `verified_fixed` und
`deferred`; ein Status wird erst nach den zugehörigen Regressionstests auf
`verified_fixed` gesetzt.

| Befund / Vertrag | Status | Remediation / verbleibende Abnahme |
| --- | --- | --- |
| Inbound-LIFO und same-owner Re-Lease | verified_fixed | Neueste Nachricht je Thread bleibt erhalten; Threads altern nach `first_pending_at`, privilegierte Absender erhalten höchstens 24h Zeitvorsprung. Leasing ist pending-only, per CAS geschützt und `message_key`-Dedupe gilt unabhängig vom Busy-State. Fairness-, CAS- und Pending-only-Regressionstests sind grün. |
| Approval-Replies ohne serverseitige Autorisierung | verified_fixed | Autorisierte exakte `APPROVE`/`REJECT`-Zeilen, Quote-Ausschluss, Approval-Ledger mit Body-Hash und idempotente Folgearbeit sind implementiert. Der End-to-End-Test für Autorität, Ledger, Gate-Transition und genau eine Folgearbeit ist grün. |
| Nag-Ende macht offenes Gate unsichtbar | partial | Gate bleibt offen, Nag-Ende wird einmalig als Governance-Eskalation persistiert. Operator-Projektion ist noch zu verifizieren. |
| `review-1`: NoSend Namens-/Text-Scrape | verified_fixed | Service konsumiert `ReviewDisposition::NoSend`, `NoSendReason` und optionalen `WAIT_REF` strukturell; Rewrite/Rework-Findings verhindern den Abschluss. Der frühere Vornamen-/Keyword-Scrape ist entfernt. |
| Sprach-/Fingerprint-basierte Review-Pflicht | verified_fixed | Review-Pflicht hängt nur noch an durabler Queue-/Ticket-Arbeit oder externer Wirkung. Text und Sprache beeinflussen die Pflicht nicht. Ausnahmen sind nur als `ReviewRequirement::Exempt { policy_id }` darstellbar. |
| Heuristische Approval-Plan-Waits | verified_fixed | `PlanWaitCondition`, `planned_goal_waits`, CLI `--wait-for`, Restart-Reconcile und automatische Goal-/Step-Aktivierung sind implementiert. Altbestände werden `legacy_unresolved`; strukturierte Wait-/Wake- und Failed-Plan-Regressionstests sind grün. |
| Ein Plan-Goal stoppt alle Due-Goals | verified_fixed | Fehler werden pro Goal mit Zähler, Governance-Evidenz und `300s × 2^n` bis 1h isoliert. Der Fehler-Injektionstest bestätigt, dass weitere Goals verarbeitet werden. |
| Durable Backlog fehlt im Pressure-Guard | partial | Guard verwendet `max(in_memory_prompts, durable_pending_queue)`; Recovery und Approval laufen vor dem Pressure-Return. Schedule-/Workflow-Producer-Abnahme steht noch aus. |
| Ticket-Reconcile stoppt bei Queue-Backlog | verified_fixed | Backlog-Early-Return ist entfernt; Lease-, Blocked- und Failed-Reconcile laufen unabhängig von Queue-Tiefe. |
| Owner-gebundene / nicht auslaufende Leases | verified_fixed | Routing-State hat additive Lease-/Retry-Felder, Leases laufen nach 15 Minuten owner-unabhängig aus und aktive Worker erneuern alle 60 Sekunden. Spill-Restore verwendet durable Blocked-/Pending-Gründe statt permanenter Lease. |
| Ticket-Event-Retry ohne Failure-Klasse | verified_fixed | `TicketEventFailureClass`, Retry-Budget 3, Backoff und terminaler Failure-Proof sind implementiert. Der Retry-Test bestätigt drei restart-feste Versuche und anschließende Terminalisierung. |
| Retrybarer Modell-/API-Fehler bei Business-OS-Kommandos | verified_fixed | Derselbe Command und dieselbe Queue-Zeile wechseln über `running -> retry_wait` und `leased -> pending`; der Cooldown steht in Queue-Metadaten und Routing-State. Der Regressionstest bestätigt den zweiten Attempt ohne neue Identität und verhindert die frühere ungültige Transition `running -> leased`. |
| Prozesslokaler Continuity-Refresh | verified_fixed | Durable Statuszeilen je Conversation/Kind, pending Retry/Backoff, Head-Advance-Verbrauch und 8-Turn-Kommunikationsgrenze sind implementiert. Boundary-, Restart- und Kommunikations-Regressionstests sind grün. |
| `continuity-update` fällt auf Conversation 1 zurück | verified_fixed | `--conversation-id` ist verpflichtend; fehlender Wert wird abgewiesen. |
| Vollhistorie im Live-Turn / Forgotten lädt Renderstände | verified_fixed | Live-Turn nutzt ein auf 512 Context-Items begrenztes Working-Set; Forgotten nutzt nur die benötigten letzten Diff-Texte. Der 50.000-Messages-/10.000-Commits-Stresstest ist grün. |
| Heuristic-Summarizer im Live-Pfad | partial | Toolfreier semantischer Modell-Summarizer mit deterministischem Fallback implementiert; Qualitäts-/Kostenmessung steht noch aus. |
| Reviewer ist nur prompt-seitig read-only | verified_fixed | Reviewer erhalten einen leeren, wegwerfbaren Scratch-CWD; das autoritative Workspace-/Runtime-Verzeichnis bleibt außerhalb davon read-only. Die Rolle `SubAgentSource::Review` erzwingt auch mit schreibbarem Scratch die nicht-mutierende Tool-Surface. Scratch-Lifecycle-, Boundary- und Fork-Tool-Registry-Tests sind grün. |
| Agent-Job-Leaf angeblich „report-only“ | verified_fixed | Leaf-Surface ist Workspace-Arbeit plus `report_agent_job_result`, ohne Spawn/Channel/Meeting/Control-Plane-Mutationen; Liveness-Proof beschreibt die reale Surface. |
| `Some([])` restauriert persistierte Tools | verified_fixed | Leere explizite Tool-Liste bleibt in Session-Metadaten erhalten, gewinnt vor State-DB-Restore und löscht persistierte dynamische Tools. |
| Evidence-Store für Worker-Shell schreibbar | verified_fixed | Workspace-Sandboxes schützen `.agents`, `.codex`, `.ctox`, `runtime` und `.git`; Proof-, Review- und Approval-Transitions laufen über serverseitige State-Machine-Guards. Sandbox- und State-Write-Guard-Tests sind grün. |
| Typed Business-OS-Command scheitert an read-only Worker-Runtime | verified_fixed | `business-os commands dispatch` überspringt den caller-seitigen CLI-Turn-Ledger und routet bei laufendem Dienst über lokalen Service-IPC. Policy-, State-Machine-, Projection- und Evidence-Writes erfolgen im Daemon; ein vorhandener, aber nicht erreichbarer Socket führt fail-closed zum Fehler. IPC- und Ledger-Boundary-Regressionstests sind grün. |
| Temporär nicht verfügbarer Reviewer blockiert retrybares Business-Command | verified_fixed | Technische, fehlende Review-Evidenz und fehlende Artefakte bleiben als `held` protokolliert, projizieren das Command aber nach `retry_wait` statt `blocked`. Der Queue-Backoff bleibt autoritativ und derselbe Command kann nach Ablauf erneut von `retry_wait` geleast werden; `WaitingExternal` bleibt dagegen dormant `blocked`. |
| Harness-Bench übersieht erfolgreiche Threads-Route | verified_fixed | Statusauflösung verwendet den kanonischen `business-os/threads/<id>`-Record-Key für Thread-, Approval- und Notification-Korrelation und gibt denselben vollständigen Key aus. Der Verifier kann eine serverseitig erfolgreich persistierte Human-Route dadurch nicht mehr fälschlich als fehlend melden. |
| Semantischer Answer-only-Review bewertet Worker-Runtime-Wrapper als Fachauftrag | verified_fixed | Der begrenzte Reviewer erhält Ziel und Textvertrag ausschließlich aus dem ursprünglichen durablen Business-Command-Payload. Workspace-/Execution-Wrapper, Retry-Feedback und doppelte Prompt-Felder werden nicht angehängt; ohne durablen Textvertrag fällt die Arbeit fail-closed auf Full-Evidence-Review zurück. Ein Regressionstest verwendet einen real verknüpften Command und einen absichtlich kontaminierten Worker-Prompt. |
| Queue-Repair Plan-/Apply-Trennung | verified_fixed | Repair erzeugt einen read-only Plan, verifiziert ihn und wendet nur registrierte deterministische Rust-Operationen an; Reviewer besitzen keine mutierenden Tools. |
| Spawn-Budget pro Work-Episode | verified_fixed | Budget-Keys verwenden die durable Work-Episode; eine neue Parent-Episode erhält ein neues endliches Budget, historische Spawn-Edges bleiben unverändert. Episode- und Kaskaden-Regressionstests sind grün. |
| Vollständige HoldReason-/WaitRef-State-Machine | verified_fixed | `WaitingExternal` blockiert und wacht ausschließlich am referenzierten Ereignis; technische und Evidenz-Holds verwenden das Fünfer-Budget mit Backoff. Typed-Hold-/Wake-Tests und der Core-State-Write-Guard sind grün. |

## Abnahme

Vor einer vollständigen Freigabe sind mindestens auszuführen:

- gezielte Tests für Channels, Approval-Nag, Plan, Review, Tickets, Continuity und Tool-Surface;
- `cargo fmt --check`;
- `cargo check`;
- `cargo test`;
- `cargo run -- process-mining spawn-liveness`.

Offene oder nur partielle Zeilen dürfen nicht durch Backlog-Checkboxen als
abgeschlossen dargestellt werden. Der vollständige Workspace-Testlauf vom
10.07.2026 war wegen zahlreicher bereits vorhandener, nicht harness-bezogener
Fehler im stark geänderten Worktree nicht grün (2018 bestanden, 204
fehlgeschlagen). Die dabei sichtbar gewordenen harness-nahen Regressionen
(Plan-NULL, Teams-Review-Text, Spawn-Episode und geschützte State-Writes) wurden
anschließend einzeln korrigiert und gezielt grün verifiziert; ein erneuter
vollständiger Lauf bleibt vor Release erforderlich.
