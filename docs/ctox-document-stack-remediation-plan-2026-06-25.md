# CTOX Document Stack — Remediation Plan

Stand 2026-06-25 · Ableitung aus [docs/ctox-document-stack-review-2026-06-25.md](ctox-document-stack-review-2026-06-25.md) · Ticket-IDs verweisen auf die Befund-IDs des Reviews (H1–H16, M1–M20, L1–L40).

## Ziel & Leitplanken

Zwei systemische Wurzeln treiben die meisten High-Findings:

1. **Kein gemeinsames Dokumentmodell** → 4 inkompatible Blob/Version-Repräsentationen, kaputtes Cross-Runtime-Encoding, dreifach duplizierte Editoren/Parser.
2. **Kein Policy-Chokepoint bei Command-Ingestion** → `DataWrite` existiert in `policy.rs`, wird aber nie evaluiert; mutierende Parse/Import/Match/Writeback-Commands fallen ungeprüft durch.

Der Plan adressiert zuerst Korrektheits-/Sicherheits-Stopper, dann kaputte Funktionalität, dann die strukturelle Konsolidierung, die die Wurzeln zieht.

**Verbindliche Leitplanken (AGENTS.md):**

- Arbeit auf `main` im origin-Checkout, sofern nicht anders gewünscht. Atomare Commits, früh stagen (paralleler Codex-Agent läuft mit).
- **Datengrenze:** kein Dokument-/Blob-/File-Datum über HTTP. Fixes dürfen die `server.rs:225-232`-Sperre + `assert-rxdb-only.mjs`-Guard nie aufweichen.
- **Capability-Symmetrie:** neue Fähigkeiten als native Rust-Engine mit dünner Harness-CLI **und** `business_commands`-Surface (und MCP-Descriptor, wo sinnvoll).
- **Server-authoritativ:** jede record-mutierende Aktion durch native Policy/Capability-Prüfung. Kein UI-only-Gate.
- **Keine neuen Prozess-Env-Toggles** für Runtime-Verhalten — Config in typed config / SQLite-Runtime-Store / Secret-Store.
- **Generierte Wire-Contracts** (`src/core/rxdb/tests/fixtures/*.json`) nie einseitig editieren; Fixtures ändern → beide Seiten regenerieren → Consumer neu bauen. `dist/ctox-rxdb-js.mjs` nie direkt patchen; `src/` editieren, mit gepinntem esbuild bauen, drei `?v=`-Cache-Buster bumpen.
- **Keine Guard-Tests schwächen.** Ein rotes Guard ist ein Finding, kein Hindernis.

---

## Phasenübersicht & Abhängigkeiten

```
Phase 0  Stopper (parallel, unabhängig)        DS-0.1 … DS-0.6
Phase 1  Funktion reparieren                    DS-1.1 … DS-1.6   (DS-1.x meist unabhängig)
Phase 2  Strukturelle Konsolidierung            DS-2.1 (Engine) ──► DS-2.2/2.3/2.4/2.5
Phase 3  Hygiene & Härtung                      DS-3.1 … DS-3.7
```

Kritische Abhängigkeitskanten:

- **DS-0.2 (Policy-Chokepoint)** ist Voraussetzung dafür, dass DS-0.6/DS-1.4/DS-1.5 nicht je einen eigenen Ad-hoc-Gate bauen.
- **DS-2.1 (kanonische Document-Engine)** ist das Fundament für DS-2.2 (OOXML-Konsolidierung), DS-2.4 (cv-print/spreadsheets-Faltung), DS-1.2 (nativer DOCX-Writer) und DS-1.3 (nativer DOCX-Reader). Wer DS-2.1 überspringt, baut die Duplikate fort.
- **DS-0.3 (Blob-Encoding)** sollte *vor* DS-2.4 landen, weil die Konsolidierung auf der gewählten Konvention aufsetzt.

---

## Phase 0 — Korrektheits- & Sicherheits-Stopper

### DS-0.1 · PDF-Parser-Panic (H1)
- **Problem:** `min_x` ist Char-Index, wird als Byte-Offset in `&line[min_x..]` genutzt; Guard prüft keine Char-Boundary. Panic auf realem Input (NBSP, Akzente, CJK) reproduziert. Uncaught, erreichbar aus untrusted PDF (CV-Uploads, Web-Fetch).
- **Dateien:** `src/tools/pdf-parse/src/processing/clean_text.rs:29,48-49,62`; `src/tools/pdf-parse/src/processing/grid_projection.rs:1153`.
- **Fix:** `min_x` als Byte-Offset über `char_indices()` berechnen, oder vor dem Slice auf `is_char_boundary(min_x)` prüfen und ans nächste gültige Boundary runden.
- **Akzeptanz:** neuer Test in `src/tools/pdf-parse/tests/parity.rs` mit NBSP/Akzent/CJK-Margin, der vorher paniced; `parse_pdf_text` panic-frei.
- **Validierung:** `cargo test -p ctox-pdf-parse`.
- **Abhängigkeit:** keine. Sofort.

### DS-0.2 · Server-seitiger Policy-Chokepoint für Command-Ingestion (H5, H9)
- **Problem:** Nur App-Build-Commands werden in `record_command` via `reject_app_build_command_if_denied` gegated. `source.parse`/`matching.*`/`business_os.chat.task` (cv-print)/`documents.*` fallen durch den Default-Arm und werden ohne Session-Auflösung oder `DataWrite`-Check inserted + als `ctox_queue_task` enqueued. `importer.rs` enthält null Policy-Referenzen.
- **Dateien:** `src/core/business_os/store.rs` — `record_command` (~8528), `accept_rxdb_business_command_with_origin` (~14402) inkl. Default-Arm `_ => {}` (~15800), `process_source_parse_command` (~12109), `create_ctox_queue_task` (~26182), `rxdb_session_from_command`/`REQUIRE_CAPABILITY_TOKEN` (~20214); `src/core/business_os/policy.rs:42,114` (`DataWrite`); `src/core/business_os/rxdb_peer.rs:2713-2718`.
- **Fix:** **Einen** Chokepoint einführen, durch den jedes record-mutierende Command läuft, bevor es persistiert/enqueued wird: authentifizierte Session auflösen (Capability-Token-Actor, nie browser-behaupteter Actor) und `DataWrite` scoped auf Ziel-Collection/Modul evaluieren. Default-Arm fail-closed statt `record + enqueue`. Tabellarische Command→Permission-Map statt pro-Arm-Logik.
- **Akzeptanz:** Test, der ein `source.parse`-/`chat.task`-/`matching.match`-Command ohne gültige Capability/ohne DataWrite-Scope abweist (kein Record, kein Queue-Task); positiver Test mit gültigem Token. Bestehende App-Build-Gates bleiben grün.
- **Validierung:** `cargo test -p <business_os-crate> store::` (gezielt); `node src/apps/business-os/rxdb/tests/run-all.mjs`.
- **Abhängigkeit:** keine; blockiert DS-0.6, DS-1.4, DS-1.5 (die sonst eigene Gates bauen). **Höchste Priorität** — deckt sich mit der Auth-Audit-Wurzel ([project_auth_audit_2026_06]).

### DS-0.3 · Blob-Chunk-Encoding vereinheitlichen (H6, M6)
- **Problem:** Native Writer schreibt per-chunk-base64 von raw-byte-slices; Browser schreibt whole-blob-base64 + String-Slice und liest concat-dann-decode. `256000 % 3 == 1` → `=`-Padding mitten im Stream → Browser-Reader korrumpiert. Generierte DOCX > 250 KB (`ctox_generated_docx`) korrumpieren beim Browser-Öffnen. Keine Content-Integrität auf der document/spreadsheet-Lane.
- **Dateien:** `src/core/business_os/store.rs:22474-22476` (native write, `DOCUMENT_BLOB_CHUNK_SIZE` ~55), `:22057,22309` (Writeback-Pfad); `src/apps/business-os/modules/documents/index.js:2142-2169`; `src/apps/business-os/modules/spreadsheets/index.js:2050-2064`; `src/core/business_os/rxdb_peer.rs:6701-6726`; Vorbild `src/apps/business-os/desktop-apps/.../file-integrity.js:144-202`.
- **Fix:** Eine Konvention — **per-chunk base64 von raw-byte-slices** (ermöglicht Range/Streaming) — für alle Writer + Demand-Fetch-Source. `chunk_id`-Schema angleichen (`{blob_id}_{idx:04}`). Per-Chunk- + whole-content-sha256 zu `document_blob_chunks`/`spreadsheet_blob_chunks` ergänzen und bei Reassembly verifizieren.
- **Akzeptanz:** Cross-Runtime-Round-Trip-Test (native write → browser read und umgekehrt) auf Byte-Identität für Blobs > 1 Chunk; sha256-Mismatch wirft.
- **Validierung:** `node src/apps/business-os/rxdb/tests/run-all.mjs`; gezielter Rust-Test im store-Crate; falls Schema/Cache-Buster betroffen, RxDB-`dist/` neu bauen (siehe Leitplanken).
- **Abhängigkeit:** vor DS-2.4.

### DS-0.4 · notes Klartext-Leak (H13)
- **Problem:** „Zero-Knowledge"-Notes synchronisieren Titel (erste Zeile des entschlüsselten HTML) **und** `lock_passcode` im Klartext in die replizierte Collection — die Verschlüsselung ist damit hinfällig.
- **Dateien:** `src/apps/business-os/modules/notes/index.js:1226-1242,1309-1338,1734`; Claim `index.html:271`.
- **Fix:** `lock_passcode` nie synchronisieren — nur PBKDF-Salt/Verifier persistieren. Titel gesperrter Notes nicht im Klartext speichern: entweder mitverschlüsseln oder durch Platzhalter-Label ersetzen. `updated_at`-Leak akzeptieren oder dokumentieren.
- **Akzeptanz:** Test/Inspektion: bei `is_locked` enthält das replizierte Dokument weder Klartext-Passcode noch ableitbaren Titel.
- **Validierung:** `node src/apps/business-os/modules/notes/notes.test.mjs` (erweitern); manueller RxDB-Record-Dump.
- **Abhängigkeit:** keine.

### DS-0.5 · SSRF-Guard für outbound Fetches (M2)
- **Problem:** Importer fetcht beliebige user-gelieferte URLs ohne Host/IP-Allowlist (`ureq::get`); auch ungegateter Report-figure-add-Pfad.
- **Dateien:** `src/core/business_os/importer.rs:651-659,1075-1082`; `src/core/report/cli.rs:1443-1500`; Referenz gegateter Pfad `src/core/report/sources/full_text.rs:32-83`.
- **Fix:** Geteilte Guard-Funktion: nur `https`, Host auflösen und RFC1918/Loopback/Link-Local/Metadata-IPs ablehnen, konfigurierbare Host-Allowlist via **SQLite-Runtime-Store** (kein Env-Toggle). Vor jedem outbound-Fetch in Import-/Report-Lanes anwenden.
- **Akzeptanz:** Test mit `http://`, `http://169.254.169.254`, `http://10.x` → abgelehnt; erlaubter https-Host → durch.
- **Validierung:** gezielter `cargo test` im business_os-/report-Crate.
- **Abhängigkeit:** keine.

### DS-0.6 · ats.signature.sign an Signer-Identität binden (H11)
- **Problem:** Handler setzt Signer per ID-Match auf „signed", vergleicht nie mit dem authentifizierten Session-Actor; `signed_artifact_id` wird nie geschrieben. Ein State-Flip released Billing + AÜG-Gate.
- **Dateien:** `src/core/business_os/store.rs:24155-24202` (sign-Handler), `:23963-24020` (`ats.leistungsnachweis.signoff`-Consumer), `:23626` (`ats_actor_value`); `src/core/business_os/ats_gates.rs:276-282` (`signature_request_status`); `src/apps/business-os/modules/esign/{index.js:47-79,collections.schema.json:26}`.
- **Fix:** Verifizierten Token-Actor mit dem Signer-Record matchen (kein Self-Service-Flip für fremde Signer ohne Authentifizierung); unveränderliches signiertes Artefakt/Hash persistieren (`signed_artifact_id` schreiben). Solange echtes Signing fehlt, abhängige Legal-Gates (`leistungsnachweis.signoff`, AÜG) als non-production markieren.
- **Akzeptanz:** Test: sign mit nicht-matchendem Actor → abgelehnt; erfolgreicher sign schreibt Artefakt-Hash; `signature_request_status=="completed"` nur mit echten Signaturen.
- **Validierung:** `cargo test` ats_gates/store; `esign`-Modultest.
- **Abhängigkeit:** profitiert von DS-0.2 (Session-Auflösung), kann aber eigenständig landen.

---

## Phase 1 — Kaputte Funktionalität reparieren

### DS-1.1 · OCR: implementieren oder ehrlich abschalten (H2, M9)
- **Problem:** `ocr_enabled` default `true`, aber kein OCR-Code; `render_page_image` toter Seam ohne Caller. Scan-PDFs liefern leeren Text mit `Ok`. Matching-UI akzeptiert Bilder, die der PDF-only-Parser still verschluckt.
- **Dateien:** `src/tools/pdf-parse/src/core/config.rs:5-8,51-53`; `src/tools/pdf-parse/src/engines/pdf/pdfium_backend.rs:209-234` (`render_page_image`); `src/core/business_os/importer.rs:2323-2335`; `src/apps/business-os/modules/matching/index.html:122`, `ctoxCommandAdapter.js:57-78`.
- **Entscheidung (strategisch):** (A) OCR über `render_page_image` + Engine (`ocrs`/tesseract via privates LocalTransport-IPC, **kein** Loopback-HTTP) implementieren, ODER (B) `ocr_*`-Config entfernen und ein explizites `"no extractable text (scanned/needs OCR)"`-Signal statt leerem Body emittieren.
- **Sofort (unabhängig von A/B):** Bild-Accept im matching-UI entfernen, bis OCR existiert; bei Bild-Input expliziter „unsupported/needs OCR"-Fehler.
- **Akzeptanz:** Scan-PDF/Bild liefert ein erkennbares „needs OCR"-Signal statt stillem Erfolg; Pfad A zusätzlich: Round-Trip-Test auf einem Scan-Fixture.
- **Validierung:** `cargo test -p ctox-pdf-parse`; matching-Modultest.

### DS-1.2 · Nativer DOCX-Writer (H7, H7b)
- **Problem:** Einziger DOCX-Writer ist ein Python-Subprozess (`python-docx`); zudem sucht der Render-Pfad `root/skills/...`, aber System-Skills liegen in SQLite (`include_dir!`), nie auf der Platte → `ctox report render --format docx` scheitert auf jedem Runtime.
- **Dateien:** `src/core/report/render/docx.rs:1-12,82-196`; `src/core/report/cli.rs:3031-3060` (Pfad-Resolution `:3038-3042`); `src/core/skill_store.rs:20-28,458-484`.
- **Fix (bevorzugt):** Nativer Rust-DOCX-Writer gegen die `zip`-Crate (schon Dependency für Reads), aufbauend auf DS-2.1. **Minimal-Bridge** (falls Writer später kommt): Skript-Pfad über `materialize_skill_bundle` auflösen statt `root/skills/`.
- **Akzeptanz:** `ctox report render --format docx` produziert eine valide .docx auf einem frischen Runtime (kein `python-docx`); `#[test]` in `docx.rs` als Smoke + Strukturprüfung.
- **Validierung:** gezielter `cargo test` report; manueller `ctox report render --format docx`.
- **Abhängigkeit:** bevorzugter Pfad nach DS-2.1.

### DS-1.3 · Nativer DOCX/Markdown-Read-Pfad + App-Surface (M11, H3, M12)
- **Problem:** DOCX/Markdown-Import läuft nur im vendored Browser-`document-format.mjs`; dessen Source importiert `word-port`, der nur unter ignoriertem `archive/` existiert → laufzeit-zerbrechlich. Kein nativer Reader, keine CLI/MCP-Surface, um eine .docx in die documents-Collection zu ingestieren.
- **Dateien:** `src/apps/business-os/modules/documents/index.js:175,482-486`; `document-format/src/index.ts:1,10`; `scripts/build-business-os-vendor.mjs:10,122`; `src/core/business_os/mcp_channel.rs:741` (tool_descriptors).
- **Fix:** Nativen DOCX/MD-Read-Pfad auf doc-stack konsolidieren (siehe DS-2.2); `business_commands`-Import-Surface + Harness-Tool + MCP-Descriptor ergänzen. `word-port`-Import aus `archive/` in den aktiven Tree ziehen oder ersetzen.
- **Akzeptanz:** Agent kann via CLI/Command eine .docx in `document_versions` ingestieren; Browser-Import nutzt denselben nativen Pfad oder einen stabilen vendored Build.
- **Validierung:** doc-stack-Crate-Tests; `node .../run-all.mjs`; MCP-Channel-Smoke.
- **Abhängigkeit:** nach DS-2.1/DS-2.2.

### DS-1.4 · Nativer credential.verify-Command (H12, M19)
- **Problem:** Deployment-/AÜG-Gate vertraut einem browser-gesetzten `verified`-Flag; kein `ats.*`-Arm schreibt `verified=true`. nachweise-Modul schreibt Credentials per direktem `col.insert` statt Command.
- **Dateien:** `src/core/business_os/ats_gates.rs:52-58`; `src/apps/business-os/modules/nachweise/index.js:117-153`; `src/core/business_os/store.rs:23239-23400` (ats-Command-Set).
- **Fix:** Nativen, gegateten `ats.credential.capture` + `ats.credential.verify`-Command (schreibt `verified=true` + `verified_by` aus authentifizierter Session). nachweise-Modul auf Command umstellen. Liefert zugleich die fehlende Capability-Symmetrie für Credentials.
- **Akzeptanz:** Gate ist nur nach nativem verify passierbar; Browser kann `verified` nicht direkt setzen; unautorisierter verify abgelehnt.
- **Validierung:** `cargo test` ats_gates/store; nachweise-Modultest.
- **Abhängigkeit:** auf DS-0.2 (Chokepoint/Session).

### DS-1.5 · Matching-Scorer entkernen (H10, M10)
- **Problem:** Scorer ist eine in Rust kompilierte hardcodierte deutsche Recruiting-Keyword-Liste; off-domain-Rollen landen auf fixen 28%. SKILL.md verspricht LLM-Reasoning; Browser baut einen toten LLM-Prompt-Pfad, den die native Seite nie aufruft.
- **Dateien:** `src/core/business_os/importer.rs:3530-3669,3707-3718,113,1011`; `src/apps/business-os/modules/matching/{matchingTools.js:104-136,ui/ctoxCommandAdapter.js:30-119}`; `src/skills/packs/business/business-os-requirement-matching/SKILL.md`.
- **Fix:** Keyword-Vokabular aus dem kompilierten Code in **SQLite-Runtime-Store** auslagern (kein Env). Echtes requirement↔CV-Term-Overlap oder Embedding-Scoring (doc-stack-`EmbeddingExecutor` ist vorhanden). Toten Browser-LLM-Pfad entfernen **oder** nativ echtes LLM-Scoring implementieren — und den SKILL.md-Vertrag mit der Realität in Einklang bringen.
- **Akzeptanz:** Test-Fixtures (in-domain + off-domain + leer) mit plausibler Score-Spreizung; kein toter Prompt-Pfad mehr; SKILL.md beschreibt das tatsächliche Verhalten.
- **Validierung:** `cargo test` importer; matching-Modultest; `commands process <id>`-Smoke.
- **Abhängigkeit:** profitiert von DS-2.1 (Embedding-Surface), nicht hart.

### DS-1.6 · Toten Report-Code entfernen (H8)
- **Problem:** ~80 KB „staged report"-Pipeline (`claims/evidence/scoring/state_machine/manuscript-v1/store/runs/scope/blueprints`) nirgends als `mod` deklariert; `claims.rs:21` importiert undeklariertes `blueprints` → nicht kompilierbar. Eigenes paralleles `Status`-Enum. Irreführt Reviewer/Agenten massiv.
- **Dateien:** `src/core/report/mod.rs:23-37`, `cli.rs:16-35`; die o.g. Orphan-`.rs`.
- **Fix:** Orphan-Tree löschen oder nach `archive/` verschieben. Prüfen, ob einzelne Bausteine bewusst WIP sind (`scoring.rs` zuletzt Jun 24 angefasst) — falls ja, in ein Tracking-Ticket statt löschen.
- **Akzeptanz:** `cargo check` unverändert grün; `rg "mod claims|mod evidence|mod scoring|mod state_machine"` leer; keine toten Tests mehr.
- **Validierung:** `cargo check`; `cargo fmt --check`.
- **Abhängigkeit:** keine (reine Hygiene, früh erledigbar).

---

## Phase 2 — Strukturelle Konsolidierung (zieht die Wurzeln)

### DS-2.1 · Kanonische Document-Capability-Engine (H16) — Fundament
- **Problem:** 4 inkompatible Document/Version/Blob-Repräsentationen (documents/spreadsheets/cv-print/notes); inkohärente Versionierung (native `MAX(version)+1` vs. documents hardcoded `_v1`); 3 divergente Rich-Text-Engines.
- **Dateien (Ist):** `src/apps/business-os/modules/{documents,spreadsheets,notes,cv-print-builder}/schema.js` + Blob-Plumbing; `src/core/business_os/store.rs` (Writeback/Version-Logik ~22903-Region; `documents/index.js:479,503`).
- **Fix:** **Ein** kanonisches Document/Version/Blob-Modell als geteilte native Engine (Schema + Version-Semantik + Blob-Lane, aufbauend auf DS-0.3-Encoding) **plus** ein geteilter Browser-Helper. Einheitlich exponiert über CLI / `business_commands` / MCP. Eine Rich-Text-Engine + ein Content-Modell wählen (SuperDoc *oder* Lexical), mit geteilter Sanitization/Serialization.
- **Akzeptanz:** documents schreibt echte inkrementelle Versionen; ein Schema/Helper, den die anderen Module konsumieren; Wire-Contract-Fixtures regeneriert (beide Seiten), `dist/` neu gebaut, Cache-Buster gebumpt.
- **Validierung:** `node .../run-all.mjs`; `cargo test --manifest-path src/core/rxdb/Cargo.toml`; `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`.
- **Abhängigkeit:** nach DS-0.3; Fundament für DS-2.2/2.4, DS-1.2/1.3. **Größtes Stück — eigene RFC/Teil-Tickets sinnvoll.**

### DS-2.2 · OOXML-Reader/Writer konsolidieren (H4, M16)
- **Problem:** Vier unabhängige zip+roxmltree-Extraktoren + ein OOXML-Writer; Zip-Bomb-Limits/Bugfixes N-fach gepflegt. Keine Decompression-Ratio-Guards.
- **Dateien:** `src/tools/doc-stack/src/parse.rs:203-296,788-816`; `src/core/report/cli.rs:3325-3446`; `src/core/business_os/store.rs:24794-24823` (+ Writer ~22569); `src/core/service/service.rs:9075-9190`; Browser `universal-importer.js:910-1019`.
- **Fix:** doc-stack zum kanonischen OOXML-Reader machen; die anderen drei darauf umstellen. Geteilter Zip-Open-Helper mit Decompression-Ratio- + Total-Size-Limit. Browser-XLSX-Parser: DOCTYPE/Entities explizit neutralisieren (XXE).
- **Akzeptanz:** ein Reader-Pfad; Zip-Bomb-Test failt kontrolliert; kein duplizierter Extraktor mehr (`rg` zeigt einen Owner).
- **Validierung:** doc-stack-Crate-Tests; report/service-Tests; `node .../run-all.mjs`.
- **Abhängigkeit:** nach DS-2.1.

### DS-2.3 · doc-stack + report symmetrisch machen (H3, M12, L10)
- **Problem:** doc-stack ist harness-only (nur `search`/`read`-Tools; `corpus`/`index` nicht als Agent-Tool, keine App-Surface). report ist CLI-only ohne RxDB-Projektion. MCP exponiert **keine** Dokument-Operation.
- **Dateien:** `src/core/harness/core/src/tools/spec.rs:4233-4248`; `src/tools/doc-stack/src/surface.rs:150-204`; `src/core/business_os/mcp_channel.rs:741`; `src/core/report/` (Projektion fehlt).
- **Fix:** `corpus`/`index` als Agent-Tools registrieren; `business_commands`-Search/Index-Surface in `handle_doc_command`; report-Outputs als RxDB-Projektion exponieren (statt nur Datei). Parse/Import/Search-Descriptors zum MCP-`tool_descriptors`-Set.
- **Akzeptanz:** Agent + App + MCP können dieselbe Parse/Search-Fähigkeit nutzen; Symmetrie-Matrix im Review von „durchgefallen" auf „symmetrisch" für doc-stack/report.
- **Validierung:** harness-Tool-Spec-Tests; `node .../run-all.mjs`; MCP-Smoke.
- **Abhängigkeit:** nach DS-2.1 (geteilte Engine).

### DS-2.4 · cv-print / spreadsheets auf das kanonische Blob-Modell falten (H14, H15, L15, L27)
- **Problem:** cv-print nutzt 16 KiB-Chunks auf `desktop_file_chunks`; spreadsheets ist Fork-Copy der documents-Blob-Lane; spreadsheet-Formelwerte sind display-only und werden nie persistiert/exportiert; spreadsheets hat keine native Engine.
- **Dateien:** `src/apps/business-os/modules/cv-print-builder/index.js:986-1034`; `src/apps/business-os/modules/spreadsheets/index.js:892,948-990,1054,1511,1538`; `store.rs:15800-15802`.
- **Fix:** cv-print + spreadsheets auf die DS-2.1-Blob-/Version-Lane kollabieren. Spreadsheet: berechnete Werte ins Modell zurückschreiben (`setValueFromCoords`) **oder** native Formel-Evaluation als Engine-Teil; native Spreadsheet-Engine bereitstellen oder die Runbook-Commands entfernen, bis eine existiert.
- **Akzeptanz:** ein Blob-Format über alle Module; CSV/JSON-Export emittiert berechnete Werte, nicht rohe `"=..."`; Spreadsheet-Commands haben einen nativen Handler oder sind entfernt.
- **Validierung:** Modultests documents/spreadsheets/cv-print; `node .../run-all.mjs`.
- **Abhängigkeit:** nach DS-0.3 + DS-2.1.

### DS-2.5 · WebRTC-File-Fetch autorisieren (M3)
- **Problem:** `run_file_fetch` hat keinen Authorization-Branch; jeder verbundene Peer kann jede Datei per ID streamen (`desktop_file`/`document_blob`/`spreadsheet_blob`). `FILE_FETCH_ERROR_UNAUTHORIZED` ist definiert, aber ungenutzt.
- **Dateien:** `src/core/business_os/file_fetch_handler.rs:64,193-278`; `src/core/business_os/rxdb_peer.rs:6578-6664`.
- **Fix:** Authentifizierte Peer/Actor-Identität in die `FileChunkStreamFn`-Closure durchreichen; jeden Fetch über `policy.rs` gaten (Collection-Scope + Record-Owner/Modul) vor dem Streamen; bei Denial `FILE_FETCH_ERROR_UNAUTHORIZED`.
- **Akzeptanz:** Fetch ohne Berechtigung abgelehnt; berechtigter Fetch streamt; Test deckt Deny + Allow.
- **Validierung:** `cargo test` file_fetch/rxdb_peer.
- **Abhängigkeit:** nutzt den DS-0.2-Session-Mechanismus.

---

## Phase 3 — Hygiene & Härtung

- **DS-3.1 · Latente HTTP-Handler aus `server.rs` löschen (M1):** knowledge/document-, dataframe/rows-, reports-, users-, channels-Match-Arme + Payload-Funktionen (`server.rs:421-444,2180-2377`) entfernen; `assert-rxdb-only.mjs` erweitern, sodass es failt, wenn `knowledge_dataframe_rows_payload`/`knowledge_document_payload`/`scan_parquet` in einem HTTP-Match-Arm auftauchen.
- **DS-3.2 · Typisierte Invoices/Ats-Permissions (M5):** `InvoicesManage`/`AtsManage` (oder scoped `DataWrite`) in `policy.rs:40-114` ergänzen; `store.rs:14981,15005` darüber evaluieren statt coarse `manage_all`.
- **DS-3.3 · Invoices-Steuer-Mapping daten-getrieben (M4):** Steuer-Account-Mapping (inkl. 16 %/ausländische Sätze) aus dem SQLite-Runtime-Store; unmapped-Satz hart failen statt still droppen; native + JS auf gemeinsame Journal-Logik cross-checken. `invoices.rs:945-997`, `invoice-types.js:146-156`.
- **DS-3.4 · Idempotenz & Dedup (M7, M8, M20, L33):** `source_sha256`-Pre-Insert-Lookup für Import-Idempotenz; CV-Rohtext einmal in eine Blob-Lane statt 2–4× inline; `dunning.letter.send`-Idempotenz-Guard; verwaiste `*_chunks` GC (documents-Draft-Leak L12, desktop-Trash L33).
- **DS-3.5 · Report-Renderer-Korrektheit + Tests (M13, M14, L40):** Kind-Allowlist in `render/markdown.rs:360-387` mit dem Strip-Verhalten in `manuscript.rs:399-408` angleichen (sonst stiller Tabellenverlust); `ascii_dashes()` nicht auf Code-Spans/URLs; Render-Fidelity-Tests (Markdown-Snapshot, Cross-Ref, Table) + DOCX-Smoke.
- **DS-3.6 · Test-Gates verdrahten (L6, L28, L37):** realer PDF-Eval-Korpus in `cargo test` (`evaluation.rs:329`); invoices-Browser-Tests + cv-print-Modultest in `run-all.mjs`/`package.json` aufnehmen.
- **DS-3.7 · Kleinkram:** doc-stack Default-Corpus-Root entfernen (M17), notes `CustomHTMLNode` innerHTML sanitisieren (L25), file-viewer-iframe `sandbox`-Attribut (L32), Registry-/README-Claims korrigieren (L19 „Native XLSX", L38 invoices-README), Doppel-Registrierungen `notes/notizen` (L26).

---

## Validierungsmatrix (pro Bereich)

| Bereich | Pflicht-Checks |
|---|---|
| pdf-parse / doc-stack | `cargo test -p ctox-pdf-parse`, doc-stack-Crate-Tests, `cargo fmt --check` |
| business_os native (store/importer/invoices/ats/policy/server) | gezielter `cargo test`, `cargo check`, `cargo fmt --check` |
| CTOX-DB / Wire-Contract | `node src/apps/business-os/rxdb/tests/run-all.mjs`, `cargo test --manifest-path src/core/rxdb/Cargo.toml`, `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml` |
| Browser RxDB `src/`-Änderung | `dist/` neu bauen (gepinntes esbuild), drei `?v=`-Cache-Buster bumpen, dann JS-Suite |
| Datengrenze | `node src/apps/business-os/scripts/assert-rxdb-only.mjs` muss grün bleiben/erweitert werden |
| Module | jeweiliger `*.test.mjs` |

Wenn ein Check nicht laufen kann: explizit benennen, was nicht lief und warum (AGENTS.md).

---

## Risiken & Hinweise

- **Paralleler Codex-Agent auf `main`:** Repo bewegt sich; atomar committen, eigene Dateien stagen, Rebase-Autostash, nicht auf mid-flight-Core-Changes aufsetzen ([project_parallel_agent_on_main]).
- **DS-2.1 ist groß** — als eigene RFC + Teil-Tickets behandeln, nicht als Einzel-Commit. Migration der vier Module ist schrittweise (pro Modul ein Commit).
- **`store.rs`-Zeilen sind durch parallele Edits versetzt** — vor jedem Ticket die zitierte Funktion per Symbol-Suche re-lokalisieren, nicht blind auf Zeilennummern verlassen.
- **DS-0.2 zuerst:** verhindert, dass DS-0.6/DS-1.4/DS-1.5 je einen eigenen Ad-hoc-Gate bauen, der später erneut konsolidiert werden muss.
- **Build-Target nach `/tmp`** (`CARGO_TARGET_DIR=/tmp/...`), nicht in den iCloud-Documents-Checkout ([reference_build_target_must_be_tmp]).
