# Abnahme der Outbound-App nach dem Queue/Harness-Upgrade (05.09.2026)

Maßstab (Eigentümer, 05.09.): Die Fixes müssen darauf einzahlen, dass die
Outbound-App anschließend korrekt funktioniert. Grüne Tests sind Voraussetzung,
nicht Abnahme. Abgenommen wird auf `thesen.ctox.dev` im Browser und im
Routing-State (`communication_routing_state`), nie in der Projektion allein.

## Was jeder Befund in der App bewirken muss

| Befund | Sichtbares Verhalten in der App nach dem Upgrade | Messung |
|---|---|---|
| 1 Lease-Sweep | Stirbt ein Worker mitten in einer Recherche, fällt der Lead binnen ≤ 20 min von „Läuft" auf „Wartet" zurück und wird erneut gestartet. Kein Lead bleibt für immer auf „Läuft". | Routing-State: `leased` → `pending` nach `lease_expires_at`; Lead-Zeile in der App wechselt. Probe: laufende Recherche, Worker-Prozess beenden, 20 min warten. |
| 2 Reparatur-Entdopplung | Bei Quellenfehlern entsteht je Ziel höchstens EINE offene Reparatur; nach drei Fehlschlägen ist Schluss (`failed` mit Grund). Die Recherchen stehen nicht mehr hinter Dutzenden Reparaturen. | `ctox queue list --status pending` nach Titel gruppiert: kein Titel mehrfach. Über 24 h beobachten. |
| 3 Kapazität | Mit `ctox queue capacity --workers 4` laufen vier Recherchen gleichzeitig: „Auswahl recherchieren (8)" → vier Leads auf „Läuft", nicht einer. Eine 19-Firmen-Kampagne dauert Stunden, nicht einen Tag. | Routing-State: `count(route_status='leased')` ≥ 4 bei ≥ 8 wartenden. App-Zähler „Laufend". |
| 4 Projektion | Stornieren (CLI oder App) nimmt die Aufgabe sofort aus „Läuft"; die sieben Altlasten vom 28.–31.08. verschwinden; CTOX-Modul zeigt nur, was es gibt. | Projektion `ctox_queue_tasks` == Routing-State je `id`; Anzahl `status=running` in Projektion == `leased` im Routing-State. |
| 5 Writeback | Jede abgeschlossene Recherche schreibt zurück (Lead → „Prüfung nötig" mit Feldern) ODER der Task endet `failed` mit Grund. Nie mehr „erfolgreich" mit 0 Feldern. | Kein Lead mit Task `completed`/`handled` und `field_status`-Zählung 0. Chat zeigt den Grund bei Fehlschlag. |

## Was nicht schlechter werden darf (Regression)

- Klick „Auswahl nachrecherchieren" → Hinweis, Chatfenster, Lead auf „Wartet" (B1, 1.0.94–1.0.99)
- Personenwechsel im Lead-Detail zeigt verschiedene Inhalte
- Dialoge als Overlay in der App, nicht im Seitenfluss (1.0.90)
- Chateingabe bleibt beim Tippen stehen (c378aaf83)
- Kampagnenliste: eine Kampagne „Chemie", 19 Leads, 19 mit Ergebnis
- Schreibtisch lädt in < 90 s; keine Neustart-Schleife der Kollektionen

## Ablauf

1. Befund 1–4 auf `origin/main` (Befund 5 darf nachziehen) → ein Upgrade `ctox upgrade --dev`.
2. Nach dem Umschalten: `ctox queue capacity --workers 4` — **Achtung: der Code-Standard ist bereits 4** (`unwrap_or(4)` in `service_queue_capacity.rs`), nicht 1 wie zunächst angenommen. Parallelität greift also sofort nach dem Umschalten; das explizite Setzen macht sie zur dokumentierten Entscheidung. Erste Beobachtung direkt danach: welche vier Aufgaben werden geleast (Recherche, Auth-Assist, Reparatur?).
3. Regressionsliste im Browser durchklicken.
4. Befund 2/3/4 sofort messen (Routing-State + App); Befund 1 per Probe; Befund 5 an der nächsten realen Recherche.
5. Ergebnis mit Zahlen in dieses Dokument, dann Push auf `main`.

## Baseline VOR dem Upgrade (05.09.2026, 06:38 UTC, Release branch-main-20260904T161717Z)

| Messung | Wert vorher |
|---|---|
| Befund 3: Routing-State | 0 `leased`, 30 `pending` — Kapazitätsbefehl existiert nicht |
| Befund 4: Projektion `running` vs Routing-State `leased` | **7 vs 0** — sieben Phantome (alle im Routing-State `cancelled` seit 28.–31.08.) |
| Befund 2: doppelte offene Titel | **24 Dubletten** (19× evi-gv-at, 3× maps-google-com, 2× handelsregister-de) — vierte Welle; von Hand auf je eine reduziert (21 storniert, 9 offen) |
| Befund 5: Leads mit Ergebnis | 19 / 19 (`research_status`), keiner mit 0 Feldern |
| Befund 1: aktive Leases | keine zum Messzeitpunkt |

Erwartung nach dem Upgrade: Befund 4 → 0 Phantome (oder Reconciler räumt sie);
Befund 2 → über 24 h keine Dubletten mehr; Befund 3 → nach `capacity --workers 4`
mehrere `leased` bei Rückstau; Befund 5 → an der nächsten Recherche; Befund 1 → per Probe.

## Ergebnis NACH dem Upgrade (05.09.2026, Release branch-main-20260905T072559Z, umgeschaltet 07:53 UTC)

Upgrade 07:26:06–07:53 UTC (27,5 min), Start automatisch nach Ende der laufenden
BOOMEX-Recherche (die mit 23 Feldern abschloss, vorher 15). Beim Start war nur eine
Scraper-Reparatur geleast, keine Recherche.

| Befund | Messung nach dem Upgrade | Stand |
|---|---|---|
| 3 Kapazität | `ctox queue capacity` → `max_workers 4, workers_per_thread 1, scope independent business_os.chat.task sessions, storage SQLite runtime store` (explizit gesetzt). Eine Minute nach dem Umschalten **2 `leased`** gleichzeitig (07:53:19 / 07:53:20, beide Worker `21b39433…`, 15-min-Ablauf), 11 `pending`. Vorher nie mehr als 1. | **belegt** (2 von max. 4; mehr braucht mehr unabhängige Aufgaben im Rückstau) |
| 4 Projektion (neu) | Aufgabe `ebe4782bea95…` (Auth-Assist xing): Projektion `queued/pending/rev 1-civfwsvgrf` → `ctox queue cancel` im frischen CLI-Prozess → Projektion **`cancelled/cancelled`, lease_owner null, rev `2-1887bf89…`** um 07:56:57; Routing-State `cancelled`. | **belegt** |
| 4 Projektion (Altlast) | 7 Phantome unverändert (Reconciler `9b3f44e09` + Merge-Fix `5a16d0061` sind NICHT in diesem Release; sie belegen keine Kapazität). | offen bis zum zweiten Upgrade |
| 2 Reparatur-Dubletten | 4× `evi-gv-at` offen, alle **ohne** `scrape_repair`-Metadatum → vom alten Release erzeugt (nach meiner Bereinigung um 06:38, vor dem Umschalten). Drei per CLI storniert. Neue Einreihungen tragen das Metadatum; Dubletten-Freiheit über 24 h zu beobachten. | nicht widerlegt, 24-h-Messung offen |
| 1 Lease-Sweep | Zwei live Leases mit `lease_expires_at` (+15 min) und `lease_worker_id`. Probe „Worker stirbt" noch nicht gefahren. | Probe offen |
| 5 Writeback | 19/19 Leads mit Ergebnis, keiner mit 0 Feldern. App 1.0.100 liefert `payload.writeback_contract` mit `mechanism`/`command_type`. End-to-end (persistierter Vertrag → `business_os.execute_writeback` → Felder) braucht eine neue Recherche aus dem Browser. | **blockiert: Browser-Sitzung nach dem Upgrade abgemeldet**, Anmeldung durch den Eigentümer nötig |
| Regression Browser | — | **blockiert** (Anmeldung) |

Nächste Schritte: (a) Eigentümer meldet sich im Browser an → Regressionsliste + Befund-5-Lauf
(DrinkStar, 7 Felder); (b) zweites Upgrade mit `5a16d0061` (Reconciler + Merge-Fix) → Phantome 7 → 0;
(c) 24-h-Beobachtung Dubletten; (d) Probe Befund 1 bei Gelegenheit.

## Nachmessung 06.09.2026, 17:00 UTC (33 h nach dem Umschalten)

| Messung | Wert | Bewertung |
|---|---|---|
| Queue | 0 `leased`, 0 `pending` | leer, kein Rückstau |
| Befund 2 Dubletten | **0 doppelte offene Titel über 33 h** (vorher vier Wellen in 24 h) | belegt über den Beobachtungszeitraum |
| Befund 3 Kapazität | `max_workers 4`, in der Umschaltminute 2 parallel geleast | belegt (Maximum 4 nur mit ≥4 unabhängigen Aufgaben messbar) |
| Befund 4 neu | Cancel projiziert sofort (05.09. 07:56:57) | belegt |
| Befund 4 Altlast | 7 Phantome unverändert | offen — Reconciler `9b3f44e09`/`5a16d0061` nicht ausgeliefert |
| Befund 5 | Leads 19/19, 249 Felder gesamt, Minimum 7, keiner mit 0 Feldern. **Der neue Writeback-Guard hat noch nie gefeuert** (0 Treffer `Business command writeback failed`) — kein Task mit dem 1.0.100-Vertrag ist bisher gelaufen; der jüngste Nachrecherche-Befehl (05.09. 08:24) trägt noch den alten Vertrag (Daemon-Recovery kopiert das ursprüngliche Payload). | End-to-end offen |
| Seit Umschaltung `failed` | 14 Routing-Übergänge auf `failed`, darunter Beiersdorf („research contract is materially unmet: 2 von 8 Personen-Kategorien … Finite review budget exhausted 5/5") und eine Recovery-Aufgabe („10-path requirement structurally unsatisfiable"). **Das ist der Review-Mechanismus, der Endlosschleifen terminal stoppt — nicht der neue Guard.** Die Writebacks landeten trotzdem: Beiersdorf 12 → 18 Felder, BOOMEX 23. | erwartetes Verhalten, aber im CTOX-Modul als Fehler sichtbar |
| Testdaten | Writeback-Versuche mit Ids `…-writeback-test`, `…-test-v6-…` für BOOMEX vom Worker selbst; ein Versuch gegen `lead_test_001` wurde **abgelehnt**, kein solcher Lead existiert (54 Datensätze, 19 aktiv) | keine Verunreinigung |

**Blockiert:** Browser-Regression und Befund-5-End-to-end — die Browser-Sitzung ist seit dem
Upgrade abgemeldet, kein Chrome verbunden; Anmeldung nur durch den Eigentümer.

**Upgrade 2 zurückgestellt:** `origin/main` liegt 93 Commits / 38.865 Zeilen vor dem
ausgelieferten Stand (Sync-Engine 51 Dateien, RxDB 34, Shell 91; darunter „production peer
crash and data-preserving recovery", „rejected cutover and verified production rollback").
Ein `ctox upgrade --dev` würde das komplett und ungeprüft auf die Kundeninstanz bringen — nur
für sieben kosmetische Phantome. Entscheidung des Eigentümers.

## Upgrade 3 — 07.09.2026, 23:04 UTC (Release branch-main-20260906T223502Z: Threads-Fix, Skill §7, Reconciler; sync.js-Buster vor dem Umschalten im Release-Verzeichnis gepatcht, byteidentisch zu `d714d887a`)

| Messung | Wert | Bewertung |
|---|---|---|
| Shell-Boot | `ready`, Nutzer angemeldet, `sync.js` lädt `command-bus.js`/`sync-contract.js` mit `?v=20260906-office-page-exit` (200) | belegt |
| Befund 4 Altlast | `projektion_running=0`, `routing_leased=0` — **7 Phantome → 0** (Projektionstabelle jetzt `ctox_queue_tasks__v3`) | belegt |
| Kapazität | `max_workers 4` | unverändert |
| Leads | 19/19 mit Ergebnis | unverändert |
| Recherchestart (Threads-Fenster maximiert, keine Kollektionen ausgesetzt) | Klick 23:39:33 → Befehl angelegt 23:39:42 → Server `accepted` 23:40:50 → Worker gestartet 23:39:49 (Lease 7 s nach Anlage) | **Latenz 15–20 min → 77 s**, davon 47 s Sellify-Vorabgleich |
| Writeback | Worker recherchierte 32 Felder (18 verifiziert), meldete Erfolg, Task 23:47:20 `failed`: „no successful outbound.lead.research_writeback receipt" — 0× `business_os.execute_writeback`, 3× `propose_action` (alle `success:false`) | **Defekt 2, siehe unten** |

### Defekt 1 (07.09., 23:04–23:36 UTC): Instanz nach dem Upgrade 32 Minuten für ALLE schreibgeschützt

Der erste Recherchestart nach dem Umschalten scheiterte still: `CTOX_MAINTENANCE_READ_ONLY`
(„CTOX wird aktualisiert – Apps bleiben schreibgeschützt"), unbehandelte Promise-Ablehnung im
Lead-Schreibpfad, Status fiel von „Wird gestartet" auf „Prüfung nötig" zurück. Ursache im
Wartungsprotokoll (`src/core/install/mod.rs`, `src/apps/business-os/app.js`):

1. Nach dem Dienst-Neustart steht der Zustand in `ctox-maintenance.sqlite3` auf
   `waiting_collections` und wird **nur** durch den Business-Command
   `ctox.maintenance.client_ready` eines Browsers beendet, der alle Pflichtkollektionen seiner
   offenen Module `complete` sieht (`tryAcknowledgeMaintenanceReadiness`).
2. Der einzige offene Tab war verdeckt; `maintenancePollDelay()` liefert bei
   `visibilityState === 'hidden'` 0 → kein Poll, keine Bestätigung — obwohl alle 17
   Pflichtkollektionen längst `complete` waren.
3. Serverseitig läuft die Wartezeit unbegrenzt: bei lebendem Peer-Heartbeat wird der Lease
   nur verlängert („a live peer heartbeat owns the wait"). `lease_expires_at` 23:09:11 war
   um 23:36 noch `active`.

Sofortmaßnahme: den Retry-Pfad der Shell (`[data-maintenance-retry]`) ausgelöst — derselbe
Code, den ein sichtbarer Tab bei jedem Poll ausführt; Bestätigung 23:36:15 UTC über den
WebRTC-Befehlskanal, Zustand `completed`. Fix auf main: (a) Server gibt `waiting_collections`
nach 10 min ohne Browser-Bestätigung selbst frei (`MAINTENANCE_CLIENT_ACK_GRACE_MS`,
neues Feld `waiting_collections_since_ms`, Tests), (b) verdeckter Tab pollt alle 30 s
weiter, solange ein Upgrade läuft (`CTOX_MAINTENANCE_HIDDEN_POLL_MS`).

### Defekt 2 (07.09., 23:47 UTC): Der Recherche-Worker hat das Writeback-Werkzeug nie gesehen

`business_os.execute_writeback` steht im MCP-Katalog der Instanz (75 Werkzeuge), aber die
Worker-Sitzung bekommt nur die Allowlist `BUSINESS_OS_MCP_SESSION_TOOLS` aus
`src/core/execution/agent/direct_session.rs` (41 Werkzeuge) — und die enthielt das Werkzeug
nicht. Befund 5 (`e9a346e38`) hat Katalog und Guard ergänzt, die Sitzungs-Allowlist nicht.
Folge: **jede** Recherche seit dem Upgrade vom 05.09. endet zwingend im Guard
(DrinkStar 20:54, Cereda 23:47), unabhängig von Skill und Auftragstext. Die Umstellung
von Skill §7 (`3cf70ee05`) und App 1.0.102 war richtig, aber nicht hinreichend.
Fix: Werkzeug in die Allowlist, Test `business_os_mcp_thread_config_is_local_scoped_and_tool_bounded`
verlangt es ausdrücklich. Auslieferung nur per Binary → Upgrade 4.

## Upgrade 4 — 07.09.2026, 05:29 UTC (Release branch-main-20260907T045949Z = main `c69119697`: Wartungsfreigabe, `execute_writeback` in der Worker-Allowlist, dazu 16 fremde main-Commits: Office-Fixes, Tombstone-/Replikations-Fixes der Sync-Engine)

| Messung | Wert | Bewertung |
|---|---|---|
| Umschaltung | 05:29:01 Stop → 05:29:04 Start → 05:29:14 „replication up for 205 collections" | wie Upgrade 3 |
| Wartungsfreigabe | `waiting_collections` → `completed` 05:29:37 durch Browser-Ack (33 s nach Neustart; Tab sichtbar) | Grace-Pfad nicht gebraucht, aber vorhanden |
| Worker-Werkzeuge | `tools_count=42` (vorher 41) in den Responses-Requests → `business_os.execute_writeback` ist in der Sitzung | belegt |
| Shell | `app.js`/`sync.js` mit `?v=20260906-office-page-exit`, Boot `ready`, angemeldet | belegt |
| App | 1.0.103 live (Übersicht trennt „recherchiert" von „belegt (inkl. Import)") | belegt |
| Datenbestand | Alle Leads/Kampagnen per App-Dialog gelöscht (0 lebend, 54 Löschmarken), „Chemie Test 2026" mit 19 Firmen neu importiert (Import 05:40–05:43, IDs der Löschmarken wiederbelebt, keine Dubletten) | belegt |
| Kampagnenstart | „Alle recherchieren (19)" erzwingt Variante Nachrecherche → 18 Abbrüche „nur Neue Recherche möglich"; danach „Auswahl neu recherchieren (19)" 05:51:11 → Anlage sequenziell ~1 Lead/2,5 min | **App-Defekt 3**, Fix 1.0.104 vorbereitet (Variante je Lead automatisch) |

### Defekt 4 (07.09., 05:50 UTC): Harness bricht Recherche nach zwei Minuten ab — „task execution steps must be a completed prefix"

Der Worker für Carbosulf (Thread 01a07a69…) starb nach drei LLM-Aufrufen mit
`durable task progress failed: task execution steps must be a completed prefix, one active step, then pending steps`
(`src/core/context/lcm/mod.rs`, `validate_task_execution_steps`, seit `ab718a53d` 30.08.). Das Modell
meldet Planfortschritt legitim außer der Reihe (späterer Schritt fertig, zwei aktiv, keiner aktiv);
die Prüfung machte daraus einen Abbruch des ganzen Auftrags. Fix: Statusfolge wird normalisiert
(zweiter aktiver Schritt → pending, ohne aktiven Schritt → erster offener wird aktiv), fünf
Unit-Tests; Auslieferung per Upgrade 5.

### Defekt 5 (07.09., seit Upgrade 3): Thread `cockpit-projections` verbraucht dauerhaft einen Kern

`top -H`: 78–100 % CPU auf dem Pump aus `harness_cockpit_projections.rs:153` bei leerer Queue,
Dienst 194 % CPU, RSS 2,7 GB; keine Schreiblast (WAL-Größen konstant, 5 `business_records`-Writes in
5 min). Erstmals mit Upgrade 3 ausgeliefert (Pump seit `46e3b6d3a`, 05.09. 17:43). An den
Crew-Cockpit-Codex-Thread 01a07107 gemeldet (Queue-Nachricht 01a07a6a…). Nicht belegt, aber
verdächtig: die seit Upgrade 3 gehäuften `ctox_webrtc_incoming_transfer_stalled` /
`Timed out waiting for WebRTC response masterChangesSince` im Browser (jede Reload-Sitzung nach
1–3 min, alle Kollektionen der App), die Löschung (13 von 19 Leads beim ersten Versuch) und Import
(erst nach Wipe des lokalen Browser-Speichers) nur mit Verzögerung durchbrachten und im Sammellauf
einzelne Aufträge lokal als `failed` enden lassen (Beiersdorf 05:53: Befehl nie beim Server).

### Owner-Browser: 36 statt 19 Leads

Der Browser des Eigentümers zeigte 17 seit 04.09. gelöschte Leads („Offen") neben den 19 lebenden und
„synchronisiert 0/5": die Löschmarken kamen dort nie an. Der Neuaufbau (Löschen + Neuimport) und die
Tombstone-Fixes aus main (`20199fe28`, `7a54790c0`, `fcce19763`, mit Upgrade 4) adressieren das;
Nachweis steht aus, bis der Eigentümer neu lädt.

### Sammellauf 07.09., 05:51–06:45 UTC (App 1.0.103, Release branch-main-20260907T045949Z)

| Messung | Wert |
|---|---|
| Aufträge angelegt | 13 von 19 in 54 min — die App startet sequenziell, jeder Start hängt am Sellify-Vorabgleich (Bedarfsabfragen stallen, 120-s-Deckel) |
| Worker parallel | 4 (Kapazität greift) |
| **End-to-End-Nachweis** | Dr. Kurt Richter: `execute_writeback` 06:11:51 + 06:17:22 `completed`, Lead `needs_review`, **13 Felder**; Cereda 06:38:39 `completed`, **4 Felder** |
| Writeback-Ablehnungen | 8, davon 6 „invalid payload" (Destilla 4×: verschachtelter `firma_land`-Schlüssel, `deny_unknown_fields`), 1 „non-verified field must not carry a populated value" — der Worker korrigiert nur, wenn die Ablehnung den Grund nennt → Fix `f3a2aa7fd` |
| Terminal gescheitert durch Plan-Prüfungen | Carbosulf 05:50 („completed prefix"), AKEMI 06:16 („plan is incomplete 1/3"), CHEMOFAST 06:41 („exactly one in-progress step") → Fixes `92a84679b` (Normalisierung) + `80561cbc9` (Wiederholung statt Terminal) |
| Technische Wiederholungen | 4× `stream disconnected before completion` (llm.ctox.dev), 1× `thread/start failed` (Aeroxon) — Backoff 60 s, `technical:worker-runtime-api-failure` |
| Lokale Fehlstarts | Beiersdorf, BEWI RAW, BOOMEX u. a. zeitweise `failed` ohne Server-Auftrag — Submit im Browser lief in den Sync-Stall |

Upgrade 5 gestartet 06:46 UTC (main `f3a2aa7fd`: Plan-Normalisierung, Wiederholungsklasse, Writeback-Fehlerdetail).
Danach: App 1.0.104 ausliefern, offene Leads in kleinen Gruppen nachstarten, Feldtabelle messen.

## 07.09., 07:16–08:35 UTC: Upgrade 5 (Harness-Fixes), Sync-Ursache gefunden, Upgrade 6 gestartet

| Ereignis | Befund |
|---|---|
| Upgrade 5, 1. Anlauf 06:46 | Installer scheiterte: `src/core/coding_agents/pi-sidecar/dist` war als Dev-Symlink eingecheckt (`7f1209e37`, Crew-Cockpit-Merge) → esbuild „dist: file exists". Entfernt mit `6d933a8f1`. |
| Upgrade 5, 2. Anlauf 07:03 → 07:16 | Release `branch-main-20260907T064620Z`. Wartungsfreigabe **ohne Browser-Ack** um 07:26:20 („Freigabe ohne Browser-Bestätigung") — der Grace-Pfad aus `c69119697` hat gegriffen, weil alle Kollektionen im Browser auf `pending`/`stalled-waiting-for-peer` standen. |
| Nach dem Neustart | Worker-Starts scheitern reihenweise mit `thread/start failed: … ctox-business-os: timed out handshaking with MCP server` (Aeroxon, Cereda ×2, BOOMEX ×2, Dr. Kurt Richter); Threads: tokio-runtime 83 %, cockpit-projections 72 %; `[ctox direct-session] lagged: dropped 472 events`. Kapazität 4 → 2 gesenkt (08:00). |
| Wiederholungsklasse | `80561cbc9` wirkt (failure_class `technical` für „plan is incomplete"), aber drei Handshake-Timeouts erschöpfen das Budget → terminal `failed`. |
| Writebacks nach Neustart | BOOMEX 32× `no_match`, Cereda 31× `no_match` + 1 `action_required`, jeweils **0 Quellen, 0 Belege** — vom Daemon angenommen; Cereda verlor damit seine 4 zuvor verifizierten Felder. Wiederholungsversuch ohne Kontext nach dem Neustart. |
| Skill-Einhaltung (Richter, Calvatis) | Zwei-Quellen-Regel bei verifizierten Feldern eingehalten (16/17, 9/9), Belege mit URL+Zitat, Konflikte gemeldet, Login-Quellen als `action_required`. Verstöße: Calvatis ohne Personendatensatz trotz Geschäftsführer im Impressum; Richter 23 statt 32 Felder, `person_titel` mit einer Quelle; keine `person_email_validation`. |
| Kampagnen-Knopf | 1.0.104 live: „Alle recherchieren" wählt die Variante je Lead automatisch. ANGUS und Beiersdorf (Sellify-Kampagnen-Import) und Carbosulf brauchen Nachrecherche. |
| Sync-Ursache (Codex-Thread 01a06de4, 07:28–08:22) | Im Rework `f972b3ed6` (06.09.) konnte eine kleine priorisierte Antwort in der nativen WebRTC-Sendewarteschlange den laufenden Großtransfer einer anderen Kollektion abbrechen („WebRTC send queue result dropped"). Fix `1a58665ca` / PR #65 (`67e1671fb`): Regressionstest deterministisch, Browser-Fixture 21/21 Kollektionen, Reloads 16–28 s, Revision 23 + 17 Löschmarken kommen an. Doku `docs/dev/ctox-sync-interleaved-receipt-20260907.md`. |
| Meine Fixes für Upgrade 6 (`304536098`) | Cockpit-Pump: höchstens ein Refresh je Root alle 3 s (Wakes werden gebündelt). Writeback-Guard: belegfreie Writebacks (nichts verifiziert, keine Quelle, kein Versuch) werden mit erklärender Meldung abgewiesen; 2 Tests. |

Upgrade 6 gestartet 08:35 UTC (main `304536098`). Abnahme danach: Reload → 21/21 Kollektionen in < 60 s; MCP-Handshake der Worker; Cockpit-Thread < 20 %; Nachstart aller unvollständigen Leads; Feldtabelle.

## 07.09., 08:56–10:05 UTC: Upgrade 6 gemessen, Lauf über alle 19, Anbieter-Login-Pfad geprüft, Upgrade 7 gestartet

| Messung | Wert |
|---|---|
| Upgrade 6 (Release branch-main-20260907T082637Z) | Wartungsfreigabe 09:00:00 per Browser-Ack (33 s); seit 08:56 **0** MCP-Handshake-Fehler bei Kapazität 2; Browser-Erstreplikation nach Reload 12/16 nach 2 min, Richter-Lead zeigt 13/32 (Codex-Sync-Fix PR #65 wirkt). Cockpit-Thread weiter 73–86 % → PR #66 (Ereignis-Cursor) mit Upgrade 7. |
| App 1.0.105 | Vorabgleich-Antwort „wartet noch auf die Rückmeldung" gilt wie Timeout; vorher brachen Berg, Chemotechnik, Dreidoppel und ein 8er-Batch still ab. „Alle recherchieren (15)" legte danach alle offenen Aufträge an (19 in Queue um 09:46). |
| Ergebnisse 09:56 | Richter 13, Additiv 13, Destilla 11, Calvatis 9 Felder; Cereda `needs_review` mit 0 (Neulauf eingereiht). Writeback-Ablehnungen jetzt mit Grund („expected a sequence": `sources` als String/Objekt → Fix `c66b2b8ad` akzeptiert alle drei Formen). |
| Skill-/Prompt-Einhaltung (Schritte 0–4 aus `research_instructions`) | 1 Register ja (Northdata/Handelsregister; GF, Prokura, Status verifiziert), 2 Website ja (Impressum, Domain, E-Mail, Telefon), **3 Kennzahlen bei keinem Lead** (D&B Hoovers/Leadfeeder = Login-Quellen → `no_match`/`action_required`), 4 Personen: Additiv 2, Richter 1, Calvatis 0 trotz GF im Impressum (Verstoß), `person_email_validation` nirgends. Zwei-Quellen-Regel bis auf `person_titel`, `firma_prokura`, `person_geschlecht` eingehalten. |
| **Anbieter anlegen/freischalten (Login-Quellen)** | Kette App → Secret `OUTBOUND_<QUELLE>_LOGIN` (`ctox.secret.put`) → `credential_secret_name` in `payload.source_policy` (belegt für 6 Quellen) → Worker `auth-assist-request` → Owner-Browser-Stream. **Bruch:** jeder CLI-Aufruf des Workers, der den RxDB-Store öffnet, scheitert seit 02.09. mit RxDB DB6 (`workjet_computers`-Schemadrift; Dienst überspringt optional, CLI-Pfad strikt) → nie eine Anmeldeanforderung. Fix `8de040615` (`register_collections_tolerant`), Upgrade 7. Nebenbefund: alle 14 Adapter `failed` („adapter reconciliation reply must be one strict JSON object"). |
| Cockpit-Ansicht | „Waiting in queue" bei terminal `failed` (Cereda, 4 Versuche, Handshake-Timeouts) — Live-Flow zeigt den Plan der Vorversuche (8/8, 79 %) neben einem geschönten Routingstatus. An Crew-Cockpit-Thread gemeldet. |

Upgrade 7 gestartet 10:05 UTC (main `8de040615`).

## 07.09., 10:34–11:36 UTC: Upgrade 7 live, Auth-Assist belegt, Writeback-Formfehler, Upgrade 8

| Messung | Wert |
|---|---|
| Upgrade 7 (Release branch-main-20260907T100418Z) | Wartung per Browser-Ack 10:37:54; Cockpit-Thread trotz PR #66 weiter 75–99 % (an Crew-Thread gemeldet); Kapazität 2 → 3, weiterhin 0 Handshake-Fehler. |
| **Auth-Assist (Anbieter-Login) funktioniert** | `ctox business-os web-stack auth-assist-request --source-id dnbhoovers.com --task-id <geleaste Task>` → `ok:true`, Befehl `accepted`, Sitzung `browser_session_web_stack_auth_dnbhoovers_com_michael_welsch…`, Ziel `https://app.dnbhoovers.com/login`, Owner michael.welsch, erwartetes Secret `DNB_HOOVERS_BROWSER_LOGIN`, `trusted_local_intake:true`. Vor Upgrade 7 brach derselbe Aufruf mit RxDB DB6 ab. Offene Anmeldeanforderung liegt beim Eigentümer. |
| Fortsetzung | Daemon legte heute 32 „Nachrecherche"-Folgeaufträge an; Destilla stieg damit von 11 auf 15 Felder. „Fortsetzen: …" nach Anmeldebestätigung laut Skill §8; freier Chat-Turn trägt den Writeback-Vertrag nicht. |
| Stand 11:33 | Destilla 15, Richter 13, Additiv 13, Calvatis 9, BNT 1 Felder; 3 Worker, 17 wartend. **Bremse:** Formfehler im Writeback (ANGUS 25 Ablehnungen in 9 min: `result` fehlt, Listen als ""/Objekt, `item`-Hülle, Feldschlüssel oben). |
| Fix `e582dd595` (Upgrade 8, 11:36) | Empfänger leitet `result.fields` aus verifizierten `field_status`-Einträgen ab, akzeptiert ""/Objekt/JSON-String als Liste (`sources`, `attempts`, `evidence`, `person_records`, `fields`); Werkzeugbeschreibung nennt das exakte Format und die typischen Fehler. 23 Tests. |

## 07.09., 12:05–13:40 UTC: Upgrade 8 und 9 (Writeback-Toleranz), Stand des Laufs

| Messung | Wert |
|---|---|
| Upgrade 8 (Release branch-main-20260907T112714Z, 12:05) | Wartung per Grace-Pfad 12:07:20 (kein Client). Danach verbleibende Ablehnungen: „missing field `source_id`" 16, „invalid type: number, expected a string" 8 (in 1 h). Handshake-Fehler nur als Burst bei drei gleichzeitigen Worker-Starts (12:32–12:35) → Kapazität 3 → 2. |
| Upgrade 9 (Release branch-main-20260907T124810Z, 13:17; `5b4ab72ff`: `source_id` aus Host/URL, Zahlen als Text) | Wartung per Grace-Pfad 13:27:55. **Seit 13:17: 6 Writebacks angenommen, 0 abgewiesen** (11:36–13:17: 20 angenommen, 50 abgewiesen). |
| Kampagnenstart 13:28 („Alle recherchieren (10)") | alle 10 Aufträge in 10 min angelegt (App 1.0.105 + stabiler Sync). |
| Stand 13:40 | 10 von 19 Leads mit Ergebnis, 96 Felder: Destilla 15, Aeroxon 15, Richter 13, Additiv 13, Chemotechnik 12, Dreidoppel 11, Calvatis 9, ANGUS 6, BNT 1, Carbosulf 1. Aeroxon vollständig regelkonform (Register, Impressum, 2 Personen mit LinkedIn, Mitarbeiter verifiziert). Kennzahlen (WZ/Umsatz/Mitarbeiter aus D&B Hoovers) weiterhin offen: Anmeldeanforderung liegt beim Eigentümer. |
| Eigene Falle | Monitore 11:36–12:43 blind: `timeout` existiert auf macOS nicht (Memory `macos-kein-timeout`). |

## 07.09., 13:40–14:10 UTC: Zusammenführungsfehler bei Folge-Writebacks, Upgrade 10

| Messung | Wert |
|---|---|
| Stand 13:58 | 11 Leads mit Ergebnis (BEWI RAW 19, Destilla 15, Aeroxon 15, Richter 13, Additiv 13, Chemotechnik 12, Dreidoppel 11, Calvatis 9, BNT/Carbosulf/ANGUS 1). Seit Upgrade 9: 25 angenommene, 1 abgewiesene Writebacks (Feldschlüssel auf oberster Ebene). |
| **Defekt: Folge-Writeback ersetzt statt zusammenzuführen** | ANGUS 13:58:44: ein Lückenschluss-Writeback mit **einem** Feld (`firma_name`) ersetzte `field_status` (verifiziert 6 → 1) und `payload.researched_field_keys` (→ `["firma_name"]`); `data` (6 Felder) und `contacts` (4 Personen) blieben. Ursache `handle_research_writeback`: `lead["field_status"] = request.field_status` und der Outcome-Patch setzt die Schlüssellisten nur aus dem aktuellen Ergebnis. |
| Fix `2bfddd2fe` (Upgrade 10, 14:08) | `merge_field_status` (Feld für Feld, neuer Eintrag gewinnt) und `union_research_keys` (researched/verified/unverified vereinigt, verifiziert verlässt unverified). Test `follow_up_writeback_keeps_previous_field_status_and_unions_keys`. Bereits geschrumpfte Leads (ANGUS) füllen sich mit dem nächsten vollständigen Writeback wieder. |

## 07.09., 14:45–17:05 UTC: Upgrade 10 gemessen, Füller-Status und Ablehnungs-Semantik, Upgrade 11 und 12

| Messung | Wert |
|---|---|
| Upgrade 10 (Release branch-main-20260907T140503Z, `2bfddd2fe`) | Wartung per Grace-Pfad 14:45:17 freigegeben. Union der Schlüssel wirkt: Carbosulf 1 → 17 → 22, Aeroxon 15 → 16. |
| **Defekt: Füller überschreiben `field_status`** | Der Sanitizer füllt jedes angeforderte, nicht gelieferte Feld mit `unsupported` („Vom Rueckschreiben nicht geliefert"). `merge_field_status` kopierte diese Füller über frühere `verified`-Einträge: Aeroxon 15:24 nach drei Ein-Feld-Writebacks **kein** Feld mehr verifiziert, `data.*` und `researched_field_keys` (15) unverändert. Sichtbar nur im Feldstatus, nicht in der Zählung. |
| **Defekt: offene Felder als `rejections`** | Die Antwort listete die 31 nicht gelieferten Felder unter `rejections`; der Worker las das als Fehlschlag und schickte 15:24–15:28 sieben Mal dasselbe Feld `firma_name` (einmal `reason: "debug"`). |
| Fix `4dc9a912d` (Upgrade 11, Release branch-main-20260907T153953Z, Wechsel 16:06, Wartung per Grace 16:20:00) | Füller landen nur auf Feldern ohne aussagekräftigen Status (`is_filler_field_status`). Antwort trennt `accepted_fields`, `open_fields` (aus dem zusammengeführten Lead-Status), `rejections` (nur Defekte) und `summary`; Tool-Beschreibung sagt dasselbe. Tests `filler_statuses_of_a_follow_up_do_not_downgrade_earlier_verified_fields`, `writeback_response_separates_open_fields_from_rejections`. |
| **Defekt: Formvarianten kippen ganze Läufe** | Seit 14:45 zwölf Ablehnungen: 6× `missing field kind` (Attempts ohne `kind`), 3× `expected struct FieldStatus` (`""` oder Liste als Feldeintrag), `unknown field firma_fruehere_namen` / `person_records` (Einträge neben statt in `field_status`/`result`). |
| Fix `a022b1e96` (Upgrade 12, Start 17:02) | `FieldAttempt` vollständig optional; `lenient_field_status` verwirft Nicht-Objekte, akzeptiert Listen mit `field`-Schlüssel und JSON-Strings; `hoist_top_level_field_entries` hebt Feldeinträge in `field_status` und `fields`/`person_records`/`evidence` in `result`. Tests (28 grün im Modul). |
| Stand 16:56 | 15 von 19 mit Ergebnis: Carbosulf 22, DrinkStar 20, BEWI RAW 19, Aeroxon 16, Destilla 15, Berg 14, Richter 13, Dreidoppel 13, Chemotechnik 13, Additiv 13, **Beiersdorf 12 (completed)**, Calvatis 9, ANGUS 1, BNT 1, BOOMEX 1. Ohne: CHEMOFAST, BÜFA, AKEMI (Tasks terminal: Plan-unvollständig, kein Writeback-Beleg, MCP-Handshake), Cereda. |
| Cereda | „Auswahl neu recherchieren" legt keinen Task an: App lehnt Sellify-bekannte Firma ab (contact_id 17625), nur Nachrecherche erlaubt. Nachrecherche 16:42 → `chat.task` 16:47:36 angenommen, App meldete trotzdem „nicht rechtzeitig an die Queue übergeben" (Daemon-Ack kam nach dem App-Timeout). |
| Neustarts 16:56/16:58 | „Alle recherchieren (5)" und „Auswahl nachrecherchieren (3)" (ANGUS, BNT, BOOMEX) ausgelöst; App: „4 gestartet, 1 nicht gestartet". Serverseitige Bestätigung offen (siehe Folgeabschnitt). |
| Tenant nicht erreichbar 16:23–16:40 | Alle SSH-Sitzungen der Control-Plane hingen 17 min und wurden 16:40:46 gleichzeitig freigegeben; parallel im Browser `masterWrite`/`masterChangesSince`-Timeouts. HTTP und Signaling antworteten. Ursache nicht gemessen (Last 2,8 danach). |
| Worker-Enden 16:10–17:00 | 2× `database is locked` (Release-Wechsel), 3× Stream-Disconnect, 1× Plan unvollständig (4/6), 1× 1800-s-Timeout; alle mit Wiederholung. Keine MCP-Handshake-Fehler bei Kapazität 3. |
| Lokale Falle | `cargo test` 45 min in Zustand `UN` auf `~/.cargo/.package-cache` bei IO-Sättigung (fremdes `rg`, Swap 5 GB); Neustart des Prozesses half. |

## 07.09., 17:05–18:15 UTC: Upgrade 12 gemessen, Chat-Projektion als Ursache des Browser-Stalls, Upgrade 13

| Messung | Wert |
|---|---|
| Upgrade 12 (Release branch-main-20260907T170121Z, `a022b1e96`), Wartung per Grace 17:41:18 | Seit 17:41 14 Writebacks, 13 angenommen. Beiersdorf 32 Felder in einem Aufruf (12 verifiziert, 20 no_match, 14 Belege verworfen), Cereda 31 Felder (8 verifiziert); Ein-Feld-Nachlieferungen ergänzen sauber. Eine Ablehnung: leerer Aufruf ohne `field_status`. |
| Stand 17:52 | 16/19 mit Ergebnis: Carbosulf 22 (completed), DrinkStar 20, BEWI RAW 19, Aeroxon 16, Destilla 15, Berg 14, Richter/Dreidoppel/Chemotechnik/Additiv 13, Beiersdorf 12, Calvatis 9, BOOMEX 6, Cereda 4, ANGUS/BNT 1; CHEMOFAST/BÜFA/AKEMI 0 (Tasks terminal). |
| **Browser-Pfad tot** | Nach Neuladen und IndexedDB-Wipe bleiben alle 16 Kollektionen in `initialReplicationState=pending/stalled` (restartCount 22), `lastLifecycleEvent`: „WebRTC native peer did not open for business_workspace_branding within 30000ms" (`peer_connect_timeout`), 766 Journal-Schreibungen (6 MB) anstehend. Nativer Peer meldet replicationUp/dataChannelOpen=true. Startklicks 16:56/16:58 kamen nie an. |
| **Ursache: Chat-Projektion** | `[ctox cockpit phases]` 17:36–17:57: total 153–348 s je Runde, davon `project_chat_us` 190–345 s, alle anderen Phasen < 3 s. Dienst 5,3 GB RSS / 260 % CPU, Thread `cockpit-projections` 60 %, dreimal `database is locked`. `harness_cockpit_chat::project` lief je Runde über alle 173 Chat-Aufträge (170 terminal) inkl. `business_command_projection`, `load_queue_task`, Flow-Event- und LCM-Progress-Abfrage, bevor der Fingerprint griff. Die Einzelabfragen sind schnell (attempt_id 50 ms, Indizes vorhanden); die Summe je Auftrag liegt bei ~2 s. |
| Fix `d729826b8` (Upgrade 13, Start 18:12) | `cockpit_chat_delivery.terminal` (ALTER TABLE, idempotent): Zustellung für Tasks mit terminalem Routing-Status markiert den Auftrag, spätere Runden überspringen ihn vor jeder Abfrage. Test `terminal_chats_are_delivered_once_and_skipped_afterwards`; Cockpit-Gruppe 35 grün. An Crew-Thread 01a07107 gemeldet (Nachrichten 01a07d06…, 01a07d07…, 01a07d0e…). Mit dabei `c4f34a277` (Feldwerte direkt in `result` → `result.fields`). |
| Auth-Assist | Worker stellte ~18:00 „web stack auth assist request · handelsregister.de" (wartend, Owner-Browser). |
| Lokale Falle | Test-Filter: Modulpfad ist `business_os::harness_cockpit::chat::tests`, nicht `harness_cockpit_chat`; Waiter auf `phase=completed` muss den Release-Namen prüfen, sonst matcht der Vorgänger. |

## 07.09., 18:47–19:05 UTC: Upgrade 13 gemessen, Starts belegt, Retry-Lücke bei unvollständigem Plan, Upgrade 14

| Messung | Wert |
|---|---|
| Upgrade 13 (Release branch-main-20260907T180804Z, `d729826b8`), Wartung per Grace 18:47:46 | Cockpit-Runden 1,5–2,6 s, `project_chat_us` 0,7–1,4 s (vorher 190–345 s); 169 Chats settled; Thread `cockpit-projections` 0 % CPU; Last 1,2. Browser-Erstreplikation nach Neuladen in 30 s (`business_commands` complete/connected). |
| Starts 18:49–18:53 | „Alle recherchieren (2)" → CHEMOFAST (Lease 18:51:11), BÜFA (18:50:06); AKEMI per „Auswahl neu recherchieren (1)" (pending 18:55); ANGUS/BNT/BOOMEX per „Auswahl nachrecherchieren (3)" (alle drei pending bis 19:01). Alle serverseitig in `communication_routing_state` belegt. |
| Stand 19:01 | 17/19 mit Ergebnis: BÜFA neu 16, Cereda 5 (completed); offen CHEMOFAST (0, terminal) und AKEMI (0, pending). |
| **Defekt: unvollständiger Plan bleibt terminal** | CHEMOFAST 18:51:13–18:52:58: Worker endet „task execution plan is incomplete (2/12 steps completed)", Task sofort `failed` (attempt 1, failure_attempt_count 0, kein failure_class). Ursache: `runtime_error_is_transient_api_failure` (service.rs) hat eine eigene Substring-Liste; 80561cbc9 hatte nur den Cooldown-Klassifizierer erweitert. Betroffen heute: CHEMOFAST, AKEMI (0/9, 5/6), BÜFA (8/9), BNT (7/8, 4/7), Dreidoppel (5/6), DrinkStar (4/5). |
| Fix `e3ab5c7b3` (Upgrade 14, Start 19:03) | Beide Marker passieren das Gate → Technik-Hold mit 5-Versuche-Budget, Plan wird fortgesetzt. Test `incomplete_durable_plan_keeps_queue_work_retryable`. |
| Offen | CHEMOFAST nach Upgrade 14 neu starten; Auth-Assist handelsregister.de wartet auf Owner. |

## 07.09., 19:43–20:30 UTC: Upgrade 14 live, 18/19, Queue durch Adapter-Abgleich verstopft

| Messung | Wert |
|---|---|
| Upgrade 14 (Release branch-main-20260907T190322Z, `e3ab5c7b3`), Wartung per Grace 19:43:14 | Technik-Holds (`failure_class=technical`) statt terminaler Fehler bei „plan is incomplete" — an Cereda (7 Versuche), AKEMI, Adapter-Abgleich sichtbar. |
| Stand 19:44 | 18/19 mit Ergebnis: ANGUS 1 → 17, BNT 1 → 13, BOOMEX 6 → 14 (completed), BÜFA 21, AKEMI 9. Nur CHEMOFAST 0. |
| CHEMOFAST-Neustart | 19:45 „Auswahl neu recherchieren (1)": App meldete „CTOX konnte den Task nicht an die Queue übergeben" (business-chat.js: `submitChatMessage` ohne Queue-Annahme), zweiter Klick 19:49; `chat.task` 19:54:20 angenommen, Task pending. Browser-Journal stieg auf 1220 anstehende Schreibungen; Peer-Schleifen nach Dienstneustart (ticket_state 113 s, knowledge_tables 106 s, business_records max 122 s, desktop_file_index max 92 s, business_commands Ø 1 s/Tick) blockieren den Datenkanal minutenlang → an Crew-Thread gemeldet (01a07d6d…). |
| **Queue verstopft** | 19:56–20:28 lief nur „Recherche-Adapter abgleichen" (urgent, von jedem Recherche-Start neu eingereiht, je 6–7 min, mit Retry nach Plan-Fehler) und erzeugte zehn „repair scrape target …" (high). Die Nachrecherchen (normal) warteten 30 min ohne Hold. Kapazität 3 half nicht: nur zwei Leases gleichzeitig beobachtet. Gemeldet an Crew-Thread (01a07d81…; Queue-Thread 01a07015 ist archiviert). |
| Gegenmaßnahme 20:29 | `ctox queue reprioritize`: CHEMOFAST/ANGUS/AKEMI → urgent, elf Adapter-/Scraper-Tasks → low. |
| Offen | Adapterabgleich dedupen und von der chat.task-Parallelität entkoppeln; `cmd_cred_*` „database is locked" (10× in 13 min); Auth-Assist handelsregister.de wartet auf Owner. |

## 07.09., 20:36 UTC: 19 von 19 mit Ergebnis

| Messung | Wert |
|---|---|
| Nach Reprioritisierung 20:29 | CHEMOFAST geleast 20:30:35, um 20:36 bereits 11 Felder; ANGUS/AKEMI-Nachrecherchen beendet (17 / 9 Felder, unverändert). |
| Feldtabelle 20:36 (recherchierte Felder) | Carbosulf 22 (completed), BÜFA 21, DrinkStar 20, BEWI RAW 19, ANGUS 17, Aeroxon 16, Destilla 15, Berg 14, BOOMEX 14 (completed), BNT 13, Richter 13, Dreidoppel 13, Chemotechnik 13, Additiv 13, Beiersdorf 12, CHEMOFAST 11, Calvatis 9, AKEMI 9, Cereda 5 (completed). Summe 269 Felder, Ø 14,2 von 32. |
| Bewertung | Alle 19 haben Ergebnisse; keine Firma vollständig. Umsatz/Mitarbeiter/WZ hängen an Login-Quellen (D&B Hoovers, handelsregister.de: Auth-Assist-Anfragen offen). |
| Stand 21:12 | 276 Felder über 19 Leads (Cereda 5 → 12). Laufend: Nachrecherche CHEMOFAST (Lease 21:06), Cereda-Wiederholung 21:22 (Technik-Hold), Adapter-Abgleich, Auth-Assist handelsregister.de (geleast, wartet auf Owner-Login). |

## 07.09., 21:40–22:00 UTC: Nacharbeit an den offenen Punkten

| Punkt | Stand |
|---|---|
| Adapter-Abgleich verdrängt Recherchen | App 1.0.106 live (21:46): `outbound.research.adapters.reconcile` mit Priorität `low` statt `urgent`. Daemon `d890ca4df` (nächstes Upgrade): Scrape-Reparaturen und Adapter-Erzeugung `low` statt `high`. Dedupe im Daemon bleibt offen (Crew-Thread 01a07d81…). |
| 30-Minuten-Session-Limit | `CTOX_CHAT_TURN_TIMEOUT_SECS=1800` in ctox-runtime.sqlite3 (Operator-Einstellung, TUI-Auswahl bis 3600). Nicht geändert: der Shell-Drawer „Runtime" lud den Laufzeitstatus nicht („Runtime nicht geladen", Provider/Modell leer); „Übernehmen" hätte die leere Provider-/Modellwahl mitgeschrieben. Kein CLI-Pfad für einen einzelnen Wert. Owner kann es in der TUI setzen; Timeout-Fortsetzungen existieren (`maybe_enqueue_timeout_continuation`). |
| `database is locked` | busy_timeout ist bereits 30 s (`persistence::sqlite_busy_timeout_duration`); die Sperren kommen von langen Schreibtransaktionen der Peer-Schleifen (ticket_state/knowledge/business_records je ~110 s nach Neustart). Ursache liegt im Peer, gemeldet (01a07d6d…). |
| Live-Flow zeigt „Queued" für terminal Fehlgeschlagene | Auf main bevorzugt `routingProblemStatus` (modules/ctox/index.js) den Routing-Status `failed` vor der Phase; der auf thesen aktive Shell-Slot ist älter. Wird mit dem nächsten Shell-Release aus main verifiziert, nicht heute. |
| Tests | `cargo test -p ctox --bin ctox -- capabilities::scrape store_outbound_commands`: 99 grün, 3 rot — dieselben 3 sind auf unverändertem origin/main rot (adapter_reconciliation_projects_typed_result_without_secrets, …_rejects_invalid_batch_before_any_write, embed_texts_via_local_socket_uses_internal_embedding_contract). Vorbestand, nicht meine Änderung. |
| Endstand 22:06 UTC | 287 Felder über 19 Leads (CHEMOFAST 11 → 16, Cereda 18). Nachrecherche CHEMOFAST läuft weiter; Queue danach: Adapter-Abgleich (low), Auth-Assist handelsregister.de (wartet auf Owner), Scraper-Reparaturen (low). |

## 08.09., 04:30–05:10 UTC: Abarbeitung der offenen Themen

| Thema | Stand |
|---|---|
| Recherche | 291 Felder über 19 Leads (CHEMOFAST 20). Queue leer bis auf Scraper-Reparaturen (low). |
| Logins (Owner) | Alte Auth-Assist-Anfragen waren tot: handelsregister.de starb 01:22 im Pfad `outcome-witness-recovery` („requires an owned, expiring queue lease before leased"), dnbhoovers.com am 07.09. 13:18 („plan is incomplete (0/3)"). Neu angelegt per CLI `ctox business-os web-stack auth-assist-request`: dnbhoovers.com (Secret `DNB_HOOVERS_BROWSER_LOGIN`, Ziel app.dnbhoovers.com/login), leadfeeder.com (`LEADFEEDER_BROWSER_LOGIN`), xing.com (`XING_BROWSER_LOGIN`). Quellen mit `auth_status=required`: dnbhoovers.com, xing.com, linkedin.com, leadfeeder.com; handelsregister.de braucht kein Login. Lease-Fehler an Crew-Thread gemeldet (01a07f61…). |
| Adapter-Abgleich | Daemon `b93170494`: zweiter Abgleich bei offenem Lauf wird sofort gegen den laufenden Task abgeschlossen (`maybe_complete_duplicate_adapter_reconciliation`, Test grün). App 1.0.106 reiht mit `low` ein. Kapazitäts-Blockade durch Nicht-Chat-Tasks bleibt beim Crew-Thread. |
| Session-Limit | Neuer CLI-Pfad `ctox runtime timeout <secs>` (nur `CTOX_CHAT_TURN_TIMEOUT_SECS`, ohne Modellwechsel). Wird nach Upgrade 15 auf 3600 gesetzt. |
| Peer-Erstschleifen | Auftrag mit vier Abnahmekriterien an Crew-Thread (01a07f4f…): Browser < 30 s synchron nach Neustart, kein Loop-Tick > 2 s, keine `database is locked` bei Kommandoannahme, keine Env-Toggles. Eigene Analyse: Schleifen laufen per `spawn_blocking`, halten aber lange Schreibtransaktionen auf der RxDB-Datei; der Datenkanal (masterWrite) wartet. |
| Recherche-Qualität | Live-Richtlinie (Version 4, Owner-Text) enthält bereits Zwei-Quellen-Regel, E-Mail-Prüfung über experte.de und Personenreihenfolge. Die Verstöße sind Nicht-Befolgung, kein Regelmangel. Serverseitige Durchsetzung (verified nur mit ≥ 2 distinkten source_ids) wäre möglich, würde aber Werte streichen — Owner-Entscheidung, nicht umgesetzt. |
| Dependabot | npm: fast-uri 3.1.6, @xmldom/xmldom 0.8.15 (Lockfile, 0 Findings). Rust: rand 0.8.8/0.9.5, tar 0.4.46, actix-http 3.13.5, serde_with 3.23.0 (zieht jiff/darling 0.24 nach). `lru` mehrdeutig (0.12.5/0.16.4/0.18.1), verwundbar wäre nur < 0.16.3 — nicht mehr im Baum. Build-Verifikation läuft. |
| Rote Tests auf main | `outbound_adapter_reconciliation_projects_typed_result_without_secrets` / `…_rejects_invalid_batch_before_any_write`: `load_rxdb_collection_record` findet den Adapter nach `apply_outbound_adapter_reconciliation_reply` nicht (Option → „adapter writeback"/„first source"). Vorbestand vor allen heutigen Änderungen; Ursache nicht weiter verfolgt. `embed_texts_via_local_socket_uses_internal_embedding_contract`: Antwortform des Embedding-Sockets („invalid type: map, expected f32"). |
| 10:05 | Codex PR #70 (`ebe57e602`, „Crew: bounded projection and fair queue admission") auf main: serieller Worker belegt einen Slot statt alle Chat-Slots zu blockieren; identische Adapter-Abgleiche werden koalesziert (`adapter_reconciliation_superseded`); Chat-Projektion gebunden; rxdb_peer-Änderungen. Mein pauschales Dedupe (dd39699dd) zurückgenommen (`9d3bbe83a`), da es geänderte Konfigurationen verschluckt hätte. Upgrade 15 (Start 10:07, Log upgrade-20260908-dedupe-timeout-b) enthält #70, d890ca4df und `ctox runtime timeout`. Erster Startversuch lief ohne die neuen Commits los (Rebase scheiterte an einem lokalen rustfmt-Rest in lcm/tests.rs) und wurde samt verwaistem cargo-Build gestoppt; Lehre: `pkill -f "[c]argo build"` extra, das Startskript prüft nur `pgrep -x cargo`. Lokal: Systemplatte zweimal voll (ENOSPC im cargo check), 26 GB eigenes Target, Inkrementaldaten gelöscht. |

## 08.09., 10:07–13:20 UTC: Upgrade 15 scheitert am Symlink-Wechsel, manueller Abschluss, Messung mit PR #70

| Messung | Wert |
|---|---|
| Upgrade 15 (Release branch-main-20260908T100752Z; main 0b08b71cb mit PR #70, d890ca4df, `ctox runtime timeout`, Cargo-Updates) | Installer fertig 10:36 (28 min). 10:36:45 stoppte/startete systemd den Dienst (Wrapper `~/.local/bin/ctox` exportiert bereits CTOX_ROOT=neues Release; ctox-watchdog lief 10:36:46), Dienst bootete mit root=…100752Z. Step 13 (10:37:01) `stop_background_guarded(current=alt)` schlug fehl: „refusing to switch CTOX release while service stop is not safe: service pid 3931559 still alive" (Stop-Timeout 15 s, Dienst in Erstschleifen). Ergebnis: neuer Code lief 2,5 h, `current`/Manifest/update_state zeigten aufs alte Release; Wartungslease `failed` (terminal, kein Read-only). Mein Waiter wartete auf `completed` und blieb 2 h blind. |
| Queue in der Zeit | 15 Worker-Starts seit 10:36 (seriell, Scraper-Reparaturen); UI-Warnung „Harness verarbeitet keine Queue" bezog sich auf eine wartende Adapter-Abgleich-Aufgabe hinter den Reparaturen. |
| Manueller Abschluss 13:14:57 | `systemctl --user stop ctox` (sauber in < 1 s), Symlink via `.new` + `mv -T` auf …100752Z, `start`; Boot root=…100752Z, Socket im neuen Release. Manifest (`install_manifest.json`) und `update_state.json` bleiben stale bis zum nächsten `ctox upgrade`. Befund an Crew-Thread (01a08128…): Watchdog während Upgrade pausieren / Neustart erst nach Wechsel / Stop-Timeout an Bring-up koppeln. |
| **Peer-Erstschleifen mit PR #70** | Nach Neustart 13:14:57: runtime_settings max 7,9 s, desktop_file_index 5,7 s, business_records 1,3 s, business_users 1,2 s, channel_state 0,9 s, business_commands 0,7 s, knowledge_tables 0,5 s, ticket_state 0,2 s (vorher 73–122 s). |
| Session-Limit | `ctox runtime timeout 3600` → `CTOX_CHAT_TURN_TIMEOUT_SECS=3600` (vorher 1800), 13:17. |
| Browser nach Neuladen | 13:17:40 geladen; 13:19:13 noch 0/33 Kollektionen komplett (kein Wartungsbanner, nicht read-only) — Abnahmekriterium „< 30 s" noch nicht belegt, Messung folgt. |
| 13:20–14:40 Beweislauf-Versuch aus dem Claude-Browser | Kein Recherche-Start gelungen. Tab im Hintergrund (`document.hidden=true`): kleine Kommandos (sellify.lookup 433 B, inline < 14336 B) kommen an; gechunkte Transfers scheitern — Leads-Pull (Dokumente 50–92 KB, batch 20, 10-KB-Chunks, ackWindow 4) bleibt `stalled` (masterChangesSince-Timeout), Research-Start (chat.task 16–19 KB) scheitert mit masterWrite-Timeout → „CTOX konnte den Task nicht an die Queue übergeben". Dienst idle, Peer dataChannelOpen. Versuche: IndexedDB-Wipes, Sichtbarkeits-Override, rAF-Polyfill, setTimeout→MessageChannel — keiner trägt. Befund an Crew-Thread (01a081…): Ack-Scheduling nicht an Seiten-Timer koppeln, Batchgröße nach Dokumentgröße. Nachrecherche für Calvatis von der App abgelehnt (nicht in Sellify → nur Neue Recherche). Beweislauf braucht einen sichtbaren Tab. |
