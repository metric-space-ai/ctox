# cv-print-builder – v4-Handler (Strang H8, Shell v4)

Stand 15.10.2026. Quelle der Einstufung: `~/.local/state/shell-v4/analysis/E-befehlstypen.md` (§2 Tabelle, §3 Capabilities),
Trennregel Architektur §16.13, Handler-Vertrag §16.16. Rust nur gelesen: `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/`.
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`; Fehler `throw {code, message}`
(`invalid|forbidden|not_found|conflict|revoked|expired|quota|unavailable`). Tests: `node --test handlers/tests/*.test.mjs`
(In-Memory-ctx `handlers/tests/_ctx.mjs`: eigene Collections = v4-Manifest `collections`, Fremdzugriff = Manifest
`foreign_collections` + hier beantragte Ergänzungen; Capabilities protokolliert und je Test beantwortbar).

**Stand: 1 von 2 Befehlstypen als JS-Handler, 0 über `ctx.cap.legacy.<command_type>`, 1 PLATTFORM (nur dokumentiert). Tests: 8 grün (Strang CVH: Datei-Ref aus `files`).**

## Befehlstypen

| # | Befehlstyp | Einstufung | Rust-Ort | Ziel | Stand |
|---|---|---|---|---|---|
| 1 | `ctox.cv_print.apply_parse` | GEMISCHT | `store.rs:23772`, `:23866`, `:24092`, `:25926`, `:27384` | `handlers/ctox.cv_print.apply_parse.js` (`default`, `onReply`, `onFailed`) + `cap.agent.task`, `cap.doc.extract`; Rückrufe `.done` / `.failed` | portiert (SKL-B4) |
| 2 | `business_os.chat.task` | PLATTFORM | `store.rs:6753 record_command` | Kern (Queue) | nur dokumentiert |

Heute sendet die UI `business_os.chat.task` mit `writeback_contract.command_type = 'ctox.cv_print.apply_parse'`
(`index.js:1934-1975`); der Kern erkennt das über `is_cv_print_parse_command` und schreibt die Agentenantwort zurück.
v4: der Befehlstyp `ctox.cv_print.apply_parse` hat einen eigenen Handler:
- `default` – Quelle je Profil (Strang CVH, 15.10.2026):
  - **neue Profile** (`payload.source_file_ref` bzw. `source_file:{kind:'file', ref}`, Upload über `api.files`):
    Größenprüfung (≤ 32 MiB) über `files.size` (Dokument per `source_file.file_id`, sonst per Selektor `{ref}`),
    PDF-Text über `cap.doc.extract({source:{kind:'file', ref}})`; kein Zugriff auf `desktop_files`.
  - **ältere Profile** ohne Ref (`source_file_id` + `generation_id`): unverändert – Größe über `desktop_files`,
    `cap.doc.extract({source:{kind:'desktop_file', file_id, generation_id}})`.
  Danach gemeinsam: Text ≤ 24 000 Zeichen, ```` ``` ```` → `'''`, Prompt wörtlich nach `cv_print_command_prompt`
  (Anreicherungstext nennt bei neuen Profilen `files` und zusätzlich `source_file_ref`), Queue mit Skill
  `ctox-cv-print-parser` und Rückkehr-Hooks.
- `onReply(command, {task_id, text}, ctx)` – `writeback_cv_print_parse` vollständig in JS: JSON aus der Antwort,
  Umschlag (`schema`, `workflow`, `source`, `print`-Vorgaben), Version = max+1 je Dokument, `document_versions` anlegen und
  `documents` patchen – in **einer** Transaktion (Rust: zwei Upserts plus RxDB-Projektion). Neue Profile: die
  Version behält `source.file_ref`/`source.file_id` und `blob_id = ref` (sonst verliert die UI das Original-PDF);
  ältere Profile wie bisher `source.desktop_file_id` und `blob_id = source_file_id`.
- `onFailed` – nur Fehlermeldung; `business_commands` schreibt die Laufzeit (wie news, Strang H6).
**UI-Folgearbeit (App-Port-Strang):** statt `business_os.chat.task` den Typ `ctox.cv_print.apply_parse` senden.
`business_os.chat.task` (Chat-Verlauf) bleibt PLATTFORM.

## Fremdzugriffe (Manifest `foreign_collections`, für den Manifest-Generator)

Stand Strang CVH (15.10.2026), UI speichert neue PDFs über `api.files`; nichts mehr in `desktop_files`,
`desktop_file_chunks` oder `business_chats`.

| Collection | Eigentümer | ops | Wofür |
|---|---|---|---|
| `files` | ctox-platform | `read` | neue Profile: Größe/Metadaten der Quelle (Handler), Original-PDF über `api.files.get(ref)` (UI) |
| `documents` | documents | `read`, `write` | Profil (UI-Import, Handler `onReply`) |
| `document_versions` | documents | `read`, `write` | Versionen (UI-Import, Handler `onReply` max+1) |
| `desktop_files` | desktop | `read` | **nur ältere Profile** ohne `source_file_ref` (Größenprüfung im Handler); kein `write` mehr |
| `desktop_file_chunks` | desktop | `read` | **nur ältere Profile** (Original-PDF in der UI aus lokalen Chunks); kein `write` |
| `business_chats` | – | – | **entfällt** (weder UI noch Handler lesen oder schreiben ihn) |

| Capability | Signatur (Vorschlag) | Beispielaufruf | Herkunft |
|---|---|---|---|
| `ctx.cap.agent.task.enqueue` | `({title, instruction, skill[], thread_key, output_contract, payload, metadata, reply_to:{app_id, command_id, hook, on_failure}}) → {task_id}` | `await ctx.cap.agent.task.enqueue({instruction, skill:['ctox-cv-print-parser'], reply_to:{app_id:'cv-print-builder', command_id, hook:'onReply', on_failure:'onFailed'}})` | E §3, Rückkehr-Hooks wie news (H6) |
| `ctx.cap.doc.extract` | `({source:{kind:'file', ref} \| {kind:'desktop_file', file_id, generation_id}, command_id, format:'text'}) → {text, name, mime_type, size_bytes, content_hash}` | neu: `await ctx.cap.doc.extract({source:{kind:'file', ref}, command_id, format:'text'})`; alt: `…({source:{kind:'desktop_file', file_id, generation_id}, …})` | E §3 `doc.extract`; Quelle statt `blob_id` (Materialisierung aus Chunks + pdftotext/Normalisierung im Kern) |


## Kernform der Capabilities (Strang CAPSIG, 15.10.2026, §16.30/§16.32)

| Capability | Aufruf (Kernform) |
|---|---|
| `doc.extract` | `({blob_id:{ref}, kind:'pdf'})` |
| `agent.task.enqueue` | flach `({title, instruction, skill, thread_key, payload:{command, output_contract, source}, idempotency_key, on_complete, on_failed})` |
| `agent.task.get` | `({task_id})` im Rückruf-Handler |

Rückruf §16.30 Nr. 3: neuer Befehlstyp `ctox.cv_print.apply_parse.done` (`{task_id, status, result}` → onReply/onFailed
des Ausgangsbefehls). `reply_to`-Hooks entfallen.

### Kern-Import
- **desktop_files → files** (einmalig): je Desktop-Datei ein files-Datensatz mit **derselben id**, ref = sha256 der aus
  `desktop_file_chunks` zusammengesetzten Bytes, size = size_bytes. Danach liest der Handler ältere Profile
  (`source_file_id`) über files; der Altpfad `desktop_files`/`desktop_file_chunks` ist entfernt (Fremdzugriff gestrichen).


## Strang SKL-B4 (15.10.2026): App-Skill, Template, Rückrufe

- **Skill im Paket:** `skills/parser/SKILL.md` = `src/skills/packs/business/ctox-cv-print-parser` + frühere
  `PARSER_SKILL.md` (gelöscht), auf den App-Ablauf zugeschnitten (kein `business_os.chat.task`, kein
  `desktop_file_chunks`, kein Kern-Rückschrieb). Handler nennt `skill:'cv-print-builder/parser'`.
- **Template:** `prompts/parse.md` = `cv_print_command_prompt` (`store.rs:27384`) + Abschnitt aus
  `cv_print_pdf_text_prompt_enrichment` (`:25926`), Text wörtlich (Test vergleicht den ganzen Prompt).
- **Rückrufe:** `on_complete:'ctox.cv_print.apply_parse.done'` (Version anlegen; Doppelzustellung = keine zweite
  Version, Schlüssel `document_versions.business_command_id`), `on_failed:'ctox.cv_print.apply_parse.failed'`
  (neu; Fehlschlag und Abbruch `status:'cancelled'` setzen an der wartenden Version `workflow.phase='error'`,
  `task_status`, `error`, am Dokument `status`/`display_cache.phase='error'`, wie die UI bei lokaler Ablehnung).
  `reply_to`-Reste entfernt (die Kapabilitätstabelle oben mit `reply_to` ist überholt).
- **UI:** sendet kein `skill`/`skill_id` mehr im Payload (der Handler wählt den Skill); Anweisungstext nennt
  `cv-print-builder/parser`.
- **Kern-Abbau:** `src/skills/packs/business/ctox-cv-print-parser/` (Quelle jetzt das Paket);
  `store.rs suggested_skill_for_command :27737` (cv-print-Zweig), `is_cv_print_parse_command :23772`,
  `cv_print_command_prompt :27384`, `cv_print_pdf_text_prompt_enrichment :25926`, `writeback_cv_print_parse :23866`,
  `process_cv_print_parse_command :24092`; `service/business_os.rs` `SKILL_APP_BINDINGS`-Eintrag `ctox-cv-print-parser`.
- **Manifest (Generator-Lauf):** `skills:[{name:'parser'}]`, `prompts:[{name:'parse'}]`, Handler `.failed`;
  `PARSER_SKILL.md` fällt aus `files`.
- Tests: `node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` (aus `handlers/`), 15/15.
