# THESEN Outbound Lead Generation — Sanierungsboard (Stand 02.09.2026, 19:30 UTC)

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

### 02.09. 04:0x — R1 fachlich NOCH NICHT wirksam: Worker kann `ctox` gar nicht erst starten (negatives Ergebnis)

- Reparaturaufgabe `queue:system::36c31fa9e0ca33519745a3f4` (bundesanzeiger-de, 03:02 UTC) endete nach 11
  Slices **failed**. Worker-Beleg, wörtlich: „`ctox` binary still EACCES (exit 126) when invoked via
  /usr/local/bin/ctox → /home/ctox/.local/bin/ctox; `cat` of the target binary also EACCES; `getent hosts
  www.bundesanzeiger.de` still empty; curl → Could not resolve host (exit 6)". Das Relais (R1) kam nie zum Zug.
- Ursache (Code, origin/main): `managed_worker_sandbox_policy` setzt für Queue-Worker
  `ReadOnlyAccess::Restricted { include_platform_defaults: true, readable_roots: [] }`; Landlock-Plattform-
  Defaults sind nur /bin, /usr, /etc, /lib, /lib64, /dev, /proc (landlock.rs:35–44). Damit ist
  (a) `/home/ctox/.local/lib/ctox/current/bin/ctox-real` (und der Wrapper in ~/.local/bin) nicht lesbar →
  execve EACCES, und (b) `/etc/resolv.conf` → Symlink auf `/run/systemd/resolve/stub-resolv.conf` — `/run`
  ist nicht lesbar → **kein DNS in jedem Worker** (erklärt auch frühere „temporary_unreachable"-Läufe aus
  Workern). DAC-Rechte sind korrekt (alles ctox:ctox, 755/775) — es ist die Sandbox.
- Der frühere Fehler „readonly database" stammte aus einer anderen Worker-Klasse (Reviewer: full read-only),
  daher zwei verschiedene Fehlbilder für dieselbe Wurzel: Worker-Sandbox ohne Zugang zur CTOX-Installation.
- **R5** (Sol): Standard-Leseroots für Managed-Worker = CTOX-Installationswurzel (aus `current_exe()`),
  `~/.local/bin`, `/run/systemd/resolve` (DNS-Stub); Schreibroots unverändert; Tests. Danach Build B2 und
  Probe-Task im Worker (`ctox status`, `getent hosts`, Relais-Aufruf).
- Launchpad auf origin/main aktualisiert (Branch `main2`, inkl. execution/agent, landlock.rs, protocol.rs).

### 02.09. 04:3x — R5 geliefert, integriert (Gate läuft), Vorher-Messung im Worker läuft

- R5 (Sol, run …4c227e2f) `completed`: nur `direct_session.rs` (+204/−12). Lesewurzeln = Release-Wurzel aus
  `current_exe()` + `lib/ctox/current`-Alias + PATH-Verzeichnisse mit `ctox` + `/usr/local/bin` +
  `/run/systemd/resolve`, `/run/resolvconf` + Elternverzeichnis des aufgelösten `/etc/resolv.conf`; auf Linux
  immer `Restricted` mit Plattform-Defaults ∪ Defaults ∪ Zusatzwurzeln. Schreibwurzeln unverändert.
  Auf origin/main 388dad97e angewendet (Branch `thesen-r5`, Commit 39b3de7b4); Gate (check + 3 Tests) läuft.
- Sicherheitsprüfung der neuen Leseroots (selbst): `current/` enthält den Symlink `runtime` → State-Root mit
  `ctox-secrets.key`/`ctox-secrets.sqlite3` (0600). Landlock-Regeln hängen am Ziel-Inode; ein Zugriff über
  den Symlink landet im State-Root-Baum, der NICHT freigegeben ist → Secrets bleiben unlesbar. Der Worker
  liest sein Repair-Bundle über sein eigenes Workspace (cwd, Schreibwurzel). Nach B2 im Probe-Task gegenprüfen
  (`cat …/ctox-secrets.key` muss EACCES liefern).
- Vorher-Messung: Diagnose-Task `queue:system::86489f0c97639a536be6e67a` („sandbox probe (baseline)", 9 Befehle:
  id, ctox status über Wrapper und Binary, getent, resolv.conf, /run/systemd/resolve, bin-Listing, Socket,
  curl) läuft im Worker; dieselbe Probe nach B2 = Nachher-Messung.

### 02.09. 04:4x — Vorher-Messung im Worker: Sandbox-Diagnose bestätigt UND zweiter Systemdefekt gefunden

- Diagnose-Task `queue:system::86489f0c97639a536be6e67a` endete `failed` nach 5 Review-Runden. Worker-Rohausgabe
  (aus der Worker-Sandbox): `/usr/local/bin/ctox status` → „Permission denied, EXIT=126", `cat /etc/resolv.conf`
  → Permission denied, `ls /run/systemd/resolve/` → Permission denied, `ls …/current/bin/` → Permission denied,
  `getent hosts` leer. Das ist exakt das Landlock-Bild aus R5 (Plattform-Defaults ohne /home/…, /run).
- **Zweiter Defekt (Harness-Review-Gate):** Der Reviewer hat „alle neun Befehle selbst erneut ausgeführt" und
  erhielt EXIT=0 mit echten Listings — weil das Reviewer-Profil laut HARNESS.md „full filesystem read-only"
  ist, also eine ANDERE Sandbox. Er wertete die Worker-Ausgaben daraufhin als „fabricated" und ließ die
  Aufgabe 5 Runden drehen, bis das Budget erschöpft war. Dasselbe Muster erklärt die 11-Slice-Schleifen der
  Reparaturaufgaben (31.08.: 23 failed). Konsequenz: Review-Wiederholungen von Shell-Befehlen sind nur
  beweiskräftig, wenn sie im selben Sandbox-Profil laufen — sonst sind sie ein Falschbefund-Generator.
  → Karte Backlog „R6".

### 02.09. 04:5x — R5 auf main, Build B2 gestartet

- R5-Gate im Vollklon: `cargo check` fehlerfrei; die beiden bestehenden Sandbox-Tests grün; der neue Test ist
  `#[cfg(target_os = "linux")]` und läuft auf dem Mac nicht (Nachweis kommt aus der Nachher-Messung auf thesen).
- Push: origin/main = **39b3de7b4** (fast-forward auf 388dad97e).
- **B2 gestartet** (`claude-upgrade-dev-thesen.ts 39b3de7b4`), zweiter Neustart des Tages, ~27 min. Danach:
  identischer Probe-Task „sandbox probe (after-b2)" + Secrets-Gegenprobe, dann ein Heal-Lauf → Reparaturaufgabe
  muss `handled` erreichen.

### 02.09. 04:45 UTC — B2 abgeschlossen

- Release **branch-main-20260902T041756Z** aktiv (27 min 34 s), Dienst aktiv, HTTP 200, Binary-Marker
  SandboxedCli 15 / liveScreenshot 2. State-Backup update-20260902T041813Z.
- Nachher-Messung gestartet: Probe-Task „sandbox probe (after-b2)" (11 Befehle inkl. Secrets-Verweigerung und
  Relais-Aufruf `scrape execute` aus dem Worker) + ein Heal-Lauf bundesanzeiger-de für eine frische
  Reparaturaufgabe.

### 02.09. 05:xx — Nachher-Messung nach B2: noch NICHT ausreichend (negatives Ergebnis) → R5b

- Probe-Task „after-b2" (`queue:system::59fc1c58287c7e661911f8e8`) endete `handled`, aber die Worker-Antwort ist
  in keiner Queue-/DB-Struktur persistiert (nur Review-Feedback); das Kontext-Log speichert Tool-Aufrufe ohne
  Ausgabe, und der Exit-Code ist wegen `; echo EXIT=$?` immer 0 → als Beleg unbrauchbar. Belastbare Messung:
  der Worker-Aufruf `ctox scrape execute handelsregister-de` (04:55:57 und 04:58:03 UTC) erzeugte **keinen**
  Scrape-Lauf → die CLI ist aus dem Worker weiterhin nicht gestartet.
- Ursache: Harness-Log „managed worker readable roots: 4 (releases/…, current, /usr/local/bin)" + Resolver-Dir.
  `~/.local/bin` fehlt, weil der Daemon-PATH (systemd user unit) `~/.local/bin` nicht enthält; Landlock löst
  `/usr/local/bin/ctox` → `/home/ctox/.local/bin/ctox` auf, und dieses Verzeichnis ist nicht lesbar → EACCES.
- **R5b** (selbst, klein): Elternverzeichnis des symlink-aufgelösten Wrappers (für jeden PATH-Treffer und
  `/usr/local/bin/ctox`) zu den Leseroots. Branch `thesen-r5b`, Gate läuft; danach Push + Build B3 (dritter
  Neustart).
- Lehre (Umgebungsfalle): Probe-Tasks müssen ihre Ausgabe als DATEI ins Workspace schreiben, sonst ist nichts
  nachprüfbar; und Exit-Codes nie hinter `; echo` verstecken.

### 02.09. 06:4x — R5b grün, Push + Build B3

- R5b (12b32c5da): `cargo check` fehlerfrei, 4 Sandbox-Tests grün (ein verschobenes `cfg`-Attribut zuvor
  selbst korrigiert). Push auf origin/main (fast-forward auf c2dda6da0), anschließend Build B3 gestartet
  (dritter Neustart; Wächter läuft). Nachher-Messung danach mit dem Datei-schreibenden Probe-Task
  (`sandbox-probe-after-b3.txt` im Workspace) + frischer Reparaturaufgabe.

### 02.09. 05:3x — A1-Reproduktion Teil 1 (Owner angemeldet, Viewport 1440×900, Shell 0.1.25, App 1.0.64, während B3 = read-only)

- **Tabellen-/Shard-Umschaltung schließt die App: NICHT reproduziert.** JS-Klick auf `[data-action="view-mode"]`
  (Tabelle→Shards→Tabelle) und echte Mausklicks auf beide Icons: Fenster bleibt, 3 Leads bleiben, keine
  Konsolenfehler außer den erwarteten `CTOX_MAINTENANCE_READ_ONLY` (Build läuft). Codex' Befund bleibt
  unbestätigt (evtl. Breiten-/Timing-abhängig); Karte auf „Beobachten".
- **Importtyp-Wechsel behält Vorschau + Import-Knopf: REPRODUZIERT.** Freitext „KUKA Deutschland GmbH" →
  „1 gültige, eindeutige Leads" + Knopf „Gültige Leads importieren"; Wechsel auf „URL" → Vorschau und Knopf
  bleiben unter dem URL-Formular stehen (Screenshot in der Sitzung). Zusätzlich: Freitext „KUKA Deutschland
  GmbH, Augsburg" wird als CSV mit Website-Spalte gelesen → „Website ist keine gültige HTTP(S)-URL", 0 Leads —
  Komma in Name/Ort-Angaben killt den Import (UX-Defekt, neu).
- Noch offen (brauchen Schreibpfad, nach B3): echter Import (IDB-closing), Sellify-Kampagnensuche, Nachrecherche
  (Identität + Personen + Auth-Sitzung), „Erledigt – Recherche fortsetzen".

### 02.09. 05:4x — Ursache 5 gefunden: nach Phase A gibt es KEINEN Agenten-Schritt (Owner-Frage „warum hört der Agent auf?")

- Messung Beiersdorf-Nachrecherche (01.09. 16:47 UTC): Business-Commands im Fenster = nur `sellify.lookup`,
  `research_source.auth_assist`, `web_stack.person_research` (completed). Kein `business_os.chat.task`, kein
  Queue-Task mit „Nachrecherche/Beiersdorf", keine Harness-Sitzung (context-log: 0 agent_messages mit
  „Beiersdorf"; Sitzungsarten nur mission=Repair-Worker und review). Lead → `needs_review` mit 8/32.
- Code: `web_stack.person_research` ∈ EXACT_CONTROL_TYPES → `person_research_command::start` (nativ,
  command_plane.rs:1223); `outbound_lead_generation_research_outcome_patch` setzt `completed`, wenn alle
  recherchierten Felder verifiziert sind, sonst `needs_review` — es gibt keinen Träger für Phase B/C
  (Websuche, Seiten öffnen, Belege, strukturiertes Nachtragen), obwohl der Owner-Prompt sie verlangt.
  Der „Agent" hört nicht auf, er beginnt nie. Die 17.08.-Threads „Nachrecherche: … Blockiert" stammen aus
  der früheren Chat-Variante, die seit dem Umbau auf den Steuerbefehl nicht mehr läuft.
- → **D7 Discovery-Panel** (Grok/Kimi/GLM, identischer Brief `briefs/D7-lueckenschluss-discovery.md`):
  verbindlicher Lückenschluss-Vertrag je Feld (`verified | no_match(mit Versuchen) | unsupported |
  action_required`), Träger = Queue-Task nach Phase A (Vorbild Repair-Task), Rückschreibkanal, Review-Gate,
  App-Anzeige. Danach konsolidierter Produktionsbrief R7 an Sol.

### 02.09. 05:53 UTC — B3 abgeschlossen; Nachher-Messung läuft

- Release **branch-main-20260902T052455Z** aktiv (27 min 43 s), Dienst aktiv, HTTP 200, Marker 15/2. Backup
  update-20260902T052512Z. Owner-Sitzung im Browser durch den Neustart beendet (erneuter Login nötig).
- Probe-Task „sandbox probe (after-b3)" (schreibt `sandbox-probe-after-b3.txt` ins Workspace) und ein frischer
  Heal-Lauf (Reparaturaufgabe bundesanzeiger-de) eingereiht; Wächter aktiv.

### 02.09. 06:0x — D7-Panel ausgewertet, R7a/R7b an Sol

- Drei Prototypen (Grok, Kimi, GLM) unabhängig, Konsens mit Datei:Zeile-Belegen: Träger = Queue-Task nach
  Phase A (Vorbild Repair-Task `execute.rs:352`), Lead bleibt `running` + `research_phase=gap_closure`; neuer
  EXACT_CONTROL_TYPE `outbound.lead.research_writeback`, der `outbound_lead_generation_research_outcome_patch`
  wiederverwendet (2-Quellen-Regel, person_key); Worker-Werkzeuge `ctox web search|read|browser-capture` und
  `business-os commands dispatch` sind ledger-frei und laufen ohne Relais (main.rs:357–414) — dank R5/R5b
  jetzt aus der Sandbox möglich; Review-Gate über Outcome-Witness (service.rs ~6846/12048) mit
  `gap_closure/field_status.json` + Writeback-Nachweis; `no_match` nur mit ≥1 Suche + ≥2 Lektüren als
  Artefakte; Turn-Budget 3600 s statt 180 s; Rework-Grenze; Guard je record_id; Idempotenz je Kommando.
  Difficulty: Träger 2, Werkzeuge 1–3, Rückschreiben 3–4, Vertrag 3–4, App 2.
- **R7a** (Kern: Träger, Status-Split, Prompt, Writeback-Befehl + Validierung, Guard/Recovery) und **R7b**
  (Service: Witness-Artefakte, 3600-s-Budget, Rework-Grenze 3) an Sol, Whitelists disjunkt, gemeinsamer
  Metadata-Schlüssel `person_research_gap_closure`. App-Anzeige (x/32, Feldstatus) folgt als A2 im App-Repo.

### 02.09. 06:2x — Nachher-Messung B3 (Datei-Beleg `sandbox-probe-after-b3.txt` aus dem Worker)

| CMD | Ergebnis im Worker (Landlock-Sandbox) |
|---|---|
| 3 `ctox-real status` | **EXIT 0**, JSON-Status → Binary ist aus dem Worker ausführbar (R5 wirkt) |
| 4 `getent hosts` | **EXIT 0**, `2001:aa8:…` → DNS funktioniert (R5 wirkt) |
| 5/6 resolv.conf, /run/systemd/resolve | lesbar |
| 7 `current/bin` | lesbar |
| 8 Socket | sichtbar |
| 9 curl bundesanzeiger | HTTP 302 → Netz + DNS ok |
| 10 `ctox-secrets.key` | **Permission denied** → Secrets bleiben geschützt (Symlink-Argument bestätigt) |
| 2/11 `/usr/local/bin/ctox …` | **EXIT 1**: Wrapper Zeile 11 `source ~/.config/ctox/business-os.env: Permission denied` |

- Neuer, letzter Blocker: der vom Installer generierte Wrapper prüft die Env-Dateien mit `-f` und sourced sie
  unter `set -e`; `~/.config/ctox` ist im Worker unlesbar → jeder `ctox`-Aufruf über den Wrapper stirbt, obwohl
  das Binary läuft. **R5c** (d6f8dff75, auf origin/main): Template testet `-r` und überspringt unlesbare
  Env-Dateien (der Daemon bekommt sie ohnehin über systemd `EnvironmentFile`). Auf thesen zusätzlich als
  Hotfix direkt im Wrapper gesetzt (Backup `~/.local/bin/ctox.bak-20260902`; der nächste Upgrade regeneriert
  den Wrapper aus dem gefixten Template). Erneuter Probe-Task „after-b3-hotfix" läuft.
- Duplikat `9499cee1…` storniert.
- KORREKTUR 06:38 UTC: der erste Wrapper-Hotfix (06:2x) lief NIE — das SSH-Skript brach vor dem Absetzen ab
  (JS-`String.raw` interpoliert `${HOME}` → „HOME is not defined"). Zweiter Versuch mit Plain-String: Wrapper
  zeigt jetzt `-r` in Zeile 8/14, Syntax ok, interaktiv funktionsfähig. Der Probe-Task „after-b3-hotfix" lief
  gegen den ALTEN Wrapper (ungültig); Bestätigungsprobe „after-hotfix2" eingereiht.
- 06:35 UTC: Cloudflare 525 (SSL handshake failed) im In-App-Browser für ~1 min; Dienst lief ohne Neustart
  weiter (NRestarts=0, lokal 200), von außen kurz darauf wieder 200 → transient am Edge/Ingress, nicht auf der VM.
- Reparaturaufgabe `6ace265f…` (alter Wrapper) endete `handled` ohne neue Revision — die Schleife bricht nicht
  mehr, der Worker kam nur mangels CLI nicht zur Registrierung; Negativkontrolle. Frische Reparaturaufgabe nach
  Hotfix eingereiht.

### 02.09. 06:4x — A1-Reproduktion Teil 2 (Owner erneut angemeldet, nach B3, Schreibpfad)

- **Import (IDB closing): NICHT reproduziert.** Freitext „KUKA Deutschland GmbH" → Vorschau 1 Lead → „Gültige
  Leads importieren" → nach 10 s: Dialog zu, 4. Zeile „KUKA Deutschland GmbH" in ABNAHME-E2E04, keine
  Fehler/Alerts, keine `IDBDatabase`-Konsolenfehler. Codex' Befund trat in einer lang laufenden Sitzung nach
  DB-Neuöffnung auf → bleibt als Robustheits-Karte (Import als dauerhafter Business-Command), nicht als
  reproduzierter Defekt.
- **Sellify-Kampagnensuche hängt: NICHT reproduziert.** „Welle" → nach ~20 s „40 Kampagnen gefunden" mit
  Mitgliederzahlen (Screenshot). Codex' Befund lag im Zeitfenster der `database is locked`-Vorfälle des
  Sellify-Intakes (31.08./01.09.) → bleibt als Intake-Karte (M-Backlog), nicht als App-Defekt.
- Live-Test „Neue Recherche" auf dem KUKA-Lead gestartet (R2/R3-Messung: Identität am Kommando, Besitzer der
  Auth-Sitzungen, Personen im Lead).

### 02.09. 06:4x — Live-Nachrecherche KUKA (Owner-Sitzung): Sellify-Weiche ok, Recherche läuft, Identität FEHLT weiterhin

- „Neue Recherche" auf KUKA → Weiche greift: „bereits in Sellify (contact_id 14644), nur Nachrecherche" (korrekt).
  „Nachrecherche" → Sellify-Lookup + `web_stack.person_research` per Command-Bus in 2,6 s `push_confirmed`,
  Crew-Chat öffnet, Zeile „Läuft"; nach ~90 s `completed`: **4 von 32 Feldern**, Quellen dnbhoovers/leadfeeder/
  sellify, **0 Personen**, 1 Quelle braucht Browser-Autorisierung → Lead `needs_review`.
- **R2/R4 fachlich NICHT wirksam:** das persistierte Kommando hat KEIN `actor`/`owner_user_id` im client_context,
  obwohl `native_authorization.actor.id = michael.welsch@…` (die Queue-Autorisierung kannte den Nutzer). Folge:
  Journal „auth assist owner unresolved source_module=ctox_harness task=KUKA Deutschland GmbH" und die
  RocketReach-Auth-Sitzung wieder unter `ctox_harness`, geblockt durch Budget 3/3 (drei alte Harness-Sitzungen).
- Journal: 5× „accepting business command … failed: a valid capability token is required" (06:43:18–23) für
  ReplicatedPeer-Zustellungen desselben Docs — der Kontext des Kommandos trägt kein `capability_token`; die
  Annahme kam über den Chat-/TrustedLocal-Pfad (Stempelung greift nur für ReplicatedPeer). Genau die Stelle,
  die R2 als „Builder nicht gefunden" ausgelassen hat. → **R8**: Chat-abgeleitete Steuerkommandos erben die
  verifizierte Chat-/Queue-Identität (dieselbe Quelle wie `native_authorization.actor`); Auth-Assist aus dem
  Recherche-Lauf muss den Besitzer aus dem Kommando lesen (R2-Kette), sonst Fallback nie `ctox_harness`, sondern
  Fehler mit Klartext.
- UI-Nebenbefund: der Hinweis-Dialog („nur Nachrecherche möglich") blieb nach OK-Klick per Skript stehen (zwei
  gestapelte Dialoge) — Karte A1.

### 02.09. 06:5x — Wurzel der verlorenen Identität gefunden (präzisiert R8)

- Die vier Auth-Assist-Kommandos des KUKA-Laufs (dnbhoovers 06:43:12, leadfeeder :33, xing :49, rocketreach
  geblockt) stammen ALLE aus dem CLI-Pfad `ctox business-os web-stack auth-assist-request` mit
  `requesting_task_id = "KUKA Deutschland GmbH"` und `source_module = ctox_harness`. Aufrufer ist das Harness-
  Werkzeug `ctox_web_auth_assist_request` (harness/core/src/tools/handlers/ctox_web.rs:314–328): der LLM-Agent
  im Business-Chat (`inbound_channel business_os.llm.chat`) setzt den Firmennamen als Task-ID, das Werkzeug
  reicht ihn 1:1 durch, hat selbst keinen Identitätskontext, und die Besitzer-Auflösung findet zu „KUKA
  Deutschland GmbH" weder Task-Link noch Kommando → `ctox_harness`. R2s Kette (person_research →
  `--owner-user-id`) deckt nur runtime-Scrape-Targets ab, nicht diesen Pfad.
- Das ausgeführte Kommando selbst kannte den Nutzer: `native_authorization.actor.id = michael.welsch@…`, und das
  RxDB-Doc trägt `client_context.actor` (via `recovered_from`), nur der Spiegel in `business_commands` nicht.
- Die drei neuen Harness-Sitzungen füllen das Budget 3/3; Budget je Nutzer im Runtime-Store auf 6 gesetzt
  (`runtime_env_kv CTOX_BROWSER_MAX_SESSIONS_PER_USER=6`), TTL-Reaping wird ab 06:58 UTC erwartet.
- **R8 (nächster Rust-Auftrag):** (a) Harness-Werkzeug übergibt IMMER die durable Bindung des Turns (Chat-/
  Thread-ID bzw. Queue-Task) als `--task-id` und, wenn bekannt, `--owner-user-id`; modellgelieferte Freitexte
  sind kein Task-Bezug; (b) Besitzer-Auflösung zusätzlich über Chat/Thread → `business_chats.owner_user_id`
  und über `native_authorization.actor.id` des referenzierten Kommandos (daemon-signiert); (c) Spiegel
  `business_commands.client_context` erhält die verifizierte Identität; (d) Fallback nie mehr stumm
  `ctox_harness`, sondern Auth-Assist mit `owner=unresolved` + Fehler an den Aufrufer.

### 02.09. 07:0x — Probe „after-hotfix2": `-r` reicht unter Landlock NICHT (negatives Ergebnis) → R5d

- `sandbox-probe-after-hotfix2.txt`: CMD 2/11 weiterhin „line 11: business-os.env: Permission denied".
  Ursache: bash `[[ -r ]]` prüft DAC-Bits (Datei gehört ctox, 0600 → lesbar), Landlock verweigert erst das
  `open()` → `source` scheitert unter `set -e`. Alles andere unverändert grün (Binary EXIT 0, DNS, Secrets
  verweigert).
- **R5d** (Template): `source … 2>/dev/null || true` für beide Env-Dateien; auf origin/main nach Compile.
  Tenant-Hotfix v2 mit demselben Muster gesetzt (Backup `ctox.bak2-20260902`), Probe „after-hotfix3" läuft.
- Umgebungsfalle: Prüfungen auf Lesbarkeit müssen den Zugriff VERSUCHEN; Permission-Bits sind unter LSM-
  Sandboxes bedeutungslos.

### 02.09. 07:1x — Owner-Frage „folgt der Agent dem Prompt mit dem Web-Stack?" — gemessen: NEIN

- KUKA-Chat (`chat_a016e766…`) enthält genau zwei Nachrichten: (1) `user`: der 8.685-Zeichen-Auftrag mit dem
  verbindlichen Rechercheablauf, (2) `ctox`: „Recherche für KUKA … abgeschlossen: 4 von 32 Feldern gefunden."
  Kein LLM-Turn, keine Websuche, keine Seitenlektüre, kein Browser durch einen Agenten; die Tool-Aufrufe im
  Zeitfenster stammen vom Repair-Worker (bundesanzeiger). Der Prompt wird dem Nutzer angezeigt, aber von
  niemandem ausgeführt (`research_instructions` wird nur der Länge nach persistiert). Adapter = einzige Phase.
- **KORREKTUR zu R8 (Fassung 3):** Die vier `ctox_harness`-Auth-Anfragen stammen NICHT vom Harness-Werkzeug,
  sondern aus dem nativen Capture-Pfad: `person_research_command.rs:1357` baut `source-capture … --task-id
  <FIRMENNAME>` ohne Besitzer, der Handler (`service/business_os.rs` ~2262–2290) reiht mit `"ctox_harness"`
  und `owner=None` ein (zweite Schleife in business_os.rs:3271). R8 v2 (Harness-Werkzeug als Hauptursache)
  gestoppt/verworfen, R8 v3 mit korrigierter Ursache gestartet (run …059e8552); Harness-Werkzeug bleibt als
  Härtung B enthalten.
- R7a geliefert (d335c04a9, 5 Dateien, +2236: neuer Steuerbefehl `outbound.lead.research_writeback`, Gap-Task
  nach Phase A, Feldvertrag, Guard, Recovery; 15 neue Tests) und auf `thesen-r7` über R7b gestapelt; Gate läuft.

### 02.09. 07:2x — Integrationsstand und Plan bis zum Nachweis

- Reparaturaufgabe `094332e8…` (bundesanzeiger, nach Hotfix v1) endete `handled` ohne Schleife, ohne neue
  Revision (Worker: kein echter Portal-Drift) — Negativ-/Regelkontrolle ok; der Beweis „CLI aus dem Worker über
  das Relais" kommt aus Probe „after-hotfix3" (CMD 11 → scrape_run-Zeile).
- Branch `thesen-r7` = origin/main 8448ad77f + R7b (66cec9266) + R7a (94d7b0792, Konflikt EXACT_CONTROL_TYPES
  gelöst: 75 Einträge inkl. `outbound.lead.research_writeback`). Gate (check + 21 Tests) läuft.
- R5d (Wrapper-Template) wird auf `thesen-r7` gestapelt statt separat gepusht (mein `pkill` hatte den R5d-Gate-
  Wrapper mit erwischt — Falle: `pkill -f "cargo check"` trifft auch die bash-Hülle mit demselben Text).
- Reihenfolge: R7-Gate grün → R8 v3 einsammeln, stapeln, Gate → EIN Push → **B4** (vierter Neustart) →
  Nachweis: (1) Probe im Worker: `ctox status`, `getent`, Relais-`scrape execute` erzeugt Lauf; (2) Owner-
  Nachrecherche auf KUKA/Beiersdorf: Lead bleibt „läuft/gap_closure", genau ein Task „Lückenschluss: …",
  Worker schreibt `gap_closure/field_status.json`, Rückschreibbefehl akzeptiert, Endstatus mit 32
  Feldzuständen; Auth-Sitzungen gehören michael.welsch@…; (3) Feldzahl vorher/nachher je Lead.

### 02.09. 07:12 UTC — URSACHE 1 BEWIESEN BEHOBEN: Worker führt `ctox` aus, Relais erzeugt echten Lauf

- Probe „after-hotfix3" (Datei `sandbox-probe-after-hotfix3.txt`, geschrieben vom Worker in der Landlock-Sandbox):
  CMD 2 `/usr/local/bin/ctox status` → JSON, EXIT 0 (Wrapper + Binary); CMD 3 `ctox-real status` → JSON;
  CMD 4 DNS ok; CMD 9 curl 302; CMD 10 Secrets `Permission denied` (geschützt);
  **CMD 11 `ctox scrape execute --target-key handelsregister-de …` aus dem Worker → `ok:true, status:succeeded,
  run_id scrape_run-d3000576405f541e, records_found 2`**; Lauf-Tabelle: 07:12:48 manual succeeded 2.
  Damit ist die Kette Worker-Sandbox → Wrapper → Binary → Daemon-Relais → nativer Scrape-Lauf erstmals
  durchgängig. Stand auf thesen: Release B3 + Wrapper-Hotfix v2 (Template-Fix R5d folgt mit B4).
- Offen bleibt der fachliche Nachweis, dass eine Reparaturaufgabe ein Skript registriert (Relais
  `register-script`) — nächste Gelegenheit: der erste Adapter mit echtem Portal-Drift nach B4.

### 02.09. 08:10 UTC — R7-Gate rot → zwei Befunde behoben; R8 v3 integriert; Identitätskette für Lückenschluss geschlossen

- **R7b-Gate (Log `/Volumes/tmp/thesen-ctox-r7b-gate.log`): 3 grün, 2 rot.** Ursachen (selbst verifiziert, Testbinary
  `/Volumes/tmp/ctox-check-target/debug/deps/ctox-d2d4e11076007a83`):
  1. `person_research_gap_witness_accepts_complete_status_and_writeback`: der Core-Guard
     (`src/core/core_state/guard.rs::load_artifact_terminal_state`) prüft jedes gelieferte Artefakt gegen dauerhaften
     Zustand; für den neuen Schlüssel `business-command:outbound.lead.research_writeback:<record>:<task>` kannte er
     nur `communication_messages` → `WP-Outcome-Missing`. **Fix:** Resolver für `business-command:`-Schlüssel, liest
     `business_commands` (Nachbar-DB `business-os.sqlite3`, read-only, gleiche Regel wie
     `person_research_gap_closure_writeback_exists`).
  2. `…fails_terminally_after_third_rejection`: Proof-IDs sind deterministisch aus dem Request; identische
     Ablehnungen kollabieren zu einer Zeile (Zähler blieb 1). Produktion variiert den Audit-Key je Runde
     (`vrun_…` mit `created_at`), der Test nicht. **Fix:** Test-Treiber mit explizitem Audit-Key je Runde.
- **R8 v3 (Run `local-2026-09-02T071046Z-059e8552…`, Patch `/Volumes/tmp/thesen-R8v3.patch`, 6 Dateien) auf
  `thesen-r7` angewendet.** Kern: nativer Capture-Pfad übergibt `--task-id <command_id>` + `--owner-user-id`
  (vorher Firmenname als Task-ID); `ctox_harness`-Fallbacks entfernt; Auth-Assist aus dem Harness nur noch mit
  gebundenem Command-Session-Token; Recovery persistiert `owner_user_id` in `business_commands.client_context_json`.
  Zwei Korrekturen von mir: let-chain in `resolve_ctox_binary` wiederhergestellt (Fork-Struktur), Owner-Prüfung vor
  Phase A statt danach (kein Adapter-Budget verbrennen, wenn der Befehl ohnehin abgelehnt wird).
- **Befund beim Review (nicht von Sol abgedeckt): der Lückenschluss-Task hatte keine Identität.** R7a legt den Task
  per `create_queue_task` an — ohne `business_command_task_links`-Zeile und ohne `business_os_command_id`.
  Folge: `source-capture --task-id <gap-task>` findet keinen Owner; der Harness bekommt keinen Session-Token
  (nur `business_os.chat.task` bekam einen). **Fix (direkt):** `channels::link_business_command_task` (neu,
  idempotent) + Metadatum `business_os_command_id` beim Anlegen; `configure_business_os_mcp_session_for_queue_job`
  stellt für Lückenschluss-Tasks einen Token mit `allowed_actions=[outbound-lead-generation/
  outbound.lead.research_writeback]` aus.
- **Gate 2 läuft:** `/Volumes/tmp/thesen-r7-gate2.sh` → Log `/Volumes/tmp/thesen-ctox-r7-gate.log`
  (Zielfilter `person_research_gap outcome_witness core_state`, danach volle `ctox`-Bin-Suite). Fertig =
  Zeile `=== full exit <code> …`. Danach: Commit auf `thesen-r7`, R5d cherry-pick, Push, B4.
- Umgebungsfalle (neu): **macOS hat kein `setsid`** — `setsid nohup … &` scheitert stumm; Gate 1 lief dadurch
  20 Minuten lang gar nicht. Hintergrundstart auf dem Mac: `(nohup script > log 2>&1 &)`.

### 02.09. 10:51 UTC — Gate 2 (Ziel-Tests): 79 grün / 5 rot → Ursache FK, behoben; Gate 3 läuft

- Harness-Crate nach Sols R8: `mcp_servers.get("…")` gegen `Constrained<HashMap>` → `get().get(..)`;
  `cargo check -p ctox-core` grün (120 min bei Load 30–76 durch fremde Builds).
- Ziel-Tests 10:51: 79 ok, 5 rot — alle in `person_research_gap_closure::tests`, Panik
  `FOREIGN KEY constraint failed`: `business_command_task_links.command_id` referenziert
  `business_command_aggregates`; in den R7a-Tests existiert der Recherche-Befehl dort nicht. **Fix:**
  `link_business_command_task` prüft Existenz und bestehende Bindung (Befehl ↔ höchstens ein Task), liefert
  `bool`; Lückenschluss-Ergebnis trägt `gap_closure.owner_linked`, Fehlbindung wird geloggt statt die
  Recherche zu kippen. Offen: kein Unit-Test für den positiven Pfad (braucht Aggregat-Seed) — Nachweis auf B4
  über `business_command_task_links` für den KUKA-Task.
- Gate 3 gestartet 10:52 UTC (gleiches Skript/Log). Fertig = `=== full exit`.

### 02.09. 11:40 UTC — Gate 3: 80/4 → Sols R7a-Fixture war nie grün; Push-Kandidat vorbereitet

- Gate 3 Ziel-Tests: 80 grün, 4 rot (`manual_rerun…`, `no_match_writeback…`, `verified_writeback…`,
  `writeback_rejects_wrong_gap_task_id…`), Panik „expected gap task". Ursache: `create_gap_fixture` baut den
  Recherche-Befehl OHNE `payload.writeback_contract`; `outbound_lead_generation_writeback_record_id` (unverändert seit
  origin/main) verlangt `collection` + `record_ids`. Die vier Tests waren in Sols R7a-Lieferung rot — der
  Completion-Receipt war eine Falschbehauptung (KORREKTUR zur Karte R7a). Produktion ist NICHT betroffen: die
  letzten drei `web_stack.person_research`-Befehle auf thesen (KUKA 06:44, Beiersdorf, Carbosulf) tragen den
  Vertrag mit `record_ids` (geprüft per `claude-payload-check.ts`). **Fix:** Fixture trägt den Vertrag.
- Branch-Stand committed: `thesen-r7` = 217ce4d64 (R7a+R7b+R8+Fixes). **Gate 4** gestartet 11:42 UTC auf
  diesem Stand (Log `/Volumes/tmp/thesen-ctox-r7-gate.log`).
- `origin/main` ist inzwischen c3e99712f (5 Commits: Web-Research-Fixes, Retry-Hold, Workjet-Transfer-Git).
  Push-Kandidat in Worktree `/Volumes/tmp/thesen-merge-wt`, Branch `thesen-r7-merge` = 217ce4d64 + Merge
  origin/main (konfliktfrei) + R5d (b0945e8c8). Nach grünem Gate 4: Ziel-Tests auf dem Merge-Stand (Compile
  ~40 min bei aktueller Last), dann Push, dann B4.

### 02.09. 12:10 UTC — Gate 4: 80/4 → R7a-Tests hatten nie eine RxDB-Tabelle; Gate 5 läuft

- Gate 4 Ziel-Tests: dieselben vier Tests rot, jetzt eine Ebene tiefer: „research writeback lead record does
  not exist". `create_gap_fixture` schreibt den Lead per `upsert_rxdb_collection_record`, aber im Test-Root
  existiert der RxDB-Store (`business-os-rxdb.sqlite3`) mit der Tabelle
  `ctox_business_os__outbound_lead_generation_leads__v*` nicht; der Writer überspringt den Upsert dann still
  (`RxdbCollectionWriter::open → None`). In Produktion legt der Browser-Peer die Tabellen an. Gegenprobe:
  auch Sols Test `recovery_enqueues_missing_gap_task_for_completed_phase_a_command` (person_research_command)
  ist rot — er lag außerhalb des Gate-Filters. Damit waren in R7a fünf Lead-Tests nie grün (KORREKTUR R7a).
- **Fix (testseitig, kein Produktionscode):** `seed_rxdb_collection_table_for_tests(root, collection)` legt die
  Tabelle nach dem bestehenden Muster aus `store_outbound_commands.rs` an; Fixture und Recovery-Test rufen sie.
  Gate-Filter erweitert auf `person_research` (deckt beide Module).
- **Gate 5** gestartet 12:14 UTC. Merge-Worktree `thesen-r7-merge` muss nach grünem Gate diese zwei
  Testdateien nachziehen (Commit auf `thesen-r7`, dann `git merge thesen-r7` im Worktree).

### 02.09. 14:40 UTC — Gate 6: 112 grün / 2 rot (1 vorbestehend, 1 Fixture) → Gate 7 auf dem Push-Kandidaten

- Gate 5 scheiterte am Modulpfad des neuen Test-Helfers (`super::` statt `crate::business_os::`); Gate 6
  (13:53–14:38 UTC, Filter `person_research outcome_witness core_state`): **112 grün, 2 rot**:
  `outbound_lead_generation_exposes_native_scoped_person_research` (vorbestehend auf origin/main, s. o.) und
  `recovery_enqueues_missing_gap_task_for_completed_phase_a_command` (Sols Recovery-Test ohne
  `writeback_contract` → Resolver liefert None → 0 statt 1; Fixture ergänzt). Die fünf R7a/R7b-Kernpfade
  (Lückenschluss-Task, Zeuge, Writeback, Abbruch, Guard) sind damit erstmals grün.
- Log-Verunreinigung erkannt: verwaiste Testbinaries abgebrochener Gates (PPID 1) schrieben weiter in dasselbe
  Log (ererbter FD) → scheinbare mcp_channel-/rxdb_peer-Ausfälle; maßgeblich ist nur die `test result`-Zeile.
  Drei Waisen beendet (3 h, 1 h 42, 2 h 14 Laufzeit); Merkregel gespeichert.
- Fixtures committed: `thesen-r7` = 31d7a2ca0. **Push-Kandidat `thesen-r7-merge` = 12b08eae1** (= 31d7a2ca0 +
  Merge origin/main c3e99712f, konfliktfrei + R5d b0945e8c8). `ctox-rustfix` steht jetzt auf diesem Stand.
- **Gate 7** gestartet 14:40 UTC auf 12b08eae1 (sauberer Baum). Grün = Ziel-Tests ≤ 1 rot (nur der
  vorbestehende MCP-Test) → Push nach origin/main → B4.

### 02.09. 15:49 UTC — Gate 7 GRÜN (113/1, nur der vorbestehende MCP-Test) → main erneut bewegt → Gate 8

- Gate 7 (Neustart 15:02 nach Sitzungs-Restart, der den Lauf um 14:40 mitgerissen hatte) auf 12b08eae1:
  **113 grün, 1 rot** = `outbound_lead_generation_exposes_native_scoped_person_research` (vorbestehend auf
  origin/main, unverändert). Damit erfüllt.
- Push-Versuch 15:48: `origin/main` war inzwischen 57dc87f39 (3 Commits: Importer-Standalone-Fix, Workjet
  Session-Transfer-Events #50, Web-Research-Messdaten #56; Rust nur `store.rs`/`store_workjet_sessions.rs`).
  Kein Fast-Forward → Merge konfliktfrei = **e46d3a1d5** (neuer Push-Kandidat, `ctox-rustfix` steht darauf).
- **Gate 8** gestartet 15:49 UTC auf e46d3a1d5 (gleiches Skript/Log). Regel: gepusht wird nur der exakt
  gemessene Stand; bewegt sich main erneut nur in JS-Dateien, wird nach Merge ohne weiteres Gate gepusht.
- Zeitplan (lokal): Gate 8 ~16:35 UTC → Push → B4 ~17:05 UTC aktiv → Nachrecherche ~17:10 UTC (19:10 Uhr).

### 02.09. 16:06 UTC — PUSH e46d3a1d5 → origin/main; Build B4 läuft auf thesen

- Gate 8 auf e46d3a1d5: 112 grün, 2 rot — der vorbestehende MCP-Test und
  `person_research_execute_is_idempotent_and_record_bound` (in Isolation 3/3 grün → Flackern im Parallellauf,
  keine Regression; als Backlog-Notiz „flaky" geführt).
- **Push 16:06 UTC:** `origin/main` 57dc87f39 → **e46d3a1d5** (Fast-Forward). Inhalt: R7a, R7b, R8 v3, R5d,
  Guard-Resolver, Task-Link/Token, Fixture-Fixes.
- **B4:** `ctox upgrade --dev` gestartet 16:06 UTC, Release `branch-main-20260902T160618Z` (Quelle validiert,
  Build läuft; ~27 min). Wächter: `claude-wait-upgrade-thesen.ts`. Nach Aktivierung: Nutzer meldet sich neu an.
- Workjet-Läufe markiert: R7a `…055758Z-07c9bb8a`, R7b `…055759Z-e176f415`, R8 v3 `…071046Z-059e8552` →
  integrated; R8 v1/v2 → abandoned.

### 02.09. 16:33–16:45 UTC — B4 AKTIV; drei Befunde, davon einer blockierend → B5 (Gate 9 läuft)

- **B4 aktiv 16:33 UTC**: `current_release = branch-main-20260902T160618Z`, HTTP 200, Binary trägt R1-Relais
  (15 Marker) und Live-Op-Marker. Vorher-Messung (`claude-lead-fields.ts`): KUKA 4 gefüllte Felder / 0 Kontakte
  / 0 Feldzustände; Beiersdorf 8 / 1 / 0.
- **Befund B4-1 (Wrapper):** `ctox upgrade` schreibt `~/.local/bin/ctox` mit dem Template des ALTEN Binaries
  (B3) — die R5d-Zeile fehlte wieder, obwohl das neue Binary sie enthält (`strings ctox-real` Zeile 14228).
  R5d greift erst beim nächsten Upgrade. Hotfix v2 erneut angewendet (16:38), Wrapper geprüft. Regel: nach
  jedem Upgrade Zeile 11 prüfen, bis ein Upgrade von einem R5d-Binary aus lief.
- **Befund B4-2 (Recovery-Flut):** Der Daemon legte beim Start 11 Lückenschluss-Tasks für alle historischen
  abgeschlossenen Phase-A-Befehle an (inkl. Testleads „CTOX Abnahme Chemie", „E2E04 Gueltig Eins"); Befehle
  ohne Owner-Identität. Backlog: Recovery nur für Befehle mit verifiziertem Owner und nicht für Testleads.
- **Befund B4-3 (BLOCKIEREND, eigener Fehler):** Mein `business_command_task_links`-Eintrag für den
  Lückenschluss-Task bindet ihn an einen bereits TERMINALEN Befehl. `command_saga.rs` (lease-1/F-002) wertet
  einen geleasten Task eines terminalen Befehls als verwaisten Lease und **setzt ihn ohne Ausführung auf
  `handled`** (mit Terminal-Grant → Zeuge umgangen); `queue cancel` scheitert mit „terminal command transition
  conflict". Genau das zeigte thesen: 11 Tasks leased, 0 Worker, nicht abbrechbar. Abhilfe live: Link-Zeilen
  per SQL entfernt, 11 Tasks per CLI abgebrochen (pending 1). **Fix B5 = 9cda2385f:** kein Link mehr;
  Owner-Auflösung folgt `business_os_command_id` aus den Task-Metadaten (`web_stack_auth_owner_from_task_metadata`),
  der MCP-Session-Token tat das bereits. KORREKTUR zur Karte 14:40 (Task-Link).
- **Gate 9** gestartet 16:44 UTC auf 9cda2385f. Danach Push → B5 → Nachrecherche. Nachweis auf B4 wäre
  wertlos gewesen (Task würde ohne Lauf „erledigt").
- Worker-Probe „after-b4" (Task `queue:system::55f500a3…`, Datei `sandbox-probe-after-b4.txt`): `ctox status` EXIT 0,
  DNS ok, Socket ok, Secrets verweigert, **Relais `scrape execute` → `scrape_run-704d540d51116635` succeeded, 2 Datensätze**.
  Ursache 1 auf B4 erneut bestätigt (mit handgepatchtem Wrapper).

### 02.09. 17:05 UTC — B5 gepusht OHNE lokales Gate (Nutzerentscheid: Tempo), Build läuft; Live-Wächter auf B4

- Nutzerfrage „warum immer ein Komplett-Compile": Rust-Binary → jede Verhaltensänderung braucht den
  Tenant-Build (~27 min, unvermeidbar); das lokale Gate (~45 min) ist Kontrolle, keine Voraussetzung. Für B5
  (kleiner, im Kern subtraktiver Fix) umgedreht: Push zuerst, Gate 9 läuft parallel als Nachkontrolle.
- `origin/main` war erneut bewegt (24920c9ff, Tenant-Smoke für importierte Apps). Merge konfliktfrei →
  **Push 8fc855795** (= B5 9cda2385f + main). **B5-Build** gestartet 17:05 UTC, Release
  `branch-main-20260902T170525Z`; Wächter `claude-wait-upgrade-thesen.ts`. Nach Aktivierung: Wrapper Zeile 11
  prüfen (B4-1), Nutzer meldet sich neu an, Nachrecherche KUKA + Beiersdorf.
- Live-Überbrückung auf B4: transiente User-Unit `gap-unlink-watch.service` (Skript `/tmp/gap-unlink-watch.py`,
  Log `/tmp/gap-unlink-watch.log`) entfernt neue `business_command_task_links`-Zeilen von Lückenschluss-Tasks
  binnen 0,5 s (nur Links ab 17:06 UTC). KORREKTUR: erste Fassung war zu breit und löste den Juli-Link des
  abgebrochenen Tasks „Nachrecherche Firma: WITTENSTEIN SE" (queue:system::753f32ca…) — wiederhergestellt.
  Unit nach B5 stoppen (`systemctl --user stop gap-unlink-watch`).
- Lehre (Umgebungsfalle): `nohup … &` und `setsid nohup … &` aus dem SSH-Helper heraus starten auf thesen keinen
  überlebenden Prozess (der Helper bricht bei `pkill`-Exit 1 ab bzw. reißt die Gruppe mit); `systemd-run --user
  --unit <name> --collect <cmd>` funktioniert zuverlässig.

### 02.09. 17:34 UTC — B5 AKTIV (`branch-main-20260902T170525Z`); Wrapper jetzt aus dem R5d-Template

- Gate 9 (Nachkontrolle, 9cda2385f): 113 grün / 1 rot (vorbestehend) — B5 bestätigt.
- B5 aktiv 17:34 UTC, `running True`, Queue leer (pending 0). Wrapper `/usr/local/bin/ctox` Zeilen 12/19 tragen
  `2>/dev/null || true` **aus dem Template** (Upgrade lief vom B4-Binary aus, das R5d enthielt) → B4-1 geschlossen,
  kein Handpatch mehr nötig. `gap-unlink-watch.service` gestoppt; keine Task-Links seit 16:00 UTC.
- Nächster Schritt: Nutzer meldet sich an → Nachrecherche KUKA (lead_1cfi6y6, vorher 4/0/0) und Beiersdorf
  (lead_1awj3nw, vorher 8/1/0) → Messung mit `claude-proof-b4.ts` + `claude-lead-fields.ts`.

### 02.09. 17:5x UTC — NACHRECHERCHE LIVE auf B5 (KUKA + Beiersdorf, Owner-Sitzung)

- Ausgelöst im In-App-Browser der Nutzer-Sitzung (App „Outbound Lead Generation" → Lead → „Nachrecherche").
  Befehle: KUKA `leadgen-lead-research-d6de4f9b-0394-44af-87f2-f8001217c6bb`, Beiersdorf
  `leadgen-lead-research-9e64c66d-493c-48b2-a07e-adb6eb0823c5`, beide `client_context` Owner =
  **michael.welsch@metric-space.ai** (Identitätskette R2/R4/R8 wirkt). Drei Auth-Assist-Requests
  (leadfeeder, xing, dnbhoovers; später rocketreach) ebenfalls mit diesem Owner — **kein `ctox_harness` mehr**.
- Phase A fertig → **genau ein Lückenschluss-Task je Lead**: KUKA `queue:system::0dd3c84df033431e0cfc07eb`,
  Beiersdorf `queue:system::28ced93c8dd694bb9804b1da`; Metadatum `business_os_command_id` = Recherche-Befehl,
  Workspace = Phase-A-Workspace, **keine `business_command_task_links`-Zeile** (B5 wirkt; die 4 neuen
  Link-Zeilen gehören zu Auth-Assist-Befehlen). Leads: `research_phase=gap_closure`, `gap_task_id` gesetzt,
  `research_status=needs_review` (Abweichung zur R7a-Beschreibung „running" — Beobachtung, kein Blocker).
- Worker war mit Auth-Assist-Task `dnbhoovers.com` belegt (wartet auf Login) → die 4 Auth-Assist-Tasks
  abgebrochen (OWNER-Thema Logins bleibt offen); Queue danach: nur die zwei Lückenschluss-Tasks, `busy False`.
- Läuft: Poller `claude-b5-run-poll.ts` (Task-Status, `gap_closure/field_status.json`, `research_writeback`).

### 02.09. 18:42 UTC — Befund B5-1: beide Lückenschluss-Worker nach 8 s tot → B6 gepusht, Build läuft

- Journal: `prompt worker start … Lückenschluss KUKA` 18:42:33 → `prompt worker end … error=Business OS command
  authorization permission changed` 18:42:41; Beiersdorf identisch (18:42:42 → 18:42:44). Beide Tasks
  `route_status=failed`, keine Workspace-Dateien, kein Turn. Leads bleiben `gap_closure` mit totem Task.
- Ursache (eigener Fehler, Edit D aus 10:xx UTC): der Session-Token für Lückenschluss-Tasks ruft
  `revalidate_business_command_execution_authorization` auf dem **Recherche-Befehl** auf; dessen
  Native-Authorization ist ein Control-Command-Receipt, `revalidate_queue_native_authorization` vergleicht aber
  gegen das Queue-Command-Permission-Ziel → „permission changed" → Job-Fehler vor dem ersten Turn.
- **Fix B6 = dd3edf92b:** Token nur noch für `business_os.chat.task` (ursprüngliches Verhalten); Lückenschluss-
  Tasks laufen ohne gebundene MCP-Session, Owner über Task-Metadaten. Folge: der Worker kann
  `ctox_web_auth_assist_request` nicht selbst auslösen → Login-Quellen enden `action_required` ohne
  Auth-Assist-Referenz. **Backlog B7:** rechercheb­efehls-taugliche Revalidierung + Token für Lückenschluss-Tasks.
- **Backlog B8:** Setup-Fehler vor dem ersten Turn setzen den Lückenschluss-Task sofort auf `failed` (kein
  Retry, keine 3-Runden-Logik, Lead bleibt „läuft"). Braucht Requeue oder Terminal-Disposition mit Lead-Status.
- Beobachtung: Suchmaschinen waren während Phase A rate-limitiert (google/brave „skipped after rate limit",
  duckduckgo/bing „low relevance") — Umgebungsfaktor für die Feldquote.
- Push 88ee71264 (B6 + main), **B6-Build** gestartet 19:27 UTC; Kontroll-Gate 10 lokal auf 88ee71264 gestartet.
  Nach B6: Nachrecherche KUKA + Beiersdorf erneut (neue Befehle → neue Tasks; die `failed`-Tasks bleiben als
  Beleg), dann Messung.

### 02.09. 18:40–20:25 UTC — KURSWECHSEL (Owner-Vorgabe): Button = ein Satz, Skill = das Wissen, Harness = die Arbeit, CLI = der einzige Weg

- **Zielbild (Owner):** Button „Nachrecherche" baut nur einen Prompt und öffnet den Chat. Der Harness bekommt den
  Task wie jeden anderen, mit Skill. Der Skill erklärt universell den Web-Stack (Browser inkl. Streaming-Unblocking
  mit Fortsetzung, Scraping-Skripte in SQLite) und je App Ein-/Ausgabe. CLI-Befehle wirken auf SQLite, Sync pusht in
  die UI. Kein MCP im Harness-Pfad, keine Sonder-Orchestrierung.
- **Ist-Soll-Befund (aus dem Code):** (1) App sendete `control_command: true` + `web_stack.person_research` → nativer
  Adapter-Stapel, kein Agenten-Turn — der Roman im Chat war die Nutzer-Nachricht der App. (2) Skill
  `outbound-lead-generation-research` existierte nirgends (App verlangte ihn); `prospect-research` (219 Zeilen) war da,
  aber ohne Outbound-Bezug und ungenutzt. (3) Kein CLI zum Record-Lesen; Weg ist `commands inspect <id>` auf den
  eigenen Auftrag. (4) Rückschreiben nur per `commands dispatch outbound.lead.research_writeback`, Handler verlangte
  Lückenschluss-Task. (5) Meine R7a/R7b/R8/B5/B6-Orchestrierung ist für diesen Weg unnötig (bleibt schlafend, wird nur
  von nativen `web_stack.person_research`-Befehlen ausgelöst, die die App nicht mehr sendet).
- **Umgesetzt:**
  - Skill `src/skills/system/research/outbound-lead-generation-research/SKILL.md` (neu): vollständiger CLI-Katalog
    aus den Usage-Texten (`ctox web …`, `ctox scrape …`, `ctox business-os web-stack …`, `commands inspect/dispatch`),
    SQLite-Tabellen (`scrape_target`, `scrape_script_revision`, `scrape_source_revision`, `scrape_run`,
    `scrape_record_latest`) und Arbeitsverzeichnis, 32 Felder, Quellenreihenfolge, Belegregeln, Unblocking mit
    Fortsetzung (`auth-assist-request` → `auth-assist-status --session-id` → `browser-automation --session-id` /
    `source-capture --session-id` / `context-capture`), Rückschreib-Payload exakt nach `ResearchWritebackRequest`.
  - Handler `handle_research_writeback`: `gap_task_id` optional; leer = Chat-Auftrag (`research_command_id` =
    `business_os.chat.task`), Feldsatz = gemeldete Felder (kanonisch geprüft), keine Phase-Vorbedingung;
    Queue-Task-Pfad unverändert. Lückenschluss-Task bekommt `suggested_skill` = der Skill. Commits 30fc209b4, 65b356562.
  - App 1.0.65 (thesen-apps 2258058): Prompt = `Starte eine Outbound Nachrecherche für <Firma> [<lead>] (Auftrag <cmd>).`,
    kein `control_command`, kein `command_type`, `payload.lead_snapshot` (data + contacts). **Deployt 20:24 UTC** nach
    `runtime/business-os/local-modules/outbound-lead-generation` (Backup `…before-1.0.65-20260902T202407Z`), live 1.0.65.
- **Gate 11** läuft (30fc209b4+65b356562, Filter person_research/outcome_witness/core_state). Danach Push → B7-Build
  (Skill ist im Binary eingebettet; ohne B7 findet der Harness den Skill nicht) → Nachrecherche → Zählung.
- Befund zu Owner-Fragen: kein Score für Scraping-Skripte in SQLite (nur `record-template-example --challenge-score`
  für Template-Beispiele); keine App-Zuordnung außer `scrape_target.target_kind` (`prospect-research`).
- KORREKTUR (eigener Fehler): „MCP-Rückschreibrechte" war falsch — MCP ist der Kanal externer Agenten; der Harness nutzt
  die CLI. `allowed_actions` in der App wieder entfernt.

### 02.09. 20:29 UTC — Gate 11 grün (113/1 vorbestehend) → Push 65b356562 → B7-Build läuft

- Gate 11 Ziel-Tests auf 65b356562: 113 grün, 1 rot (`outbound_lead_generation_exposes_native_scoped_person_research`,
  vorbestehend). Push 20:30 UTC (Fast-Forward 88ee71264 → 65b356562). **B7-Build** gestartet 20:31 UTC
  (`claude-upgrade-dev-thesen.ts 65b356562`, Wächter `claude-wait-upgrade-thesen.ts`).
- Backlog T1 (Triage, nicht blockierend): volle `ctox`-Bin-Suite auf dem B6-Stand unter Volllast: 2908 grün / 155 rot
  (Service-Loop-App-Recovery, Appsec-Pipeline, TUI-Settings, Runtime-Lifecycle, `merges_legacy_files_into_ctox_db`);
  auf ruhiger Maschine wiederholen und gegen origin/main vor heute abgrenzen. Log-Verunreinigung durch Waisen-Binaries
  erneut aufgetreten und beendet.
- Nach B7: Nutzer meldet sich an → Nachrecherche KUKA/Beiersdorf über App 1.0.65 → Chat zeigt nur den Satz →
  Agent mit Skill → `research_writeback` (Chat-Pfad) → Zählung mit `claude-lead-fields.ts`.

### 02.09. 21:12–21:19 UTC — ERSTER ECHTER AGENTENLAUF: KUKA 4 → 19 Felder, 32 Feldzustände, 5 Personen

- B7 aktiv (`branch-main-20260902T203007Z`), Skill `outbound-lead-generation-research` im Binary registriert
  (`ctox skills system show` → class `ctox_core`, state `stable`). App 1.0.65 im Browser geladen (Buster `…1.0.65`).
- Falle: Nach dem Upgrade stand die Instanz ~40 min im Wartungsmodus (`ctox-maintenance.sqlite3`, Phase
  `waiting_collections`, 96 %); Befehle wurden mit `CTOX_MAINTENANCE_READ_ONLY` abgelehnt. Ursache: Der Shell
  quittiert Bereitschaft erst, wenn die Collections der offenen Module initial repliziert sind; `business_chats`
  und `user_thread_states` standen auf `pending` (nicht Datenmenge: 0,7 bzw. 2,3 MB). **Ein Reload der Oberfläche
  löste es** (`phase=completed`, 21:11 UTC). Backlog M-1: hängende Erst-Replikation nach Peer-Neustart.
- **Lauf KUKA** (`leadgen-lead-research-3d466dcb…`, Chat-Task, Owner michael.welsch@…): Worker-Start 21:12:12 mit
  dem Ein-Satz-Prompt, Abschluss 21:19:09. Ergebnis am Lead `lead_1cfi6y6`:
  **19 gefüllte Felder (vorher 4), 32 Feldzustände (17 verified / 15 no_match), 5 Ansprechpartner (vorher 0),
  49 Belege aus 9 Hosts** (online-handelsregister 16, kuka.com 10, northdata 8, firmendata 5, myguide 4, sellify 2,
  dnbhoovers 2, leadfeeder 1, unternehmensverzeichnis 1). Werte inhaltlich plausibel (Anschrift Zugspitzstraße 140,
  86165 Augsburg; Domain kuka.com; frühere Firmierung „KUKA Roboter GmbH"; Geschäftsführung mit Bestelldaten).
  Keine Auth-Assist-Anfrage, kein Scrape-Lauf nötig.
- Rückschreiben brauchte 4 Versuche (21:15:53 / 21:16:35 / 21:16:43 fehlgeschlagen, 21:16:57 angenommen). Gründe aus
  der Projektion: (1) „invalid …research_writeback payload", (2) „verified result field `firma_geschaeftsfuehrung`
  value does not match field_status value", (3) „verified person result field `person_email` requires person_key".
  Der Agent hat sich selbst korrigiert; die drei Regeln stehen jetzt im Skill (Commit 7832befc1) → nächster Build.
- Schwachpunkt für die nächste Runde: alle 11 `person_*`-Felder stehen auf `no_match`, obwohl 5 Personen im Lead
  landeten (Feldstatus je Person vs. Personenliste). Skill-Ergänzung ist drin, Wirkung mit B8 messen.
- Beiersdorf-Lauf gestartet 21:19 UTC (Queue-Task „Nachrecherche: Beiersdorf Manufacturing Leipzig GmbH").

### 02.09. 21:40 UTC — Unblocking-Fortsetzung: existierte bereits, Identität ergänzt; Skill um Randfälle erweitert

- Befund (Code): `rxdb_peer_browser.rs` behandelt `web_stack.auth_assist.complete` (Knopf „Anmeldung bestätigt")
  bereits vollständig: `settle_auth_assist_queue_task` bricht die wartende Auth-Assist-Aufgabe ab,
  `resume_auth_assist_requesting_task` setzt den Recherche-Task fort — `pending` bei blocked/failed, sonst neue
  Aufgabe **„Fortsetzen: <Titel>"** im selben Thread, gleicher Workspace, gleicher Skill, parent = Originaltask.
  Damit ist das vom Owner beschriebene Modell vorhanden; es war nur nie erprobt.
- **Ergänzt (8ade4d738):** Die Fortsetzungsaufgabe trägt jetzt `business_os_command_id`, `business_os_module`
  und `business_os_record_id` aus dem Originaltask. Ohne sie hätte der fortgesetzte Lauf keinen Owner für weitere
  Browsersitzungen und keinen Bezug für den Rückschreibbefehl.
- **Skill erweitert** (jetzt 221 Zeilen): §8 „Unblocking across turns" (Turn nicht offenhalten, `action_required`
  mit `session_id` zurückschreiben, in der Fortsetzung nur offene Felder, danach wieder VOLLSTÄNDIGE 32
  Feldzustände, weil der Handler die Karte ersetzt) und §9 Randfall-Tabelle mit 17 realen Fällen (Rate-Limit,
  Blockade, fehlender Login, portal_drift, temporary_unreachable, kein Adapter, Quellenwiderspruch, veraltete
  Sellify-Person, Namensdubletten, Profil-URL als person_key, Tochter/Umfirmierung, inaktives Register, AT/CH,
  abgelehntes Rückschreiben, Budget-Ende, `no_match` ohne Beleg).
- **Qualitätsbeleg KUKA** (Chatantwort des Agenten, gekürzt): „17 Felder verifiziert, je ≥ 2 unabhängige Quellen —
  HRB 14914 Amtsgericht Augsburg, aktiv seit 02.02.1982, früherer Name KUKA Roboter GmbH; Zugspitzstraße 140,
  86165 Augsburg; Geschäftsführung mit Bestelldaten; 15 aktive Prokuristen, Dirk Busch zum 10.07.2026
  ausgeschieden. no_match mit Begründung: WZ-Code/Umsatz/Mitarbeiter verlangen Login (Bundesanzeiger, D&B,
  Leadfeeder), LinkedIn blockt (HTTP 999). Detail in research_summary.md im Workspace."
  Das ist erstmals Produktqualität statt Adapter-Rohdaten.
- Beiersdorf läuft seit 21:19 in Turn 3 (21:19–21:32, 21:33–21:36, ab 21:36); jeder Turn endet `ok`, ohne
  Rückschreiben — Beobachtung offen, Ursache noch nicht gemessen (Backlog B9).
- Gate 12 auf 8ade4d738 gestartet 21:36 UTC.

### 02.09. 21:42–21:50 UTC — Beiersdorf 8 → 17 Felder; Lücke „21 statt 32" geschlossen; B8 gepusht

- **Lauf Beiersdorf** (`leadgen-lead-research-7cd3942e…`): 3 Turns (21:19–21:32, 21:33–21:36, 21:36–21:42), jeder
  Turn endete `ok`, Rückschreiben im dritten: 3 Fehlversuche (21:40:50 / 21:41:11 / 21:41:49), angenommen 21:42:12.
  Ergebnis `lead_1awj3nw`: **17 gefüllte Felder (vorher 8), 21 Feldzustände (14 verified, 6 no_match,
  1 action_required — erstes `action_required` im Feldbetrieb: firma_telefon), 9 Kontakte.** Werte plausibel
  (Paul-Beiersdorf-Str. 2, 04356 Leipzig; frühere Firmierung „Beiersdorf Manufacturing Waldheim GmbH";
  Geschäftsführung mit Wechselhistorie; Tochter der Beiersdorf AG).
- **Befund B9 (eigene Lücke, behoben):** Im Chat-Pfad nahm der Handler die Feldliste aus dem, was der Worker
  meldete → 21 statt 32 Feldzustände wurden akzeptiert. **Fix 151652af3:** `research_command_requested_fields`
  liest `payload.fields` des Recherche-Befehls (die 32 aus der App); wer weniger meldet, wird abgewiesen.
  Skill-Regel entsprechend verschärft.
- Gate 13 auf 151652af3: 113 grün / 1 rot (vorbestehend). **Push 21:49 UTC** (65b356562 → 151652af3),
  **B8-Build** gestartet 21:50 UTC.
- Offene Datenqualität (Backlog B10): Im Beiersdorf-Lead stecken Altkontakte aus früheren Adapterläufen mit
  XING-Profil-URLs als `person_key`, dreimal „Frederic Heilmann" und einer fremden Firmen-E-Mail
  (heiko.fischer@daw.de). Der Skill verbietet URLs als person_key jetzt; Altbestand muss separat bereinigt werden.

### 02.09. 21:50–22:05 UTC — App-Agenten-Ablagen als Vertrag: Policy im Scraping-Bereich, Skill liest genau drei Orte

- **Owner-Vorgabe:** Der gepflegte Rechercheablauf darf nicht im Prompt landen und muss dort liegen, wo der Agent
  ihn auch ohne den ursprünglichen Auftrag findet; der Skill soll nicht „die App reflektieren", sondern nur die
  vorgegebenen Bereiche lesen, die jede App für Agenteninformationen anlegen kann.
- **Neuer typisierter Befehl `outbound.research_policy.publish`** (Rust, `store_outbound_commands.rs`): schreibt
  Ablauf und Einstellungen als Scrape-Target `<app-id>-policy` (`target_kind: app-policy`,
  `config.policy_contract: ctox.outbound.research_policy.v1`) über `scrape upsert-target`, zusätzlich als Record
  `outbound_research_policies`. Manifest unter `runtime/scraping/app-policy/<app>-policy.json`.
- **App 1.0.66 → 1.0.67 (beide deployt):** 1.0.66 schickt den Ablauf (Nachrecherche-Fassung, sonst Standard) plus
  `research_instructions_variant` im Payload; 1.0.67 veröffentlicht ihn beim Speichern zusätzlich in den
  Scraping-Bereich (`publishResearchPolicyToScrapeStore`). Prompt bleibt ein Satz.
- **Skill (268 Zeilen)**: §2a „Der App-Ablauf ist der Auftrag (Schritte 0..x)" — laden statt annehmen, Rangfolge
  über Quellen-/Feldreihenfolge, aber nie über Beleg- und Rückschreibregeln, jeder Schritt begründet abgehakt;
  §2b **„Agenten-Ablagen der App, fester Vertrag — genau diese drei lesen"**: (1) Auftrag per `commands inspect`,
  (2) `<app-id>-policy`-Target, (3) Quellen-Targets samt bereits gesammelter Datensätze. Ausdrücklich verboten:
  App-Quellcode, UI, beliebige Collections. Auftrag schlägt Policy-Target; fehlt beides, Standardreihenfolge plus
  Meldung. Schreiben ist Sache der App bzw. einer Adapteraufgabe.
- Gate 14 auf dem Stand mit dem neuen Befehl: 113 grün / 1 rot (vorbestehend). **Push eab452a0a** 22:05 UTC.
  B8 (151652af3) baute noch; B9 (eab452a0a) startet verkettet direkt danach (`claude-chain-build.ts`).

### 03.09. 04:00–06:20 UTC — App-Durchgang begonnen; fünf Fehlerklassen, drei behoben

Owner: „die app läuft mal so überhaupt nicht" / „du musst die app einmal systematisch durchgehen."

**Behoben und ausgerollt:**
- **1.0.70 Dialoge öffneten nie.** Die app-eigenen Dialoge (Quellen-Einstellungen, Zugangsdaten) hängten die
  Ebene korrekt in `ctx.host`, setzten aber nie `is-open`. CSS: ohne die Klasse `opacity: 0` UND
  `pointer-events: none` — unsichtbar, und jeder Klick/Tastendruck ging an das Fenster darunter. Genau das
  wirkte wie „Dialog öffnet unter dem Dialog" und „ich kann nichts eingeben".
- **1.0.71 Passwortmanager über der App.** `autocomplete="username"/"current-password"` luden macOS-Schlüsselbund,
  1Password, LastPass, Bitwarden ein. Jetzt Unterdrückungs-Marker; Zugangsdaten gehören in den Secret Store.
- **1.0.72 „48 Kontakte zurückgestellt".** Zustand statt Aufgabe: 48 Zeilen, Dubletten, Fremdfirmen, keine
  Handlung. Jetzt: eine Meldung mit Konsequenz und Knopf „Prüfung wiederholen" (leert die Zwischenspeicher),
  höchstens fünf gesperrte Kontakte mit Grund, Fremdfirmen als Datenfehler markiert.

**KORREKTUR zu „wo sind die Fixes hin":** Nichts überschrieben. Die Datei vor meinem ersten Deploy war
bytegleich mit der lokalen Fassung (`2b522835037de9fa`, 368496 B). Der Repo-Verlauf beginnt allerdings erst mit
der Momentaufnahme vom 31.08.; frühere Direktänderungen auf der Instanz wären schon vorher verloren gewesen.

**Bestandsaufnahme (Skript über index.js):** 52 `data-action`-Werte, keine toten Knöpfe (`lead-sort` ist ein
Select), 4 Dialogstellen, 19 Render-Funktionen, 48 Meldungsstellen — davon **9 ohne Handlungsangabe**, 12
Zustands-Etiketten.

**Live-Audit der vier Lead-Ansichten (KUKA):**
| Ansicht | „fehlt/offen"-Zeilen | Dubletten |
| --- | --- | --- |
| Übersicht | 15 | „zu prüfen" ×5 |
| Unternehmen | 2 | Firmenname ×3, „3 Quellen" ×3 |
| Personen | 19 | „Geschäftsführung" ×5, „0 Quellen" ×7 |
| Einordnung | 15 | „eintragen" ×15 |

**Offene Owner-Befunde (Reihenfolge noch zu bestätigen):** Personen als Reiter mit Detailbereich statt Liste;
Adapter-Skript ist in der App nicht lesbar (nativer Lesepfad fehlt); Befehlskanal reißt unter Last ab und
erscheint als „Sellify-Abgleich fehlgeschlagen"/„business_commands was cancelled"; Shell landet nach Reload
im Wiederherstellungsbildschirm; 9 Meldungen ohne Handlung.

**Kampagne pausiert** (03.09. 06:05 UTC): Die 18 Agentenläufe machten die Instanz für die Bedienung unbrauchbar.
Restliche Aufträge abgebrochen, ein Lauf läuft aus. Zwischenstand: 210 Felder (vorher 146), 256 Feldzustände
(vorher 21), 48 Kontakte (vorher 29); 8 Leads mit vollen 32 Feldzuständen. Neustart nach dem UI-Durchgang.

## Working

| Karte | Worker / Log | Fertig heißt |
|---|---|---|
| R1 CLI-Relais: `scrape register-script/register-source-module/execute` + `continuity-update` laufen aus dem Worker über den Daemon-IPC (Muster `knowledge`), Ledger-Skip, Fallback ohne Daemon | Sol (Start nach Board-Commit) | Patch importiert, `cargo check` + Tests grün im Vollklon, Repair-Task auf thesen endet `handled` mit registrierter Revision |
| R2 Auth-Identität + Sitzungs-TTL: Chat-Steuerkommandos tragen den Nutzer als actor; Recherche→scrape execute→Reauth-Handoff reichen `--owner-user-id` durch; Owner-Fallback über Thread/Chat statt `source_module`; Idle-TTL für `web_stack_auth`-Sitzungen | Sol | Auth-Sitzung aus einem Recherche-Lauf gehört `michael.welsch@…`; nach TTL frei; kein `_ctox_harness`/`_scrape_executor` mehr |
| R3 Personen-Vertrag: `person_priorities`, `known_person_records`, `research_instructions` nativ; öffentliche person_*-Treffer je Profil-URL zu `person_records` gruppiert; Sellify-Personen führend; Rollen-Validierung; Priorisierung | Sol | Beiersdorf-Fixture: 8 person_records, „Leipzig" keine Funktion, Hahn/Gund erhalten |

| Review R1–R3 (Kimi · Cyber, nur lesen): Privilegiengrenze des IPC-Relais (Pfadargumente), Impersonation über client-gelieferte `actor.id`, Owner-Env an Kindprozess, TTL-Race, R3-Datenhoheit | Kimi Cyber 1, run local-2026-09-01T230523Z-bb436fd9-0db0-49fe-b6a7-f6f4be2469cd; Review-Stand: Launchpad-Branch `integrated` 5366f5a = ctox-rustfix 69814adee | Befunde nach Schweregrad; kritische/hohe vor B1 fixen |

| R4 Härtung (Sol): K2 Pfad-Sanitisierung + Strip `--runtime-root/--db`, H1 IPC pro Verbindung im Thread + Timeout-Klemme 600 s + M1 Größenlimits, H2 verifizierte Identität IMMER für ReplicatedPeer (`claimed_actor` für Audit) + Owner-Flag/Env nur bei Übereinstimmung/TrustedLocal, M4/M5 Kontakt-IDs + Zwei-Signal-Merge, M6.3 Timestamp-Klemme, N1 Logs | Sol, Brief `briefs/R4-hardening.md`, Basis Launchpad-Branch `integrated` 5366f5a | 7 Tests grün im Vollklon, dann Push main + B1 |

| R8 Identitätskette (Sol): Harness-Werkzeug `ctox_web_auth_assist_request` übergibt die durable Bindung des Turns (Command-Session-Token bzw. Owner/Chat-ID aus der Thread-Konfiguration), nie den Modelltext; daemon-seitige Auflösung über Session-Token, Chat-Besitzer, `native_authorization.actor`; kein stummer `ctox_harness`-Fallback; Spiegel bekommt actor | Sol, Brief `briefs/R8-auth-identity-chain.md`, run local-2026-09-02T065826Z-af20d602 | Auth-Sitzung aus einer Owner-Nachrecherche gehört dem Owner; ohne Bindung Fehler statt Harness-Sitzung |
| R7b (Service: Witness/Budget/Rework) geliefert (8383d0d), auf origin/main-Basis angewendet (66cec9266, Branch `thesen-r7`); Gate läuft. R7a (Kern) läuft noch bei Sol. | Sol | R7a+R7b zusammen gepusht, Build B4 |

Sol-Kontingent: 2/3 belegt (R7a, R8). B3 abgeschlossen, Owner angemeldet.

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

- R6 (Harness): Das Review-Gate führt Worker-Shell-Befehle im Reviewer-Profil (voller Lesezugriff) erneut aus
  und wertet Landlock-Verweigerungen des Workers als „fabricated" → endlose Rework-Schleifen. Fix: Nachlauf im
  Worker-Profil oder EACCES/Denials als Umgebungsbeleg klassifizieren; nach R5/B2 messen, wie viele
  Reparaturaufgaben noch drehen.
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
