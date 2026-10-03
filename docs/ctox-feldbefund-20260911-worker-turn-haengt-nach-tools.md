# Feldbefund 11.09.2026 — Recherche-Turn steht nach zwei Terminal-Aufrufen still, Lease läuft weiter

Mandant: THESEN (`thesen.ctox.dev`), Release `branch-main-20260910T143924Z`,
Daemon-PID 1269110 (seit 10.09. 17:59 UTC), Modell MiniMax-M3 über
`ctox_core_api` (`https://llm.ctox.dev/v1`, `wire_api=responses`).

## Beobachtung

Outbound-Nachrecherche Sasol Germany GmbH, Befehl
`leadgen-lead-research-979a0cfc-2f45-4e72-98c4-0152446e2cdf`, Queue-Aufgabe
`queue:system::0e2ec9a71fa7fac4b38de605`, Thread
`01a08dd2-bdb6-7e33-b50f-e2003eab8656`, Turn
`01a08dd2-ca40-7de1-9786-d1d6082d9050`.

Zeitlinie (UTC, aus `context-log.jsonl`, `ctox_harness_flow_events`, Journal):

| Zeit | Ereignis |
|---|---|
| 00:16:26 | `prompt worker start`, Crew Pico (Thread-Kontinuität) |
| 00:16:36 | `turn_request … timeout: Some(3600s)` |
| 00:16:40 | `[ctox responses-request]` #1 (tools_count=1) |
| 00:16:46 | `[ctox responses-request]` #2 (tools_count=42) |
| 00:16:48.391 | `token_count` call_input=41201 call_output=219 (Response #2 abgeschlossen) |
| 00:16:48–49 | zwei `exec_command`: `ctox scrape show-target --target-key outbound-lead-generation-policy \| head -80` und `ls -la <workspace>`, beide exit 0 in je ~0,6 s |
| danach | **nichts mehr**: kein drittes `responses-request`, kein Flow-Event, kein Kontextlog-Eintrag |
| 00:22:25, 00:33:26 | Queue-Lease wird weiter erneuert (`updated_at`), Lead zeigt „Läuft" |

In der vorangehenden Kiesow-Recherche (22:53 UTC) folgte auf dieselbe Stelle
nach 10 s die dritte Anfrage mit `previous_response_id` — der Ablauf ist sonst
normal.

## Was nicht die Ursache ist

- **Kein Netz-Warten.** Die einzige ESTAB-Verbindung des Daemons nach :443 ist
  der seit 19:00 offene Signaling-Socket (fd 260, lastsnd/lastrcv je 2 s).
  Keine offene Verbindung zum Modell-Gateway, keine zum MCP-Kanal (nur der
  Loopback-Tunnel auf 8765).
- **Keine Freigabe.** Direct Sessions laufen mit `AskForApproval::Never`.
- **Kein Thread-Deadlock.** `gdb thread apply all bt` (Datei
  `thesen-operations/2026-09-09-outbound-production-ready/evidence/ctox-bt-20260911T0033Z.txt`):
  der Prompt-Worker (Thread 18) steht in
  `PersistentSession::run_turn_inner… → Runtime::block_on → park`, die beiden
  Worker-Threads dieser Laufzeit (15, 16) sind geparkt bzw. im `epoll_wait`.
  Die drei `business-os-http`-Threads in `Mutex::lock_contended` sind der
  Leerlauf des Thread-Pools (Thread 43 hält den Empfänger in `recv`).

Der Turn-Future wartet also auf ein Ereignis, das niemand mehr auslöst. Wo
genau, lässt sich ohne Instrumentierung nicht sagen: zwischen dem Ende der
Tool-Aufrufe und `stream_responses_api` (Log-Zeile `client.rs:1408`) liegen
`drain_in_flight`, `clone_history`, `get_pending_input`, Hooks und
`current_client_setup` (`AuthManager::auth`).

Auffällig im Client: `direct_session.rs:2007`
`InProcessServerEvent::ServerRequest(_) => {}` verwirft jede Server-Anfrage
ohne Antwort. Mit `approval_policy = Never` sollte keine kommen; falls doch
(Elicitation, `DynamicToolCall`, `ChatgptAuthTokensRefresh`), wartet der Turn
bis zur Frist.

## Folge für den Nutzer

- Der Lead steht bis zu **60 min** auf „Läuft" (Frist 3600 s), obwohl nichts
  passiert; der Lease wird die ganze Zeit erneuert, der Prompt-Worker ist
  blockiert.
- Die Outbound-App bietet keinen Abbruch; `ctox.command.cancel` setzt nur den
  Befehlsstatus und erreicht den blockierten Turn nicht.
- Nach einem Neustart bleibt die Aufgabe `leased` mit veraltetem Lease, bis
  die Lease-Rückgewinnung greift.

Behelf am 11.09. 00:34 UTC: `systemctl --user restart ctox`.

## Bitte an die Harness-/Queue-Seite

1. Leerlauf-Wächter im Turn: kommt vom Harness `N` Sekunden (z. B. 300) kein
   Ereignis und läuft keine Tool-Ausführung, Turn unterbrechen und als
   `stalled` melden — nicht erst nach der Gesamtfrist.
2. `ServerRequest` im Direct-Session-Client nicht stumm verwerfen:
   protokollieren und mit Ablehnung/Fehler beantworten.
3. Einen Trace-Punkt vor `current_client_setup` und nach `drain_in_flight`
   (eprintln wie `[ctox responses-request]`), damit der nächste Fall die Stelle
   zeigt.
