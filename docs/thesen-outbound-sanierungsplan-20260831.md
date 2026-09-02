# THESEN Outbound Lead Generation — Sanierungsboard (Stand 02.09.2026, 03:40 UTC)

**Headline / kritischer Pfad:** Die Recherche läuft (7/7 completed, Beiersdorf 01.09. 16:47
UTC: 8 Felder, 45 Belege, 6 Quellen inkl. Sellify), aber vier strukturelle Ursachen halten
das Produkt unbenutzbar: (1) der Harness-Worker darf die CTOX-CLI nicht ausführen, deshalb
heilt kein Adapter; (2) Auth-Sitzungen aus Recherche-Läufen gehören `ctox_harness` bzw.
`scrape_executor` statt dem Nutzer und fressen das Sitzungsbudget; (3) öffentliche
Personen-Treffer kollabieren nativ auf EINEN Kontakt, Prioritäten/Sellify-Personen werden
ignoriert; (4) drei Agenten deployten gegeneinander (5 Neustarts 31.08. 18:37–01:42).
Kritischer Pfad: Rust-Batch R1+R2+R3 (parallel bei Sol, EIN Build) → App-Fixes A1 →
gemeinsame Live-Abnahme. Besitzer des Tenants ab jetzt: diese Sitzung (Fable), sonst niemand.

Board-Regeln: nur selbst verifizierte Fakten; Worker-Berichte sind Behauptungen bis zur
Nachmessung. Messskripte (rein lesend): `~/Documents/ctox-dev/output/claude-status*-20260901.ts`.

## Ist-Zustand thesen.ctox.dev (gemessen 01.09. 20:00–21:30 UTC)

| Ebene | Stand | Beleg |
|---|---|---|
| Rust-Release | `branch-main-20260831T174442Z` (main-Basis 5d413b66e, 31.08. 17:44 UTC), Dienst aktiv seit 01.09. 01:42 | update_state.json phase=completed |
| main seit Release | 32 Commits, Rust in 14 Dateien (Service, App-Authoring), Shell-Tags bis 0.1.35 | `git log --since=2026-08-31T17:44Z origin/main` |
| Shell-Slot | 0.1.25 (aktiviert 01.09. 01:42); main ist 10 Releases weiter | slots/, journal |
| App-Modul | outbound-lead-generation 1.0.64 (index.js 31.08. 21:18 UTC), Quelle jetzt in Git: `~/Documents/thesen-apps` (Commit a04fe1a) | local-modules/…/module.json |
| Recherche-Läufe | 7 `web_stack.person_research` completed (36 h), 0 failed; Leads: 19 needs_review, 10 new, 2 completed, 1 failed | business_commands, RxDB |
| Journal 6 h | 0 Token-Ablehnungen, 0 Response-404, **8× database is locked** (sellify_lookup-Intake) | journalctl |
| Adapter-Reparatur | 31.08.: 23 failed / 12 handled; 01.09.: 2 failed / 1 handled. Fehlerbild in allen failed: `ctox` scheitert im Worker-Sandbox (NoNewPrivs=1, CapEff=0, ~/.codex read-only): „failed to start CTOX CLI turn ledger / attempt to write a readonly database" | `ctox queue show queue:system::bea55fb691b616726df40262` |
| Auth-Sitzungen | 01.09. 16:45–16:47 rocketreach/xing/dnbhoovers unter `ctox_harness` → Budget 3/3 → leadfeeder für Harness geblockt; 53 Chromium-Prozesse in 5 Profilen | journal `gate=user_budget` |
| Adapter-Status in der App | 14 Adapter zeigen `last_error` 404 — 36 h alt, KEINE neuen 404; Status wird nie zurückgesetzt, weil Reparatur nicht laufen kann | adapters__v2 |
| Prompts | Policy-Records: 3320/3380 Zeichen, Nachrecherche leer = Fallback; letzte 5 Läufe trugen den vollen 3320-Zeichen-Prompt | research_policies__v0, payload |
| Feldvertrag | App sendet 32 Felder; native `FieldKey` hat 33 Varianten, deckt alle 32 (schon im Release) | sources/mod.rs |
| Personen-Vertrag | App sendet `person_priorities`(8), `known_person_records`(2), `research_instructions`; nativ: 0 Treffer für diese Felder in src/core+src/tools. Beiersdorf: XING lieferte 8 Profile, Lead hat 1 Kontakt („Jana Laufer", Funktion „Leipzig" vom Profil Heilmann) | person_research_command.rs, Lead-Evidenz |
| Worker-Sandbox | `managed_worker_sandbox_policy`: WorkspaceWrite, State-Root read-only; HARNESS.md:518 bestätigt `runtime/`, `.codex` read-only. Relais-Muster existiert für `ctox knowledge` (ServiceIpcRequest::KnowledgeData, Ledger-Skip main.rs:365) | direct_session.rs:169, service.rs:3556 |

## KORREKTUREN (an früheren Behauptungen, sichtbar gehalten)

- **Codex 01.09. 17:28:** „Nachrecherche-Prompt ist `Adapter-Abgleich läuft.`" — FALSCH. Das ist das
  Status-Label der Adapter-Reconciliation (index.js:1403), kein Prompt.
- **Codex-Plan:** „Native Seite kennt nur 22 Felder" — FALSCH (33 Varianten, alle 32 gedeckt).
- **Codex-E2E:** „Viele Quellen melden 404" — Anzeige-Altlast (36 h), keine aktuellen Fehler.
- **Fable 31.08. 19:2x:** „Abnahme grün" nach dem Build galt nur Journal/Unlock/Queue, nicht den
  UI-Flows (Import, Tabellenansicht, Kampagnensuche) — die Codex am 01.09. rot gemessen hat.
- **Fable 31.08.:** 14 Deploys mit Neustarts während Owner-Tests; jeder Neustart tötete Login-,
  Browser- und Sync-Sitzungen. Ab jetzt: Deploys nur gebündelt und angekündigt.

## Done (01.09.)

- **Diagnose beider Threads + Live-Messung** (12 Messskripte, s. o.).
- **App-Quelle versioniert:** `~/Documents/thesen-apps/outbound-lead-generation` = Tenant-Stand
  1.0.64 (index.js sha256 2b522835…), Commit a04fe1a. Bisher lag sie nur als Tarball vor.
- **Launchpad Rust-Batch:** `~/.local/state/workjet-launchpads/thesen-rust-batch` (Teilbaum von
  origin/main 565a8ff6c: business_os, service, capabilities, web-stack, knowledge, channels,
  main.rs; Snapshot < 64 MiB). Briefs unter `briefs/`.
- **Workjet-Health (checkedAt 2026-09-01T21:34:47Z):** alle 12 Worker `ready`.


### 02.09. 00:xx — R1–R3 geliefert und integriert (Compile/Tests laufen)

- Sol-Läufe R1/R2/R3 alle `completed` (R3 erst nach Brief-Fassung 2; erster R3-Lauf korrekt
  `blocked`: Struct-Literale von `PersonResearchRequest` außerhalb der Whitelist).
- Patches per `workjet result import` geholt (refs/workjet/<run>), auf origin/main 3731d4ba0 im
  Vollklon `ctox-rustfix` angewendet — alle drei sauber (Branch `thesen-rust-batch`, Commits
  516f4ca17 R1, 11dc21213 R2, e73703f76 R3).
- **Web-Stack-Pin-Falle (Memory bestätigt):** der Daemon baut `ctox-web-stack` aus dem
  workjet-Git-Pin (48f0d7cb), `src/tools/web-stack` ist workspace-excluded. Die Web-Stack-Anteile
  von R2 (scrape_bridge Owner-Parameter) und R3 (person_research, person_ranking, lib, surface)
  wurden deshalb per Blob-3-Way auf workjet main portiert: Klon
  `~/.local/state/workjet-launchpads/workjet-webstack-thesen`, Branch `thesen-person-contract`,
  Commit d54bbdcce (gepusht). Zusätzlich fehlte dort `aggregate_candidate_eligible` (nur im
  ctox-Spiegel vorhanden) — nachgetragen. Web-Stack: `cargo check --features full` grün,
  24/24 person_research+person_ranking-Tests grün, Lib-Suite 469 grün / 1 rot
  (`real_registry_has_fifteen_adapters_with_valid_shared_config` — **vorbestehend auf workjet
  main a091f858c**, Adapterzahl ≠ 15 seit dem LinkedIn-Target; nicht durch diese Änderung).
- ctox-Pin auf d54bbdcce gesetzt (Cargo.toml + Cargo.lock im Vollklon), `cargo check -p ctox`
  ohne Fehler; die 13 gezielten Tests (5×R1, 4×R2, 4×R3) laufen gerade (Test-Build des Bins).
- Sitzungs-Login im In-App-Browser für die A1-Reproduktion steht noch aus (Owner).

### 02.09. 01:1x — Compile + Tests grün, Review liefert 2 kritische / 2 hohe Befunde

- Vollklon `ctox-rustfix` Branch `thesen-rust-batch` @ 69814adee: `cargo check -p ctox` fehlerfrei
  (ein Slice-Pattern-Fehler in R1 selbst behoben), **13/13 gezielte Tests grün** (5×R1, 4×R2, 4×R3);
  breitere Regression (command_plane, web_stack, person_research, browser_runtime, scrape) läuft.
- **Kimi-Review (run …bb436fd9, 02.09. 00:0x, nur lesen) — Befunde von mir am Code nachgeprüft:**
  - K1 kritisch: Relais + `register-script`/`execute` = worker-autorisierter Code läuft unsandboxed im
    Daemon. Vorbestehendes Architekturmodell (Repair-Worker schreibt Adapter-Skripte, Daemon führt
    sie aus); durch R1 erstmals wieder erreichbar. → OWNER-Karte (Runner-Sandbox), nicht in R4.
  - K2 kritisch: `--script-file/--input-file/--module-file/--runtime-root/--db` ungeprüft → Daemon-
    seitiges Arbitrary-Read/-Write. Verifiziert (cli.rs:141–173, execute.rs:84, main.rs:4408). → R4.
  - H1 hoch: IPC-Accept-Loop synchron (service.rs:1556), Timeout aus Client-argv ungeklemmt
    (service.rs:4406–4412). Verifiziert. → R4.
  - H2 hoch: Owner-Identität client-behauptet über die ganze Kette (Intake stempelt nur Lücken;
    `--owner-user-id`/`CTOX_OWNER_USER_ID` ungeprüft). Teils vorbestehend, durch R2 geschlossen
    durchgereicht. → R4.
  - Mittel: M1 unbounded stdin/IPC-Zeile, M2 Ledger-Skip auch interaktiv, M3 `known_person_records`
    client-fabrizierbar, M4 Kontakt-IDs kollidieren, M5 Namensvettern-Merge, M6 TTL-Lücken
    (Screenshot ≠ Aktivität, kein Timer, Dokument-Timestamp klemmbar). → M1/M4/M5/M6.3 in R4;
    M2/M3/M6.1–2 Backlog.
  - Niedrig: N1 argv in Logs (→ R4), N2 Capture nur `write_json`, N3 Socket ohne Peer-Auth,
    N4 Rollen-Positivliste zu eng („Account Manager", „Verkauf") → Backlog/Web-Stack.
- Breitere Regression (127 grün / 5 rot) — alle 5 **vorbestehend auf origin/main 3731d4ba0**, nicht durch
  R1–R3: `business_command_inventory_matches_exact_control_types` (6 `kundenpipeline.*`-Literale ohne
  EXACT_CONTROL_TYPES-Eintrag, auf main vorhanden), `outbound_lead_generation_exposes_native_scoped_person_research`
  (Test erwartet Fehler bei `payload.fields`, aber der neuere Normalizer in mcp_channel.rs:4802 entpackt
  `{item:[…]}` → erster Fehler ist `include_private`; mcp_channel.rs von uns unberührt),
  `appsec_worker_dispatches_business_os_web_stack_auth_assist_contract` (accepted vs pending_sync, seit
  31.08. bekannt), `embed_texts_via_local_socket…` (Embedding-Socket-Umgebung), `continuity_prompt_contains_
  document_and_diff_rules` (LCM-Prompttext). → Karte Backlog „main-Testbaseline reparieren (fremd)".

### 02.09. 02:3x — R4 geliefert und integriert; N4 im Web-Stack erledigt

- R4 (Sol, run …ea811059) `completed`, importiert und auf `thesen-rust-batch` als b52e382aa angewendet:
  K2 Pfad-Sanitisierung (Dateien nur unterhalb `runtime/scraping/targets/<target_key>`,
  `--runtime-root/--db` abgelehnt), H1 IPC-Verbindungen in Threads `ctox-ipc-<n>` + Timeout-Klemme 600 s
  + 16/32-MiB-Limits, H2 verifizierte Identität IMMER für ReplicatedPeer (`claimed_actor` bleibt als Audit),
  Owner-Flag/Env nur bei Übereinstimmung mit dem verifizierten Kommando-Besitzer, M4/M5 Kontakt-IDs +
  Zwei-Signal-Merge, M6.3 Zeitstempel-Klemme, N1 Logs. **Nebenwirkung:** service.rs wurde komplett
  rustfmt-normalisiert (1402 Diff-Zeilen; Datei war vorher nicht fmt-sauber) — bewusst akzeptiert,
  erhöht Merge-Risiko für Parallelarbeit an service.rs.
- N4 (Rollen-Positivliste) direkt im workjet-Klon behoben: workjet `thesen-person-contract` @ cdf64f856
  (gepusht), ctox-Pin darauf (9bcc1a7f4), Spiegel `src/tools/web-stack/src/person_ranking.rs` gleichgezogen.
- origin/main ist seit unserer Basis 3731d4ba0 um 10 Commits weiter (a10058bfb; Importer, Workjet-Sessions,
  Harness-Scope) und berührt service.rs (+6) und service/business_os.rs (+17) → Rebase vor dem Push,
  Probelauf in separatem Worktree.
- Compile + 22 gezielte Tests (R1–R4) laufen im Vollklon; danach Push auf origin/main und B1.

### 02.09. 03:0x — R4 getestet, zwei rote Tests selbst behoben, Branch auf main rebased

- R4-Testlauf: 17/19 grün; rot waren `web_stack_auth_owner_resolution_prefers_flag_then_env_then_task` und
  `web_stack_auth_assist_reuses_active_task_across_request_ids` (beide `left: None`). Ursache: R4 hatte die
  verifizierte Besitzer-Suche auf `client_context.owner_user_id` verengt — server-eingereihte Auth-Assist-
  Kommandos tragen den Besitzer in `payload.owner_user_id`/`actor.id`; nicht vertrauenswürdige Aufrufer
  liefen damit fail-closed ins Leere. Fix a0a1abf49: Reihenfolge `client_context.owner_user_id` → `actor.id`
  → `user_id` → `payload.owner_user_id`. Danach 9/10 web_stack_auth-Tests grün (rot nur der vorbestehende
  appsec-Test).
- Scratch-Volume /Volumes/tmp war zu 100 % voll (Linker scheiterte): 31 GB veraltete Build-Caches entfernt
  (ctox-codex-owner-identity-target, ctox-leak-check-target, ws-target-main). Noch dort: workjet 89 GB,
  ctox-rustfix-target 31 GB (alt), DeepSeek 27 GB, state-backups 25 GB.
- Branch `thesen-rust-batch` auf origin/main a10058bfb rebased (7 Commits, konfliktfrei): 0765cec0e R1,
  ba0e75c0b R2, a82f1eb61 R3, 643863f5d Pin d54bbdcce, 3a9100ed7 Pin cdf64f856, 814c2bc2b R4, a0a1abf49 Fix.
  Finaler Gate-Lauf (check + 22 Tests) läuft → dann Push auf origin/main.

### 02.09. 03:1x — Gate grün, auf main gepusht, Build B1 gestartet

- Finaler Gate-Lauf auf dem rebasten Branch: `cargo check -p ctox` fehlerfrei, **19/19 Tests grün**
  (R1 5, R2 4 + R4 3, R3 4 + R4 3). Push: origin/main = **a0a1abf49** (7 Commits, fast-forward auf a10058bfb).
- workjet: Branch `thesen-person-contract` @ cdf64f856 gepusht; main dort nicht fast-forwardbar → PR
  geöffnet (Link in Evidenzkarte). Der ctox-Pin zeigt auf den Commit, unabhängig vom Merge.
- **B1 gestartet** (`claude-upgrade-dev-thesen.ts a0a1abf49`, abgesetzt via setsid/nohup, Log
  `~/upgrade-dev-<ts>.log` auf thesen, pid in `~/upgrade-dev.pid`). Erwartung 30–40 min; der Neustart
  beendet alle Browser-/Login-Sitzungen auf thesen. Wächter: `claude-wait-upgrade-thesen.ts`.

### 02.09. 03:0x — B1 abgeschlossen und abgenommen (Technik-Ebene)

- Release **branch-main-20260902T023408Z** aktiv (Build 26 min 42 s, previous branch-main-20260831T174442Z,
  State-Backup update-20260902T023425Z), Dienst aktiv seit 03:01:04 UTC, HTTP 200, `ctox doctor` zeigt das
  Release; Binary: 15 SandboxedCli-Marker, 2 liveScreenshot-Marker (Web-Stack-Live-Op intakt, Pin-Falle umgangen).
- **Relais-Abnahme:** `--input-file /etc/hosts` → „path is outside allowed workspace prefix
  …/scraping/targets/handelsregister-de" (abgelehnt); `--runtime-root /tmp/x` → „flag --runtime-root is not
  allowed over the relay" (abgelehnt); relayter `scrape execute handelsregister-de` → `ok:true, succeeded,
  2 records` (Daemon führt aus, Ausgabe kommt zurück). Socket `ctox_service.sock` vorhanden.
- Modul 1.0.64 und Shell-Slot 0.1.25 unverändert (Upgrade tauscht nur das Binary). 0 Chromium-Prozesse.
- Journal seit Neustart: einziger Fehler ist RxDB DB6 beim Registrieren der OPTIONALEN Collection
  `workjet_computers` (neu auf main durch 445c4d669 „workjet sessions", nicht aus dieser Kampagne; die
  Collection wird übersprungen). WebRTC-Replikation: „multiplexed WebRTC replication up for 201
  collections", Peer-Status ok, p2p-first.
- Kontrollierter Heal-Lauf bundesanzeiger-de → `portal_drift`, Reparaturaufgabe
  `queue:system::36c31fa9e0ca33519745a3f4` angelegt (03:02:47 UTC); Wächter prüft, ob sie erstmals ohne
  CLI-Sperre endet.
- Hinweis Repo-Zustand: das lokale main-Checkout ist gegenüber origin/main abgewichen (58/31 Commits, fremde
  Arbeit im Baum). Board-Commits liegen lokal UND werden über den Klon ctox-rustfix nach origin/main gepusht.
- Offen für die fachliche Abnahme: (a) Reparaturaufgabe endet `handled` (kontrollierter Heal-Lauf
  angestoßen), (b) Auth-Sitzung aus einem Recherche-Lauf gehört dem Nutzer (braucht Owner-Recherche),
  (c) mehrere Personen im Lead (braucht Owner-Nachrecherche).

## Working

| Karte | Worker / Log | Fertig heißt |
|---|---|---|
| R1 CLI-Relais: `scrape register-script/register-source-module/execute` + `continuity-update` laufen aus dem Worker über den Daemon-IPC (Muster `knowledge`), Ledger-Skip, Fallback ohne Daemon | Sol (Start nach Board-Commit) | Patch importiert, `cargo check` + Tests grün im Vollklon, Repair-Task auf thesen endet `handled` mit registrierter Revision |
| R2 Auth-Identität + Sitzungs-TTL: Chat-Steuerkommandos tragen den Nutzer als actor; Recherche→scrape execute→Reauth-Handoff reichen `--owner-user-id` durch; Owner-Fallback über Thread/Chat statt `source_module`; Idle-TTL für `web_stack_auth`-Sitzungen | Sol | Auth-Sitzung aus einem Recherche-Lauf gehört `michael.welsch@…`; nach TTL frei; kein `_ctox_harness`/`_scrape_executor` mehr |
| R3 Personen-Vertrag: `person_priorities`, `known_person_records`, `research_instructions` nativ; öffentliche person_*-Treffer je Profil-URL zu `person_records` gruppiert; Sellify-Personen führend; Rollen-Validierung; Priorisierung | Sol | Beiersdorf-Fixture: 8 person_records, „Leipzig" keine Funktion, Hahn/Gund erhalten |

| Review R1–R3 (Kimi · Cyber, nur lesen): Privilegiengrenze des IPC-Relais (Pfadargumente), Impersonation über client-gelieferte `actor.id`, Owner-Env an Kindprozess, TTL-Race, R3-Datenhoheit | Kimi Cyber 1, run local-2026-09-01T230523Z-bb436fd9-0db0-49fe-b6a7-f6f4be2469cd; Review-Stand: Launchpad-Branch `integrated` 5366f5a = ctox-rustfix 69814adee | Befunde nach Schweregrad; kritische/hohe vor B1 fixen |

| R4 Härtung (Sol): K2 Pfad-Sanitisierung + Strip `--runtime-root/--db`, H1 IPC pro Verbindung im Thread + Timeout-Klemme 600 s + M1 Größenlimits, H2 verifizierte Identität IMMER für ReplicatedPeer (`claimed_actor` für Audit) + Owner-Flag/Env nur bei Übereinstimmung/TrustedLocal, M4/M5 Kontakt-IDs + Zwei-Signal-Merge, M6.3 Timestamp-Klemme, N1 Logs | Sol, Brief `briefs/R4-hardening.md`, Basis Launchpad-Branch `integrated` 5366f5a | 7 Tests grün im Vollklon, dann Push main + B1 |

Sol-Kontingent: 0/3 belegt. B1 abgeschlossen — Owner-Tests wieder möglich.

## To-Do (Trigger-Kette)

1. **Integration R1–R3** — startet, wenn ein Sol-Lauf terminal ist: `workjet result import`, Patch in
   Vollklon `~/.local/state/workjet-launchpads/ctox-rustfix` (auf origin/main resetten), `cargo check`,
   gezielte Tests, `rustfmt <datei>` nur für eigene Dateien, Push auf origin/main.
2. **Build B1 auf thesen** — startet, wenn alle drei Patches auf origin/main sind: `setsid nohup ctox
   upgrade --dev`, ~30–40 min, EIN Lauf. Vorher Owner-Tests ankündigen (Neustart killt Sitzungen).
3. **A1 App-Fixes (JS, Repo thesen-apps)** — startet nach eigener Browser-Reproduktion der fünf
   Codex-Befunde (IDB closing beim Import, Tabellenansicht schließt App, Kampagnensuche hängt,
   Importtyp-Wechsel behält Vorschau, „Erledigt" ohne Prüfung) + `client_context.actor` im
   Recherche-Command. Worker: Sol oder Completion Worker (Grok) mit Render-Smoke als Gate.
4. **Tenant-Konfiguration** — nach B1: `CTOX_BROWSER_MAX_SESSIONS_PER_USER` im Runtime-Store
   prüfen/anheben, verwaiste Harness-Sitzungen beenden, Adapter-`last_error` durch echten
   Reparaturlauf zurücksetzen lassen.
5. **Live-Abnahme gemeinsam** — nach B1 + A1: Leadfeeder-Login mit Nutzer-Sitzung, Nachrecherche
   mit ≥2 Personen, Import E2E, Tabellenansicht, Kampagnensuche. Erst dann „funktioniert".
6. **Shell-Drift** — thesen 0.1.25 → aktuelles main-Shell-Release, aber NUR aus main und NUR im
   Bündel mit B1 (ein Neustart).

## Backlog + Owner

- OWNER: **K1 — Trust-Grenze des Adapter-Runners.** Repair-Worker (LLM) schreiben Adapter-Skripte, der
  Daemon führt sie ohne Sandbox aus (execute.rs `execute_registered_script`, absichtlich mit
  `ctox secret get`-Zugriff). R1 macht diesen Pfad wieder nutzbar. Optionen: (a) Runner unter dem
  Worker-Sandbox-Profil starten, (b) Interpreter-Allowlist + Workspace-Zwang (R4 liefert nur den
  Workspace-Zwang), (c) Modell akzeptieren und dokumentieren. Bis zur Entscheidung: B1 nur mit R4.
- OWNER: Codex-Thread `01a052a8…` beenden oder ausdrücklich an einen anderen Tenant binden;
  parallele Deploys auf thesen sind die Ursache Nr. 4.
- OWNER: Leadfeeder/RocketReach/LinkedIn-Zugangsdaten — ohne gültige Logins bleiben die
  Personenquellen leer, egal wie gut der Unlock-Pfad wird.
- `database is locked` (8×/6 h, sellify_lookup-Intake) trotz busy_timeout — Messreihe nach B1,
  dann Intake-Serialisierung als eigener Punkt.
- `ctox continuity-update` scheitert aus JEDEM Worker (gleiche Sandbox-Ursache) — Teil von R1,
  Wirkung auf andere Kampagnen nach B1 messen.
- Import als dauerhafter Business-Command statt Browser-Schreibschleife (Codex-Plan §1) —
  nach A1-Reproduktion entscheiden.

- Härtung Identität (aus Selbstreview R2): Intake stempelt nur, wenn der Client KEINE Identität liefert; ein
  Browser-Client kann weiterhin `client_context.actor.id` frei setzen (vorbestehend). Ziel: verifizierte
  Sitzungsidentität VOR Client-Angaben, Ausnahmen nur für Server-/Harness-Ursprünge. Entscheidung nach Kimi-Review.

## Umgebungsfallen (neu, zusätzlich zu Archiv unten)

- Worker-Sandbox = WorkspaceWrite; State-Root, `~/.codex`, `runtime/` read-only. JEDER
  `ctox`-Aufruf mit DB-Zugriff (auch Lesen, WAL braucht -shm) aus dem Worker scheitert.
  Verträge, die Worker-CLI verlangen, sind damit tot, bis R1 landet.
- Drei Besitzer-Identitäten für Browser-Sitzungen: Nutzer, `ctox_harness`, `scrape_executor`
  (Sitzungs-ID `browser_session_web_stack_auth_<quelle>_<owner>`); Budget 3 je Owner.
- `git archive origin/main -- $VAR` in zsh: Variable wird nicht wortgetrennt → leerer Export.
- Codex-Threads liegen unter `~/.codex/sessions/<Y>/<M>/<D>/rollout-*.jsonl` (hier 163 MB);
  Claude-Sitzungen über `list_events`.
- zsh: `$BASE:src/...` wird als History-Modifier gelesen → `${BASE}:` schreiben.

## Evidenzkarte

- workjet-PR für den Web-Stack: https://github.com/metric-space-ai/workjet/pull/11 (Branch thesen-person-contract @ cdf64f856).
- Board: diese Datei (committed) + stabile Artifact-URL: https://claude.ai/code/artifact/76c22911-7059-44b9-98b2-408f5e7d0b0b (Rendering: Scratchpad thesen-outbound-board.html, bei Updates neu publizieren).
- Sol-Läufe R1/R2/R3: local-2026-09-01T214228Z-e2266936-869e-49a3-9f68-a7fbb0501ac0 / local-2026-09-01T214231Z-b3bd36e5-8f7d-4b13-9944-96a861dea3ca / local-2026-09-01T214233Z-2fc882cf-5fc1-49b8-91b8-721f3ec774f0 (runs.jsonl im Launchpad).
- App-Repo: `~/Documents/thesen-apps` (privat, lokal). Tenant-Deploy-Skripte:
  `~/Documents/ctox-dev/output/deploy-olg-*.ts`, Tenant-ID `7f02e63d-aada-430a-928b-87e454b354d3`.
- Rust: Launchpad `thesen-rust-batch` (Briefs `briefs/R1-*.md`, `R2-*.md`, `R3-*.md`), Vollklon
  `ctox-rustfix` für Compile/Tests, origin/main für Push.
- Codex-Thread: `~/.codex/sessions/2026/08/30/rollout-2026-08-30T14-32-31-01a052a8-36e2-7591-aa5d-c321f3773310.jsonl`;
  Codex-E2E-Bericht: `~/.codex/worktrees/122c/ctox/docs/thesen-outbound-e2e-ui-ux-report-20260831.md`.
- Eigener Vorgänger-Thread: Claude-Sitzung `local_f5b3ad21-59a9-4d7b-af96-84017ae50ed6` („outbound app").

---

# Archiv — Sanierungsplan Stand 31.08.2026 (historisch, unverändert)

**Headline / kritischer Pfad:** Ein einziger Shell-Defekt (P0: `business_commands`-Kanal
stirbt beim Service-Neustart und erholt sich im lebenden Tab nie) erzeugt fast alle
„toten Buttons". Erst P0 fixen, dann Button-Sweep und Recherche-E2E; der Rust-Batch
(P2) räumt die native Restliste ab.

Arbeits-Checkout App: `~/Documents/ctox-dev/output/outbound-lead-generation-runtime-root-fix/2026-08-29/outbound-lead-generation/` (NICHT in Git — siehe P3).
Deploy: `~/Documents/ctox-dev/output/deploy-olg-1.0.51-v2-ui.ts` (SSH, SHA-geprüft, Backup+Rollback).
Tenant: thesen.ctox.dev = `ctox-e5ed9648`, Release `branch-main-20260830T135158Z`.

---

## DONE (selbst verifiziert, nicht nur behauptet)

### Upgrade-Pfad freigegeben (Owner, 31.08. ~19:00)

Owner-Direktive: „dann fahre jetzt den upgrade pfad". Befund vor dem Start:
ALLE sechs Rust-Batch-Punkte liegen bereits auf origin/main — 19c067835
(Codex-Parallelsession: auth_assist-Nutzeridentität, Trusted-Local-Intake,
Sofort-Tab, Harness-404-Selbstheilung `retry_without_response_chain`),
c8d9e3e20 (Sellify-Lookup/Kampagnen-Entity), busy_timeout store.rs:1613,
URL-Parse-Skip person_research_command.rs:908. Kein eigener Push nötig;
mein Browser-UI-Commit c64a40945 ist die main-Spitze. Gestartet:
`setsid nohup ctox upgrade --dev` auf thesen (pid 759287,
Log ~/upgrade-dev-20260831c.log). Abnahme danach: Journal ohne
Token-Ablehnung, Unlock mit Nutzer-Sitzung + Sofort-Tab, Nachrecherche E2E,
dnbhoovers-Duplikate abräumbar.

### Build-Vermeidung als Arbeitsprinzip (Owner-Direktive 31.08. Nachmittag)

Direktive: „abarbeiten und auf teure upgrades verzichten, wenn es geht." Teuer =
Tenant-Rust-Build (`ctox upgrade --dev`, ~40 Min, killt laufende Sitzungen).
Jede Sanierung dieses Nachmittags wurde bewusst auf einen build-freien Weg
umgeplant; der Rust-Batch bleibt geparkt, bis nur noch Punkte übrig sind, die
wirklich Rust brauchen:

- **Sellify-Weiche ohne Build gelöst** (statt Rust-Latenzfix am Kanal): App
  1.0.61 deklariert `sellify_companies` als Fremd-Collection; die Shell
  repliziert sie (vorgesehener Mechanismus, app.js:5915ff). Weiche entscheidet
  lokal in **19 ms** (warm) statt 60-s-Kanal-Rundreise mit fail-closed-Abbruch.
  Beweis: Carbosulf „bereits in Sellify … nur Nachrecherche" / Gueltig Eins
  „nicht gefunden … nur Neue Recherche" / Nachrecherche Carbosulf startet („Läuft").
- **Queue-Stau operativ geheilt** (statt sofortigem Harness-Build): 20
  crash-loopende Aufgaben abgeräumt (5 sofort, 15 geschützte lösen sich nach
  Token-Fix); Wurzelursache (Gateway verliert Response-Ketten → 404-Schleife)
  diagnostiziert und als Harness-Selbstheilung (Retry ohne Kette) im geparkten
  Rust-Batch implementiert + kompiliert — Auslieferung gebündelt, nicht einzeln.
- **Klick-Blocker per App-Hotdeploy** (1.0.62/1.0.63, Minuten statt Build):
  gestapelte `document.body`-Dialogschichten → in `ctx.host`, Singleton,
  Escape/Backdrop; Quellen-Dialog-Rewrites gedrosselt (5 s / Zeigerkontakt-Sperre).
- **Browser-App über den Asset-Release-Kanal** (statt Binary): Shell 0.1.21 via
  Git-Tag `business-os-shell-v0.1.21` → GitHub-Action baut+signiert (Ed25519,
  54 s) → `shell-update stage/activate` auf dem Tenant. Kein Rust-Build nötig.
  Inhalt: Präzise Eingabe (Klickpunkt → lokales Textfeld → click/type am Punkt;
  Runner konnte `click` links/rechts + `keyboard.type` schon), einklappbare
  Sitzungsleiste, kompakte Anmeldezeile, ⌘V-Paste auf die Bühne. Live
  verifiziert: Toggle klappt, Overlay öffnet, „Punkt 640×360 gewählt".
- **Noch offen und WIRKLICH buildpflichtig** (geparkt als Stash `rust-batch-wip`
  im Launchpad ~/.local/state/workjet-launchpads/ctox-rustfix, kompiliert grün):
  Harness-404-Selbstheilung, auth_assist-Nutzeridentität + Sofort-Tab,
  Token-Intake (2 Stellen), SQLite busy_timeout, URL-Parse-Skip. Auslieferung
  als EIN Batch, wenn der Owner den 40-Min-Build freigibt.

- **Recherche-Ergebnisverlust behoben** (App 1.0.50): `source_policy` schickt nur noch
  Quellen mit HTTP(S)-URL; vorher tötete die eingebaute Quelle `impressum` (url='',
  angelegt 30.08. 21:16 durch seedSources) JEDEN Lauf NACH getaner Arbeit mit
  „invalid runtime source URL" (`person_research_command.rs:889`, Commit ccd84bbd3).
  Beweis: lead_1kthlyz 23:13 completed (27 Belege), lead_qxx4u1 00:06 completed
  (39 Belege, 10 Felder, dnbhoovers+leadfeeder als Quellen sichtbar in der UI).
- **Boot-Gate entschärft** (1.0.50): App wartete auf Readiness `live`, die ein
  Follower-Tab nie meldet (readiness `catching-up` = Sammeleimer; Listener feuert
  nur bei Wechsel). Jetzt nicht-fatal. App rendert mit Daten trotz „(0/5)".
- **Shell-V2-Umbau** (1.0.51, Style-Builds v2…v17, alle live deployt):
  - Ein Header-System nach Knowledge-Muster: `data-shell-v2-header-row` 1/2, alle
    drei Spalten identische Zeilengeometrie (gemessen y=185/y=222, je 37px),
    Icon-Block-Freiraum links, Fensterknöpfe-Freiraum rechts.
  - Statisches Skelett; Renders schreiben nur Scroll-Körper → kein Scroll-Springen,
    keine Animations-Neustarts; `<details>`-Zustand überlebt Renders (Detail + Center).
  - Kompakte Feldzeilen 46px (vorher 87/200; Ursache: `.leadgen-review-badge
    { grid-column: 1/-1 }` erzwang eigene Grid-Zeilen).
  - Personen-Slots nach Priorität, Name+Position, „offen" sichtbar; seit v16 die
    8 Owner-Kategorien (GF/Gesamtverantwortung, Prokura, Finanzen, Einkauf,
    Supply Chain, Operations, Technik, Entwicklung) + „Weitere".
  - Tab „Entscheidung" entfernt; Aktions-Buttons und Sellify-Empfängerauswahl in
    der Übersicht; Pflegefelder unter Einordnung.
  - Filter/Sortier-Tray im Knowledge-Stil (Select+Richtung+Reset, Status-Chips),
    einklappbar; Fortschrittsleiste nur bei aktivem Lauf; Sync-Status als Fußzeile;
    Buch-Icon entfernt; Quellen-Dialog feste Größe (kein Springen bei Tab-Wechsel).
  - Farbwelt: `--accent` = Frame-Palette (Orange) statt Shell-Blau (40 Verwendungen).
  - Quellen-Einstellungsdialog: URL + Zugang (Secret-Store, nur Referenz) +
    **Freitext-Anweisungen je Quelle** → `operator_instructions` in allen drei
    Verträgen (scrapeContract, Reconcile-Snapshot, source_policy; serde toleriert
    Zusatzfelder — geprüft: kein deny_unknown_fields).
  - Responsive: Container-Tiers mit V2-Spezifität am Dateiende (Ursache: Zeile-15-
    Regel schlug per Spezifität jedes `@container`-Tier). Gemessen: 988px → 2 Spalten,
    Überlauf 0; 628px → 1 Spalte, Kampagnen als Chip-Leiste; breit → 3 Spalten.
  - Einzel-Nachrecherche öffnet sofort das Chatfenster (`open: options.openChat`),
    Bulk bleibt still. Policy-Save-Button vom Reconcile-Pending entkoppelt.
  - Ehrliche Texte: „21 Kontakte zur Prüfung zurückgestellt – Sellify-Abgleich
    ausstehend" statt „ausgeschlossen"; Adapter-Inspector sagt, dass Skripte in der
    nativen Registry liegen.
- **Neuer Recherche-Prompt** formuliert (Owner-Struktur 0–7, ALLE 32 RESEARCH_FIELDS
  namentlich abgedeckt — maschinell geprüft): `docs/thesen-outbound-recherche-prompt-20260831.txt`.
  Als DEFAULT_RESEARCH_POLICY in App v16 eingebaut. **NOCH NICHT im gespeicherten
  Policy-Record** (dort 2475-Zeichen-Rohentwurf, updated 00:28) — blockiert durch P0.
- **Harness/Queue lebt wieder**: `ctox prompt worker start/end source=queue` im Minutentakt
  (Wiederbelebung durch Service-Neustarts). ABER: Reconcile-Task dreht im Kreis (→ P2.6).
- **Adapter-Inventar** (31.08. 00:5x, `scrape_target` × letzter Lauf):
  - 15/22 grün mit Skript + letztem Erfolg: bundesanzeiger(22), dnbhoovers(25), evi(2),
    firmenabc(23), handelsregister(25), impressum(11), justizonline(2), moneyhouse(25),
    shab(2), xing(23), zefix(28) + heute erfolgreich impressum/handelsregister/bundesanzeiger.
  - 4 blockiert (Provider-Challenge): google-de(28), companyhouse-de(25),
    maps-google-com(25), rocketreach-com(23) → brauchen Unlock/Login (P2.4, OWNER).
  - 3 transient: northdata(31), leadfeeder(23), linkedin(3, zusätzl. auth_required).
  - 2 portal_drift (Skript kaputt): mailtester-com(1), experte-de(26) → P1.4.
  - 2 E2E-Dummies ohne Skript (abnahme-e2e07*) — erwartbar.

## Ereignis-Log (fortgeschrieben)

- 14:25 **SELLIFY-WEICHE OHNE BUILD GELÖST + KOMPLETT-E2E GRÜN** (App v1.0.62):
  Statt des geplanten Rust-Patches deklariert das Modul `sellify_companies`
  als Fremd-Collection (der Shell-Mechanismus dafür existiert seit 11.08.);
  die Weiche entscheidet auf dem LOKALEN Replikat. Gemessen im Browser:
  - „Neue Recherche" auf Carbosulf (im CRM): Abbruch „bereits in Sellify
    geführt (contact_id 17622) — nur Nachrecherche möglich" (Treffer in
    **19 ms**, warm; 8,3 s beim Erstsync).
  - „Nachrecherche" auf E2E04-Testfirma (nicht im CRM): Abbruch „nicht
    gefunden — nur Neue Recherche möglich".
  - „Nachrecherche" auf Carbosulf: startet, Lead „Läuft", CRM-Vorwissen im
    Auftrag.
  - **Sellify-Kampagnen-Import E2E**: Suche „Welle 3 - 04.09.2025" → 4
    Kampagnen mit Mitgliederzahlen → Import „Automatiktüren & Drehtüren D"
    → Kampagne „Sellify: Automatiktüren…" mit 7 eindeutigen Firmen-Leads
    (aus 16 Mitgliedszeilen; Rest Personen-/Doppelzeilen) in der Liste.
  - Zwei-Prompt-Settings sichtbar: „Prompt: Neue Recherche" (aktiver
    0–7-Prompt, 3320 Zeichen) + „Prompt: Nachrecherche" (leer=Fallback).
  Der Command-Fallback bekam 90 s + 1 Retry (Terminal-Beobachtung über den
  Sync-Kanal bleibt träge); der geplante native-Weiche-Rust-Patch ist damit
  NICHT mehr nötig — von der Nächster-Build-Liste gestrichen.
- 14:25 Verbleibende Build-Kandidaten (alle „wenn es geht"-verzichtbar,
  OWNER entscheidet Zeitpunkt): Aux-Kanal-Priorisierung (RPCs vor Frames;
  Wurzel der 20-s-Timeouts), P2.4 nativer Skript-Lesepfad, P2.5
  Versionshistorie für Direkt-Deploys, split_name-Adelspartikel (liegt im
  workjet-Repo, nicht in ctox), harte maschinelle Stop-Anweisungen.

- 13:56 **ABNAHME-MESSUNG nach Deploy** (Release branch-main-20260831T122521Z
  aktiv seit 12:45; App v1.0.60/v32):
  - Token-Ablehnungen seit Deploy: **0** (vorher Dauerschleife). 404-Tode: **0**.
  - **WITTENSTEIN SE: Nachrecherche completed** (aus der Queue, durch den
    Harness, mit Adaptern). Beiersdorf Manufacturing: needs_review, 10/32
    Felder + Belege, 2 Quellen fordern Browser-Autorisierung. BNT: 14/32.
  - Unlock-Fenster zeigt echten Seiteninhalt (FirmenABC, Zugangsdaten-Leiste).
  - Queue: Auth-Assist-Duplikate weg; Rest sind Repair-Tasks in Abarbeitung.
- 13:05–13:51 App v1.0.52→v1.0.60 (8 Iterationen, alle visuell verifiziert):
  zwei Buttons „Neue Recherche"/„Nachrecherche" + harte Sellify-Weiche
  (fail-closed), Sellify-Kampagnen-Import (Icon+Dialog, native campaign-
  Entity), Zwei-Prompt-Settings (instructions/followup_instructions, Auswahl
  nach Modus), Fuzzy-/Domain-Dublettensuche, Schutzschalter gegen den toten
  Direkt-RPC, sequenzielle statt paralleler Proben (parallel = Selbst-DoS:
  STREAM_LIMIT_EXCEEDED gemessen).
- 13:50 **OFFENER KERNBEFUND — Sync-Leitung**: Die Browser↔Server-Rundreise
  eines Sellify-Lookups dauert ~50–60 s (nativ <1 s; Command completed
  serverseitig in Sekunden, die Terminal-Beobachtung im Browser verhungert
  hinter Live-Frames/Chat-Streams auf dem Aux-Kanal; Direkt-RPC 20-s-Timeout,
  shell-seitig nicht konfigurierbar). Folge: die Sellify-Weiche blockiert
  derzeit oft fail-closed („Abgleich fehlgeschlagen … erneut versuchen") statt
  zu entscheiden. SAUBERER FIX (nächster Build, OWNER-Freigabe nötig):
  Weiche in den nativen person_research-Intake verlagern (variant im Payload,
  Lookup nativ, Abbruch als Command-Fehler mit Klartext). Zweiter Kandidat:
  Aux-Kanal-Priorisierung (RPCs vor Frames) — Shell/Server-Thema.
- 13:45 Stale-Modul-Falle dokumentiert: Die Shell serviert Module aus dem
  Cache („fetch:stale-served") — nach einem Deploy braucht es ZWEI Reloads,
  bis der neue Stand ausgeführt wird. Für jede Browser-Messung Pflicht:
  Ressourcen-Log auf die tatsächlich AUSGEFÜHRTE Version prüfen.

- 12:25 **P2-Batch gepusht und Server-Build gestartet** (origin/main `c8d9e3e20`,
  Log `upgrade-outbound-heilung-c8d9e3e20-20260831T122521Z.log`, pid 598199).
  Owner-Ansage: EIN Upgrade-Lauf, danach keiner mehr. Inhalt (2 Commits, lokal
  `cargo check` grün, Sellify-Evidenz-Test grün):
  1. Auth-Assist-Sessions gehören dem anfragenden NUTZER (zentrale
     Besitzer-Auflösung aus dem Task-Actor statt `ctox_harness`); Session+Tab
     werden bei Annahme sofort projiziert (Unlock-View hat sofort etwas zu
     zeigen); Browser-Automation läuft im Profil des Besitzers.
  2. auth-assist-login/-signup Intake auf trusted-local — die Token-Ablehnung
     („a valid capability token is required", live 06:22Z in Dauerschleife,
     15 dnbhoovers-Duplikate in der Queue) ist damit an der Wurzel weg.
  3. Harness: 404 „Response ... was not found" auf `previous_response_id` →
     Kette verwerfen, EINMAL mit voller Historie neu senden. Gemessen: die
     Queue-Worker starben daran seriell (02:07/07:14/07:29, je andere ID);
     Gateway-Events zeigen für diese IDs NULL Einträge (Stream serverseitig
     nie fertig, Client übernahm die ID trotzdem).
  4. `impressum`-Wurzelfix (Builtin-Skip VOR URL-Parse), RxDB-busy_timeout
     10s→30s (Sellify-Lookups starben an „database is locked" + vergiftetem
     „canonical command replay remained nonterminal").
  5. Sellify als sichtbare Belegquelle im Rechercheergebnis (Name/Domain/
     E-Mail/Telefon/CRM-Nr., exakt + rechtsformfreier Fuzzy-Probe) und
     `outbound.sellify_lookup` mit `fuzzy_selectors`, `website_url`-Feld und
     `campaign`-Entity (Kampagnen-Mitglieder, Limit 2000).
- 12:25 KORREKTUR zur Adapter-Forensik: der `/v1/responses`-404 des Reviews
  ist ein GATEWAY-Persistenzverlust bei abgebrochenen Streams (Vercel-Pfad
  `storeFallbackResponseStateWithRetry` im SSE-`onCompleted` ohne Event bei
  Abbruch), kein Tenant-Zuordnungsfehler. Harness-Selbstheilung (Punkt 3)
  entschärft ihn; Gateway-Härtung bleibt offen (ctox-dev, separater Deploy).
- 12:20 Test-Befund: `appsec_worker_dispatches_business_os_web_stack_auth_
  assist_contract` scheitert IDENTISCH auf Basis f8271bd2d (accepted vs
  pending_sync) — vorbestehend; der Test-Build des ctox-Bins war upstream
  ohnehin kaputt (ring-Konvertierungen, in Batch repariert).
- 12:30 **App v1.0.52 gebaut** (Tarball + Deploy-Skript bereit, Deploy NACH
  dem Upgrade): zwei Buttons „Neue Recherche"/„Nachrecherche" mit harter
  Sellify-Weiche (existiert→nur Nachrecherche; fehlt→nur Neue Recherche;
  Prüffehler→kein Start), Domain- und Fuzzy-Fallback in der Dublettensuche,
  Sellify-Kampagnen-Import (eigenes Icon, Suche→Mitglieder→Kampagne mit
  Leads), Zwei-Prompt-Settings (Neue/Nachrecherche, leer=Fallback).
- 11:00 Queue-Räumung Teil 2: 5 Tasks gecancelt (rocketreach/google/mailtester/
  2×reconcile); 15 dnbhoovers-Duplikate transition-geschützt — lösen sich
  mit dem Token-Fix. 3 frische „Nachrecherche WITTENSTEIN SE"-Aufträge warten
  auf den Deploy. WITTENSTEIN: 32 Sellify-Treffer (v0), BNT vorhanden.
- 10:56 Launcher-Schreck „Apps weg": Module+Katalog serverseitig intakt
  (17 Einträge, outbound+sellify enthalten); Ursache veraltete Browser-Ansicht
  nach Dienst-Neustarts 08:36–08:38; Reload zeigt alles. Kein Datenverlust.

- 01:15 v18 deployt: **Kanal-Selbstheilung in der App** (`recoverCommandChannel` via
  `ctx.sync.restartCollection` auf der geteilten Shell-Runtime + Handle-Neuauflösung
  + Einmal-Retry) für researchLead, saveResearchPolicy, toggleSource,
  Adapter-Reconcile. Damit ist P0 App-seitig gemildert, ohne Shell-Bypass.
  Shell-seitig bleibt der saubere Fix (P2.8) — Slot-System ist Ed25519-signiert,
  Hot-Patch wäre Integritäts-Bypass → OWNER.
- 01:25–01:45 **Queue bereinigt** (Owner-Auftrag): 6 doppelte Reconcile-Tasks + 11
  doppelte Repair-Tasks + 178 failed-Altlasten gecancelt; 279 failed-Reste sind
  durch die Zustandsmaschine geschützt (Command terminal = reine Historie).
  Aktiv jetzt: 6 einzigartige Repair-Tasks, 2 Auth-Assists (rocketreach, google),
  1 laufender Reconcile.
- 01:34 **P1.4 aufgelöst — kein Defekt**: mailtester/experte melden
  `CTOX_SCRAPE_INPUT_JSON.email missing` = Validierungs-Targets brauchen eine
  Eingabe-E-Mail; ohne Input ehrlich portal_drift (Phantom-Lead!). Mit Input
  zuletzt 18:02 erfolgreich. ⇒ ALLE 20 echten Targets haben funktionierende
  Skripte. (Kosmetik-Punkt: Status-Label „input fehlt" statt „portal_drift".)
- Mess-Pane-Zustand: frisches Browser-Profil resynct langsam (160 Docs nach
  Minuten); Katalog-Eintrag der App noch nicht repliziert — Verifikation v18
  wartet darauf.

- 02:00–02:20 **P1 abgeschlossen (visuell + serverseitig verifiziert):**
  - P1.1 ✅ Neuer Prompt ist der aktive Policy-Record (Server: 3320 Z., alle 32
    Felder annotiert, 8 Kategorien; updated 01:18 — v18-Heilung drückte den Write durch).
  - P1.2 ✅ Button-Sweep: 21 Aktionen interaktiv geprüft — view-mode, Tray/Filter/
    Chips/Reset, 4 Detail-Tabs, Auswahl (einzeln/alle/aufheben), Lead-Editor auf/zu,
    Quellen-Dialog + Suche + Settings-Dialog + Skript-Inspector, rename/new/delete-
    Kampagne-Dialoge, toggle-source (Server: enabled-Flip 02:16:12), test-adapter
    (Command completed 02:16:11), import-leads (Importer öffnet), Nachrecherche.
    Zwei App-Fixes dabei: v19/v20 Signatur-Skip + Tipp-Fokus-Guard gegen das
    Klick-Verschlucken durch Panel-Rewrites (Ursache von „nichts am Menü geht").
  - P1.3 ✅ Recherche-E2E mit NEUEM Prompt: BNT Chemicals → Chatfenster öffnet
    mit Auftrag (Owner-Prinzip), Lauf completed 02:20, **9 Felder, 53 Belege aus
    8 Quellen**, Person GF Robert Süße. Kein Fehler.
  - P1.4/P1.5 ✅ Selbstheilung wirkt: **maps-google-com und northdata-de wurden
    durch die Repair-Tasks geheilt** (beide succeeded mit Treffern im BNT-Lauf).
    mailtester/experte sind funktionsfähig (Input-abhängig). Offen bleiben nur:
    google-de + companyhouse-de (Provider-Challenge) und rocketreach/linkedin
    (Login) — Auth-Assist-Tasks stehen, Token-Fix ist P2.2 (Rust).
  - Bekannter Rest: Erst-Klick direkt nach Dialog-Öffnung kann noch verschluckt
    werden (Öffnungs-Rewrite-Fenster); Personen-Ausbeute >1 pro Lead hängt an den
    Auth-Quellen (LinkedIn/Xing-Personensuche).

- 06:13 **v21: Owner-Befund behoben — der gepflegte Rechercheablauf erreichte den
  Agenten NIE** (er floss nur in die Adapter-Generierung; der Recherche-Prompt
  war ein zweiter, hartkodierter Kurztext). Jetzt steht der Ablauf wörtlich in
  beiden Prompts (Einzel + Kampagne) + als `research_instructions` im Payload.
- 06:14 **v22: Quellen-Glossar + Phasenmodell im Agenten-Prompt** (Adapter =
  Werkzeuge mit Glossar; Phase B = aktives Lückenschließen per Websuche/
  CTOX-Browser mit Belegpflicht; Phase C = strukturiertes Nachtragen).
- 06:10 **Mega-Reconcile-Task geblockt** (Review: erfundene Blocker; Rework-Kreis).
  Strategie: Einzel-Generierung je Quelle (3× nachweislich erfolgreich).
- Befunde: Brave-Insert im STREAM_LIMIT verloren (neu anlegen); Testleichen-
  Löschung kam NICHT am Server an (delete-source NICHT e2e — Korrektur);
  experte.de serverseitig AUS (wieder einschalten); BNT inzwischen 14/32.
- Browser-Pane verlor die Sitzung — UI-Verifikation wartet auf Owner-Login.

- 06:18–06:35 **Entsperr-Pfad live seziert** (Owner-Test): (a) v23-Button →
  eigene Sitzung → „Chromium reported ready" — **funktioniert**; mein Deploy-
  Neustart hat die erste Sitzung gekillt (23 Chromium-Tode/24h = meine
  Deploys; ab jetzt Deploy-Stopp während Owner-Tests). (b) Live-View zeigt
  nichts, weil der Scraper nur die SITZUNG anlegt, aber keinen TAB öffnet
  („0 Tabs" → „Inhalt wird geladen" wartet ewig). (c) Worker-initiierte
  Auth-Sitzungen (rocketreach/xing/bundesanzeiger) laufen unter
  `_ctox_harness`-Identität — für den Owner unsichtbar/unsteuerbar („Kein
  laufender Browser-Prozess"). Das IST der Capability-Token-Defekt P2.4 in
  letzter Konsequenz: Worker kann Sitzungen nicht an den Nutzer übergeben.
  (d) „Zugangsdaten einsetzen" scheiterte einmal an SQLite „database is
  locked" (Store-Contention unter Agentenlast) → P2-Punkt busy_timeout/Retry
  im Command-Intake.
  → P2.4 präzisiert: auth_assist muss die Sitzung unter der NUTZER-Identität
  anfordern (oder übertragen) UND beim Start direkt einen Tab mit der Ziel-URL
  öffnen.

## P0 — Der eine Bruch, der alles tötet (Glied 3 der Kette)

**Befund:** Lokale App-Writes gelingen (Trace `write ok`), aber `business_commands`-
Kanal ist nach Service-Neustart `cancelled` und wird im lebenden Tab nie neu
aufgebaut. Symptome: Recherche-Start „Command konnte nicht an CTOX übergeben …
was cancelled", Crew-Karten FEHLGESCHLAGEN, Policy-Save kommt nie am Server an,
Toggles/Löschen wirken tot. `sync.js` L799–821 ersetzt cancelled Bridges bei
`startCollection` — aber niemand ruft es erneut auf.

**Fix-Ansatz:** Dispatch-/Write-Pfad: bei `was cancelled` einmal Kanal re-akquirieren
(startCollection/restartCollection) und Operation wiederholen; Crew-Ack-Timeout-
Meldung nur zeigen, wenn der Command wirklich fehlt (nicht bei später Quittung).

**FALLE (zuerst klären!):** Die servierte `shared/sync.js` ist NICHT die Release-Datei
(md5 served f11d9437… ≠ Release c03e7537…). Die Shell kommt aus einer anderen Quelle
(Kandidaten: `~/.local/state/ctox/business-os-source-snapshots/`, Stage-Verzeichnisse,
eingebettete Assets im Binary). VOR jedem Shell-Patch die wahre Quelle finden,
sonst patchen wir ins Leere. → allererster Schritt.

**Bis zum Fix (Workaround):** Seite neu laden stellt den Kanal her; frisch geladene
Tabs dispatchen nachweislich (23:08, 00:06 completed).

## P1 — Nach P0: sichtbare Funktion herstellen (Reihenfolge)

1. **Neuen Prompt speichern** (über die UI, testet zugleich den Save-Button) und
   serverseitig verifizieren (`research_policies` len/Marker).
2. **Systematischer Button-Sweep** im Browser: jede `data-action` einmal auslösen,
   Wirkung serverseitig/DOM verifizieren; destruktive nur auf E2E-Daten; Ergebnis
   als Matrix in diesem Dokument.
3. **Recherche-E2E mit neuem Prompt**: echter Lead (kein Phantom!), Chat öffnet,
   Lauf completed, Felder+Belege+Personen-Slots gefüllt, 8 Kategorien angestrebt.
4. **mailtester-com + experte-de reparieren** (portal_drift): Skripte auf
   Live-Portale nachziehen, `register-script`, `execute` grün. (E-Mail-Validierung
   ist Prompt-Punkt 4 — ohne sie fehlt person_email_validation.)
5. **northdata/leadfeeder/linkedin transient**: erneut ausführen, bei Wiederholung
   Ursache (Rate-Limit? DNS?) messen statt raten.
6. **Dialog-Z-Index** (Lösch-Popup hinter Chat) + Restpunkte visuell bestätigen.

## P2 — Rust/main-Batch (ein Build, eine Auslieferung)

1. `person_research_command.rs:889`: `continue` für eingebaute Quellen VOR den
   URL-Parse (Defense-in-Depth zum App-Filter).
2. Auth-Assist: Intake lehnt Harness-Commands ab („a valid capability token is
   required", `service/business_os.rs:4601`-Pfad) → Token für native Worker-Requests
   ausstellen. Ohne das bleibt Unlock für die 4 Challenge-Targets tot.
3. Sellify-Evidenz: Lookup-Treffer als Feld-Belege schreiben (heute 1 Beleg im
   ganzen Bestand) — Prompt-Punkt 0/5 verlangt Sellify als Quelle.
4. Adapter-Skript-Lesebefehl (typed command) für die App; Inspector zeigt echte
   Revision aus `scrape_script_revision` (Skripte existieren, Browser kann sie
   nicht lesen).
5. Modul-Lifecycle: Direkt-Deploys erzeugen keine Versionseinträge → Versionshistorie/
   Source-Editor leer. Entweder Deploy über Lifecycle-Kommando oder Importpfad bauen.
6. Reconcile-Kreisel: Review lehnt ab („contractual ctox scrape upsert-target …
   blocked" = Worker darf CLI nicht ausführen), Task requeued endlos → Worker-Rechte
   oder Vertrag ändern; offene Tasks stoppen.
7. Kleinkram: Personen-Namenszerlegung („Johannes von"/"Cossel"), Mitarbeiter-Einheit
   („71 M"), leerer Lead `lead_fresh_wittenstein…` (Name leer) reparieren/löschen.
8. Shell: P0-Fix ordentlich in `shared/sync.js` + Guard-Test; Dialog-Z-Index;
   Andock-Sensitivität; Hintergrund-Fenster-Transparenz; Morph-Cleanup bei
   `el.isConnected`-Ausfall.

**OWNER-Entscheidungen (offen):**
- main-Update im Haupt-Checkout freigeben (52 der 88 offenen Dateien überlappen
  mit den 12 eingehenden origin/main-Commits) — Voraussetzung für P2.
- Shell-Hotpatch auf dem Tenant erlaubt (an Release vorbei, dokumentiert), oder P0
  nur über den P2-Release-Weg?
- Zugangsdaten für rocketreach/linkedin (und Entscheidung zu google/companyhouse-
  Unlock) — ohne Logins bleiben 4 Adapter blockiert.

## P3 — Ordnung

1. App-Quelle in Git (durables Launchpad, z. B. `~/.local/state/workjet-launchpads/…`
   oder eigenes Repo); die 17 Deploy-Backups liegen nur als Tenant-Tars unter
   `~/.local/state/ctox/backups/outbound-lead-generation-before-*`.
2. Deploy-Disziplin: Batches statt Stakkato — JEDER Service-Restart reißt Kanäle
   (P0-Kaskade) und invalidiert Tabs.
3. Dieses Dokument bei jedem Ereignis fortschreiben (Kanban-Pflicht).

## Umgebungsfallen (in dieser Sitzung real bezahlt)

- Served Shell ≠ Release-Datei (md5-Differenz) — Quelle VOR Patch klären.
- Bash-Mehrzeiler: Zeilen laufen einzeln weiter, wenn ein Heredoc-Python scheitert;
  zweimal lief dadurch sed+Deploy mit inkonsistentem Stand (Rollback griff sauber).
- Tab-Cache: Modul-Buster hängt an der replizierten Modulversion; nach Deploy 1–2
  Reloads + Wartezeit; „App geht nicht" erst nach Buster-Prüfung diagnostizieren.
- Verstecktes Browser-Pane: WAAPI-Animationen eingefroren (`is-shell-v2-morphing`
  bleibt), lange JS-Schleifen >45s timeouten.
- Multi-Tab: User-Tab ist Leader, mein Pane Follower; Leases überleben tote Kontexte.
- Maintenance-Gate („Recovery exportieren") nach Restarts; „Erneut prüfen" löst es.
- E2E-Phantomfirmen erzeugen ehrliche portal_drifts — kein Adapterfehler.
- Modul-Guards des Repos prüfen `local-modules` NICHT — die App kann jeden Vertrag
  brechen, ohne dass etwas rot wird.

## Evidenzkarte

- Messskripte (alle rein lesend außer Deploys): `~/Documents/ctox-dev/output/claude-*.ts`
- Deploy-Skript: `~/Documents/ctox-dev/output/deploy-olg-1.0.51-v2-ui.ts` (erwartet v17)
- App-Quelle: `~/Documents/ctox-dev/output/outbound-lead-generation-runtime-root-fix/2026-08-29/outbound-lead-generation/`
- Tenant-Backups: `~/.local/state/ctox/backups/outbound-lead-generation-before-*`
- Neuer Prompt: `docs/thesen-outbound-recherche-prompt-20260831.txt` (kanonisch)
- Kettenbeweise: business_commands (SQLite `business-os.sqlite3`), Läufe
  (`ctox.sqlite3`: scrape_run/scrape_script_revision), Leads (`business-os-rxdb.sqlite3`)
