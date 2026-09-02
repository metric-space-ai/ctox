# SKF Nachrecherche — Kampagnen-Board (drone_bearing_design_verified)

**Headline:** Phase 0 läuft (Snapshots → Texte lokal); kritischer Pfad = Textextraktion fertig → Sol-Slices starten (3 parallel, OpenAI-Pool).

Owner-Auftrag 02.09.2026: „da müssen noch Stunden an nachträglicher Quellenauswertung rein“ — 138 verifizierte Quellen sind nur zu 17 inhaltlich ausgewertet (31 Claims), 121 Quellen hängen im Graph frei, Knowledge zeigt rohe Textfetzen, Messdaten nur aus SRC-0123.

Zielbild: jede der 138 Quellen inhaltlich ausgewertet (Relevanzurteil, 3–10 präzise deutsche Ingenieur-Claims mit wörtlichem Zitat + Seite, Statement-Typ, Limitationen, Themenfeld), Claims quellenübergreifend konsolidiert (mehrere Quellen je Claim), Graph mit Kanten für alle relevanten Quellen, Messdaten aus SRC-0099/0126/0128/0129 extrahiert, Rückschreiben in die SKF-Instanz per `ctox knowledge data import`, Knowledge Books v1.2, Export neu gebaut.

---

## Done

- **[P0a] Befund gemessen (02.09. 19:20):** Dump `/Volumes/tmp/skf-research-board/skf-drone-domain-dump.json.gz`: source_catalog 138 (alle verified, full_text, Snapshot-Pfade auf skf-vm unter `/home/ctox/.local/share/ctox/research/skf-baseline-20260725/snapshots/sources/`, 1,03 GB); evidence_points 4346 = 4177 direct_measurement_row (nur SRC-0123) + 138 source_relevance (Roh-Chunks) + 31 claim_support; claims.csv (v4, 31 Claims, 17 Quellen, gute deutsche Claim-Texte) liegt NICHT in den Knowledge-Tabellen, nur im v4-Ordner; Graph-CSV: 9 topic + 138 source + 31 claim, Kanten supports (source→claim) + informs (claim→KB).
- **[P0b] Rückschreibepfad verifiziert (Code):** `ctox knowledge data import --domain X --key Y --from-file <csv|parquet|json> [--mode replace|append]` (src/core/knowledge/ops.rs:632); Evidence-Tabellen werden beim Import normalisiert (`evidence_eligible` nur mit passendem Snapshot/Receipt → snapshot_path/sha256 müssen stimmen). Knowledge Books = `ctox knowledge skill add-skillbook` (Skript `update-skf-knowledge-v4.sh` auf skf-vm).
- **[P0c] Worker-Health 02.09. 17:23:23Z:** alle 12 Worker ready (Sol gpt-5.6 3,9 s; Terra 6,6 s).

## Working

- **[P0d] Snapshots → lokal → Text.** rsync der 138 Dateien nach `/Volumes/tmp/skf-research-board/snapshots/` (Log `rsync.log`), danach automatisch `extract-texts.py` → `texts/SRC-XXXX.txt` mit `[[PAGE n]]`-Markern (Log `nachrecherche/logs/extract.log`, Statistik `texts/_stats.json`). Fertig = 138 Textdateien, Liste der Kleinst-/Fehl-Extraktionen.

## To-Do

- **[P1] Sol-Slices Quellenauswertung** — TRIGGER: P0d fertig. 138 Quellen in ~28 Slices à 5 (nach Textgröße balanciert), fixes JSON-Schema je Quelle nach `nachrecherche/out/<slice>/SRC-XXXX.json`; 3 Sol parallel (OpenAI-Pool, Terra ruht). Launchpad `~/.local/state/workjet-launchpads/skf-nachrecherche`. Abnahme je Slice: 5 JSON-Dateien, Schema-Validator grün, Zitate wörtlich im Text nachweisbar (Skript prüft Substring).
- **[P2] Konsolidierung** — TRIGGER: ≥ 80 % der Slices integriert. Claims clustern (gleiche Aussage aus mehreren Quellen → ein Claim, mehrere Evidenzzeilen), Themenfelder ggf. erweitern, Widersprüche markieren. Sol-Brief mit allen out-JSONs als Input; Ausgabe claims-v5.csv + evidence_points-claims-v5.csv + Graph-CSVs (Generator-Skript von Fable).
- **[P3] Messdaten-Extraktion** — TRIGGER: P0d (unabhängig von P1). SRC-0099 (Vibration/Strom/Drehmoment-Datensatz), SRC-0126 (mehrachsige Vibration), SRC-0128 (ENOLA), SRC-0129 (Enodise B3): Archivinhalt prüfen, Messzeilen ins measured_load_points-Schema (nur direkt berichtete Werte, Einheiten original).
- **[P4] Import auf skf-vm** — TRIGGER: P2 + P3 abgenommen. Backup der Runtime-DB, `ctox knowledge data import` je Tabelle, Skillbooks v1.2, RxDB-Projektion prüfen, Export neu bauen, Board-HTML nach ~/Downloads.

## Backlog + Owner

- OWNER: Sollen off-topic-Quellen (Relevanzurteil „nicht relevant“) aus dem Katalog fliegen oder mit Urteil bleiben? Vorschlag Fable: bleiben, aber im Graph nur mit Relevanz-Kante, nicht als „verifizierte Quelle“ gezählt.
- Generische Vertriebs-Bewertung (`buyer_clarity` …) im Research-Modul `buildSourceModels` ersetzen — Codeänderung in `src/apps/business-os/modules/research/index.js`, eigener PR.

---

## Environment traps

- skf-vm hat kein pdftotext/PyMuPDF → Extraktion lokal (poppler 26.04 unter /opt/homebrew).
- `/Volumes/tmp` 97 % voll (15 GB frei) — Snapshots 1 GB, nach Kampagne löschen.
- Evidence-Import: `normalize_evidence_rows_with_server_receipts` setzt `evidence_eligible=false` ohne passenden Snapshot → snapshot_path/sha256 aus source_catalog übernehmen.
- Workjet-Snapshot-Limit 64 MiB → Launchpad-Repo, absolute Datenpfade unter /Volumes/tmp im Brief (Daten, nicht Repo).

## Error patterns

1. (leer)

## Evidence map

- Dump + v4-Referenz: `/Volumes/tmp/skf-research-board/{skf-drone-domain-dump.json.gz,v4/}`
- Manifest: `/Volumes/tmp/skf-research-board/source-manifest.json` (138 Zeilen)
- Texte: `/Volumes/tmp/skf-research-board/texts/`
- Worker-Ausgaben: `/Volumes/tmp/skf-research-board/nachrecherche/out/`
- Export-Board: `/Volumes/tmp/skf-research-board/SKF-Web-Research-Board-drone_bearing_design_verified.html` (Build: `node build-page.mjs`)
