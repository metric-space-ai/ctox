# RFC: Canonical Document/Version/Blob Model for Business OS

Stand 2026-06-25 · Adressiert die **zweite Wurzel** aus [docs/ctox-document-stack-review-2026-06-25.md](ctox-document-stack-review-2026-06-25.md): „kein gemeinsames Dokumentmodell" (H16), inkl. der Folge-Findings H6 (Blob-Encoding inkompatibel), L13 (Versionierung kaputt), M6 (keine Blob-Integrität), H4/M11 (DOCX), H15 (XLSX), FTS-Lücke. Die erste Wurzel (Policy-Chokepoint, H5+H9) ist bereits gefixt (`6863c1a0`).

Dies ist ein **Design-RFC mit phasiertem, ticketisiertem Migrationsplan** — jede Phase ist ein eigenständig auslieferbarer Commit. Kein Code in diesem Dokument ist final; es legt das Zielmodell und die Reihenfolge fest.

---

## 1. Ist-Zustand (verifiziert)

Vier wechselseitig inkompatible Dokument-Repräsentationen:

| Modul | Collections | Blob-Chunking | Versionierung |
|---|---|---|---|
| **documents** | `documents` / `document_versions` / `document_blob_chunks` | whole-blob base64 → 256 000-**Zeichen**-Slice (browser); per-chunk base64 von raw-slices (native) | `_v1` hartcodiert (`index.js:479,503`) |
| **spreadsheets** | `spreadsheets` / `spreadsheet_versions` / `spreadsheet_blob_chunks` (Fork-Copy von documents) | dito, `CHUNK_SIZE=256000` | `_v1` hartcodiert |
| **cv-print-builder** | `desktop_file_chunks` (geteilte Desktop-Lane) | raw-byte, `CHUNK_SIZE=16 KiB` | `_v1` |
| **notes** | flache `notes`-Row (HTML inline) | keine Chunks | keine Versionen |

Drei harte Defekte daraus:

- **H6 — Cross-Runtime-Korruption:** Browser kodiert den ganzen Blob zu base64 und slict den **String** bei 256 000; nativ wird per raw-Chunk base64-kodiert. `256000 % 3 == 1` → native Chunks tragen `=`-Padding **mitten im Stream**, der Browser-Reader (concat-dann-decode) korrumpiert daran. Eine vom Agenten generierte DOCX > 250 KB (`ctox_generated_docx`) korrumpiert beim Browser-Öffnen.
- **L13 — Versionierung tot:** documents/spreadsheets schreiben ewig `_v1`; der native cv-print-Writeback inkrementiert dagegen `MAX(version)+1` — **dieselbe** `document_versions`-Collection trägt zwei gegensätzliche Semantiken.
- **M6 — keine Integrität:** Die document/spreadsheet-Blob-Lane hat keinen sha256 (die desktop-files-Lane hat ihn — `file-integrity.js`).

Plus: 3 divergente Rich-Text-Editoren (SuperDoc / Lexical / textarea) und kein nativer DOCX-Writer/-Reader oder XLSX-Pfad, der auf einem geteilten Modell aufsetzt.

---

## 2. Zielmodell — eine kanonische Lane

**Ein** Document/Version/Blob-Tripel, das alle binären/versionierten Dokumente tragen (documents, spreadsheets, cv-print, generierte DOCX; notes optional, siehe §6).

### 2a. Collections (kanonisch)

```
doc_documents
  id                : string (PK)
  module            : string   // owning module: "documents" | "spreadsheets" | "cv-print" | …
  doc_type          : string   // "markdown" | "docx" | "xlsx" | "pdf" | …
  title             : string
  current_version_id: string   // FK -> doc_versions.id
  owner_id          : string   // server-authoritative (set from session, not browser)
  index_text        : string   // FTS source (computed values, not formulas — see L17/H14)
  created_at_ms, updated_at_ms

doc_versions
  id                : string (PK)   // `${document_id}_v${n}`  (n MONOTONIC, not hardcoded)
  document_id       : string (FK)
  version           : integer        // 1,2,3… real increments (resolves L13)
  blob_id           : string (FK -> doc_blobs)  // immutable per version
  model_json        : object | null  // editor round-trip model (formulas, blocks) — optional
  source_kind       : string         // "imported" | "edited_docx" | "generated" | …
  created_at_ms

doc_blob_chunks
  id          : string (PK)  // `${blob_id}_${idx:04}`
  blob_id     : string
  document_id : string
  version_id  : string
  idx, total  : integer
  encoding    : "base64"
  mime_type   : string
  data        : string   // base64 of a RAW-BYTE slice (see §2b)
  chunk_sha256: string   // per-chunk integrity (M6)
  created_at_ms
  // + a whole-content sha256 stored on doc_versions.blob_sha256
```

### 2b. **Eine** Blob-Encoding-Konvention (löst H6)

Kanonisch = **per-Chunk base64 von raw-byte-Slices**:

1. Quelle sind die rohen Bytes (nicht ein vorab-base64-String).
2. Slice die **rohen Bytes** in `CHUNK_SIZE`-Stücke (ein Wert, z.B. 192 KiB — `% 3 == 0`, sodass kein Slice-internes Padding nötig wäre, aber per-Chunk-Encoding macht das ohnehin irrelevant).
3. base64-kodiere **jeden Chunk einzeln**.
4. Reassembly: **jeden Chunk einzeln** decoden, dann die rohen Bytes konkatenieren (nicht concat-dann-decode).
5. Beide Runtimes (`store.rs`-Writeback + Demand-Fetch `rxdb_peer.rs`, und `documents`/`spreadsheets`-Browser-Writer/Reader) nutzen exakt diesen Codec.
6. Integrität: per-Chunk `chunk_sha256` + whole-content `blob_sha256` auf der Version; bei Reassembly verifizieren (M6; spiegelt die desktop-files-Lane).

Vorteil: ermöglicht Range/Streaming (jeder Chunk ist unabhängig dekodierbar), schließt die Padding-Korruption, und ist byte-identisch über Runtimes.

### 2c. Geteilte native Engine + Browser-Helfer (Capability-Symmetrie)

- **Native Engine** (`src/core/business_os/doc_model.rs`, neu): kanonische Read/Write/Version/Blob-Operationen + Codec + sha256. Exponiert über:
  - **business_commands** (`doc.create`, `doc.new_version`, `doc.read`, `doc.blob.fetch`) — server-authoritativ, durch den **bestehenden Policy-Chokepoint** (DataWrite) gegated.
  - **Harness-CLI** (`ctox doc model …`) für Agenten.
  - **MCP-Descriptor** (das Review fand: MCP exponiert 0 Dokument-Ops).
- **Browser-Helfer** (`src/apps/business-os/shared/doc-model.js`, neu): EIN Codec + Version/Blob-Plumbing, den documents/spreadsheets/cv-print konsumieren (statt drei Fork-Copies). Bezieht den DB-Handle vom Shell (kein eigenes Sync).

---

## 3. Was das auflöst

- **H6** Blob-Korruption → ein Codec, byte-identisch, Round-Trip-getestet.
- **L13** Versionierung → echte monotone Versionen; native/browser einig.
- **M6** Integrität → per-Chunk + whole-content sha256.
- **H16** ein Modell → documents/spreadsheets/cv-print teilen Collections + Helfer.
- **H4/M11** DOCX → der native DOCX-Reader (doc-stack `parse_docx`) und ein nativer DOCX-Writer schreiben/lesen in **eine** Blob-Lane; der documents-Import braucht nicht mehr den fragilen `word-port`-Vendored-Pfad aus `archive/`.
- **H15** XLSX → analog ein nativer XLSX-Pfad auf derselben Lane.
- **FTS** → `index_text` (berechnete Werte, nicht Roh-Formeln — baut auf H14/L17 auf) speist EINEN FTS-Index über alle Dokumente, statt nur das lokale FS (doc-stack).
- **Editor-Duplikation** → eine Rich-Text-Engine-Entscheidung (§6).

---

## 4. Migrationsplan (ticketisiert, phasiert)

Jede Phase ist ein eigenständiger, getesteter Commit. Reihenfolge ist load-bearing.

**DM-1 — Codec + Integrität, abwärtskompatibel.** Implementiere den kanonischen Codec (§2b) als geteilte Funktion auf beiden Runtimes, hinter einem `encoding_v`-Diskriminator, sodass alte Blobs weiter lesbar bleiben. Per-Chunk + whole-content sha256 ergänzen. **Cross-Runtime-Byte-Identitäts-Test** (native write → browser read und umgekehrt). *Schließt H6/M6 ohne Schema-Bruch.*

**DM-2 — Versionierung reparieren.** documents/spreadsheets schreiben echte `MAX(version)+1` statt `_v1`; native cv-print-Writeback angleichen; `current_version_id` zeigt auf die neueste. Migration: bestehende `_v1`-Rows bleiben gültig (version=1). *Schließt L13.*

**DM-3 — Kanonische Collections + Engine.** `doc_*`-Collections + `doc_model.rs`-Engine + `doc-model.js`-Browser-Helfer einführen (Wire-Contract-Fixtures regenerieren, beide Seiten, `dist/` neu bauen, drei `?v=`-Cache-Buster bumpen — siehe AGENTS.md). Engine-Surfaces (business_commands/CLI/MCP) registrieren, durch den Policy-Chokepoint gegated.

**DM-4 — Module migrieren (je 1 Commit).** documents → spreadsheets → cv-print auf die `doc_*`-Lane + den geteilten Helfer falten. Pro Modul: Daten-Migration der alten Collections in `doc_*` (oder Dual-Read-Übergang), Modul-Tests grün, `run-all.mjs` grün.

**DM-5 — Native Office-Pfade.** Nativer DOCX-Writer (Rust, gegen die `zip`-Crate; ersetzt den python-docx-Subprozess, H7) und DOCX-Reader (doc-stack `parse_docx` → kanonisches Modell, M11); nativer XLSX-Pfad (H15). Alle schreiben/lesen die `doc_*`-Lane.

**DM-6 — FTS + Editor-Konsolidierung.** EIN FTS-Index über `doc_documents.index_text`; eine Rich-Text-Engine wählen (§6) und das zweite Modul darauf migrieren; geteilte Sanitization/Serialization (schließt zugleich die notes-XSS-Surface L25).

---

## 5. Test- & Guard-Strategie

- **DM-1** Pflicht: Cross-Runtime-Round-Trip-Byte-Identität (Rust-Test + `run-all.mjs`), sha256-Mismatch wirft.
- Jede DM-Phase: `node src/apps/business-os/rxdb/tests/run-all.mjs`, `cargo test --manifest-path src/core/rxdb/Cargo.toml`, betroffene Modul-`*.test.mjs`, gezielter `cargo test -p ctox`.
- Schema-Änderungen (DM-3): Fixtures regenerieren **beide** Seiten, nie eine einseitig (AGENTS.md); `dist/` neu bauen + 3 Cache-Buster.
- Neuer Guard: assert, dass kein Modul mehr einen eigenen Blob-Codec / eigene `*_blob_chunks`-Collection definiert (verhindert Re-Divergenz).

---

## 6. Offene Entscheidungen (brauchen Produkt-Input)

1. **Rich-Text-Engine:** SuperDoc (documents, DOCX-nativ) **oder** Lexical (notes). Empfehlung: SuperDoc als kanonisch (DOCX-Round-Trip ist die Kern-Anforderung), notes darauf migrieren — aber das ist eine UX-Entscheidung.
2. **notes in `doc_*`?** notes ist heute flach (kein Blob/Version). Optionen: (a) notes bleibt eine schlanke Sonder-Collection (kein Blob); (b) notes wird ein `doc_type="markdown"`-Dokument. Empfehlung (a) kurzfristig — notes braucht keine Blob-Chunks; nur die Editor-Engine vereinheitlichen.
3. **Migrations-Modus:** Hart-Migration (einmalig `doc_*` füllen, alte Collections droppen) vs. Dual-Read-Übergang. Empfehlung: Dual-Read pro Modul (DM-4), dann Cleanup — minimiert Re-Pull-Schmerz (IndexedDB).

---

## 7. Risiken

- **Schema-Migration über RxDB/WebRTC:** Re-Pull nötig (IndexedDB clearen auf Upgrade). Pro Modul ausrollen, nicht big-bang.
- **Paralleler Core-Effort:** `store.rs`/`rxdb_peer.rs` werden parallel stark bearbeitet — DM-1/DM-3 sollten erst nach Settle dieser Arbeit (oder in enger Koordination) landen, sonst Merge-Schmerz im Blob-/Projection-Pfad.
- **DOCX-Fidelity (DM-5):** Ein nativer Rust-DOCX-Writer muss die Layout-Qualität des bestehenden (jetzt via H7b funktionierenden) python-docx-Pfads mindestens halten, sonst ist es eine Regression — Snapshot-Tests gegen Referenz-Dokumente.

---

**Kürzestfassung:** Eine `doc_documents/doc_versions/doc_blob_chunks`-Lane mit **einem** per-Chunk-base64-raw-slice-Codec + sha256, einer nativen Engine (chokepoint-gegated, CLI+command+MCP) und einem geteilten Browser-Helfer. Phasen DM-1 (Codec/Integrität) und DM-2 (Versionierung) sind abwärtskompatibel und sofort wertstiftend; DM-3..6 falten die Module und Office-/FTS-Pfade darauf.
