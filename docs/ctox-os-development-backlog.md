# CTOX OS Entwicklungs-Backlog

Stand: 2026-07-06
Status: Arbeitsdokument (Tickets)
Grundlage: `docs/ctox-os-framework-strategy.md`

Dieses Dokument übersetzt die CTOX-OS-Strategie in abarbeitbare Tickets,
geordnet nach Clustern. Konventionen:

- Ein Ticket = ein atomarer Commit (plus Tests/Doku im selben Commit), mit
  Trailer `Backlog: <Ticket-ID>` in der Commit-Message.
- Vor Arbeit an `ctox-rxdb`-Flächen gilt `docs/ctox-rxdb.md` (Guards,
  Generatoren, dist-Rebuild + Cache-Buster, Pflicht-Testläufe).
- Erledigte Tickets wandern mit Commit-Hash in den Abschnitt "Erledigt" am
  Ende, statt gelöscht zu werden.
- Größen: S (< 1 Tag), M (1–3 Tage), L (> 3 Tage / Design nötig).

## Cluster A — Idle-Disziplin & Messbarkeit

Ziel: "Idle muss idle bleiben" ist ein rotes/grünes Signal, kein Prosa-Satz.
Der Loop-Ratchet existiert (`background_loops_use_a_sanctioned_idle_strategy`,
`a20d1436`); es fehlt die echte Messung.

- **OS-A1b (M, optional): Idle-Budget auf Daemon-Ebene.** OS-A1 (erledigt)
  misst den SQLite-External-Poll isoliert. Aufbauend: ein End-to-End-Test,
  der den vollen nativen Peer (Projektions-Loops + Poll) N Sekunden idle
  laufen lässt und Writer-Lock-Zeiten/Statements gegen ein Budget prüft —
  die Loops selbst sind durch den Ratchet (a20d1436) und die
  Backoff-Unit-Tests gedeckt, daher optional.
- **OS-A2 (M): Query-Pfad-Benchmarks.** criterion-Benches für die kritischen
  SQLite-Pfade (compiled query, candidate plan, fallback scan) im
  `ctox-rxdb`-Crate, damit Datenpfad-Änderungen eine Zahl haben.
  Achtung: Cargo-Target nach `/tmp` bzw. `runtime/build/cargo-target`.

## Cluster B — Konnektivität (Kernversprechen: Zugriff ohne VPN-Planung)

- **OS-B1b (M): TURN-Fleet-Provisioning (ctox.dev-Repo).** Die
  Architekturentscheidung ist gefallen und dokumentiert
  (`docs/ctox-turn.md`, Commit 545ddf91): externes coturn neben der
  Signaling-Ebene, KEIN Relay im Daemon; CTOX mintet ephemere Credentials
  (war bereits implementiert) und hat jetzt den Operator-Pfad
  `ctox business-os turn set/status`. Offen ist der verwaltete Teil im
  ctox.dev-Repo: coturn pro Fleet-Host deployen, Shared Secret pro Instanz
  in der Control-DB, beim Provisioning URL+Secret in die Instanz setzen.
- **OS-B2 (M): Reconnect-Testmatrix.** Tab-Reload ist nicht explizit
  getestet (nur Multi-Tab-Leadership). Smokes/Soak-Modes für: Tab-Reload,
  Netzwerkwechsel, Laptop-Sleep/Wake, Signaling-Neustart mit Passwort-
  Rotation. Watchdog-/Stall-Konstanten (30s Open, 45s Initial-Stall) als
  Vertrag pinnen.

## Cluster C — Mehrbenutzer-Semantik (Presence & Konflikte)

Presence v1 (`445cab7c`) und Feld-Merge (`a045b1d7`) sind gelandet;
customers ist Referenz-Consumer (`3fed531d`).

- **OS-C3 (M): E2E-/Soak-Mode für Presence + Merge.** browser_rust_smoke-
  Mode mit zwei Browser-Peers: konkurrierende Feld-Edits konvergieren ohne
  Verlust; Presence-Badge erscheint/verschwindet (Peer-Close, TTL).
- **OS-C4b (S, optional): Feld-Merge unterhalb Top-Level.** Rest aus OS-C4:
  für deklarierte Objektfelder rekursiv mergen statt Top-Level-atomar.
  Braucht eine Deklarationsform (welche Felder) — erst angehen, wenn ein
  realer Konfliktfall Top-Level-Granularität sprengt.

## Cluster D — Modul-Contract als Plattform-API

Für KI-gebaute Apps ist `mount(ctx)` die API des Betriebssystems — heute
reich, aber undokumentiert und ungetestet (app.js ~4100).

- **OS-D2 (M): Demand-Loading deklarativ.** Die vier Demand-Only-Collections
  sind nativ hardcoded (`DEMAND_FILE_CHUNK_COLLECTIONS`, rxdb_peer.rs
  ~9149). Pro Modul deklarierbar machen (module.json/collections.schema.json:
  demand-only + key_field), nativ registriert statt Konstantenliste; Browser-
  Seite (`isModuleDemandOnlyCollection`) aus derselben Deklaration speisen.
- **OS-D3 (M): Cross-App-Datenkonventionen.** Regeln aus Strategie-Richtung 4
  als geprüfte Konvention: wann eigene Collections, wie App B Daten von
  App A nutzt, gemeinsame Muster für Kunden-/Datei-/Aufgaben-Daten.
  Ergebnis: Doku + Guard (z. B. Namenskonventions-Check im Schema-Contract-
  Generator), App-Creator-Skill-Material.

## Cluster E — App-Lifecycle (Update-Pipeline)

Katalog- vs. Installations-Version ist sichtbar; ein echtes Update ist heute
ein Creator-Formular mit vorausgefüllten Werten.

- **OS-E1 (L): ctox.module.update-Handler.** Drei-Achsen-Modell
  (Katalog-Version, installierte Version, lokale Modifikation) mit
  Vanilla-Erkennung gegen Upstream-Hash statt MIN-seq; Update ohne
  Neuinstallation, Rollback als geprüfter Pfad. Referenz:
  App-Deployment-Review 2026-06-25.

## Cluster F — Harness↔Business-OS-Schleife schließen

- **OS-F1 (M): Creator-Status in der UI.** App-Creator-Anfragen als
  nachvollziehbare Records mit Live-Status aus `ctox_queue_tasks` direkt in
  der Creator-Oberfläche (heute: task_id nur in der Browser-Console);
  Agenten-Artefakte (geänderte Dateien, Ergebnis) über den Datenweg sichtbar.
- **OS-F2 (M): Approval-/Review-UI landen.** Der gebaute
  Approval-Flow (right-click→Reviewer in Threads) liegt neben main; Landing-
  Plan existiert (`docs/business-os-approval-review-landing-plan-2026-06-26.md`).
  Bekannte Lücke: Rollen-Enforcement — server-seitig schließen, nicht UI-only.

## Cluster G — JS/Rust-Drift → gemeinsame Rust-Bibliothek

Reihenfolge nach Drift-Risiko; kein Big-Bang-WASM-Port (Nicht-Ziel).

- **OS-G2 (L): Gemeinsame Rust-Lib + WASM-Build.** Kanonisches JSON /
  Schema-Hashes / Query-Signaturen / Checkpoint-Regeln in ein Crate mit
  wasm-bindgen-Build, Browser-Bundle konsumiert es anstelle der JS-Zwillinge.
  Beginnen mit Schema-Hash (kleinste Fläche, größter Stillstands-Schaden).

## Cluster H — Standardisierung / Snappiness (Dauerrauschen)

- **OS-H1 (S je Modul): DIY-Resizer-Migration.** Noch auf CtoxResizer:
  customers, matching, calendar, app-store, iot, knowledge, buchhaltung
  (~8 Module) → deklarative `.ctox-column-resizer[data-resizer-var]`.
- **OS-H2 (M): Loading-Skeletons auto-ableiten.** Modul-Skeletons aus dem
  echten index.html/index.css je App generieren (Regel: nie handgebaut pro
  Modul), Shell-global bleibt Fallback.

## Querschnitt / Hygiene

- **OS-X1 (M, RE-SCOPED 2026-07-06): Auth-Env-Fläche migrieren.**
  Ursprüngliche Annahme war falsch: `CTOX_BUSINESS_OS_REQUIRE_CAPABILITY_TOKEN`
  läuft BEREITS über den Runtime-Store (`runtime_env::env_or_config` liest nur
  persistierte Konfiguration, keine Prozess-Env; Tests setzen es via
  `save_runtime_env_map`). Der echte Befund: die HTTP-Session-Auth-Fläche in
  `store.rs::session_for_request` (`CTOX_BUSINESS_OS_REQUIRE_LOGIN`,
  `CTOX_BUSINESS_OS_SESSION_TOKEN`, `CTOX_BUSINESS_PASSWORD`,
  `CTOX_BUSINESS_USER`, `CTOX_BUSINESS_OS_LOGIN_URL`) ist echte Prozess-Env —
  aber eine DOKUMENTIERTE Operator-Schnittstelle (systemd-Units der
  ctox.dev-Fleet, `docs/ats-golive/tenant-config.md`). Migration in den
  Runtime-Store braucht Fleet-/Provisioning-Koordination (env als
  Legacy-Fallback während der Übergangsphase) — nicht autonom umstellen.
  Zweiter offener Teil: Strict-Default für Capability-Tokens (heute Opt-in,
  ohne Token wird auf unprivilegiert degradiert) — Produktentscheidung mit
  Migrationspfad.
- **OS-X2b (S): Wire-Daemon in CI bauen.** Lokal verifiziert (siehe
  Erledigt): beide Cross-Process-Smokes laufen gegen den release-gebauten
  `v15_wire_daemon` (Build-Pfad `runtime/build/cargo-target`, ~17 min
  Release-Build). In CI baut der run-all-Job den Daemon nicht — die zwei
  Tests skippen dort weiter. Abwägung für den Operator: +Buildzeit im
  ci.yml-Job (oder Cargo-Cache) gegen echte E2E-Abdeckung pro PR.

## Empfohlene Reihenfolge

1. **OS-B1** (TURN) und **OS-A1** (Idle-Messung) — die zwei verbliebenen
   Fliegen-oder-Fallen-Bedingungen der Strategie.
2. **OS-C1–C3** — Mehrbenutzer-Versprechen zu Ende bauen und E2E beweisen.
3. **OS-D1/D2** — die Plattform-API für KI-gebaute Apps festziehen.
4. **OS-E1**, **OS-F1/F2** — Lifecycle und Agenten-Schleife.
5. **OS-G1→G2** als stetige Begleitarbeit, **OS-H*** als Lückenfüller.

## Erledigt

- Presence v1 (ctox-presence-v1, Wire+Hub+Registry+ctx.presence): `445cab7c`
- Idle-Loop-Ratchet `background_loops_use_a_sanctioned_idle_strategy`: `a20d1436`
- Presence-Consumer customers (Badge, Referenzimplementierung): `3fed531d`
- Feld-Merge-Konfliktstrategie pro Collection (§8.2 docs/ctox-rxdb.md): `a045b1d7`
- OS-X1 verifiziert + re-scoped (Token-Flag ist bereits Store-basiert): `ce1e6e48`
- OS-C1 Wrapper-Toleranz im nativen installed-module-Schema-Parser: `21e7e1f4`
- OS-C2 (teilweise) Presence + Feld-Merge in notes und calendar: `e1af73fc`
- OS-A3 Checkpoint-Staleness (pull/pushCheckpointAgeMs) in Sync-Diagnose: `2b8de024`
- OS-C2b Presence in threads (`user_threads`); conversations bewusst
  ausgelassen: Bucket-keyed Timeline hat kein treues collection+recordId-
  Mapping: `86fd9407`
- OS-A1 Idle-Budget-Guard für den External-Write-Poll (per-DB-Wakeup-Zähler
  + Integrationstest tests/idle_budget.rs, 0 Wakeups in 3s nach Standby):
  `a59e9c77`
- OS-X2 (lokal) Wire-Daemon gebaut, Suite erstmals vollständig:
  55 pass / 0 fail / 0 skip inkl. beider Cross-Process-E2E-Smokes;
  CI-Verdrahtung als OS-X2b offen.
- OS-A4 Diagnose-CLI `ctox business-os rxdb status [--json]` (Heartbeat,
  replicationUp, Loop-Ticks, External-Poll-Wakeups; liest die
  Heartbeat-Statusdatei, prozessübergreifend): `8bc1c7c4`
- OS-G1 Fehlerklassifikations-Korpus (14 Fälle inkl. Order-Pins,
  classifyReplicationErrorKind, Rust-Alignment auf Contract-Codes):
  `654cc570`
- OS-D1 mount(ctx)-Contract v1: docs/business-os-module-context.md (41
  Felder) + assert-module-context-contract.mjs (CTX-CONTRACT-Marker in
  app.js) + CI-Wiring: `a447fcae`
- OS-E2 verifiziert bereits erledigt (Vorwelle Juni): das Start-Menü
  rendert das Lifecycle-Badge (`renderStartMenuLifecycleBadge`,
  app.js ~8475/8508) — kein offener Rest.
- OS-B1 (Repo-Teil) TURN-Architekturentscheidung + Operator-CLI
  (`docs/ctox-turn.md`, `ctox business-os turn set/status`): `545ddf91`;
  Fleet-Teil als OS-B1b offen.
- OS-C4 Base-Refresh im Push-Retry (explizite Base via baseById) +
  Merge-Zähler in der Sync-Diagnose: `ff60bbf6`; Sub-Top-Level-Merge als
  OS-C4b optional offen.
