# Auftrag CREW-COCKPIT · PR-1 „Harness-Cockpit Foundation“ und PR-2 „Crew-Identität im Harness“

Auftraggeber: Fable (Orchestrator). Implementierer: Codex-Thread `01a07107-d27c-7e80-a1dc-2311f60ad0bb`.
Lies in dieser Reihenfolge, bevor du beginnst: `docs/dev/crew-cockpit-vision.md` (wofür wir bauen: das Zuhause der Crew, die Wesen, die visuelle Sprache), dann `docs/dev/crew-cockpit-board-20260905.md` (Befunde, Zielbild, Umgebungsfallen). Rollen: Fable orchestriert und reviewt, du setzt um und meldest Abweichungen im PR-Text statt eigene Ziele zu setzen.

## 0. Rolle, Ziel und Arbeitsweise

Du baust den Kern von CTOX Business OS um: Der Harness soll für den Browser sichtbar und steuerbar werden, und die Crew-Mitglieder werden echte, persistente Personen mit Seele, Lebenslauf, Learnings und Stundenzettel, die nach Passung für Tasks ausgewählt werden. Der Harness bleibt **seriell**, alle Review- und Validierungs-Gates bleiben **unverändert**. Nichts davon ist eine Kürzung der Sicherheit, nur Sichtbarkeit, Steuerung und Identität kommen hinzu.

Das Programm hat fünf PRs. Dieser Auftrag umfasst PR-1 und PR-2 (beide Server/Rust plus Wire-Contracts). PR-3 (Cockpit-App), PR-4 (Crew-Leiste), PR-5 (Tickets) folgen als eigene Aufträge, sobald PR-1/PR-2 stehen. Du bist der Fachmann für die Umsetzung; entscheide Details selbst, halte aber die hier gesetzten Verträge (Feldnamen, Collections, Command-Namen, Policy-Scopes) ein, weil die drei Oberflächen genau darauf bauen. Wo du von einer Vorgabe abweichen musst, tu es und begründe es im PR-Text; frage nicht zurück.

Arbeitsweise (verbindlich):

- Basis ist `origin/main` von `git@github.com:metric-space-ai/ctox.git`. Lege je PR einen Worktree unter `/Volumes/tmp/worktrees/ctox/<branch>` an (`git fetch origin && git worktree add -b <branch> /Volumes/tmp/worktrees/ctox/<branch> origin/main`). Der aufrufende Checkout `~/Documents/ctox` ist nur Lesequelle für die beiden Dokumente in `docs/`; ändere dort nichts. Er liegt 137 Commits hinter und 233 vor `origin/main`, deshalb nicht davon abzweigen.
- Branch-Namen: `crew-cockpit/pr1-harness-foundation`, `crew-cockpit/pr2-crew-identity`. PR-2 zweigt von PR-1 ab (stacked), solange PR-1 nicht gemerged ist; nenne das im PR-Text.
- Commits in grünen Scheiben, spätestens alle 45 Minuten, damit ein Abbruch nichts vernichtet. Commit-Betreff: `crew-cockpit(pr1): <was>`.
- Baue mit einem eigenen Cargo-Target. Prüfe vorher `df -h /Volumes/tmp`; sind dort weniger als 25 GiB frei, nimm `CARGO_TARGET_DIR=$HOME/.cache/ctox-crew-cockpit-target`. Lösche das Target am Ende jedes PRs (es sind rund 13 GiB je Testbau). `TMPDIR` auf einen Pfad unter `/Volumes/tmp/dev-artifacts/ctox/crew-cockpit/` setzen; Unix-Socket-Tests einmal zusätzlich mit einem über 80 Byte langen `TMPDIR` laufen lassen.
- Formatieren nur je Datei (`rustfmt <datei>`), niemals paketweit `cargo fmt`. Keine neuen Prozess-Umgebungs-Toggles für Laufzeitverhalten; Konfiguration geht in typisierte Config, den SQLite-Runtime-Store oder den Secret-Store (siehe `AGENTS.md`). Guard-Tests niemals schwächen; ein roter Guard ist ein Befund.
- Wire-Contracts: `src/core/rxdb/tests/fixtures/*.json` ändern, beide Seiten regenerieren, Consumer neu bauen; `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs` nie von Hand patchen, sondern mit dem in `docs/ctox-rxdb.md` gepinnten esbuild-Kommando bauen und den einzigen `?v=`-Buster in `shared/rxdb-runtime.js` bumpen. Kein HTTP-Datenpfad zwischen Browser und CTOX: alles Neue sind RxDB-Projektionen und typisierte Business-Commands.
- PR über `gh pr create` gegen `main` mit dem PR-Text nach Abschnitt 4. Du mergst nicht selbst. Nach jedem PR: Abschlussbericht nach Abschnitt 4 ausgeben und anhalten, bis der nächste Auftrag kommt.
- Lege `docs/dev/crew-cockpit-board-20260905.md` und diese Datei unverändert in PR-1 unter `docs/dev/` ab, damit sie in `main` landen.

## 1. Zielbild in einem Absatz

Der Browser bekommt vier Wahrheiten über den Harness, alle als RxDB-Projektionen mit Retention und Indizes: **den Task** mit echtem Zustand samt Grund (Lease, Wartegrund, Retry-Fenster, Fehlerklasse), **das Live-Geschehen** (Plan-Schritte, Tool-Aufrufe, Denk-Turns, Tokens) je Task, **die Runs** (je Versuch: Modell, Tokens, Kosten, Dauer, Urteil) und **den Harness-Status** (läuft, pausiert, Kapazität, Druck, wer arbeitet). Dazu kommen Steuerbefehle als typisierte Control-Commands hinter der bestehenden Policy. In PR-2 kommt die Crew: Mitglieder als Entität, Auswahl nach Passung beim Lease, Seele im Prompt, Learnings und Stundenzettel aus den Runs.

## 2. PR-1 · Harness-Cockpit Foundation

**Objective:** Alles, was der Harness heute schon durable weiß, erreicht den Browser als Projektion, und der Browser kann den Harness über Control-Commands steuern. Keine UI-Änderung in diesem PR.

### 2a · `ctox_queue_tasks` vervollständigen

`QueueTaskView` (`src/core/mission/channels/mod.rs:281-299`) und `enrich_queue_projection_payload` (`src/core/business_os/store_projections.rs:829`) um die bereits persistierten Spalten aus `communication_routing_state` erweitern: `lease_expires_at`, `lease_worker_id`, `first_pending_at`, `failure_class`, `failure_attempt_count`, `retry_not_before`, `hold_reason`, `wait_entity_type`, `wait_entity_id`, `priority_time_credit_hours`, außerdem `attempt` (aktueller Versuchszähler) und `crew_member_id` (in PR-1 immer `null`, Feld schon anlegen). Beim Löschen eines Werts explizit `null` schreiben (wie heute bei `lease_owner`, `:869-880`). Schema in `src/apps/business-os/modules/ctox/collections.schema.json`, `business_os_schema_contract.json` und den RxDB-Fixtures nachziehen; `execution_progress` dabei vom `additionalProperties`-Schattenfeld zum vertraglich deklarierten Feld machen (auch auf `business_commands`).

### 2b · Retention für `ctox_queue_tasks`

Heute unbegrenzt (lokal 946 Zeilen, jede repliziert an jeden Browser). Regel: alle nicht-terminalen Tasks bleiben; von den terminalen (`handled`, `failed`, `cancelled`) bleiben die jüngsten N nach `updated_at_ms`, Standard N=300, konfigurierbar als typisierter Runtime-Store-Schlüssel `business_os.projection.queue_tasks_retention`. Ältere werden als Tombstone entfernt (bestehender Sweep in `rxdb_peer_tombstones.rs`). `repair_queue_projections` (`store_projections.rs:1017`) respektiert die Regel und arbeitet mit `LIMIT`, nicht als Vollscan.

### 2c · Neue Collection `ctox_harness_events`

Per-Turn-/Per-Tool-Ereignisse sind durable in `ctox_harness_flow_events` (Writer `src/core/service/service.rs:5292-5305`), erreichen den Browser aber nicht. Schreibe an derselben Stelle zusätzlich eine Projektion über den synchronen RxDB-Pfad (`upsert_rxdb_collection_record_cached`, wie `ctox_queue_tasks` in `store_projections.rs:655-668`):

- Felder: `id` (= event_id), `task_id` (= message_key), `command_id`, `attempt`, `kind` (`tool_started|tool_completed|thinking|plan_updated|token_usage|turn_completed|phase|crew_selected`), `title`, `tool_type`, `tool_name`, `call_id`, `success`, `usage {input,output,reasoning,total}`, `runtime_seconds`, `step_position`, `created_at_ms`, `updated_at_ms`.
- Nur für Tasks schreiben, die zum Zeitpunkt des Ereignisses nicht terminal sind.
- Retention: höchstens 200 Ereignisse je `task_id` (älteste weichen); Ereignisse terminaler Tasks werden 24 h nach dem terminalen Übergang getombstoned. Indizes `[task_id, created_at_ms]` und `created_at_ms`.
- Policy: lesbar für Admin und Founder; nicht für User. Dokumentiere die Entscheidung in `docs/ctox-rxdb.md`.

### 2d · Neue Singleton-Collection `ctox_harness_status`

Ein Dokument `id = "harness"`, ereignisgetrieben geschrieben aus den Worker-Start/-Stop-Hooks (`service.rs` um 2440, 2459, 4684, 4816) und aus Queue-Übergängen, **nicht** über die stamp-gated 3-s/1800-s-Schleife von `ctox_runtime_settings` (deren Stempel ignoriert die Core-DB, deshalb ist heute nichts live). Felder: `service_running`, `busy`, `paused`, `pause_reason`, `worker_active_count`, `worker_phase`, `worker_capacity`, `pending_count`, `leased_count`, `blocked_count`, `review_count`, `failed_recent_count` (24 h), `pressure_active`, `pressure_threshold` (heute Konstante 20, `service.rs:159`), `work_hours {enabled,start,end,inside_window}`, `active_task_ids[]`, `active_crew_member_id` (PR-1: `null`), `last_error`, `boot_id`, `updated_at_ms`. Policy wie 2c. Der Schreibpfad darf den Worker nicht blockieren (lossy, wie `record_harness_flow_event_lossy`).

### 2e · `ctox_runs` füllen

Die Collection existiert seit langem ohne Writer. Schreibe eine Zeile je Versuchsabschluss aus `worker_attempt_finalizations` (`src/core/context/lcm/mod.rs:1268-1294`), verknüpft mit `api_model_cost_events` (`src/core/api_costs.rs:1066-1088`) über `turn_id`: `id` (= attempt_id), `task_id`, `command_id`, `work_id`, `crew_member_id` (PR-1: `null`), `status` (`succeeded|failed|timed_out|aborted`), `agent_outcome`, `started_at_ms`, `finished_at_ms`, `metrics {model, provider, input_tokens, output_tokens, reasoning_tokens, cost_usd, tool_calls, thinking_turns, elapsed_ms}`, `review {disposition, hold_reason}`, `error_text`, `resumable`, `retrospective` (PR-1: `null`), `updated_at_ms`. Indizes `[task_id, finished_at_ms]`, `finished_at_ms`, `crew_member_id`. Retention: jüngste 500 Runs plus alle Runs nicht-terminaler Tasks. Wenn `worker_attempt_finalizations` für einen Attempt die Kostenzeile noch nicht hat, Run trotzdem schreiben und nachträglich aktualisieren.

### 2f · Control-Commands

Neue Arme in `EXACT_CONTROL_TYPES` (`src/core/business_os/command_plane.rs:265`, Dispatch um `:1000`) unter `BusinessOsPermission::CtoxTaskManage`, jeder Arm läuft durch `enforce_command_policy` bevor irgendetwas passiert, und jeder schreibt ein Harness-Flow-Ereignis mit Akteur:

- `ctox.queue.release {task_id, priority?, note?}` → bestehende Release-Logik (`queue.rs:40`).
- `ctox.queue.block {task_id, reason}` → bestehende Block-Logik (`queue.rs:41`).
- `ctox.queue.retry {task_id}` → für `failed`/`blocked`: Fehlerzähler und Retry-Fenster zurücksetzen, Task wieder `pending`.
- `ctox.queue.capacity {workers}` (1–8) → `configure_queue_worker_capacity` (`service_queue_capacity.rs:23-33`).
- `ctox.queue.pause {paused, reason?}` → weicher Pausenschalter: der Router leased keine neuen Tasks mehr, die laufende Slice läuft zu Ende, Zustand persistent im Runtime-Store, sichtbar in `ctox_harness_status.paused`. Unabhängig von den Arbeitszeiten.
- `ctox.queue.abort_turn {task_id}` (Stretch): bricht die laufende Harness-Session der aktuellen Lease ab, markiert den Attempt `aborted`, setzt den Task auf `blocked` mit `hold_reason = "aborted_by_owner"`. Wenn ein sauberer Abbruch in diesem PR nicht sicher machbar ist (Prozess-/Session-Ownership prüfen), baue **keine** halbe Lösung: dokumentiere im PR-Text genau, was fehlt, und lege den Command-Arm an, der `unsupported` mit Begründung zurückgibt.

`ctox.command.cancel` und `ctox.task.update/delete` bleiben wie sie sind.

### 2g · Chat-Vertrag: Zwischenstände statt Stille

Heute erreicht den Chat nur der terminale `outbound_text`. Live-Befund vom 05.09.: Ein Command endete in `execution_phase=retry_wait` mit vorhandenem `result.user_message`, der Chat zeigte nichts. Ergänze im Server (Autor ist der Server, `store_projections.rs:168-268`): eine Nachricht bei Lease-Übernahme, bei jeder Plan-Revision (aktueller Schritt), bei `retry_wait`/`blocked`/Review-Ablehnung (Grund und was als Nächstes passiert) und den vorhandenen `user_message` als „Zwischenstand“, sobald er vorliegt. Nachrichten tragen `kind` (`status|interim|reply|question`), `task_id`, `command_id`, `run_id`. Sprache folgt der Chat-Sprache (vorhandene Auflösung nutzen; Fallback Deutsch); die deutschen Literale in Rust (`store_projections.rs:253`) durch eine zweisprachige Tabelle ersetzen. Kappe die Nachrichtenliste weiterhin (heute 40), aber bevorzuge beim Kappen das Löschen alter `status`-Nachrichten vor `reply`.

### 2h · Nicht-Ziele PR-1

Keine UI-Änderung. Keine Crew-Identität (nur die `null`-Felder). Keine Ticket-Änderung. Keine HTTP-Endpunkte. Keine Änderung an Review-Gates, Serialität, Kapazitätslogik über 2f hinaus.

### 2i · Abnahme PR-1 (du führst alles aus und legst die Ausgaben in den PR-Text)

- `cargo check` für das Hauptpaket; gezielte `cargo test` für die geänderten Module (mindestens: `business_os::store_projections`, `business_os::command_plane`, `mission::channels`, `service` Queue-Kapazität/Pause, `context::lcm` Finalisierung/Runs, `service::harness_flow`), zusätzlich die vorhandenen Guard-Tests der Projektionen und `cargo run -- process-mining spawn-liveness`, falls du Worker-Hooks anfasst.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml`, `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`, `node src/apps/business-os/rxdb/tests/run-all.mjs`.
- Neue Tests, mindestens: Projektion enthält die neuen Felder und schreibt `null` beim Löschen; Retention hält die Grenzen (Tasks, Events, Runs); `ctox_harness_status` wird bei Worker-Start und -Stop geschrieben; Policy verweigert jeden neuen Control-Command für Rolle User und erlaubt ihn für Admin; Pause verhindert neue Leases, lässt die laufende Slice enden; `ctox_runs`-Zeile entsteht nach Finalisierung mit Kostenfeldern; Chat erhält Zwischenstand bei `retry_wait` mit `user_message`.
- `ctox business-os repair queue-projections --dry-run` gegen einen Test-Root läuft ohne Vollscan.
- Doku: `docs/ctox-rxdb.md` (neue Collections, Felder, Policy, Retention), `HARNESS.md` Abschnitt „Cockpit-Projektionen und Steuerbefehle“ mit Verweis auf die persistierten Quellen.

## 2j · Lehren aus PR-1, verbindlich für PR-2 und alle weiteren PRs

Aus Review und Abnahme von PR #58 (05.09.2026):

1. **Nicht-blockierend heißt der ganze Pfad.** Kein synchroner SQLite-Read oder -Write im Harness-Turn-Pfad, auch nicht „nur ein kleiner Lookup“. Werte kommen aus dem, was der Worker schon in der Hand hat, oder werden auf dem Projektions-Pump aufgelöst.
2. **Unbekannt ist nicht „nein“ und nicht „null“.** Ein Fehler beim Ermitteln eines Flags darf das Flag nicht auf `false` setzen (Ereignis verschwindet), ein Fehler beim Parsen einer Konfiguration darf den Harness nicht anhalten, und ein unparsebarer Zeitwert darf nie `null` in ein `required`- oder Index-Feld schreiben. Regel: Dokument nicht projizieren und einmal loggen, oder auf den kanonischen Zeitstempel zurückfallen.
3. **Jede Behauptung über Alttests braucht eine Baseline.** „Betroffene Pfade unverändert“ zählt nicht; gemessen wird derselbe Test `--exact` auf Basis-Commit und Head mit demselben Target. Ergebnis in den PR-Text.
4. **Gebundene Abfragen überall,** nicht nur im Repair-Pfad: Keyset-Paging, echte `LIMIT`s, keine `LIMIT (SELECT COUNT(*))`-Attrappen. Indizes werden per `EXPLAIN QUERY PLAN`-Test bewiesen, nicht deklariert.
5. **Grant-Materialisierung folgt der Policy.** Jede Collection, die die Policy als server-autoritativ oder rollenbeschränkt einstuft, darf von keiner Migration Standard-Grants bekommen; Test: nach Bring-up null Grants für sie.
6. **Audit spiegelt nie rohe Payloads.** Whitelist der Felder, Kappung freier Texte auf 1000 Zeichen.
7. **Umgebung:** `/Volumes/tmp` ist knapp; `TMPDIR` und Cargo-Targets auf die Systemplatte (`~/.cache/ctox-crew-cockpit-*`), das Target erst löschen, wenn Fable es freigibt. Das Pi-Sidecar-Bundle (`npm ci && npm run build` in `src/core/coding_agents/pi-sidecar`) ist Voraussetzung für jeden `cargo build` in einem frischen Worktree.
8. **Zwischenmeldungen:** Beim Start jedes Auftrags in zwei Sätzen bestätigen, wofür gebaut wird (Vision) und an welchem Abschnitt gearbeitet wird; Queue-Nachrichten von Fable werden zwischen Schritten zugestellt und haben Vorrang vor eigenem Plan.
9. **Nichts löschen, was nicht deins ist;** fremde Cargo-Targets und Verzeichnisse auf `/Volumes/tmp` bleiben unangetastet.

## 3. PR-2 · Crew-Identität im Harness

**Objective:** Crew-Mitglieder werden durable Personen. Beim Lease wählt der Harness deterministisch das passendste Mitglied; dessen Seele, Lebenslauf und relevante Learnings gehen in den Prompt; nach jedem Versuch schreibt das Mitglied Rückblick und Learnings; jedes Mitglied hat einen Stundenzettel. Der Harness bleibt seriell: genau ein Mitglied ist je aktiver Slice im Einsatz.

### 3a · Datenmodell (Core-DB, `runtime/ctox.sqlite3`)

- `crew_members`: `id`, `name`, `shape` (`round|square|triangle|blob`), `color`, `created_at`, `archived`, `soul_json`, `specialties_json`, `stats_json`, `updated_at`.
  - `soul_json`: fünf Achsen 0–100 (`gruendlichkeit_vs_tempo`, `vorsicht_vs_mut`, `knapp_vs_ausfuehrlich`, `regeltreu_vs_kreativ`, `nachfragen_vs_annehmen`), `sketch` (Charakterskizze ≤ 600 Zeichen), `voice` (Stil in einem Satz). Die Achsen sind die späteren Slider im Profil.
  - `specialties_json`: `modules[]`, `command_types[]`, `skills[]`, `tags[]`.
  - `stats_json`: `tasks_total`, `succeeded`, `failed`, `review_passed`, `review_rejected`, `avg_elapsed_ms`, `last_active_at`.
- `crew_member_learnings`: `id`, `member_id`, `text` (≤ 400), `kind` (`insight|pitfall|preference`), `scope_json {module?, command_type?, thread_key?}`, `evidence_run_id`, `created_at`, `confirmed_by_owner`, `archived`.
- Stundenzettel = `ctox_runs` gefiltert nach `crew_member_id`, ergänzt um `retrospective` (≤ 300 Zeichen, vom Mitglied am Versuchsende geschrieben).
- `communication_routing_state` bekommt die Spalte `crew_member_id` (Migration additiv).
- Seeding beim ersten Start: vier Mitglieder mit unterschiedlichen Formen/Farben, unterschiedlichen Seelen und Spezialitäten (Vorschlag: Business-OS-Apps und Code; Recherche und Wissen; Daten, Import und Tabellen; Kommunikation, Tickets und Outbound). Namen aus der bestehenden Liste `CREW_NAMES` in `shared/business-chat.js:32`, damit die Oberfläche später nahtlos umsteigt.

### 3b · Auswahl beim Lease

Dort, wo heute `lease_worker_id` gesetzt wird (`service.rs:4630, 4731`), wähle das Mitglied: Punkte für Spezialitäten-Treffer (Modul, Command-Typ, Skill, Tags im Prompt), Bonus für erfolgreiche Runs auf demselben Modul oder `thread_key`, Malus für Fehlschläge auf demselben Modul in den letzten 24 h, Gleichstand entscheidet das am längsten inaktive Mitglied. Kontinuität: Hat der Chat oder `thread_key` schon ein Mitglied, bleibt es, solange es nicht archiviert ist. Eine manuelle Zuordnung (`ctox.crew.assign`, siehe 3e) vor dem Lease gewinnt. Die Wahl ist deterministisch, wird als Harness-Flow-Ereignis `crew_selected` mit Begründung protokolliert (landet über PR-1 in `ctox_harness_events`), in die Routing-Spalte geschrieben und auf `ctox_queue_tasks.crew_member_id` sowie `ctox_harness_status.active_crew_member_id` projiziert.

### 3c · Seele im Prompt

Ein Block `{{CREW_SOUL_BLOCK}}` im Laufzeit-Prompt (`src/core/context/live_context.rs`, `render_system_prompt_template` / `render_runtime_prompt`): Name, die fünf Achsen übersetzt in drei bis sechs konkrete Verhaltensregeln, ein Satz Lebenslauf (Anzahl Tasks, Stärken aus Stats), die bis zu acht nach Scope relevantesten bestätigten Learnings (unbestätigte nur mit Kennzeichnung, maximal zwei). Obergrenze rund 1200 Tokens. Der Block steht **nach** dem CTO-Operating-Mode und allen Sicherheits-/Ausführungsregeln und darf diese nicht relativieren; das gehört in einen Test.

### 3d · Rückblick und Learnings erfassen

Kein zusätzlicher Modellaufruf. Erweitere den strukturierten Abschluss des Workers (`agent_outcome` / finale Nachricht) um ein optionales `crew_retrospective { retrospective, learnings: [{text, kind, scope}] }` und weise den Worker in den Ausführungsregeln an, es zu füllen (kurz, konkret, keine Geheimnisse, keine Pfade). Der Server validiert (Längen, höchstens drei Learnings), dedupliziert gegen bestehende Learnings des Mitglieds (normalisierter Textvergleich), speichert unbestätigt und schreibt `retrospective` in die `ctox_runs`-Zeile. `insight` nur aus erfolgreichen, review-bestandenen Versuchen; `pitfall` auch aus Fehlschlägen; `preference` nur, wenn der Inhalt eine Owner-Rückmeldung zitiert. Stats des Mitglieds nach jeder Finalisierung fortschreiben.

### 3e · Projektionen und Commands

- `ctox_crew_members` (alle Rollen: `id`, `name`, `shape`, `color`, `archived`, `state` (`home|on_duty|resting_after_failure`), `active_task_id`; Admin und Founder zusätzlich `soul`, `specialties`, `stats`), `ctox_crew_learnings` (Admin und Founder). Indizes und Retention (Learnings höchstens 200 je Mitglied, älteste unbestätigte weichen).
- Commands unter einem neuen Scope `BusinessOsPermission::CrewManage` (Admin; Founder darf `learning.confirm/update`): `ctox.crew.member.create`, `ctox.crew.member.update {name?, soul?, specialties?, archived?}`, `ctox.crew.learning.confirm|update|delete`, `ctox.crew.assign {task_id, member_id}` (nur vor dem Lease). Spiegle den Scope in `src/apps/business-os/shared/permissions.js`.
- CLI minimal: `ctox crew list`, `ctox crew show <id>` (lesend), damit Betrieb und Tests ohne Browser prüfen können.

### 3f · Nicht-Ziele PR-2

Keine UI. Keine Parallelität zwischen Mitgliedern. Keine zusätzlichen Modellaufrufe. Keine Änderung der Review-Gates oder der Queue-Semantik über die Spalte `crew_member_id` hinaus.

### 3g · Abnahme PR-2

- Tests: Auswahl ist deterministisch und respektiert Kontinuität, manuelle Zuordnung und Archivierung; Prompt enthält den Soul-Block nach den Sicherheitsregeln und bleibt unter der Grenze; Learnings-Validierung und -Deduplizierung; `insight` nur aus bestandenen Versuchen; Projektionen und Fixtures beidseitig; Policy für `CrewManage`; Migration additiv auf einer bestehenden Test-DB.
- Dieselben Build- und Contract-Kommandos wie in 2i.
- Doku: `docs/ctox-rxdb.md`, `HARNESS.md` Abschnitt „Crew-Identität“ (Auswahlregel, Prompt-Block, Learnings-Vertrag).

## 4. Abschlussbericht je PR (fest)

Am Ende jedes PRs gibst du aus und schreibst dasselbe in den PR-Text:

1. PR-Link, Branch, Basis-Commit.
2. Was umgesetzt ist, je Abschnitt (2a–2g bzw. 3a–3e) mit Dateipfaden.
3. Abweichungen von diesem Auftrag mit Begründung.
4. Jedes Abnahme-Kommando mit Ergebnis (grün/rot, Kernzeilen der Ausgabe).
5. Bewusst Ausgelassenes und offene Risiken (insbesondere `abort_turn`).
6. Was PR-3/PR-4/PR-5 über die neuen Verträge wissen müssen (Feldnamen, Command-Namen, Policy).

Dann anhalten und auf den nächsten Auftrag warten.
