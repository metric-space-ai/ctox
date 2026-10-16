# news – Shell-v4-Handler

Strang H5, Stand 15.10.2026. Quelle (read-only): `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/store_news_commands.rs`
(1 129 Z.), Anbindung `command_plane.rs:2239` (Kontrollbefehle), `store.rs:10359/24190` (Queue-Rückschrieb),
`store.rs:15094` (Fehlerprojektion), `project_chats/weekly_reports.rs:175` (Planer-Emission).
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`.
Tests: `node --test src/apps/business-os/modules/news/handlers/tests/*.test.mjs` (32/32 grün, Stand SKL-B1; alle 6 Rust-Tests der
Datei nachgebildet; In-Memory-ctx mit Schemaprüfung aus `collections.schema.json`).

E-befehlstypen.md zählt für news nur die 2 Kontrollbefehle; `store_news_commands.rs` bedient aber 4 Befehlstypen.
Alle 4 sind hier portiert; dazu seit SKL-B1 je Agenten-Auslöser ein eigenes Rückruf-Paar (4 Handler).

## Befehlstypen

| Befehlstyp | Einstufung | Rust-Ort | Ziel (v4) | Stand |
|---|---|---|---|---|
| `news.topic.schedule` | GEMISCHT | `store_news_commands.rs:142` `handle_topic_schedule`, `:235` `store_schedule_task_id` | `handlers/news.topic.schedule.js` → `ctx.cap.schedule.{list,ensure,set_enabled,remove}` | portiert, 3 Tests |
| `news.source.auth_assist` | GEMISCHT | `:252` `handle_source_auth_assist` | `handlers/news.source.auth_assist.js` → `ctx.cap.browser.auth_assist.request` | portiert, 2 Tests |
| `news.issue.generate` | GEMISCHT | Zulassung generisch (`record_command`), Planer `:318` `emit_scheduled_issue` + `:446` `generate_payload` | `handlers/news.issue.generate.js`: Planer-Auslösung `payload.trigger==='schedule'` bzw. `prompts/issue.md` füllen + `agent.task.enqueue` (Skill `news/news-aggregation`) | SKL-B1, 7 Tests |
| `news.issue.complete` | Rückruf | Rückschrieb `:591` `apply_issue_reply` | `handlers/news.issue.complete.js` (on_complete) | SKL-B1 |
| `news.issue.failed` | Rückruf | Fehler `:716` `mark_news_command_failed`, #778 | `handlers/news.issue.failed.js` (on_failed) | SKL-B1 |
| `news.source.adapter` | GEMISCHT | Zulassung generisch | `handlers/news.source.adapter.js`: `prompts/source-adapter.md` (System-Skill `universal-scraping`) bzw. `prompts/source-connect.md` (Skill `news/news-aggregation`) + `agent.task.enqueue` | SKL-B1, 6 Tests |
| `news.source.adapter.complete` | Rückruf | Rückschrieb `:566`, `:769` `parse_source_reply` | `handlers/news.source.adapter.complete.js` (on_complete) | SKL-B1 |
| `news.source.adapter.failed` | Rückruf | Fehler `:742` | `handlers/news.source.adapter.failed.js` (on_failed) | SKL-B1 |

Reine Fachlogik (Parser/Normalisierung) in `handlers/_lib/news.js`: `parse_issue_reply :844`, `normalize_post :919`,
`parse_source_reply :769`, `generate_payload :446`, `extract_json_object` (`store.rs:23799`), `clip/http_url/
non_negative/published_at_ms :810–842`, `valid_record_id :56`; Tests in `tests/lib.test.mjs`.

Offen über `ctx.cap.legacy.<command_type>`: **0 von 4**.

## Capability-Wünsche (an „ctox sync v4“)

| Capability | Signatur | Beispielaufruf | genutzt von |
|---|---|---|---|
| `ctx.cap.schedule.list` | `({name?, thread_key?}) → [{task_id, name, thread_key, enabled, next_run_at}]` | `await ctx.cap.schedule.list({name:'news:topic_1', thread_key:'business-os/news/topic_1'})` | `news.topic.schedule` (Rust `schedule::list_tasks`) |
| `ctx.cap.schedule.ensure` | `({name, cron_expr, thread_key, skill, calendar:{timezone, lead_minutes}, prompt?, command:{type, payload, actor_id}}) → task` | siehe `news.topic.schedule.js` | Rust `ensure_task_with_calendar`; neu: `command` = Befehl, den der Planer bei Fälligkeit im Namen von `actor_id` feuert. Der Planer ergänzt `payload.{schedule_task_id, schedule_name, schedule_thread_key, scheduled_for}`. `prompt` (ROUTE_MARKER) bleibt für den Rust-Emitter im Übergang. |
| `ctx.cap.schedule.set_enabled` / `.remove` | `({task_id, enabled}) → task` / `({task_id})` | `await ctx.cap.schedule.set_enabled({task_id, enabled:false})` | `news.topic.schedule` |
| `ctx.cap.browser.auth_assist.request` | `({source_id, target_url_override, credential_ref, login_hint, preferred_auth_assist, requesting_task_id, source_module, command_path, owner_user_id, deterministic_for_task, accept_as_trusted_local}) → effect` | `await ctx.cap.browser.auth_assist.request({source_id:'reddit.com', requesting_task_id: command.id, source_module:'news', command_path:'news.source.auth_assist', owner_user_id, deterministic_for_task:true, accept_as_trusted_local:true})` | `news.source.auth_assist` (Rust `service::business_os::enqueue_web_stack_auth_assist_request :5059`, Parameter 1:1) |
| `ctx.cap.agent.task.enqueue` | §16.30 flach: `({title, instruction, skill, thread_key, payload, idempotency_key, on_complete, on_failed}) → {task_id}` | `_lib/news.js enqueueIssue` / `enqueueSourceAdapter` | `news.issue.generate`, `news.source.adapter` |
| `ctx.cap.agent.task.get` | `({task_id}) → task` (Ausgangsbefehl in `payload.command`) | `_lib/news.js callbackOrigin` | die 4 Rückruf-Handler |
| `ctx.asset` | §16.34, liest `prompts/<name>.md` (über `ctox:shared/prompt-template.js`) | `_lib/news.js renderPromptLines` | beide Agenten-Auslöser |
| **Laufzeitvertrag: `ctx.command` mit fester ID** | `ctx.command(type, payload, {id, record_id, actor_id, sync_collections}) → {id, task_id?}` | `await ctx.command('news.issue.generate', payload, {id: commandId, actor_id: owner})` | Planer-Emission: die Befehls-ID steht schon in der Anweisung (`--task-id`) und an der Ausgabe, bevor der Befehl existiert (Rust: `accept_rxdb_business_command_with_origin` mit eigener `id`, Herkunft `TrustedLocal`). |

## Abweichungen vom Rust-Verhalten

1. **Modulprüfung** `command.module == "news"` → `command.app_id === 'news'` (`forbidden`).
2. **Owner**: `session.user.id` → `command.actor.id`; fehlt er → `forbidden` (Rust: Fehler „requires an authenticated user“).
3. **Doppelschreiben** `upsert_business_record` + `upsert_rxdb_collection_record` (`write_record :72`) entfällt; ein `ctx.put`.
4. **Planer-Auslösung** läuft nicht mehr über den Prompt-Marker, sondern als Befehl `news.issue.generate` mit
   `payload.trigger='schedule'`. Die Bindungsprüfung (`name`/`thread_key` passen zum Thema) bleibt, ebenso Stichtag-Skip,
   deaktiviertes Thema, `issue_sched_<sha256[..24]>`/`cmd_news_issue_generate_sched_<…>`. Rückgabe `{run_key, outcome}`
   statt `(key, "emitted"|"skipped")`.
5. **Zeitzonenprüfung**: `chrono_tz::Tz`-Parse → `Intl.DateTimeFormat({timeZone})`; ungültig → `Europe/Berlin`.
6. **onReply in einer Transaktion** (`ctx.transaction`): Rust schrieb Posts, Ausgabe, Quellen, Thema einzeln; bricht v4
   mitten drin ab, bleibt nichts halb stehen. Wirkung im Erfolgsfall identisch.
7. `published_at` (RFC 3339) wird per Muster + `Date.parse` gelesen; Rust nutzt `chrono::parse_from_rfc3339`. Exotische
   RFC-3339-Varianten (Schaltsekunde) können abweichen.
8. Fehlercodes: Validierung → `invalid`, fehlende Quelle → `not_found`, fremdes Modul/kein Owner → `forbidden`,
   Queue nimmt nicht an → `unavailable`. Meldungstexte wörtlich.

## Unklarheiten

- `command.record_id` steht nicht im festen Handler-Vertrag. Rust nutzt es als Rückfall für `topic_id`/`source_id`;
  der Port liest `command.record_id`, falls die Laufzeit es mitliefert (Empfehlung: in den Vertrag aufnehmen; die
  Browser-Umschläge `core/news-model.mjs` setzen es immer).
- Die Browser-Seite übergibt für `news.source.auth_assist` `target_url`; Rust ignoriert es (`None`). Treu übernommen.
- Ob die Queue-Zulassung (`default` von `news.issue.generate`/`news.source.adapter`) überhaupt Handler-Sache ist oder
  generisch in der Laufzeit bleibt, entscheidet „ctox sync v4“. Der Port nimmt sie in den Handler, damit die
  Planer-Auslösung denselben Typ nutzt.
- `ctx.read(…, {limit:200})` – Rust kappt Quellen je Thema bei 200; Reihenfolge war dort unbestimmt.


## App-Skills und Automatisierung (Strang SKL-B1, 15.10.2026, §16.30/§16.34)

Paket: `skills/news-aggregation/SKILL.md` (Kopie von `src/skills/system/research/news-aggregation/`, auf die App
zugeschnitten: Kontext kommt als Block „Auftragsdaten (JSON)“ am Ende der Anweisung, Ergebnis = Task-Ergebnis, das die
App per Rückruf schreibt), `prompts/issue.md` (wörtlich `core/news-model.mjs buildIssueInstruction`),
`prompts/source-adapter.md` und `prompts/source-connect.md` (wörtlich die zwei Anweisungs-Arrays aus
`buildAdapterCommand`/`buildConnectCommand`; zwei Dateien, weil die Varianten keine Zeile teilen). Die UI schickt
keine `instruction`/`required_skills` mehr; eine mitgeschickte `instruction` ignoriert der Handler.

| Auslöser | vorher | nachher |
|---|---|---|
| „Ausgabe jetzt erzeugen“ / Planer | UI baut Anweisung, `skill:'news-aggregation'` (Core), Rückruf `news.task.done` | Handler füllt `prompts/issue.md`, `skill:'news/news-aggregation'`, `news.issue.complete`/`news.issue.failed` |
| Zauberstab Scraper | UI-Anweisung, `universal-scraping`, `news.task.done` | `prompts/source-adapter.md`, `universal-scraping` (generischer System-Skill, bleibt im Core), `news.source.adapter.complete`/`.failed` |
| Zauberstab Konto (X/Reddit) | UI-Anweisung, `news-aggregation` (Core), `news.task.done` | `prompts/source-connect.md`, `news/news-aggregation`, `news.source.adapter.complete`/`.failed` |

Regeln:
- Optionale Zeilen (`renderPromptLines`): eine Zeile entfällt, wenn ein als optional genannter Platzhalter `''` ist
  (`focus`, `signed_in_task_id`, `previous_issue_date`) – bildet `lines.filter(Boolean)` des Bestands nach; fehlende
  Werte bleiben Fehler `invalid`.
- Planer (`generate_payload :446`) und Knopf nutzen dasselbe Template (Fassung der UI): der Planer-Lauf bekommt damit
  zusätzlich die Zeile „Sprache: …“, und „Fokus:“ entfällt bei leerem Fokus (Rust schrieb sie immer).
- `--task-id` ist jetzt die echte Befehls-ID (`command.id`), nicht mehr die vom Client vorab vergebene (OFFEN-2).
- Nach dem Start trägt der Handler `task_id` (und `command_id`) an der Ausgabe nach.
- Rückrufe: `news.task.done` (CAPSIG) ist gelöscht, kein Doppelweg. Jeder Rückruf-Handler liest den Ausgangsbefehl per
  `agent.task.get` und prüft dessen Typ. #778 bleibt: Status failed | blocked | cancelled (über on_failed, und auch,
  falls er über on_complete kommt) markiert Ausgabe bzw. Quelle `failed`; Grund = `status_note` bzw. `error.message`
  getrimmt, sonst „CTOX hat die Ausgabe nicht abgeschlossen.“ Eine unbrauchbare Antwort (`invalid`, z. B. kein Post
  mit Quelle) markiert die Ausgabe ebenfalls `failed` (Rust: Rückschreibfehler → `mark_news_command_failed`).
- Idempotenz bei Doppelzustellung: Ausgabe `ready`/`failed` mit derselben `task_id` → keine Schreibvorgänge; Quellen
  halten die verarbeitete `task_id` in `payload.adapter_task_id`.
- Tests: `handlers/tests/news.issue.generate.test.mjs`, `news.issue.callbacks.test.mjs`, `news.source.adapter.test.mjs`
  gegen `v4/tools/agent-sim` (echtes Paket, Skill-Auflösung, Callback-Zustellung). Node-Tests lösen `ctox:shared/`
  über `handlers/tests/_ctox-shared.mjs` auf (Handler danach dynamisch importieren).

Kern-Abbau (tot für news): `src/skills/system/research/news-aggregation/` (Quelle der Wahrheit = Paket);
`store.rs process_news_queue_command :24195` / `complete_business_command_from_queue_reply` Zweig news
(`:10312-10462`); `store_news_commands.rs apply_issue_reply :591`, `apply_news_queue_reply :566`,
`mark_news_command_failed :716/:742`; der Payload-Vorschau-Anhang in `command_prompt` für news-Befehle.
`universal-scraping` bleibt als System-Skill.

## Kernform der Capabilities (Strang CAPSIG, 15.10.2026, §16.30/§16.32)

| Capability | Aufruf (Kernform) |
|---|---|
| `schedule.upsert` | vormals `schedule.ensure` (gleiche native Payload); `schedule.list/remove/set_enabled` §16.30 |
| `agent.task.enqueue` | flach, `payload:{command, output_contract, source}`; seit SKL-B1 eigene Rückruf-Paare (oben) |
| `agent.task.get` | in den Rückruf-Handlern (SKL-B1) |

#778 (main e5f057ead): Rückruf mit Status failed | blocked | cancelled markiert die Ausgabe `failed`, Grund =
status_note getrimmt, sonst „CTOX hat die Ausgabe nicht abgeschlossen.“ (seit SKL-B1 `news.issue.failed.js`/
`news.issue.complete.js`, Tests `tests/news.issue.callbacks.test.mjs`).
