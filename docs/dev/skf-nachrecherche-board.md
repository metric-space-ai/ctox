# SKF Nachrecherche — Kampagnen-Board (drone_bearing_design_verified)

**Headline:** KAMPAGNE GELANDET 22:40 — 138/138 Quellen ausgewertet, 745 Claims (147 quellenübergreifend), 5.103 Messpunkte aus 3 Datensätzen, alle 138 Quellen im Graph verbunden; Tabellen auf skf.ctox.dev projiziert, Knowledge Books v1.2.0, Export neu in ~/Downloads. Offen: Folgekarten unten.

Owner-Auftrag 02.09.2026: „da müssen noch Stunden an nachträglicher Quellenauswertung rein“ — 138 verifizierte Quellen sind nur zu 17 inhaltlich ausgewertet (31 Claims), 121 Quellen hängen im Graph frei, Knowledge zeigt rohe Textfetzen, Messdaten nur aus SRC-0123.

Zielbild: jede der 138 Quellen inhaltlich ausgewertet (Relevanzurteil, 3–10 präzise deutsche Ingenieur-Claims mit wörtlichem Zitat + Seite, Statement-Typ, Limitationen, Themenfeld), Claims quellenübergreifend konsolidiert (mehrere Quellen je Claim), Graph mit Kanten für alle relevanten Quellen, Messdaten aus SRC-0099/0126/0128/0129 extrahiert, Rückschreiben in die SKF-Instanz per `ctox knowledge data import`, Knowledge Books v1.2, Export neu gebaut.

---

## Done

- **[P4] IMPORT AUF SKF GELANDET 22:22–22:36.** Backup `…/research/skf-baseline-20260725/nachrecherche-20260902T202228Z/backup/` (Parquet-Ordner + `knowledge_data_tables.sql`). Parquet direkt nach `state/ctox/knowledge/data/drone_bearing_design_verified/` (claims NEU kdt-a69748c0, evidence_points, semantic_graph_nodes, semantic_graph_edges, measured_load_points), Katalog aktualisiert, Projektion per `ctox knowledge data tag` angestoßen → RxDB: claims 745, evidence_points 1.289, nodes 894, edges 2.169, measured_load_points 5.103 (5.000 projiziert, rows_complete=false). Graph-Vertrag lokal und im Dump geprüft: `status ready, origin persisted, 240 Knoten/182 Kanten` bei deep/240. Knowledge Books v1.2.0: 11 Bücher (`update-skf-knowledge-v5.sh`, 24 ok), Skill-Binding notes/origin/artifact_path aktualisiert. Probe-Tabelle `nachrecherche_probe/probe` wieder gelöscht.
- **[P2] ABGESCHLOSSEN 22:19 — 9/9 Themenfelder konsolidiert (Sol):** 147 Cluster aus 553 Einzel-Claims (76 mit ≥ 3 Quellen), 6 Widersprüche dokumentiert (`p2/contradictions.json`), 29 Verschiebungen, Lücken je Themenfeld (`p2/gaps.md`). Neue Themenfelder: KB-102 „Elektrische Lagerströme und Elektroerosion“ (13 Claims), KB-110 „Randthemen außerhalb der Lagerauslegung“ (10). Stichprobe Fable: 8 Cluster + alle Widersprüche fachlich plausibel.
- **[Export] neu gebaut 22:38** aus Re-Dump der projizierten Tabellen (`skf-drone-domain-dump-v5.json.gz`): 9,5 MB, 138 Quellen, 1.289 Evidenz (nur Claims + Relevanz), 5.000 Messpunkte, 240 Graph-Knoten/164 Kanten, 13 Themenfelder → `~/Downloads/SKF-Web-Research-Board-drone_bearing_design_verified.html`; alte Fassung `…-v3-altdaten.html`.
- **[P1] ABGESCHLOSSEN 21:47 — 40 Slices, 138/138 Quellen, Validator 138/138 grün, 0 needs_review, alle Runs result-import + integrated.** Ergebnis: 1.120 Claims (542 direkte Messungen, 313 analytisch, 150 Beobachtungen, 77 Annahmen, 38 normativ), 973 mit Zahlenwerten, 394 Querbezüge zwischen Quellen, 85 Quellen mit tabellarischen Messdaten. Relevanz: 48 core, 88 context, 2 off_topic. Themenfelder: KB-009 386, KB-006 193, KB-001 149, KB-007 130, KB-008 82, KB-005 68, KB-004 54, KB-003 27, KB-002 26, NEU „Elektrische Lagerströme und Elektroerosion“ 5. Laufzeit 19:39–21:47 (2 h 8 min, 3 Sol parallel, ≈ 55 s je Quelle netto). Ausgaben `nachrecherche/out/slice-*/SRC-*.json`.
- **[P3] Messdaten aus ENOLA + Enodise extrahiert (19:45, deterministisch, `nachrecherche/extract-measurements.py`):** 926 direkt berichtete Zeilen → `nachrecherche/p3/measured_load_points_new.csv` + `evidence_points_new.csv` (Spaltensatz identisch zu den Bestandstabellen, Provenienz aus source_catalog). SRC-0128 ENOLA: 7 Windkanal-Dateien, 370 Zeilen, 852–11.261 RPM, Schub −3,4…23,4 N, Moment −0,20…0,78 Nm (negative Werte = Windmühlen/Leerlauf, wie berichtet; Header-Defekt „THRUST[N]u_THRUST[N]“ behandelt, XOAR 9x7 ohne Unsicherheiten). SRC-0129 Enodise B3-FLAP: 556 Load-Cell-Dateien mit Werten (22 nur NaN übersprungen), 23.000 RPM, Schub −0,46…13,0 N, Moment 0,058…0,160 Nm, Flügelkräfte im Feld operating_condition. SRC-0099/SRC-0126 sind Artikel-PDFs, die Rohdaten liegen extern (Mendeley/Figshare) — nicht im Snapshot, daher keine Zeilen (Owner-Frage unten).
- **[P0a] Befund gemessen (02.09. 19:20):** Dump `/Volumes/tmp/skf-research-board/skf-drone-domain-dump.json.gz`: source_catalog 138 (alle verified, full_text, Snapshot-Pfade auf skf-vm unter `/home/ctox/.local/share/ctox/research/skf-baseline-20260725/snapshots/sources/`, 1,03 GB); evidence_points 4346 = 4177 direct_measurement_row (nur SRC-0123) + 138 source_relevance (Roh-Chunks) + 31 claim_support; claims.csv (v4, 31 Claims, 17 Quellen, gute deutsche Claim-Texte) liegt NICHT in den Knowledge-Tabellen, nur im v4-Ordner; Graph-CSV: 9 topic + 138 source + 31 claim, Kanten supports (source→claim) + informs (claim→KB).
- **[P0b] Rückschreibepfad verifiziert (Code):** `ctox knowledge data import --domain X --key Y --from-file <csv|parquet|json> [--mode replace|append]` (src/core/knowledge/ops.rs:632); Evidence-Tabellen werden beim Import normalisiert (`evidence_eligible` nur mit passendem Snapshot/Receipt → snapshot_path/sha256 müssen stimmen). Knowledge Books = `ctox knowledge skill add-skillbook` (Skript `update-skf-knowledge-v4.sh` auf skf-vm).
- **[P0c] Worker-Health 02.09. 17:23:23Z:** alle 12 Worker ready (Sol gpt-5.6 3,9 s; Terra 6,6 s).

## Working

- (nichts aktiv; Pump-Schleifen beendet)

## Working (alt)

- **[P1] Slice-03 INTEGRIERT 19:46 (SRC-0006, SRC-0097; 2/2 gültig; result import + runs mark ok).** Generator `nachrecherche/build-tables.py` läuft gegen die bisherigen 10 Ausgaben: 74 neue Claims, 45 Quellen mit Kanten, 5.218 Evidenzzeilen, 926 P3-Zeilen (`nachrecherche/v5/`).
- **[P2] Konsolidierung läuft** — `nachrecherche/pump-p2.py` (Plan `p2/p2-index.json`, Ledger `p2/runs-p2.json`, Validator `validate-p2.py`): je Themenfeld KB-001…KB-009 ein Sol-Brief, Eingabe `p2/input-KB-00X.json`, Ausgabe `p2/clusters-KB-00X.json`. Fertig = 9 Cluster-Dateien grün; dann `merge-p2` → `p2/clusters.json` → `build-tables.py`. (Run-IDs in `nachrecherche/runs.json`; Ereignisse `nachrecherche/logs/<slice>.events.log`; Pump `nachrecherche/pump.py` alle 120 s, Log `logs/pump.log`). Pump validiert fertige Slices (`validate.py`, Zitat-Substring-Prüfung gegen texts/) und markiert nur grüne Slices `integrated`; rote → `needs_review` für Fable. Sol 3/3 — nicht zusätzlich starten.
- **[P0d] Snapshots → lokal → Text.** rsync der 138 Dateien nach `/Volumes/tmp/skf-research-board/snapshots/` (Log `rsync.log`), danach automatisch `extract-texts.py` → `texts/SRC-XXXX.txt` mit `[[PAGE n]]`-Markern (Log `nachrecherche/logs/extract.log`, Statistik `texts/_stats.json`). ERLEDIGT 19:36: 138 Textdateien, 18,5 Mio Zeichen, keine Fehlextraktion; Riesen: SRC-0119 5,0 M (SKF-Katalog), SRC-0121 1,3 M, SRC-0067 0,58 M; ZIP-Listen auf 90 k gekürzt. 40 Slices (`nachrecherche/slices.json`, Briefs `nachrecherche/briefs/`), sechs Briefs mit Gezielt-lesen-Hinweis.

## To-Do

- **[F1] CTOX-Defekt melden/fixen:** `ctox knowledge data` head/count/export/import panicken im Release (Polars `LazyFrame.collect()` ohne `new-streaming`); eager Reader wie in `read_rows_capped` verwenden (`src/core/knowledge/ops.rs`). TRIGGER: eigener PR.
- **[F2] Projektions-Cap 5.000 Zeilen:** measured_load_points hat 5.103 → 103 UIUC-Zeilen nicht im Browser. Entweder Cap anheben (Chunking existiert) oder Messtabelle je Datensatz splitten und Research-Modul mehrere Messtabellen lesen lassen. TRIGGER: Owner-Entscheid.
- **[F3] Research-Modul: Bewertungsvorlage** (`buildSourceModels`, 17 Vertriebskriterien) durch Aufgabenkriterien ersetzen; Bewertungsmatrix (8 Zeilen, alte IDs) neu erzeugen. TRIGGER: eigener PR.
- **[F4] Knowledge-Ansicht der Web-Research-App** zeigt Evidenz-Zitate; die konsolidierten Claims liegen in der neuen Tabelle `claims` — App-Seite darauf umstellen. TRIGGER: eigener PR.
- ERLEDIGT **[P1] Sol-Slices Quellenauswertung** — TRIGGER: P0d fertig. 138 Quellen in ~28 Slices à 5 (nach Textgröße balanciert), fixes JSON-Schema je Quelle nach `nachrecherche/out/<slice>/SRC-XXXX.json`; 3 Sol parallel (OpenAI-Pool, Terra ruht). Launchpad `~/.local/state/workjet-launchpads/skf-nachrecherche`. Abnahme je Slice: 5 JSON-Dateien, Schema-Validator grün, Zitate wörtlich im Text nachweisbar (Skript prüft Substring).
- **[P2-Merge] clusters.json zusammenführen + Generator v5** — TRIGGER: 9/9 P2-Läufe grün. Danach eigene Stichprobe (10 Cluster lesen).
- **[P2-alt] Konsolidierung** — ERLEDIGT durch Start der Läufe. Claims clustern (gleiche Aussage aus mehreren Quellen → ein Claim, mehrere Evidenzzeilen), Themenfelder ggf. erweitern, Widersprüche markieren. Sol-Brief mit allen out-JSONs als Input; Ausgabe claims-v5.csv + evidence_points-claims-v5.csv + Graph-CSVs (Generator-Skript von Fable).
- ERLEDIGT **[P4] Import auf skf-vm** Backup der Runtime-DB, `ctox knowledge data import` je Tabelle, Skillbooks v1.2, RxDB-Projektion prüfen, Export neu bauen, Board-HTML nach ~/Downloads.

## Backlog + Owner

- OWNER: SRC-0099 (Vibration/Strom/Drehmoment-Datensatz, Data in Brief) und SRC-0126 (mehrachsige Vibration, Sci Data) verweisen auf externe Repositorien (Mendeley/Figshare). Sollen diese Rohdaten nachgeladen und als neue Snapshots verifiziert werden (neue source_catalog-Zeilen)? Ohne Freigabe bleiben sie Kontextquellen ohne Messzeilen.
- OWNER: Sollen off-topic-Quellen (Relevanzurteil „nicht relevant“) aus dem Katalog fliegen oder mit Urteil bleiben? Vorschlag Fable: bleiben, aber im Graph nur mit Relevanz-Kante, nicht als „verifizierte Quelle“ gezählt.
- Generische Vertriebs-Bewertung (`buyer_clarity` …) im Research-Modul `buildSourceModels` ersetzen — Codeänderung in `src/apps/business-os/modules/research/index.js`, eigener PR.

---

## Environment traps

- **Projektion der Knowledge-Tabellen läuft nur** (a) sofort nach einem mutierenden `ctox knowledge data`-Befehl im Daemon (auch `tag`, lifecycle, kein Polars) oder (b) im Hintergrund mit Idle-Backoff. Nach Direkt-Parquet: `ctox knowledge data tag --domain … --key … --tag k=v` als Trigger.
- **Graph-Vertrag** (`validatePersistedGraph`): node kind ∈ topic|concept|source|evidence|measurement (Claims = `evidence`), relation ∈ supports|measures|derived_from|part_of|contradicts|correlates_with|co_occurs (Claim→Topic = `part_of` „Evidence belongs to topic“); ein einziger falscher Knoten macht den ganzen Graph `invalid_graph_contract`. Vorab prüfen mit `graph-check2.mjs <dump>`.
- `ctox knowledge skill query` ist für Archetyp systematic-research nicht unterstützt (kein Fehler der Bücher).
- **skf-vm ctox 0.3.22 (branch-main-20260829T234251Z): ALLE Polars-Lazy-Verben von `ctox knowledge data` (head, count, export, import) panicken** mit `get_streaming_executor_builder() failed (hint: missing feature new-streaming?)`. Die Projektion liest eager (`read_rows_capped`) und läuft. Rückschreiben daher: Parquet mit identischem Schema (pyarrow, ZSTD) nach `~/.local/state/ctox/knowledge/data/<domain>/<key>.parquet` + `knowledge_data_tables` in `state/ctox/ctox.sqlite3` (bytes/updated_at, neue Zeile für `claims`). CTOX-Defekt separat melden: `src/core/knowledge/ops.rs` import/export/head/count nutzen `LazyFrame.collect()`.
- **RxDB-Projektion bettet max. 5.000 Zeilen je Tabelle ein** (`KNOWLEDGE_TABLE_RXDB_ROW_CAP`). Deshalb: evidence_points ohne die 4.177 direct_measurement_row-Duplikate (Messzeilen leben in measured_load_points); measured_load_points = 926 P3-Zeilen zuerst + 4.177 UIUC = 5.103 → die letzten 103 UIUC-Zeilen werden nicht eingebettet (rows_complete=false). Folgekarte: Cap anheben oder Messtabelle splitten.
- pyarrow braucht Python 3.13 (uv venv unter `/Volumes/tmp/skf-research-board/.venv`), esbuild lokal per npm im Board-Ordner.
- skf-vm hat kein pdftotext/PyMuPDF → Extraktion lokal (poppler 26.04 unter /opt/homebrew).
- `/Volumes/tmp` 97 % voll (15 GB frei) — Snapshots 1 GB, nach Kampagne löschen.
- Evidence-Import: `normalize_evidence_rows_with_server_receipts` setzt `evidence_eligible=false` ohne passenden Snapshot → snapshot_path/sha256 aus source_catalog übernehmen.
- Workjet-Snapshot-Limit 64 MiB → Launchpad-Repo, absolute Datenpfade unter /Volumes/tmp im Brief (Daten, nicht Repo).

## Error patterns

2. `pdftotext -layout` verschränkt zweispaltige Paper zeilenweise → wörtliche Zitate zerreißen (SRC-0002: 6/9 Zitate „nicht gefunden“). Fix 19:50: Re-Extraktion in Lesereihenfolge für alle nicht laufenden Quellen; Validator prüft zusätzlich Wortfolge im Fenster (≥ 90 %).
3. `runs mark integrated` ohne vorherigen `result import` → workspace_rejected (1×); Pump macht jetzt import → mark.
1. Warteschleife `while pgrep -f "rsync …"` fand ihren eigenen sh-Prozess und endete nie (1×) — bei pgrep-Wartern Muster wählen, das den eigenen Aufruf nicht matcht.

## Evidence map

- v5-Tabellen: `nachrecherche/v5/*.csv`, Parquet `nachrecherche/v5/parquet/`, Skillbooks `v5/update-skf-knowledge-v5.sh`, Bericht `v5/build-report.json`; auf skf-vm `…/research/skf-baseline-20260725/nachrecherche-latest/` (Symlink auf Zeitstempel-Ordner mit Backup).
- P2: `nachrecherche/p2/{input,clusters}-KB-00X.json`, `clusters.json`, `contradictions.json`, `gaps.md`, Ledger `p2/runs-p2.json`.
- Werkzeuge: `extract-texts.py`, `make-slices.py`, `validate.py`, `pump.py`, `make-p2.py`, `validate-p2.py`, `pump-p2.py`, `merge-p2.py`, `build-tables.py`, `write-parquet.py` (.venv), `make-skillbooks.py`, `p4-import.sh`, `dump-rxdb.py`, `graph-check2.mjs`.
- Dump + v4-Referenz: `/Volumes/tmp/skf-research-board/{skf-drone-domain-dump.json.gz,v4/}`
- Manifest: `/Volumes/tmp/skf-research-board/source-manifest.json` (138 Zeilen)
- Texte: `/Volumes/tmp/skf-research-board/texts/`
- Worker-Ausgaben: `/Volumes/tmp/skf-research-board/nachrecherche/out/`
- Export-Board: `/Volumes/tmp/skf-research-board/SKF-Web-Research-Board-drone_bearing_design_verified.html` (Build: `node build-page.mjs`)
