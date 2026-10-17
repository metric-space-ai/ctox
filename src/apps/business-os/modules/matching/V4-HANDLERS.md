# matching – Handler-Port Shell v4 (Strang H6)

Stand 15.10.2026. Quelle (nur gelesen): `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/importer.rs`
(Routing `store.rs:10149-10197` `process_source_parse_command`, Abschluss `:10198-10300`).

Tests: `node --test src/apps/business-os/modules/matching/handlers/tests/*.test.mjs` → 12/12 grün; alle Handler laden
zusätzlich in einem nackten `vm`-Kontext (keine Node-/Web-Globals; `URL`-Ersatz in `origin_url`, eigenes SHA-256/UTF-8/Base64)
(Hash/Slug/Base64 gegen `node:crypto`, Scoring-Pfade, CV-Feldauswertung, Stellenabschnitte, JSON-LD, alle drei Handler).

## Grundbefund

Alle drei Typen liefen bisher als Queue-Befehl mit **nativem Harness-Prozessor** (kein Agent): der Core rief
`importer.rs` direkt. Die gesamte Logik ist Fachlogik – Texte bilden, fünf Dimensionen bewerten, Felder aus CV- und
Stellentext ziehen, Datensätze formen. Native Fähigkeiten sind nur: PDF-Textextraktion mit Vision-OCR-Rückfall,
HTML-DOM-Abfragen (html5ever/scraper) und der SSRF-geschützte URL-Abruf. Jetzt rechnen die Handler direkt, ohne Queue.

## Tabelle (Reihenfolge = UI-Häufigkeit; Basis: Aufrufstellen in modules/ + shared/)

| # | Befehlstyp | Einstufung (E) | Rust-Ort | Ziel | Status |
|---|---|---|---|---|---|
| 1 | `matching.match` | FACHLOGIK | `importer.rs:61` → `compute_matching_result :1020-1127`, Scoring `:3695-4015` | `handlers/matching.match.js`, `_lib/score.js`, `_lib/text.js`, `_lib/envelope.js` | portiert, getestet |
| 2 | `matching.source.parse_object` | GEMISCHT | `importer.rs:26` → `import_matching_objects :847-1018`, CV-Parser `:2335-2398`, `:2588-2862`, `decode_file_payload :3660` | `handlers/matching.source.parse_object.js`, `_lib/candidate.js` + `cap.doc.extract` | portiert, getestet |
| 3 | `matching.source.parse_requirement` | GEMISCHT | `importer.rs:26` → `import_matching_requirement :676-845`, `parse_job_html :2164-2316`, Abschnitte `:2863-3080` | `handlers/matching.source.parse_requirement.js`, `_lib/job.js` + `cap.doc.html` + `cap.web.fetch` | portiert, getestet |

**Legacy-Rest (`ctx.cap.legacy.<type>`): 0 von 3.**

Nicht im Umfang (andere Typen aus dem matching-UI): `business_os.i18n.translate`, `ctox.business_os.app.modify`
(Plattform/Queue). Nicht portiert, weil nicht matching-Typen, obwohl im selben Rust-Arm: `match.compute`-Altpfad
(`jobs`/`candidates`, `importer.rs:72-214`), `import_requirement_url`, `import_candidate_documents` (Altspalten-Import),
`outbound.source.import`.

## Capability-Wünsche

| Capability | Signatur | Beispielaufruf |
|---|---|---|
| `ctx.cap.doc.extract` (E §3) | `({data_base64, mime, ocr_fallback:true}) → {text}` – PDF-Text (`parse_pdf_text`), bei leerem Ergebnis Vision-OCR (`ocr_pdf_via_vision`); Fehler → Handler importiert mit leerem Text | `await ctx.cap.doc.extract({data_base64, mime:'application/pdf', ocr_fallback:true})` |
| `ctx.cap.doc.html` (neu) | `({html, queries:[{key, selector, mode:'text'\|'all_text'\|'attr'\|'micro', attr?}]}) → {key: string\|string[]\|null}`; `text` = erster Treffer, Textknoten mit `"\n"` verbunden (roh); `all_text` = je Treffer verbundener Text; `attr` = Attribut des ersten Treffers; `micro` = `content`-Attribut, sonst `clean_text` der Texte mit `" "`. Zweite Form `({html, fragment_text:true}) → {fragment_text}` = `html_fragment_text` vor `normalize_lines` | `await ctx.cap.doc.html({html, queries: JOB_HTML_QUERIES})` |
| `ctx.cap.web.fetch` (neu; E kennt nur `http.fetch` mit Allowlist) | `({url, method:'GET', public_only:true, timeout_ms, user_agent}) → {status, text}` – nur http(s), DNS-Auflösung nur auf öffentliche Adressen (auch bei Weiterleitungen), wie `fetch_url_guarded` | `await ctx.cap.web.fetch({url, method:'GET', public_only:true, timeout_ms:30000})` |

Keine Fremd-Collections: gelesen und geschrieben werden nur `matching_requirements`, `matching_objects`,
`matching_results`.

## Abweichungen / Unklarheiten (markiert)

1. **Schema-Hülle (größte Abweichung)**: `collections.schema.json` verlangt `{id, kind, title, status, data,
   updated_at_ms}`. Rust schrieb flache Dokumente direkt in `business_records`, an der Schema-Prüfung vorbei. Die
   Handler schreiben das unveränderte Rust-Dokument als `data` in die Hülle, die auch die UI schreibt
   (`ui/businessOsDataSource.js toPersistedDocument`); die UI liest beide Formen. Beim Lesen wird `data` (bzw.
   `data.requirement`/`data.object`) ausgepackt wie in `fromPersistedDocument`. Rust las UI-geschriebene Hüllen nur
   oberflächlich (nur `title` traf) – der Handler bewertet dadurch UI-angelegte Anforderungen **mit** Inhalt, Rust tat das
   nicht. Treue zum Ergebnis für Rust-Datensätze bleibt; für UI-Datensätze ist das eine bewusste Korrektur.
2. **Atomarität `parse_object`**: Rust löscht `<base>__%` zuerst und bricht ggf. mitten in der Dateischleife ab
   (Teilzustand). Der Handler wertet erst alle Dateien aus und schreibt dann in einer `ctx.transaction` – alles oder
   nichts. SQL-`LIKE` (ASCII ohne Groß/Klein, `_` als Platzhalter) → exakter Präfixvergleich.
3. **`queue_task_id`**: Rust trug die Queue-Aufgabe ein; Handler laufen ohne Queue → `null`.
4. **`write_import_artifact`** (`runtime/business-os-imports/<cmd>/*.json`) entfällt: kein Dateisystem im Handler; der
   Inhalt steht im Rückgabewert.
5. **Befehlsstatus**: Rust setzte auch im Fehlerfall `status:'accepted'` mit `error`; in v4 leitet der Core den Status aus
   Rückgabe/throw ab (Fehler → `failed`).
6. **Regex/Unicode**: Rust-`regex` ist Unicode-bewusst (`\d`=`\p{Nd}`, `\b` Unicode-Wortgrenze) – per `/u` und
   Lookarounds nachgebaut. `String::len()` (UTF-8-Bytes) in `is_plausible_role`, `joined_cv_line`, `bulletize`,
   `raw_text_length` als Bytelänge nachgebaut.
7. **Zahlen in Texten**: `push_value` schreibt JSON-Gleitkomma mit ganzem Wert in Rust als `3.0`, JS als `3`
   (betrifft nur Zahlenfelder im Bewertungstext; UNKLAR, ob je vorkommend).
8. **Nicht gegen Rust gegengemessen**: Die Tests prüfen den Port gegen die gelesene Rust-Logik, nicht gegen eine
   Rust-Ausführung (kein Build in diesem Strang). Empfehlung: einmal dieselben Eingaben durch `importer.rs` und die
   Handler schicken und Ergebnisse vergleichen.


## Kernform der Capabilities (Strang CAPSIG, 15.10.2026, §16.30/§16.32)

| Capability | Aufruf (Kernform) |
|---|---|
| `doc.extract` | `({blob_id:{ref}, kind: pdf|document|text, ocr:true})` – Base64 vorher über `ctx.files.put` (`_lib/files.js`) |
| `web.fetch` | `({url})` → `{blob_id:{ref}, sha256, url, title}` (Seitentext) |
| `doc.html` | `({blob_id:{ref}, selector, attribute?, limit?})` je Abfrage; Modi all_text/text/attr/micro im Handler (`queryJobDom`) |

Befund: `web.fetch` liefert Seitentext, nicht HTML – JSON-LD/Meta abgerufener Seiten sind nicht auswertbar (Titel-Rückfall
= vom Kern gemeldeter Titel; idFS-Regel sucht im Seitentext). Kernwunsch: Roh-HTML als zweiter Blob.
Fremdzugriff `files` (ctox-platform) jetzt `read, write` (ctx.files.put).


## Strang SKL-B4 (15.10.2026): verwaiste Kern-Skills

Geprüft: kein Auslöser in v4 nutzt `business-os-requirement-matching` oder `business-os-import-parser`. Die Handler
`matching.match`, `matching.source.parse_object`, `matching.source.parse_requirement` rechnen ohne Agent (kein
`agent.task`, kein `model`); `outbound.source.import` nennt keinen Skill. Einzige Nutzer sind Rust-Altpfade:
`store.rs suggested_skill_for_command :27741/:27749` (über `is_source_parse_command :27774`, `is_match_command :27781`)
und `service/business_os.rs SKILL_APP_BINDINGS :74/:81` samt Test `matching_skill_binding_resolves_to_on_disk_skill_pack
:9571`. Daher **nicht ins Paket**; Löschkandidaten für den Kern-Abbau: `src/skills/packs/business/business-os-requirement-matching/`,
`src/skills/packs/business/business-os-import-parser/` (mit den genannten Rust-Stellen).

## SKL-D: Kontext-Chat über den Baustein `ctox:shared/context-chat.js` (15.10.2026)

Kontextmenü (`ui/businessOsControls.js` `renderContextMenu`): vorher `business_os.chat.task` / `ctox.business_os.app.modify`, „App modifizieren“ ohne Rechte-Spiegel; jetzt Modi aus dem Baustein (ohne Frage-Modus), Spiegel `canModifyMatchingApp` (apps.source.edit oder admin/chef), Versand `sendContextChat` über `sendMatchingCommand`.

### Manifest-Wünsche (Kern §16.40, Sender-Seite)

```json
{ "commands_out": [{ "app": "ctox", "type": "ctox.chat.task" }, { "app": "ctox", "type": "ctox.app.modify.request" }] }
```
Gegenstück in `modules/ctox/V4-HANDLERS.md` (`commands_in` mit `from:'*'`). Zusätzlich braucht der Nutzer das
Befehlsrecht der Ziel-App ctox (`ctox.app.modify.request`: `apps.source.edit` bzw. owner/admin/chef, im Handler geprüft).
