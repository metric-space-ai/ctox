# Technisches Review: Der CTOX Document Stack

Stand 2026-06-25 · Lead-Review auf Basis verifizierter Subsystem-Digests und Cross-Cutting-Audits · alle Zeilenangaben gegen den aktuellen `main`-Checkout geprüft (mehrere `store.rs`-Zitate sind durch parallele Edits zeilenversetzt — korrigierte Werte stehen jeweils dabei)

---

## 1. Überblick — Was der Document Stack ist

Der "Document Stack" ist kein einzelnes Subsystem, sondern eine lose gekoppelte Sammlung von Engines und Apps entlang vier Schichten. Es gibt **kein gemeinsames Dokumentenmodell** und **keinen gemeinsamen Policy-Chokepoint** — die Schichten sind durch Konvention verdrahtet, nicht durch ein geteiltes Datenmodell.

**Schicht 1 — Native Rust-Engines (Daemon-seitig):**
- `ctox-pdf-parse` / "LiteParse" (`src/tools/pdf-parse`) — die geteilte PDF-Textquelle (pdfium-Geometrie + reine Rust-Reflow-Pipeline).
- `ctox-doc-stack` (`src/tools/doc-stack`) — ~18-Format-Parser + SQLite/FTS5-Index + Embedding-Suche (read/retrieval only).
- `report-engine` (`src/core/report`) — deterministische Manuskript-Assemblierung mit Markdown/JSON/DOCX-Rendering (create only).
- `invoices` (`src/core/business_os/invoices.rs`) — GoBD-Buchungsengine (XRechnung-XML, kein PDF).
- `importer` (`src/core/business_os/importer.rs`) + `store.rs` cv-print/documents-Writeback — Projektion in Business-OS-Records.
- `ats_gates.rs` — ATS-Entscheidungsprimitive (Credential/Consent/Signatur/Leistungsnachweis).

**Schicht 2 — Integration/Projektion:**
- `capabilities/doc.rs` + `doc_stack/mod.rs` (CLI-Shim + Embedding-Executor über privates LocalTransport-IPC).
- `rxdb_peer.rs` — projiziert `business_records` in den RxDB-Store und repliziert über WebRTC; registriert `document_blob_chunks` / `spreadsheet_blob_chunks` / `desktop_file_chunks` als Demand-Fetch-Quellen (`rxdb_peer.rs:6578-6727`).

**Schicht 3 — Browser-Module (Business OS):**
- `documents` (DOCX/Markdown, SuperDoc), `spreadsheets` (JSpreadsheet/HyperFormula), `notes` (Lexical), `cv-print-builder`, `matching`, ATS-Module (`intake`/`submissions`/`placements`/`nachweise`/`esign`), `desktop-apps/{explorer,file-viewer}`.

**Schicht 4 — Skills (Agent-Verträge):**
- `ctox-cv-print-parser`, `business-os-import-parser`, `business-os-requirement-matching`, `ctox-business-os-mcp`.

**Datenfluss (kanonischer Pfad):**

```
Datei/Bytes ──> Native Engine (pdf-parse / importer / cv-print) ──> business_records (SQLite)
                                                                       │
                          rxdb_peer.rs Projektion ──────────────────────┘
                                                                       │
            WebRTC/RxDB Replikation (records + Demand-Fetch-Blob-Chunks)
                                                                       │
                                                              Browser-Modul (ctx.db)
```

Inbound-Bytes reisen base64-kodiert **innerhalb** der `business_commands`-RxDB-Collection (`command-bus.js:49-67`). Outbound-Blob-Bytes ausschließlich über `rxdb.file.fetch`-Demand-Fetch. **Kein HTTP-Datenpfad** im aktiven Stack — siehe §5.

---

## 2. Fähigkeiten-Matrix

### 2a. Pro Format (read / write / none)

| Format | Read | Write/Create | Engine / Pfad | Status |
|---|---|---|---|---|
| **PDF** | ✅ Text+Layout (pdfium, **kein OCR**) | ❌ none | `ctox-pdf-parse`; cv-print nutzt zusätzlich `pdftotext`-Subprozess | implementiert; scan-only PDFs → leerer Text ohne Fehler |
| **DOCX** | ✅ read (doc-stack: Text/Headings/Tabellen-flach; documents: SuperDoc) | ⚠️ write (Report-Engine via **Python python-docx**, NICHT nativ; documents via SuperDoc-Export) | 4 unabhängige OOXML-Reader + 1 Python-Writer | read solide, write nicht-nativ + laufzeit-kaputt |
| **XLSX** | ⚠️ read browser-only (universal-importer.js ZIP-Parser); doc-stack: Zellwerte ohne Formeln | ❌ none | kein nativer XLSX-Reader/Writer | app-only, kein nativer Pfad |
| **PPTX** | ⚠️ read nur doc-stack (Slide-Text), **untestet** | ❌ none | doc-stack | stub-nah |
| **HTML** | ✅ read (scraper / Job-Posting-Parse) | ❌ none | importer + doc-stack | implementiert |
| **E-Mail/EML** | ✅ read (Header + best body, doc-stack) | ❌ none | doc-stack | implementiert+getestet |
| **Markdown** | ✅ read (heading-aware) | ✅ write (documents-Export, Report-Render) | documents + report | implementiert |
| **TXT/CSV/TSV/JSON/YAML** | ✅ read | CSV/JSON write nur spreadsheets (browser) | doc-stack + spreadsheets | read implementiert |
| **ODT/ODS/ODP/RTF/XML** | ✅ read (RTF lossy) | ❌ none | doc-stack | **untestet** |
| **XRechnung-XML** | — | ✅ write (browser-only) | invoices `core/invoice-xrechnung.js` | implementiert |

### 2b. Pro Fähigkeit

| Fähigkeit | Status | Beweis |
|---|---|---|
| **Parse PDF** | ✅ implementiert+getestet (synthetisch) | `pdf-parse/src/parser.rs:45,84`; reale PDFs nicht in `cargo test` |
| **OCR (scan-PDF)** | ❌ **NICHT IMPLEMENTIERT** (Config lügt) | `config.rs:5-8,51-53` `ocr_enabled=true` default, kein OCR-Code |
| **Parse DOCX/XLSX/EML/MD** | ✅ read (doc-stack), tiefenbegrenzt | `parse.rs:1044-1158` |
| **Index + lexikalische/semantische Suche** | ✅ implementiert+getestet (Fakes) | `doc-stack/store.rs:666-667`, `surface.rs:1014-1070` |
| **FTS über Business-OS-Dokumente** | ❌ **keine** | doc-stack indexiert nur lokales Dateisystem, nie BOS-Collections |
| **Create/Render DOCX** | ⚠️ Report-Engine, Python-Subprozess, **laufzeit-kaputt** | `report/render/docx.rs:82-196` |
| **Render Markdown/JSON** | ✅ pure Rust | `report/render/markdown.rs` |
| **View PDF** | ✅ iframe (file-viewer, cv-print) | `file-viewer/app.js:266-268` (kein sandbox-Attribut) |
| **Edit DOCX** | ✅ SuperDoc (browser) | `documents/index.js:1893-1998` |
| **Edit Rich-Text** | ⚠️ **3 divergente Engines** (SuperDoc / Lexical / textarea) | siehe High-Findings |
| **Versionierung** | ❌ **inkohärent** (nativ inkrementiert, browser überschreibt v1) | siehe High-Findings |
| **Sign (e-Signatur)** | ❌ **STUB** (State-Flip, keine Signer-Bindung) | `store.rs:24155-24202` |
| **Print/Export** | ⚠️ Browser-Print-Dialog (cv-print), Blob-Download (documents/invoices) | kein PDF-Renderer irgendwo |
| **Import (Spreadsheet)** | ⚠️ app-only (browser XLSX/CSV-Parser) | `universal-importer.js:429-1054` |
| **Matching (CV↔Vacancy)** | ⚠️ implementiert aber **degeneriert** (hardcodierte dt. Keywords) | `importer.rs:3602-3669` |

**Kernlücken auf einen Blick:** kein OCR, kein nativer DOCX-Writer, kein nativer DOCX-Reader, kein nativer XLSX-Pfad, keine FTS über Business-OS-Dokumente, keine echte e-Signatur, keine funktionierende Versionierung.

---

## 3. Stärken — Was wirklich solide ist

1. **Die Datengrenze hält über den gesamten aktiven Stack.** `server.rs:225-232` hart-gated jeden `/api/business-os/*`-Pfad außerhalb von 5 Control-Plane-Einträgen mit `410` ("Business OS HTTP data APIs are disabled; use RxDB/WebRTC."), und `assert-rxdb-only.mjs` pinnt das Gate-Prädikat als Mandatory-Guard. Alle Blob-Bytes reassemblieren strikt über `rxdb.file.fetch`. Der einzige Browser→CTOX-HTTP-Call ist die Capability-Token-POST (`command-bus.js:30`, Control-Plane, erlaubt). **Das ist die am konsequentesten durchgesetzte Architektur-Regel im Stack.**

2. **`ctox-pdf-parse` ist eine ernsthafte, self-contained Engine.** Reine-Rust-Reflow-Pipeline (Char-Line-Clustering, Two-Column/Right-Rail-Split, Rotation, Text-Cleanup) mit 12 Unit-Tests über synthetische Geometrie (`parity.rs`). Sauberer `PdfEngine`-Trait-Seam (`engines/pdf/interface.rs`). Sie ist die **eine** geteilte PDF-Quelle für doc-stack, importer, web-stack und report.

3. **Die `invoices`-Engine ist server-authoritativ und gut getestet.** `invoices.rs` ist der einzige Writer; Posting läuft in einem SQLite-SAVEPOINT (`invoices.rs:589`); Identität kommt aus einem nativ-signierten Capability-Token, nie dem behaupteten Actor (`store.rs:19865-19947`). Substanzielle `#[cfg(test)]`-Suite (`invoices.rs:1893-2872`) deckt Posting/Numbering/Allocation/Skonto/Dunning ab.

4. **`ats_gates.rs` hat gründliche native Unit-Tests** der Entscheidungsprimitive (Credential-Window inkl. <24h-Regression, Deployment-Gate, Consent, Retention, Double-Submission, Leistungsnachweis-Surcharge-Mathematik).

5. **Der doc-stack-`EmbeddingExecutor`-Trait** (`lib.rs:11`) entkoppelt die Engine sauber von der Inferenz und erzwingt privates LocalTransport-IPC mit explizitem Verbot von Loopback-HTTP (`doc.rs:52-55`). Eine App-Surface wäre dadurch billig nachrüstbar.

6. **Die desktop-files Demand-Fetch-Schicht** hat echte Integritätsgarantien (per-Chunk + whole-content sha256 in `file-integrity.js`) und einen dedizierten Guard (`assert-rxdb-only.mjs:454-504`), der die File-Chunk-Integrity-Vertrag-Contract für file-viewer und explorer pinnt.

---

## 4. Befunde nach Schwere

### HIGH

---

**H1 — UTF-8 Byte/Char-Index-Verwechslung in der Margin-Entfernung paniced auf realen PDFs**
`src/tools/pdf-parse/src/processing/clean_text.rs:29` (`min_x` via `chars().position`), `clean_text.rs:48-49` (`if line.len() > min_x { &line[min_x..] }`), Aufruf aus `clean_text.rs:62` + `grid_projection.rs:1153`; Caller `importer.rs:853`.

`min_x` ist ein **Char**-Index, wird aber als **Byte**-Offset in `&line[min_x..]` benutzt; der Guard vergleicht `line.len()` (Bytes) gegen einen Char-Index und garantiert keine Char-Boundary. **Panic reproduziert** (`/tmp/test_margin_panic.rs`): Input `" z"` (min_x=1) und `"\u{00A0}z"` (NBSP, 2 Bytes) → `byte index 1 is not a char boundary`. NBSP ist in realen PDFs allgegenwärtig. Der Pfad läuft auf **jeder Seite**.

**Warum es zählt:** Ein uncaught Panic, erreichbar aus untrusted PDF-Input (ATS-CV-Uploads, Web-Search-PDF-Fetches) auf dem dokumentierten deutschen/akzentuierten/CJK-Zielkorpus. Weder das `?` in `parse_pdf_text` (`importer.rs:2323`) noch das `unwrap_or_else` in `importer.rs:853` fangen einen Panic — kein `catch_unwind` im Pfad. doc-stack (`parse.rs:170`) hat dieselbe Exposition.

**Fix:** `min_x` als Byte-Offset über `char_indices()` berechnen, oder vor dem Slicing auf `is_char_boundary(min_x)` prüfen und ans nächste gültige Boundary runden. Multibyte-Margin-Test mit NBSP/Akzenten in `parity.rs` ergänzen.

---

**H2 — OCR ist in der Config angepriesen, existiert aber nicht; scan-PDFs liefern still leeren Text**
`config.rs:5-8,51-53` (`ocr_enabled` default `true`, `ocr_server_url`); kein OCR-Code; `render_page_image` ohne externe Caller (`pdfium_backend.rs:209`); `from_ocr` überall hardcoded `false`.

`LiteParseConfig` exponiert `ocr_language/ocr_enabled/ocr_server_url/dpi`, aber `rg` zeigt: diese Felder werden außerhalb `config.rs` nie gelesen. Keine OCR-Crate in `Cargo.toml` (kein tesseract/leptonica/paddle/ocrs). `render_page_image` (die OCR-Einspeisung) hat **null Caller** im gesamten Repo — toter Code. Scan-PDFs geben `Ok(ParsedPage{...})` mit leerem Text und ohne Warnung zurück (`parser.rs:34-71`).

**Warum es zählt:** Lebensläufe/Verträge, die Scans sind, indexieren als leerer Inhalt während Erfolg gemeldet wird. Der Default `ocr_enabled=true` bewirbt eine nicht-existente Fähigkeit gegenüber Callern.

**Fix:** Entweder OCR über `render_page_image` + eine Engine (ocrs/tesseract via IPC) implementieren, oder `ocr_*`-Config entfernen und ein explizites `"no extractable text (scanned/needs OCR)"`-Signal statt leerem Body emittieren. `render_page_image` ist die fertige Einspeisung.

---

**H3 — doc-stack hat keine business_commands / Business-OS-Surface (Capability-Symmetrie verletzt)**
`spec.rs:4233-4248` (nur `ctox_doc_search`/`ctox_doc_read`); grep über `src/core/business_os/` und ganz `src/apps/business-os/` nach `doc_stack|ctox-doc-stack|ctox_doc` = **leer**.

doc-stack ist eine vollwertige native Engine, aber harness-only. Apps können die native Parse/Search/Index-Engine nicht erreichen und fallen auf den separaten Browser-SuperDoc-Stack zurück. **Korrektur:** Das Digest-Zitat `superdoc.mjs (separate impl)` ist ein **False-Positive-Grep** — die Datei matcht nur auf das unverwandte DOM-Tool `superdoc_search`, nicht auf ein doc-stack-Symbol. Der substantielle Punkt (keine App-Surface) hält unabhängig durch den leeren repo-weiten Grep. Die CLI exponiert fünf Subcommands (corpus/formats/index/search/read); nur search+read sind Agent-Tools.

**Warum es zählt:** Verletzt die AGENTS.md-Regel ("native Engines mit thin CLI + business_commands"). Der trait-basierte Executor macht eine App-Surface billig.

**Fix:** `corpus`/`index` als Agent-Tools registrieren; eine `business_commands`-Search-Surface ergänzen, die in `handle_doc_command` mündet.

---

**H4 — Vier unabhängige OOXML-Parser im Tree**
`doc-stack/parse.rs:203-296`; `report/cli.rs:3325-3446`; **`store.rs:24794-24823`** (Korrektur: NICHT 24512 — Zeile 24512 ist ein ATS-Governance-Event-String); `service/service.rs:9075-9190`.

Vier unabhängige zip+roxmltree-Extraktoren, je mit eigener Cell/Run-Logik, kein geteilter Code: doc-stack (`parse_docx/pptx/xlsx_chunks`), report-Comment-Extraktor, `validate_and_extract_docx_text`, `inspect_xlsx_attachment`. Zip-Bomb-Limits, Namespace-Handling und Bugfixes müssen N-fach angewendet werden und driften. **Zusatz:** `store.rs` enthält obendrein einen OOXML-*Writer* (`build_fallback_report_docx ~22569`), die Streuung ist also noch breiter.

**Warum es zählt:** Sicherheits- (Zip-Bomb), Korrektheits- und Wartungsdrift über vier Stellen.

**Fix:** Auf doc-stack als kanonischen OOXML-Reader konsolidieren; die anderen drei darauf umstellen.

---

**H5 — Import/Document-Writeback-Commands umgehen server-seitige DataWrite-Policy vollständig**
Korrigierte Zeilen: `record_command` `store.rs:8528`; `reject_app_build_command_if_denied` `8612`; `app_build_command_policy_target` `8639`; `process_source_parse_command` **`12109`** (NICHT 11825); `accept_rxdb_business_command_with_origin` `14402`, Default-Fall `_ => {}` `15800` → `record_command` `15802`; ReplicatedPeer-Einstieg `rxdb_peer.rs:2713-2718`; SSRF `importer.rs:651,1075`.

Die einzige Policy-Hürde in `record_command` ist `reject_app_build_command_if_denied`, das nur greift, wenn das Ziel ein App-Build-Command ist (`ctox.business_os.app.modify/create` oder `target/mode=="app"`). `source.parse`/`matching.*`/cv-print/`documents.*` fallen durch und werden ohne Actor-/DataWrite-Check inserted + queued. `process_source_parse_command` validiert nur den Command-**Typ**. `importer.rs` enthält **null** Policy-Referenzen und führt `upsert_business_record` für candidates/matches/documents aus — plus server-seitiges `ureq::get` auf eine browser-gelieferte URL (SSRF). `BusinessOsPermission::DataWrite` (`policy.rs:42,114`) wird **nur** in Release-Review-Reconciliation benutzt (`store.rs:2418,2833,4049`), nie bei Command-Acceptance.

**Warum es zählt:** Jede Partei, die ein `business_commands`-Dokument über RxDB schreiben kann, kann Dokumente/Candidates/Matching-Records erzeugen/überschreiben und server-seitige URL-Fetches auslösen — mit nur einem browser-behaupteten Actor. Deckt sich mit der Auth-Audit-Wurzelursache.

**Fix:** Jedes record-mutierende Command durch **einen** server-seitigen Chokepoint routen, der die authentifizierte Session auflöst und DataWrite (scoped auf Ziel-Collection/Modul) evaluiert — innerhalb `record_command`/`accept_rxdb_business_command`, nicht pro Arm.

---

**H6 — `document_blob_chunks`-Reassembly: zwei inkompatible Encoding-Konventionen; native→browser ist bereits kaputt für Blobs > ~250KB**
Native Write `store.rs:22474-22476` (raw-byte chunks, dann per-chunk base64); `DOCUMENT_BLOB_CHUNK_SIZE=256_000` `store.rs:55`. Browser Write `documents/index.js:2142-2154` / `spreadsheets/index.js:2050-2064` (whole-blob base64, dann String-Slice). Browser-Reader `documents/index.js:2168-2169` (join, dann **einmal** decode). Nativer Demand-Reader `rxdb_peer.rs:6717` (per-chunk decode).

**Korrektur des Mechanismus (das Digest-Framing war falsch):** Die ursprüngliche Behauptung "korrekt nur durch 256000 % 3 == 0 Koinzidenz" ist faktisch falsch — `256_000 % 3 == 1`. Jeder volle native raw-Chunk produziert base64 mit `=`-Padding **mitten im Stream**. Der Browser-Reader (concat-dann-decode) scheitert/korrumpiert an diesem Mid-Stream-Padding — **empirisch verifiziert**. Damit ist der native→browser-Pfad für jeden Blob > einem Chunk (~250KB raw) **bereits kaputt**, nicht "koinzident korrekt". Dieser Pfad ist live: `process_documents_report_command` (`store.rs:22057`) → `writeback_generated_docx` (`store.rs:22309`) schreibt `ctox_generated_docx`-Blobs, die der Browser via `loadBlobBytes`/`mountSuperDocDocument` (`documents/index.js:1894`) öffnet. **Eine vom CTOX-Agenten generierte DOCX > 250KB korrumpiert beim Öffnen im Browser.** Die Gegenrichtung (browser-write, nativer per-chunk-decode) funktioniert nur, weil `256000 % 4 == 0`. Zusatzdiskrepanz: native `chunk_id` ist `{blob_id}_{idx:04}` (`store.rs:22475`) vs. browser unpadded `${blobId}_${idx}` (`documents/index.js:2146`).

**Warum es zählt:** Reale, untestete Cross-Runtime-Korruption für generierte/große Dokumente.

**Fix:** **Eine** Konvention wählen (per-chunk base64 von raw-byte-slices — ermöglicht Range-Requests/Streaming) und beide Browser-Writer + Demand-Fetch-Source darauf umstellen. Per-Chunk- und whole-content-sha256 zu `document_blob_chunks`/`spreadsheet_blob_chunks` ergänzen (desktop-files-Lane hat das bereits). Cross-Runtime-Round-Trip-Test auf Byte-Identität.

---

**H7 — DOCX-Writer ist nicht nativ — Python-(python-docx)-Subprozess; und der Render-Pfad ist laufzeit-kaputt**
`report/render/docx.rs:1-12,82-196` (`Command::new(python).arg(render_manuscript.py)`); `render_manuscript.py:73` (`import docx`); `cli.rs:3031-3060`.

`render_docx` spawnt einen Python-Subprozess und pipet das Manuskript-JSON über stdin. `render_manuscript.py` ist 857 Zeilen Python, das A4/Margins/Arial/Figures/Tables besitzt — außerhalb der Rust-Korrektheits- und Test-Surface. DOCX-Output erfordert `python3` + `python-docx` zur Laufzeit.

**Zweiter, schwerwiegender Defekt (H7b, eigenständig verifiziert):** `cmd_render` baut `skill_root = root.join("skills")...` (`cli.rs:3038-3042`), und `render_docx` hart-required `skill_root/scripts/render_manuscript.py` auf der Platte (`docx.rs:88-97`). Aber System-Skills werden via `include_dir!` in SQLite kompiliert (`skill_store.rs:21-28`) und on-demand nur unter `managed_materialized_skills_root` materialisiert (`skill_store.rs:458-484`) — **nie** unter `root/skills/...`. `find` zeigt: `render_manuscript.py` existiert nur unter `src/skills/...`, nicht `root/skills/...`. **`ctox report render --format docx` scheitert mit "render_manuscript.py not found" im aktuellen Live-Checkout UND auf jedem installierten Runtime.** `install.sh:972-996` bestätigt explizit, dass System-Skills nicht auf die Platte kopiert werden. Keine `#[test]` in `docx.rs` deckt das ab.

**Warum es zählt:** Eine dokumentierte User-facing-Fähigkeit ist überall non-funktional, ohne Test-Guard; und der einzige DOCX-Writer im Stack ist nicht-nativ.

**Fix:** Native Rust-DOCX-Writer (gegen die `zip`-Crate, die schon für Reads da ist) ODER — als Minimal-Bridge — den Skript-Pfad über `materialize_skill_bundle` auflösen statt `root/skills/`. Round-Trip-Test ergänzen.

---

**H8 — Komplette "staged report"-Pipeline ist toter, unkompilierter Code**
`report/mod.rs:23-37` (Modul-Liste lässt alle aus); `cli.rs:16-35` (use-Liste); grep: kein `mod claims|evidence|scoring|state_machine|store|runs|scope|blueprints`.

`claims.rs/evidence.rs/scoring.rs/state_machine.rs/manuscript(v1)/store.rs/runs.rs/scope.rs/blueprints/` sind nirgends als Modul deklariert (~80KB toter Rust). `claims.rs:21` importiert `crate::report::blueprints` — auch undeklariert, also nicht-kompilierbar. Eine echte parallele Engine-Modellierung mit eigenem `Status`-Enum (15 Varianten, `state_machine.rs:21`) vs. live `RunStatus` (10 Varianten, `schema.rs:35`) und eigenen Tabellen (`report_claims`/`report_requirements`). **Korrektur:** Nur `evidence.rs`, `state_machine.rs`, `store.rs` tragen `#[test]`-Marker — `claims.rs/scoring.rs/manuscript.rs/runs.rs/scope.rs` haben null. `scoring.rs` wurde zuletzt Jun 24 angefasst, bleibt aber unverdrahtet.

**Warum es zählt:** Irreführt Reviewer/Agenten massiv; tote Tests validieren nichts Ausgeliefertes.

**Fix:** Den gesamten Orphan-Tree löschen oder unter `archive/` verschieben.

---

**H9 — CV-Parse-Command hat keinen server-seitigen Capability-/Policy-Gate bei Ingestion**
Korrigierte Zeilen: `record_command` `store.rs:8528-8610`; `app_build_command_policy_target` `8612-8650`; `create_ctox_queue_task` `26182-26237`. Der entscheidende Fall: Catch-all `_ => {}` `15800` → `record_command` `15802`.

`business_os.chat.task` (cv-print) matcht keinen Arm in `accept_rxdb_business_command_with_origin` und fällt durch zu `record_command`, das nur App-Build-Commands gated. Jeder Geschwister-Arm (`ctox.*`/`invoices.*`/`ats.*`) löst eine Session auf und ruft eine Policy-Entscheidung — `chat.task` ist ein echter Ausreißer. `create_ctox_queue_task` materialisiert dann die desktop_files-PDF, läuft `pdftotext` und enqueued einen LLM-Task **ohne** Permission-Check. Der fail-closed `REQUIRE_CAPABILITY_TOKEN`-Check lebt in `rxdb_session_from_command` (`20214-20223`), das dieser Pfad nie aufruft.

**Warum es zählt:** Ein unauthentifizierter/unprivilegierter Peer triggert PDF-Materialisierung + Subprozess + LLM-Spend mit null Autorisierung. (Identisch in Mechanik zu H5.)

**Fix:** Wie H5 — `chat.task`/parse-Lanes durch den Session+DataWrite-Chokepoint routen.

---

**H10 — matching.match-Scorer ist eine hardcodierte deutsche Recruiting-Keyword-Liste, als generisch fehletikettiert**
`importer.rs:3602-3669` `dimension_score` (hardcodierte Keywords; Floor 0.28 bei `:3668`); `:3530-3600` `build_match_items` (5 fixe Dimensionen); `:3707-3718` `total_match_score` (mean×100).

`dimension_score` hardcodiert deutsche Recruiting/Maschinenbau-Elektro-Keyword-Arrays in kompilierten Rust-Match-Armen (skill→`["gebäudeautomation","automatisierung","cad","catia","sap",...]`). Score = `(0.28 + ratio*0.68).clamp(0,0.96)`. Es gibt **kein** direktes requirement↔CV-Overlap — der Score basiert auf der requirement-gefilterten Untermenge der **statischen** Liste, mit Fallback auf die volle statische Liste. Jede off-domain-Rolle (Pflege/Jura), deren Text keine der hardcodierten Terme enthält, landet exakt auf 28% Gesamt. Beide Produktions-Command-Handler nutzen diesen Scorer (`importer.rs:113,1011`) — kein Test-Fixture. Kontrastiert `pipeline.js:9-12` ("Baukasten ... reuse the same mechanics"), was die kompilierten Keywords widerlegen.

**Warum es zählt:** Der einzige Scorer hinter beiden Match-Commands, vermarktet als "Matching-Engine", emittiert plausible 28-96% unabhängig vom tatsächlichen Fit außerhalb des dt. Vokabulars. Eine Demo-Heuristik, keine Engine — und der SKILL.md-Vertrag verspricht LLM-Reasoning.

**Fix:** Keywords aus dem Vokabular in konfigurierbare Daten (SQLite-Runtime-Store, nicht Env) auslagern; echtes requirement↔CV-Term-Overlap oder Embedding-Scoring; den SKILL.md-Vertrag mit der Realität in Einklang bringen.

---

**H11 — e-Signatur ist ein Stub: `ats.signature.sign` ohne Signer-Identitäts-Bindung, als Legal-Sign-off-Beweis wiederverwendet**
Korrigierte Zeilen: Handler **`store.rs:24155-24202`** (NICHT 23871-23917 — das ist `ats.submission.present`); Consumer `ats.leistungsnachweis.signoff` **`store.rs:23963-24020`**; `signature_request_status` `ats_gates.rs:276-282`; Schema-Feld `esign/collections.schema.json:26`; `esign/index.js:47-79`.

Der Handler liest `request_id`/`signer_id` direkt aus dem Payload, findet den Signer rein per ID-Match und setzt dessen State auf `"signed"` — der authentifizierte Session-Actor (`ats_actor_value(session)`, `store.rs:23626`) wird **nie** mit `signer_id` verglichen (nur ins Audit-Event geschrieben). `signed_artifact_id` wird nie geschrieben (nur gelesen bei `esign/index.js:168`). `signature_request_status` gibt `"completed"` zurück, wenn alle Signer signiert sind — ein einzelner `sign`-Flip eines intern fabrizierten Signers passiert als externer Entleiher-Sign-off durch und released Billing (`store.rs:23994-24020`). Plan ESIGN-1 markiert den nativen Signer-Handler als 🔴 MISSING (`docs/business-os-ats-plan.md:489-503`).

**Korrektur (Scope):** Die ATS-mutating-Family ist durch `rxdb_command_session` → `require_manage_all=true` gegatet (`store.rs:14977-14991`), und ReplicatedPeers brauchen ein nativ-signiertes Capability-Token. Der Spoofer muss also ein gültiges manage_all-Token halten, nicht nur im WebRTC-Raum sein. Aber ein legitim autorisierter Operator kann **jeden** Signer (inkl. externer Entleiher) ohne Signer-Authentifizierung und ohne Artefakt als signiert markieren.

**Warum es zählt:** Integritäts-/Non-Repudiation-Versagen — ein bloßer State-Flip ist der einzige Beweis, der Billing-Release und AÜG-Gate gated.

**Fix:** Signing-Actor an Signer-Identität binden (verifizierter Token-Actor muss Signer-Record matchen); unveränderliches signiertes Artefakt/Hash persistieren; bis echtes Signing existiert, die abhängigen Legal-Gates als non-production markieren.

---

**H12 — Kein nativer Credential-Verification-Command — das Deployment-Gate vertraut einem browser-gesetzten `verified`-Flag**
`ats_gates.rs:52-58` (Gate vertraut `verified`); `nachweise/index.js:139,146` (`verified:false`, plain `col.insert`); grep: null native Writer von `verified=true` auf `business_credentials`.

`credential_status()` gibt `Unverified` zurück, außer `verified==true`; nur `Valid`/`Expiring` sind deployable. Der Browser erzeugt Credentials mit `verified:false` per plain RxDB-Write (Kommentar `nachweise/index.js:116`: "no native command for capture"). Das vollständige ats.*-Command-Set (`store.rs:23239-23400`) hat **keinen** credential-verify-Arm. Schema trägt `verified` + `verified_by` (read-only im UI gerendert, `index.js:270`) — ein Verification-Flow wurde designt, aber der Write-Pfad nie implementiert.

**Warum es zählt:** Das AÜG-Gate ist für jede UI-erzeugte Credential un-passierbar (Pfad a); falls je ein Browser-Write von `verified:true` ergänzt wird, wäre ein server-vertrautes, ungeprüftes Compliance-Flag die Folge (Pfad b). Privilegierte Compliance-Verifikation ohne server-gegateten Pfad.

**Fix:** Nativen `ats.credential.verify`-Command (gegated, schreibt `verified=true` + `verified_by` aus der authentifizierten Session). Das ist gleichzeitig die fehlende Capability-Symmetrie für Credentials.

---

**H13 — "Zero-Knowledge"-gesperrte Notes leaken Titel (und Passcode) im Klartext in den synchronisierten Store**
`notes/index.js:1235-1242` (note.title aus Klartext für gesperrte Note), `1317-1338` (commitSave patcht Titel klartextlich unabhängig von `is_locked`); Claim `index.html:271`.

`index.html:271` rendert "Zero-Knowledge-Passwort". `processContentInput` leitet `newTitle` aus der ersten nicht-leeren Zeile des **entschlüsselten** Editor-HTML ab (`index.js:1226-1230`) und setzt `note.title = newTitle` für jede Note inkl. gesperrter. `commitSave` patcht `title` klartextlich in die replizierte Collection (`index.js:1309-1338`); nur `content` wird verschlüsselt. **Schlimmer als das Digest sagt:** `note.lock_passcode` wird auf den **Klartext-Passcode** gesetzt (`index.js:1734`) und in dieselbe synchronisierte Collection gepatcht (`index.js:1336`) — der Passcode selbst wird synchronisiert, was die Verschlüsselung vollständig zunichtemacht (arguably critical).

**Warum es zählt:** Gesperrte "Zero-Knowledge"-Notes leaken Erste-Zeile-Titel, `updated_at` und den Klartext-Passcode an den replizierten Daemon/Peer-Store.

**Fix:** Titel/Passcode für gesperrte Notes nie im Klartext persistieren; entweder Titel mitverschlüsseln oder durch ein Platzhalter-Label ersetzen; `lock_passcode` niemals synchronisieren (nur einen PBKDF-Salt/Verifier speichern).

---

**H14 — Formula-Ergebnisse sind display-only — nie persistiert oder exportiert**
`spreadsheets/index.js:948-990` (esp. 970), `1054`, `1511`, `1538`, `892`.

`recalculateSpreadsheet()` berechnet Formelwerte via HyperFormula (`965`) und schreibt sie **nur** in `cellElement.textContent` (`970`). Es gibt kein `setValue`/`setValueFromCoords` (grep: null Treffer). Das Grid wird mit `parseFormulas:false` erstellt (`892`), JSpreadsheet speichert also den Literal-`"=..."`-String. `getData()` ohne Args gibt den Roh-Modellwert zurück (verifiziert gegen `jspreadsheet_unminified.js`). Folge: `saveActiveSpreadsheetDraft` persistiert Roh-Formeln (`1069,1081`), CSV-Export (`1538`) und JSON-Export (`1527/1533`) emittieren Literal-Formeln. Kein nativer Formel-Evaluator existiert.

**Warum es zählt:** Für eine berechnete Tabelle werden Summen nie dauerhaft gespeichert/exportiert; jeder non-browser-Consumer der kanonischen CSV/`model_json` sieht rohes `"=SUM(...)"`.

**Fix:** Berechnete Werte ins Datenmodell zurückschreiben (`setValueFromCoords`) ODER eine native Formel-Evaluation als Teil der Engine etablieren und die berechneten Werte persistieren.

---

**H15 — spreadsheets ist app-only, ohne native Engine oder Harness-Handler**
`index.js:28-53`, `1265-1285`; `store.rs:15800` (`_ => {}`), `15802` (`record_command`); `rxdb_peer.rs:6578-6582`.

Die drei Runbook-Command-Typen (`spreadsheet.summarize/audit-formulas/risk-review`) dispatchen nur über den Browser-Command-Bus. grep über `src/core/` = null Handler; sie fallen durch den Default-Arm. Die native Seite ist generisches Blob-Streaming (`rxdb_peer.rs:6595`), keine Spreadsheet-Capability. Zell-Modell, HyperFormula, CSV/JSON-I/O leben nur im Browser-JS. **Korrektur (Nuance):** Unmatched Commands werden recorded **und** als `ctox_queue_task` enqueued (`store.rs:8548`), könnten also an einen LLM-Agenten geroutet werden — aber das ist keine native Spreadsheet-Engine, kein Handler und keine CLI.

**Warum es zählt:** Genau das verbotene "app-only feature"; kein Weg für einen Agenten, eine Tabelle über CTOX zu erzeugen/parsen/evaluieren/exportieren.

**Fix:** Native Engine bauen (parse/eval/serialize) ODER die Runbook-Commands entfernen, bis eine existiert. Idealerweise auf eine geteilte Document-Capability-Engine aufsetzen.

---

**H16 — Kein einheitliches Dokumentenmodell — vier inkompatible Repräsentationen, und das eine geteilte Paar wird drei Wege geschrieben**
`documents/schema.js:67-85` (`document_blob_chunks`) vs. `spreadsheets/schema.js:72-90` (Fork-Copy) vs. `cv-print-builder/index.js:4,9-11,939,987-1033` (CHUNK_SIZE=16KiB, `desktop_file_chunks`) vs. `notes/schema.js:22-44` (flache HTML-Row).

Vier wechselseitig inkompatible Modelle koexistieren, und selbst das geteilte `documents/document_versions`-Paar wird widersprüchlich beschrieben. **Versionierung ist inkohärent:** native cv-print-Writeback inkrementiert `MAX(version)+1` (`store.rs:22903`-Region), während das documents-Modul `_v1` für immer hardcodiert (`documents/index.js:479,503`) — dieselbe Collection trägt zwei gegensätzliche Versions-Semantiken. **Editor-Stack dupliziert:** documents (SuperDoc + document-format.mjs) vs. notes (Lexical + CustomHTMLNode) — zwei (eigentlich drei, mit dem markdown-textarea) divergente Rich-Text-Engines und Content-Modelle ohne geteilte Sanitization/Serialization.

**Warum es zählt:** Der Stack hängt nicht als ein kohärentes Produkt zusammen; Cross-Modul-Wiederverwendung ist unmöglich, XSS-Surface und Round-Trip-Logik doppelt gepflegt.

**Fix:** Ein kanonisches Document/Version/Blob-Modell in einer geteilten nativen Engine + ein geteilter Browser-Helper. cv-print entweder auf `document_blob_chunks` falten oder die `desktop_file_chunks`-Lane explizit als Binary-Source-Lane dokumentieren; spreadsheets auf dasselbe versionierte-Blob-Primitiv kollabieren; entscheiden, ob notes an `document_versions` teilnimmt. Eine Rich-Text-Engine + ein Content-Modell wählen.

---

### MEDIUM

---

**M1 — Latente HTTP-Datenbrücke: Knowledge-Document- + Parquet-Dataframe-Row-Handler leben weiter in `server.rs`, nur durch das 410-Prädikat gegated**
`server.rs:225-232` (Gate); `:679-693` (Allowlist = 5 Einträge, knowledge fehlt); `:421-444` (knowledge-Match-Arme nur hinter dem Gate erreichbar); `:2180-2205` `knowledge_document_payload` (serviert skill/runbook/skillbook-Markdown); `:2207-2249` `skill_markdown` (`SELECT content_blob FROM ctox_skill_files` über HTTP); `:2356-2377` `knowledge_dataframe_rows_payload` (`scan_parquet().slice().collect()` → Rows über HTTP); Guard `assert-rxdb-only.mjs:131-163` pinnt nur das Prädikat, nicht die Entfernung.

Lebende Handler würden Knowledge-Dokument-Markdown und Parquet-Dataframe-Rows über HTTP servieren; sie sind nur unerreichbar, weil das Gate sie blockt — ein One-Line-Allowlist-Edit öffnet die Brücke still wieder. Per "Do not defend the HTTP sync path"-Memo als Finding behandelt, auch wenn gegated.

**Fix:** Die knowledge/document-, dataframe/schema-, dataframe/rows-, knowledge-index-, reports-, users-, channels-Match-Arme und ihre Payload-Funktionen aus `server.rs` löschen; Knowledge/Dataframe-Fenster über RxDB-Projektion exponieren, falls der Browser sie braucht. Minimal: `assert-rxdb-only.mjs` erweitern, sodass es failt, wenn `server.rs` `knowledge_dataframe_rows_payload`/`knowledge_document_payload`/`scan_parquet` in einem HTTP-Match-Arm enthält.

---

**M2 — Server-seitiger SSRF: Importer fetcht beliebige user-gelieferte URLs ohne Host/IP-Allowlist**
`importer.rs:651-659` (`ureq::get(&url)` in `import_matching_requirement`); `:1075-1082` (`import_requirement_url`); die URL stammt aus dem Command-Payload ohne Upstream-Allowlist. Kontrast: gegateter `full_text.rs:32-83` vs. ungegateter `report/cli.rs:1443-1500` (figure-add).

**Fix:** Vor jedem `ureq::get`: Schema validieren (nur https), Host auflösen und RFC1918/Loopback/Link-Local/Metadata-IPs ablehnen, konfigurierbare Host-Allowlist via SQLite-Runtime-Store (kein Env-Toggle). Spiegeln in `report/sources/full_text.rs` und `report/cli.rs` figure-add.

---

**M3 — WebRTC-File-Fetch ohne per-File-/per-Actor-Autorisierung — jeder verbundene Peer kann jede Datei per ID streamen**
`file_fetch_handler.rs:193-278` (`run_file_fetch`: kein Authorization-Branch, `peer_identity` nur geloggt); `:64` (`FILE_FETCH_ERROR_UNAUTHORIZED` definiert, ungenutzt); `rxdb_peer.rs:6578-6599` (`DEMAND_FILE_CHUNK_COLLECTIONS`), `:6649-6664` (`stream_demand_file_chunks` streamt per Key ohne Actor-Check).

Gilt für `desktop_file`, `document_blob` und `spreadsheet_blob`. Transport ist compliant (WebRTC), aber ohne Autorisierung.

**Fix:** Authentifizierte Peer/Actor-Identität in die `FileChunkStreamFn`-Closure durchreichen und jeden Fetch über `policy.rs` gaten (Collection-Scope + Record-Owner/Modul-Check) vor dem Streamen; `FILE_FETCH_ERROR_UNAUTHORIZED` bei Denial emittieren. Der Capability-Token-Actor (schon in `rxdb_command_session` vertraut) ist die richtige Identität.

---

**M4 — Nicht-0/7/19%-Steuersatz: native Journal-Behandlung ist defekt, aber nicht wie behauptet "jeder Custom-Satz unpostbar"**
`invoices.rs:945-962`, `:972-976`, `:997`; `invoice-types.js:146-156`.

**Korrektur (das Headline war überzogen):** Zwei distinkte Outcomes existieren. (1) **Mixed-Invoice** (≥1 Zeile @ 0.07/0.19 UND ≥1 Zeile @ non-standard-Satz): `tax_lines` non-empty, `party_base = net + tax_total` (inkl. unmapped Steuer), aber `tax_lines` decken nur den Standard-Anteil → Journal unbalanciert → `ensure!` `:997` bailt, Posting scheitert. (2) **All-non-standard** (z.B. eine einzelne 16%-Zeile): jede Iteration `continue`t, `tax_lines` bleibt empty, also `party_base = net_total` (`:973`) → balanciert → Posting **gelingt**, aber die **USt wird still weggelassen** (während `tax_cents` sie weiterhin meldet). Der JS-Poster divergiert in beiden Fällen durch Fallback auf 1406/3806 (`invoice-types.js:151,155`; `invoice-poster.js:80-81,117`) und emittiert immer eine balancierte Steuerzeile.

**Warum Medium statt High:** Der "unpostbar für jeden Custom-Satz"-Anspruch ist für den Single-Rate-Fall falsch; das Versagen ist auf Mixed-Rate-Invoices begrenzt, und die Silent-Tax-Drop-Variante ist ein separates Issue. Kein nativer Test deckt einen non-0.19-Satz ab.

**Fix:** Steuer-Account-Mapping daten-getrieben machen (alle relevanten Sätze inkl. 16%/ausländische); für unmapped-Sätze hart failen statt still droppen; native+JS auf gemeinsame `buildJournalEntry`-Logik cross-checken. Test mit non-standard- und mixed-rate-Invoices.

---

**M5 — Invoices/ATS-Mutationen haben keine Permission in `policy.rs` — coarse manage_all ist das einzige Gate**
`policy.rs:40-114` (Permission-Enum hat kein Invoices/Ats); `store.rs:15005` (invoices `rxdb_command_session`), `:14981` (ats). Kontrast: App-Build geht durch `module_policy_decision` (`store.rs:8622`).

Jede invoices/ats-Mutation erfordert chef/admin (manage_all), keine per-Modul-Granularität.

**Fix:** Typisierte Permissions (`InvoicesManage`, `AtsManage` oder ein scoped DataWrite) ergänzen und über `policy.rs` evaluieren, sodass diese Mutationen scope-aware, auditierbar und konsistent mit dem Rest des Modells sind.

---

**M6 — `document_blob_chunks` / `spreadsheet_blob_chunks` haben keinen Content-Hash- oder Generation-Integritätsvertrag**
`documents/schema.js:67-86`; `documents/index.js:2160-2169`; `rxdb_peer.rs:6694-6726` vs. `file-integrity.js:144-202` (desktop-files-Lane hat sha256).

Die Dokument-/Spreadsheet-Blob-Lane hat keine Per-Chunk- oder Whole-Content-Integrität, während die desktop-files-Lane sie hat. Kein Manipulations-/Korruptionsschutz.

**Fix:** Per-Chunk + whole-content sha256 ergänzen (zusammen mit H6) und bei Reassembly verifizieren.

---

**M7 — Matching-Importer bettet vollen PDF-Rohtext inline ein und dupliziert ihn über 2-4 Records, ohne Chunking**
`importer.rs:902-925` (`meta.rawText` + top-level `rawText`), `:1502-1537` (`data.object==data.candidate` Duplikat + `index_text=raw_text`).

Der gesamte CV-Rohtext wird inline in mehrere Records geschrieben — kein Chunking, mehrfache Duplikation. Sync-Bandbreite und Store-Größe leiden.

**Fix:** Rohtext einmal in eine Blob-/Chunk-Lane schreiben und per Referenz verlinken statt inline-Duplikation.

---

**M8 — sha256 wird für jedes importierte Dokument berechnet, aber nie für Dedup/Idempotenz benutzt**
`importer.rs:860-861,1401-1403,833-838`; `store.rs:22044` (`source_sha256` gespeichert, kein Lookup).

Jeder Import berechnet einen Hash, speichert ihn als Feld, prüft ihn aber nie — wiederholter Import desselben Dokuments erzeugt Duplikate.

**Fix:** Pre-Insert-Lookup auf `source_sha256` für Idempotenz; bei Treffer existierenden Record zurückgeben statt neu schreiben.

---

**M9 — Matching: UI akzeptiert Bild-Uploads, aber der native Parser ist PDF-only und produziert still leere Objekte**
`matching/index.html:122` (image accept); `ctoxCommandAdapter.js:57-78`; `importer.rs:851-856` (Parse-Failure verschluckt); `:2323-2335` (`ocr_enabled:false`, kein Image-Branch).

Bilder (png/jpg/webp) werden im UI akzeptiert, aber `parse_pdf_text` hat keinen Image/OCR-Pfad → leerer `rawText`, kein Fehler.

**Fix:** Bild-Accept im UI entfernen, bis OCR existiert (siehe H2), ODER einen expliziten "unsupported format / needs OCR"-Fehler zurückgeben.

---

**M10 — Browser `computeRequirementMatch` implementiert einen vollen LLM-Match-Pfad, der tot ist — nativ ruft nie ein Modell**
`matchingTools.js:104-136` (LLM-Prompt + `parseJsonObject`); `ctoxCommandAdapter.js:30-34,80-119`; `importer.rs:956-1063` (`compute_matching_result`, kein Modell); `SKILL.md:8`.

Der Browser baut einen LLM-Prompt und parst JSON aus `rawResponse`, aber das dispatchte `matching.match`-Command wird nativ mit null Modell-Invokation behandelt — der Prompt-Pfad erhält nie Modell-Output. Toter, irreführender Code, asymmetrisch zur nativen Realität.

**Fix:** Den Browser-LLM-Pfad entfernen (nativer Scorer ist die Wahrheit) ODER nativ echtes LLM-Scoring implementieren, sodass der SKILL.md-Vertrag stimmt (verknüpft mit H10).

---

**M11 — Keine native DOCX/Markdown-Read-Surface; documents-Import ist browser-only (und laufzeit-kaputt)**
`documents/index.js:175` (lazy `vendor/document-format.mjs`), `:482-486` (`importDocx`/`importMarkdown`); `document-format/src/index.ts:1,10` (Import-Pfad zu `word-port`, der nur unter `archive/` existiert); `build-business-os-vendor.mjs:10,122` (resolved aus `archive/reorg-review`).

DOCX/Markdown-Import läuft vollständig im vendored `document-format.mjs` im Browser; es gibt keinen nativen Rust-DOCX-Parser und keine Harness-CLI, um eine existierende .docx in die documents-Collection zu ingestieren. Der Source-Tree `document-format/src/index.ts` importiert einen Pfad, der nur unter ignoriertem `archive/` existiert — also laufzeit-zerbrechlich/kaputt.

**Fix:** Nativen DOCX-Read-Pfad (auf doc-stack konsolidiert, H4) + `business_commands`-Import-Surface ergänzen; den `word-port`-Import aus `archive/` reparieren oder die Quelle in den aktiven Tree ziehen.

---

**M12 — DOCX/Markdown-Parsing hat keine native Engine, CLI oder MCP-Surface; MCP-Channel exponiert keine Parse/Import/Match-Operation**
`document-format/src/index.ts:18`; `mcp_channel.rs:741` (`tool_descriptors`: status/modules/records/runs/artifacts/approvals/create_app/execute_action — kein Parse/Import/Extract); `skills/ctox-business-os-mcp/SKILL.md:71`.

Dieselbe Dokument-Parse-Fähigkeit ist Apps + Harness-CLI erreichbar, aber dem externen MCP-Channel nicht — Remote-Agenten können kein Dokument parsen/importieren/matchen.

**Fix:** Parse/Import/Match-Descriptors zum MCP-`tool_descriptors`-Set ergänzen, sobald die geteilte Engine existiert.

---

**M13 — Report-Engine: in Narrative/EvidenceRegister-Blöcken eingebettete Tabellen werden vom Markdown-Renderer still verworfen**
`render/manuscript.rs:399-408` (Tabelle für alle Kinds geparst+gestrippt); `render/markdown.rs:360-387` (`write_block`-Kind-Allowlist exkludiert Narrative + EvidenceRegister).

Tabellen werden für alle Block-Kinds geparst und entfernt, aber der Renderer schreibt sie nur für eine Allowlist von Kinds, die Narrative und EvidenceRegister ausschließt — stille Datenverluste im Output.

**Fix:** Die Kind-Allowlist im Renderer mit dem Strip-Verhalten in `manuscript.rs` in Einklang bringen; Round-Trip-Test für Tabellen-Einbettung.

---

**M14 — Aktive Renderer (`markdown.rs`, `render/manuscript.rs`, `docx.rs`) haben null Unit-Tests**
`grep -c '#[test]'` über die drei = 0,0,0; `tests/mod.rs:6-11` verdrahtet nur asset_pack/checks/cli/rascon/release_guard/workspace.

Rendered-Markdown/DOCX-Fidelity, Table/Figure-Einbettung, Cross-Ref-Auflösung und der DOCX-Subprozess-Pfad sind unexerciert.

**Fix:** Render-Fidelity-Tests (Markdown-Snapshot, Cross-Ref-Auflösung, Table-Einbettung); DOCX-Pfad mindestens als Smoke (verknüpft mit H7).

---

**M15 — Semantische Suche ist ein unindizierter Full-Table-Cosine-Scan mit JSON-dekodierten Vektoren**
`doc-stack/store.rs:510-563`; `:367-371`.

Jede semantische Query lädt **alle** Embedding-Rows und brute-force-cosine-scannt, mit Per-Row-JSON-Parse der Vektoren. Skaliert nicht.

**Fix:** Vektoren binär (nicht JSON) speichern; einen ANN-Index oder mindestens Vorfilterung über FTS einführen.

---

**M16 — Keine Zip-Bomb-/Dekompressions-Ratio-Guard auf OOXML/ODF-Containern**
`doc-stack/parse.rs:19,788-816`; auch Browser-XLSX-Parser `universal-importer.js:910-975` (ZIP-Reader, keine Bounds-Checks), `:981-1019` (DOMParser auf untrusted XML ohne DOCTYPE/Entity-Neutralisierung).

Kein Decompression-Ratio-Limit beim Öffnen von OOXML/ODF-Zip-Containern; der Browser-XLSX-Parser parst untrusted Workbook-XML ohne DOCTYPE/Entity-Härtung (XXE-Surface).

**Fix:** Decompression-Ratio- und Total-Size-Limits in der geteilten Zip-Open-Helper (verknüpft mit H4); DOCTYPE/Entities im Browser-DOMParser-Pfad explizit neutralisieren.

---

**M17 — doc-stack: Default-Corpus-Root indiziert still ganz `$HOME/Documents`**
`doc-stack/surface.rs:512-532,541-562`; `parse.rs:59-63`.

Ohne explizite Root indiziert doc-stack das gesamte `$HOME/Documents` — überraschend breites Datenscope.

**Fix:** Keine Default-Root; explizite Corpus-Root-Registrierung erzwingen.

---

**M18 — intake verwirft Kandidaten-Dokumente — hochgeladene CV-Referenz wird nie persistiert/verlinkt**
`store.rs:23412-23427` (kein `documents`-Feld); `intake/index.js:72-78` (Form lässt `documents` aus); `intake/core/application.js:26-28` (normalisiert ein `documents[]`-Array, das die Form nie sammelt und der native Handler vollständig droppt).

Das Core-JS normalisiert ein `documents[]`-Array, aber die Form sammelt es nie und der native Handler droppt es — die CV-Referenz wird nie an den geparsten CV gelinkt.

**Fix:** `documents`-Feld in Form + nativem Handler durchverdrahten; an die `document_versions`-Record linken.

---

**M19 — nachweise-Modul: Browser-Write von `business_credentials` umgeht den nativen Command-Pfad**
`nachweise/index.js:117-153` (`onSubmit` → `col.insert`); Kontrast zu allen anderen Modulen, die `ats.*`-Commands dispatchen.

Credentials werden per direktem RxDB-`col.insert` geschrieben statt über einen server-authoritativen `ats.*`-Command — Server-Authoritative-Policy-Schwäche (verknüpft mit H12).

**Fix:** Einen `ats.credential.capture`-Command einführen und das Modul darauf umstellen.

---

**M20 — `dunning.letter.send` ist nicht idempotent und doppel-zählt `letters_sent`**
`invoices.rs:1750-1789` (kein `if letter.status=="sent" return`-Guard); `store.rs:14139`.

Ein zweites Send-Command unter distinkter `command_id` zählt den Brief erneut.

**Fix:** Idempotenz-Guard: bei `status=="sent"` idempotent zurückkehren ohne `letters_sent` zu inkrementieren.

---

### LOW (kompakt)

- **L1 — pdf-parse: 90-Grad-Rotation-Branch mappt x<-y ohne Page-Offset**, vermutlich korrumpierte Reading-Order; kein realer Rotated-PDF-Test. `grid_projection.rs:270-280` vs. `:293-308`.
- **L2 — pdf-parse: Dokument wird pro Seite voll neu geöffnet+geparst** durch pdfium. `pdfium_backend.rs:49-71,161-172,202-206`; `num_workers` ungenutzt.
- **L3 — pdf-parse: Page-Index-Cast saturiert bei `u16::MAX`**, mis-targetet Seiten > 65535. `pdfium_backend.rs:169,218`.
- **L4 — pdf-parse: locale-spezifische Cleanup-Regeln** (`MIETRECHTSKOMPAKT`) hardcodiert in der generischen Engine. `clean_text.rs:6-9,115-127`.
- **L5 — pdf-parse: `render_page_image` implementiert aber tot**; OCR-Seam scaffolded dann abgebrochen. `pdfium_backend.rs:209-234`.
- **L6 — pdf-parse: realer Eval-Korpus nie in `cargo test` ausgeführt**; nur synthetische Boxen. `evaluation.rs:329`, `tests/parity.rs`.
- **L7 — pdf-parse: Build-time pdfium-Binary auto-download via curl ohne Checksum-Pinning**. `Cargo.toml:9-11,19-21`, `pdfium-auto-0.3.0/build.rs:110-116`.
- **L8 — pdf-parse/importer: Duplizierte Spacing/Quality-Heuristiken** (`median()` zweimal). `pdfium_backend.rs:690` vs. `grid_projection.rs:963`.
- **L9 — doc-stack: `formats.rs` Write-Taxonomie ist tote Metadaten**, die Capability überstellt; `semantic_write_ready` hardcoded `false`. `formats.rs:4-136`, `surface.rs:186-204`.
- **L10 — doc-stack: Harness-Tool-Surface ist strikte Untermenge der CLI** (Agenten können nicht indizieren/Corpus registrieren). `surface.rs:150-154`, `spec.rs:1363-1444`.
- **L11 — doc-stack: zwei divergente PDF/HTML-Extraktionspfade** (full_text.rs vs. parse.rs). `full_text.rs:126-145,163` vs. `parse.rs:169,723`.
- **L12 — documents: Draft-Autosave leakt verwaiste Blob-Chunks bei jedem Save** (unbegrenztes RxDB-Wachstum, kein Delete). `documents/index.js:2034-2047,2140-2158`.
- **L13 — documents: Versionierung nicht-funktional** (`document_versions` bekommt nie >1 Row). `documents/index.js:479,503,2042-2047` (verknüpft mit H16).
- **L14 — documents: Delete-Confirm-Dialog zeigt Literal `{title}`** statt Dokumentname. `documents/index.js:835`, `locales/de.json:37`.
- **L15 — documents/spreadsheets/cv-print: Chunk-Codec dreifach dupliziert ohne geteilten Helper**; `base64ToUint8` in spreadsheets toter Code, `spreadsheet_blob_chunks` write-only. `spreadsheets/index.js:2040,2049-2070` vs. `documents/index.js:2140` vs. `cv-print/index.js:1855-1869`.
- **L16 — documents: Whole-Document base64 encode/decode auf dem Main-Thread** bei jedem Blob-Read/Write. `documents/index.js:2315-2330,2140-2169`.
- **L17 — spreadsheets: CSV-Serialisierung force-quotet jede Zelle**, zerstört numerisches Typing beim Round-Trip. `index.js:277,1081,1538`.
- **L18 — spreadsheets: HyperFormula-Workbook bei jeder Zelländerung von Grund auf neu gebaut**. `index.js:900-919,959`.
- **L19 — spreadsheets: `module.json`/Registry bewerben "Native XLSX"**, Modul hat null XLSX-Support. `module.json:4`, `index.js:9`.
- **L20 — spreadsheets: vendored jspreadsheet behält AJAX-HTTP-Persistenz-Pfad**, der die Datengrenze brechen würde, falls aktiviert (Modul setzt keine `url`/`persistence`). `vendor/jspreadsheet_unminified.js`.
- **L21 — spreadsheets: `owner_id` immer leer** — keine server-authoritative Ownership. `index.js:307,412`.
- **L22 — notes: DIY-Column-Resizer statt geteiltem `CtoxResizer`**. **Korrektur:** Nur **2** Module (notes + ctox) duplizieren — NICHT 14; die anderen ~13 Resizer-Module nutzen bereits `CtoxResizer`. Severity Low (kosmetisch, keine Boundary/Policy-Verletzung). `notes/index.js:2606-2718` vs. `shared/resizer.js:5`.
- **L23 — notes: schwache/Placebo-Encryption-Defaults + App-Lock-Theater** (PIN `1234`, Default-Note-PW `1234`, Lock = localStorage-Flag). `index.js:1610,1727-1729,1627-1634`.
- **L24 — notes: First-Empty-Seeding ohne Cross-Client-Guard** → mehrere Peers seeden Duplikat-Sample-Notes. `index.js:200-242,625-645`.
- **L25 — notes: `CustomHTMLNode` rendert unsanitiertes HTML via `innerHTML`** (stored-HTML-XSS-Surface). `index.js:22-43,45-80,1142-1148`.
- **L26 — notes: doppelt registriert** (`notes` + `notizen`) mit verstreuten Branch-Checks. `registry.json:599-631`, `index.js:196-198 et al.`
- **L27 — cv-print: PDF-Import+Chunking+Hashing dupliziert vom documents-Modul mit divergentem, inkompatiblem Chunk-Format** (16KiB vs. 256000). `cv-print/index.js:986-1034` vs. `documents/index.js:6`.
- **L28 — cv-print: Modul-Test verwaist von CI**, asserted nur Source-Strings; nicht in `package.json`. `cv-print/tests/cv-print-builder.test.mjs`.
- **L29 — cv-print: Re-Parsing eines editierten/approved CV verwirft still browser-gewählte Print-Settings**. `store.rs:22700-22722,22799-22811`.
- **L30 — cv-print: stuck "parsing"-Phase ohne Client-seitige Recovery**, disabled Retry. `cv-print/index.js:512-538,1036-1177`.
- **L31 — desktop-files: hochgeladene Datei kann erst nach WebRTC-Round-Trip durch den CTOX-Peer geöffnet/previewt werden**. `explorer/app.js:705,717-724`, `file-viewer/app.js:292-299`.
- **L32 — desktop-files: PDF via raw `iframe` einer `blob:`-URL ohne `sandbox`-Attribut**. `file-viewer/app.js:266-268`.
- **L33 — desktop-files: Trash ist Soft-Delete only** — verwaiste `desktop_file_chunks` nie GC'd, Folder-Subtree nicht kaskadiert. `explorer/app.js:587-594,466-489`.
- **L34 — matching: zwei divergente native Object/Requirement-Schemas** (`matching_*` flat vs. `business_records/candidates` nested). `importer.rs:801-954` vs. `:1346-1571`.
- **L35 — matching: `matchScoreKey` als `(... + idx - idx)` berechnet** — immer-Null-No-Op. `importer.rs:3596`.
- **L36 — invoices: Storno (Cancel einer posted Invoice) umgeht die kumulative Over-Credit-Cap**. `invoices.rs:1241-1279` vs. `:1349-1373`.
- **L37 — invoices: Browser-Tests nicht in `run-all.mjs` verdrahtet** (Mandatory-Gate). `invoices/tests/` (10 Dateien) vs. `run-all.mjs`.
- **L38 — invoices: README widerspricht dem Code materiell** (cancel/credit-note/line-CRUD implementiert, README sagt "bail"). `invoices/README.md:138-163`.
- **L39 — ats: `find_double_submission` gibt `Some("")` für eine konfliktende Submission ohne ID** zurück. `ats_gates.rs:255-258`.
- **L40 — report: Markdown-Renderer wendet `ascii_dashes()` auf den ganzen Block-Body inkl. Code-Spans/URLs an**. `render/markdown.rs:360-371,491-503`.

---

## 5. Architektur- & Datengrenzen-Bewertung

### HTTP-vs-WebRTC-Compliance: **BESTANDEN (mit einer latenten Regressions-Falle)**

Die Datengrenze hält über den **gesamten aktiven Stack**. Kein Dokument-, Blob-, File- oder geparstes-Record-Datum kreuzt HTTP zwischen Browser und CTOX. `server.rs:225-232` gated alle `/api/business-os/*`-Pfade außerhalb der 5-Eintrag-Control-Plane-Allowlist mit `410`; `assert-rxdb-only.mjs` pinnt das als Mandatory-Guard. Alle Blob-Bytes reassemblieren über `rxdb.file.fetch`-Demand-Fetch; Inbound-Bytes reisen base64 in `business_commands`. Der einzige Browser→CTOX-HTTP-Call ist die Capability-Token-POST.

**Verdikt-Einschränkungen:** (1) **M1** — latente Knowledge-Document-/Parquet-Row-HTTP-Handler leben weiter in `server.rs`, nur durch das Gate-Prädikat unerreichbar; ein One-Line-Allowlist-Edit öffnet sie wieder, und der Guard pinnt deren Entfernung nicht. (2) **M2** — server-seitiger SSRF in den Import-Lanes (kein Browser-Bridge, aber outbound ungated). (3) **M3** — WebRTC-File-Fetch ohne per-Actor-Autorisierung. Compliance hält für den Datenbrücken-Aspekt; die Autorisierungs- und Latent-Code-Aspekte sind offen.

### Capability-Symmetrie: **DURCHGEFALLEN — nur matching/import-parser ist symmetrisch; alle anderen Engines sind einseitig**

| Engine | Harness/CLI | business_commands/App | MCP | Verdikt |
|---|---|---|---|---|
| matching/import | ✅ `commands process` | ✅ | ❌ | **symmetrisch** (caveat: toter Browser-LLM-Pfad) |
| cv-print | ❌ keine CLI | ✅ (generic chat.task) | ❌ | app-only |
| invoices | ❌ keine CLI | ✅ | ❌ | app-only |
| ats | ❌ keine CLI | ✅ | ❌ | app-only |
| doc-stack | ✅ (subset) | ❌ | ❌ (2 Tools) | **harness-only** |
| report | ✅ CLI-only | ❌ keine Projektion | ❌ | **harness-only** |
| spreadsheets | ❌ | ❌ kein Handler | ❌ | **app-only, keine Engine** |
| notes | ⚠️ nur File-Mirror | ❌ kein typed Command | ❌ | **app-only** (caveat: nativer Markdown-Mirror existiert) |
| desktop-files | ❌ keine CLI | ✅ materialize | ❌ | app-only |

**Korrekturen aus Verifikation:** (a) doc-stack-`superdoc.mjs`-Zitat war ein False-Positive — der Symmetrie-Befund hält dennoch durch den leeren repo-weiten Grep. (b) notes ist **nicht** "ohne native Engine": `store.rs:13748` `sync_local_markdown_notes` (3s-Loop, `rxdb_peer.rs:1565/2109`) spiegelt `runtime/business-os/notes/*.md` ↔ die `notes`-Collection. Die akkurate Lücke ist die Abwesenheit einer **typisierten** Agent/CLI-Command-Surface (create/read/search/export), nicht totale Abwesenheit nativen Codes.

Nur eine von neun Engines ist voll symmetrisch. Drei (spreadsheets/notes interaktiv/documents-import) haben gar keine native Engine für ihre Kernfähigkeit. Der MCP-Channel exponiert **keine** Dokument-Operation überhaupt.

### Dokumentenmodell-Kohärenz: **DURCHGEFALLEN**

Der Stack hängt nicht als ein Produkt zusammen (H16). Vier inkompatible Repräsentationen; das eine geteilte `documents/document_versions`-Paar wird drei widersprüchliche Wege beschrieben; Blob-Chunk-Encoding ist zwischen native und browser inkompatibel und **bereits kaputt** für Blobs > 250KB (H6); Versionierung ist inkohärent (native inkrementiert, browser überschreibt v1); der Rich-Text-Stack ist über drei Editor-Engines dupliziert; und Policy ist die andere systemische Schwäche — die mutierenden Parse/Import/cv-print/match/spreadsheet-Lanes passieren **null** server-authoritative Policy (H5/H9). `DataWrite` existiert in `policy.rs`, wird aber nirgends bei Command-Ingestion invoked.

---

## 6. Lücken & Empfehlungen — Priorisierte Roadmap

### Tier 0 — Korrektheits-/Sicherheits-Stopper (sofort)

1. **H1 panic fix** (margin removal char/byte boundary) — uncaught Panic aus untrusted PDF, trivialer Fix, hoher Blast-Radius. Eine Codezeile + Test.
2. **H5 + H9 Policy-Chokepoint** — *die* systemische Sicherheitslücke. Jedes record-mutierende Command durch einen Session+DataWrite-Gate in `record_command`/`accept_rxdb_business_command` routen. Schließt cv-print, matching, documents-Writeback, spreadsheet-Runbooks in einem Schlag. Deckt sich mit der bekannten Auth-Audit-Wurzelursache.
3. **H6 Blob-Encoding-Korruption** — generierte/große DOCX korrumpieren bereits beim Browser-Öffnen. Eine Konvention (per-chunk base64 raw-slices), beide Browser-Writer + Demand-Source angleichen, sha256 (M6) ergänzen, Cross-Runtime-Byte-Identitäts-Test.
4. **H13 notes-Klartext-Leak** — Passcode + Titel synchronisieren im Klartext; "Zero-Knowledge"-Claim ist falsch. Passcode nie syncen, Titel für gesperrte Notes nicht im Klartext persistieren.
5. **M2 SSRF-Guard** — geteilte Guard-Funktion (https-only, RFC1918/Loopback/Metadata-Block, Allowlist via SQLite-Runtime-Store) auf alle outbound-Fetches in Import-/Report-Lanes.

### Tier 1 — Funktionalität reparieren (kurzfristig)

6. **H7 DOCX-Render** — entweder nativer Rust-DOCX-Writer (gegen die `zip`-Crate) ODER Skript-Pfad über `materialize_skill_bundle` auflösen. Aktuell überall non-funktional. Smoke-Test.
7. **H8 toten report-Code löschen** (~80KB orphan pipeline) — reine Hygiene, hoher Verwirrungswert für Reviewer/Agenten.
8. **H11/M11 nativer DOCX-Read-Pfad** — den `word-port`-Import aus `archive/` reparieren oder auf doc-stack konsolidieren; documents-Import ist laufzeit-zerbrechlich.
9. **H12 nativer credential.verify-Command** — das AÜG-Gate ist sonst un-passierbar; Compliance-Blocker.
10. **H11 e-Signatur** (H11→meint H11 e-sign): Signer-Actor-Bindung + signiertes Artefakt; bis dahin die abhängigen Legal-Gates als non-production markieren.

### Tier 2 — Konsolidierung & Symmetrie (mittelfristig)

11. **H16 — Eine kanonische Document-Capability-Engine.** Das größte fehlende Architektur-Stück. Ein Document/Version/Blob-Modell + ein Browser-Helper, einheitlich über CLI / business_commands / MCP exponiert. Darauf migrieren: cv-print, spreadsheets, documents-import. **Zuerst doc-stack eine App-Surface geben und report eine RxDB-Projektion** — das sind die zwei echten Engines, die auf einer Surface gestrandet sind.
12. **H4/M16 OOXML-Konsolidierung** — vier Reader auf doc-stack zusammenführen, mit geteiltem Zip-Bomb-Guard.
13. **H10 Matching-Scorer** — Keywords aus dem kompilierten Vokabular in Config; echtes Overlap-/Embedding-Scoring; SKILL.md-Vertrag angleichen.
14. **H14/H15 spreadsheets** — Formel-Werte persistieren (H14) + native Engine oder Runbook-Entfernung (H15).
15. **M3 WebRTC-File-Fetch-Autorisierung** — per-Actor/per-File-Policy-Gate im File-Fetch-Handler.

### Tier 3 — Hygiene (laufend)

16. **M1 latente HTTP-Handler aus `server.rs` löschen** + Guard erweitern.
17. **M5 typisierte Invoices/Ats-Permissions** in `policy.rs`.
18. **H2/OCR** — strategische Entscheidung: OCR implementieren (`render_page_image` ist die fertige Einspeisung) ODER `ocr_*`-Config entfernen + explizites "needs OCR"-Signal. Bis dahin die Bild-Accepts (M9) entfernen.
19. Test-Lücken schließen: realer PDF-Eval-Korpus in `cargo test` (L6), report-Renderer-Tests (M14), invoices-Browser-Tests + cv-print-Test in die Mandatory-Gates (L37/L28).

### Größte fehlende Fähigkeiten (explizit)

- **OCR** — komplett abwesend; scan-PDFs (Lebensläufe/Verträge) indexieren still als leer.
- **Nativer DOCX-Writer und -Reader** — Write ist Python-Subprozess (kaputt), Read ist browser-only (zerbrechlich). Kein nativer Rust-OOXML-Round-Trip.
- **Nativer XLSX-Pfad** — gar keiner; Spreadsheet-Import/-Eval ist app-only.
- **FTS über Business-OS-Dokumente** — doc-stack indexiert nur das lokale Dateisystem, nie die synchronisierten BOS-Collections. Es gibt keine Suche über die Dokumente, die Apps tatsächlich erzeugen.
- **Echte e-Signatur** — State-Flip-Stub ohne Signer-Bindung/Artefakt.
- **Funktionierende Versionierung** — `document_versions` wird inkonsistent geschrieben (native inkrementiert, browser hardcodiert v1).
- **Ein gemeinsames Dokumentenmodell + ein Policy-Chokepoint** — die beiden systemischen Wurzeln, aus denen die meisten High-Findings folgen.

---

**Relevante Pfade (absolut):** `/Users/michaelwelsch/Documents/ctox.nosync/src/tools/pdf-parse/src/processing/clean_text.rs`, `/src/tools/doc-stack/src/{parse,surface,store}.rs`, `/src/core/report/render/{docx,markdown,manuscript}.rs`, `/src/core/report/mod.rs`, `/src/core/business_os/{store,importer,invoices,ats_gates,rxdb_peer,policy,server}.rs`, `/src/apps/business-os/modules/{documents,spreadsheets,notes,cv-print-builder,matching,nachweise,esign}/index.js`, `/src/apps/business-os/desktop-apps/{explorer,file-viewer}/app.js`, `/src/apps/business-os/scripts/assert-rxdb-only.mjs`.