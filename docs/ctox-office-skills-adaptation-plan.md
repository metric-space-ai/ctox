# CTOX Office-Skills-Adaption: Word, Excel, PDF

Stand: 2026-07-11. Status: Phase 1 vollständig umgesetzt.

Umsetzungsstand:

- `pdf`-Skill v2: portiert (Commit `b7efb183c`).
- `doc`-Skill v2: portiert inkl. `references/execution-surfaces.md` mit
  Operation→Fläche→Feature-Gruppe-Mapping (Commit `bcc2d3ac6`); vertieft um
  `references/authoring-design.md` (Token-Methode, Register, Template-Modus,
  Design-Audit) und `references/review-lifecycle.md` (Redline/Kommentar-
  Semantik, OOXML-Grundlagen, Finalisierung, Liefer-Checkliste).
- `spreadsheet`-Skill v2: portiert inkl. Gating-Referenz (Commit `e3068225d`);
  vertieft um `references/charts-and-models.md`.
- **Ebene-B-Ops Slice 1+2 implementiert** (`src/core/office-engine/src/ops.rs`,
  Commits `810b6fb05` + `a00d35d25`): `comments-extract|add|resolve|strip`,
  `a11y-audit`, `privacy-scrub`, `redact` (längenerhaltende Maskierung,
  Begriffe + E-Mail/Telefon-Muster), `tracked-changes-accept|reject` (reject
  verweigert `*PrChange` statt zu raten), `protection-set`, `style-lint`,
  `fields-report`, `table-export` — deterministische OOXML-Transformationen,
  unberührte Parts byte-identisch, CLI-Dispatch in `ctox-office-engine`,
  29 Tests grün plus Real-Fixture-Smoke.
- **Ebene-B-Ops Slice 3 implementiert** (Commit `be5134b58`):
  `fields-materialize` (REF/PAGEREF/SEQ flatten, PAGE bleibt live),
  `watermark-audit|remove`, `a11y-fix` (Alt-Text aus Namen,
  Header-Zeilen), `tracked-changes-replace` (echte del+ins-Revisionspaare
  mit Run-Splitting; komplexe Runs werden gemeldet, nicht geraten),
  `merge-append` (Seitenumbruch-getrennt; verweigert Dokumente mit
  Media/Hyperlink/Kommentar-Relationships). 34 Tests grün.
- **Ebene-B-Ops Slice 4 implementiert** (Commit `d17e45482`):
  `style-normalize` (löscht Font/Größe/Farbe-Overrides nur in
  Heading/Title-Absätzen, Emphasis bleibt), `watermark-add`
  (selbständiges VML-Fragment inkl. Shapetype, verweigert Pakete ohne
  Header), `table-import` (CSV → Tabelle mit expliziter DXA-Geometrie
  und Header-Zeile). 38 Tests grün. **Damit ist die komplette Op-Liste
  aus der ursprünglichen Capability-Matrix implementiert** (alle 20
  geplanten Batch-Operationen verfügbar).
- **Gating-Guard** (`src/scripts/check-office-skill-gating.mjs`, Commit
  `e5d0dd216`): diffst die execution-surfaces-Tabellen gegen `features.json`,
  schlägt bei Drift oder unreferenzierten Gruppen fehl. Hat unmittelbar die
  Matrix-Re-Baseline vom 2026-07-11 erkannt (differential_passed →
  oracle_captured); Tabellen und Skill-Prosa folgen jetzt dem Ist-Stand.
- Weiterhin extern blockiert: `business_commands`-Fläche der Ops
  (`business_os/mod.rs`/`server.rs` in-flight beim Port-Agenten) sowie
  Phase 2/3 (keine Gruppe `shipped`; Re-Baseline hat Statusse regressiert).

## Lizenz-Randbedingung (Umsetzungserkenntnis)

Das Codex-Runtime-Plugin-Material (`documents`, `spreadsheets`,
`template-creator`, neue `pdf`-Fassung) ist „Copyright 2026 OpenAI. All
rights reserved" (OpenAI-ToS), **nicht** frei lizenziert. Verbatim-Übernahme
in dieses öffentliche Repo ist damit ausgeschlossen. Konsequenz:

- Die v2-Skills sind **eigenständige Adaptionen in eigenen Worten** (Methodik
  übernommen, Text neu), keine Kopien; die Repo-Lizenz gilt.
- Das ursprünglich geplante 1:1-Portieren der 25 Task-Playbooks entfällt;
  deren Substanz ist in die SKILL.md-Doktrin und die Referenzen eingeflossen.
- Vorbestehend: `packs/content/doc` enthielt bereits All-rights-reserved-
  Material im öffentlichen Repo; mit dem v2-Rewrite wurde es entfernt.
- Gating wurde statt per `requires_features`-Frontmatter (hätte Loader-
  Änderung gebraucht) über die `references/execution-surfaces.md`-Tabellen
  gelöst — Statuswechsel in `features.json` werden dort nachgezogen.

Quelle der Skills: Codex-Runtime-Plugins (`documents`, `spreadsheets`, `pdf`,
Stand 2026-07-10, `~/.cache/codex-runtimes/codex-primary-runtime/plugins/`).
Ziel: `src/skills/packs/content/{doc,spreadsheet,pdf}` v2, ausgeführt über die
CTOX-eigenen Office-Implementierungen statt über Python/Poppler/Office.js.

Bezugssysteme:

- Euro-Office-Port: `docs/ctox-office-port-plan.md`,
  `src/apps/business-os/office-engine/features.json` (Feature-Matrix, 24 Gruppen)
- Rust-Kern: `src/core/business_os/office_engine.rs` (`inspect`/`export`,
  byte-erhaltender OOXML-Merge, Protokoll `ctox-euro-office-editor-bootstrap-v1`)
- Business-OS-Module: `src/apps/business-os/modules/{documents,spreadsheets}`
  (WordPort-Format), Auslieferung/Persistenz über RxDB-Records und Desktop-Files

## Grundprinzip: Vier-Schichten-Zerlegung

Jeder Codex-Office-Skill zerfällt in vier Schichten. Nur die unteren zwei
werden ersetzt:

1. **Workflow-Doktrin** (übernehmen, nahezu wörtlich):
   Render-and-verify-Gate („kein Deliverable ohne visuelle Prüfung jeder
   Seite"), Minimal-Edit-Disziplin bei Bestandsdokumenten, Form-Factor-Auswahl
   (Prosa/Steps/Checkliste/Tabelle), Table-Gate gegen Tabellen-Missbrauch,
   Formel-Auditierbarkeitsregeln, Design-Preset-Pflicht mit Token-Auflösung.
2. **Domänen-Referenzen** (übernehmen, unverändert):
   `ooxml/*.md` (Tracked Changes, Comments, Fields, Rels), `design_presets.md`,
   `header_templates.md`, `style_guidelines.md`, `charts.md`,
   `domain_guidance/*` (Finanzmodelle, FP&A, Healthcare, Marketing, Research).
3. **Ausführungskontrakt** (vollständig ersetzen):
   python-docx/`@oai/artifact-tool`/reportlab/Poppler/LibreOffice →
   CTOX-Office-Flächen (siehe unten). Keine Python-Scripts im Pack.
4. **Runtime-Plumbing** (streichen oder durch CTOX-Idiome ersetzen):
   Google-Docs-Import-Pipeline, `codex-file-citation`-Syntax,
   Workspace-Dependency-Loader, `manifest.txt`-Download-Tooling, `uv pip
   install`-Fallbacks (verletzt CTOX-Dependency-Regeln).

## Ausführungsflächen: zwei Ebenen statt Script-Toolbelt

Die 36 Python-Scripts des documents-Skills haben zwei fundamental verschiedene
Rollen, die in CTOX auf zwei verschiedene Flächen gehören:

**Ebene A — Editor-Flows (interaktiv/layoutwirksam):** Der Euro-Office-Port
liefert einen vollwertigen, headless ansteuerbaren Editor (Oracle-Flows und
`fake-runtime.mjs` beweisen die Headless-Ansteuerbarkeit bereits). Alles, was
Layout erzeugt oder verändert — Authoring, Formatierung, Tabellen, Bilder,
Tracked Changes im Kontext — läuft als typisierter Flow gegen den Editor, auf
demselben Codepfad, den auch Nutzer klicken. Das Rendering für die
Verify-Schleife kommt aus `document.open-render-zoom` /
`spreadsheet.open-render-sheets` (beide `differential_passed`) — LibreOffice
und Poppler entfallen als Renderer.

**Ebene B — Native Batch-Operationen (deterministisch/OOXML-direkt):**
Operationen, die kein Editor-Layout brauchen, sondern deterministisch am
OOXML-Paket arbeiten, gehören als semantische Funktionen in
`office_engine.rs` — mit dünner `ctox-office-engine`-CLI-Fläche (Harness) und
`business_commands`-Fläche (App), gemäß der Regel „capabilities serve apps AND
harness". Das betrifft: Privacy-Scrub, Redaction, Protection, Merge,
Watermark, A11y-Audit, Style-Lint, Feld-Materialisierung, Kommentar-Extraktion.
Diese Ebene ist vom Editor-Port unabhängig und kann parallel entstehen.

## Mapping: documents-Skill → CTOX

| Codex-Operation (Scripts) | CTOX-Fläche | Feature-Gruppe | Status |
|---|---|---|---|
| `render_docx.py`, `render_and_diff.py` | Engine-Render + Differential-Infra (existiert im Port als Oracle-Vergleich) | `document.open-render-zoom` | differential_passed |
| `accept_tracked_changes`, `add_tracked_replacements`, `comments_add/extract/apply_patch/strip` | Ebene A (im Kontext) + Ebene B (Batch-Accept/Extract) | `document.comments-track-changes` | differential_passed |
| `table_geometry`, `docx_table_to_csv`, `xlsx_to_docx_table` | Ebene A (Geometrie) + Ebene B (Konvertierung) | `document.tables` | differential_passed |
| `style_lint`, `style_normalize`, `apply_template_styles`, `heading_audit` | Ebene B (Audit/Normalisierung) | `document.styles-lists-numbering` | differential_passed |
| `section_audit`, `images_audit` | Ebene B | `document.sections-headers-footers`, `document.images-positioning` | differential_passed |
| `insert_ref_fields`, `flatten_ref_fields`, `fields_materialize/report`, `insert_toc`, `internal_nav`, `captions_and_crossrefs` | Ebene A + B | `document.links-bookmarks-fields` | differential_passed (TOC-Materialisierung prüfen) |
| `privacy_scrub`, `redact_docx`, `set_protection` | **Ebene B, keine Feature-Gruppe nötig** — reine OOXML-Ops, passt zu Policy-/Credentials-Doktrin | — | sofort baubar |
| `a11y_audit`, `merge_docx_append`, `watermark_add/audit_remove` | **Ebene B, keine Feature-Gruppe** | — | sofort baubar |
| `content_controls` (Forms/SDTs) | **Lücke**: weder Feature-Gruppe noch Engine-Op | fehlt in features.json | Kandidat für neue Gruppe |
| `footnotes_report` | **Lücke**: Fußnoten fehlen in der Feature-Matrix | fehlt in features.json | Kandidat für neue Gruppe |
| `google_docs_title_sanitize` | streichen (Google-Docs-Pipeline ist Codex-spezifisch) | — | — |
| `make_fixtures` | existiert bereits als Oracle-Fixture-Tooling im Port | — | vorhanden |

Die 25 Task-Playbooks (`tasks/*.md`) und die OOXML-Referenzen portieren nahezu
1:1 — sie beschreiben Vorgehen, nicht Werkzeuge; nur die Script-Aufrufe darin
werden durch die jeweilige CTOX-Fläche ersetzt.

## Mapping: spreadsheets-Skill → CTOX

Der Codex-Skill hängt vollständig am `@oai/artifact-tool`-JS-API
(`workbook.inspect/render/help`, `SpreadsheetFile.exportXlsx`). Das Gegenstück
in CTOX ist eine **typisierte Workbook-Fläche auf dem headless
Spreadsheet-Editor** über den bestehenden MessageChannel-Protokollpfad:

| artifact-tool-API | CTOX-Fläche | Feature-Gruppe | Status |
|---|---|---|---|
| `workbook.inspect` (values/formulas/match) | Read-API auf Editor oder `inspect` in office_engine erweitern | `spreadsheet.open-render-sheets`, `edit-save` | differential_passed |
| `workbook.render({sheet, range, scale})` | Engine-Render | `spreadsheet.open-render-sheets` | differential_passed |
| Zell-/Formel-Authoring | Editor-Flows | `spreadsheet.formulas-references` | **discovered** — gated |
| Charts (`charts.md`) | Editor-Flows | `spreadsheet.charts` | **discovered** — gated |
| Conditional Formatting erweitern | Editor-Flows | `spreadsheet.validation-conditional-formatting` | **discovered** — gated |
| Kommentare/Protection | Editor-Flows + Ebene B | `spreadsheet.comments-names-protection` | **discovered** — gated |
| `exportXlsx` | `office_engine::export` (byte-erhaltend) | — | vorhanden |

Vollständig portierbar ab sofort (execution-agnostisch): Formelregeln
(Auditierbarkeit, Helper-Zellen, keine Magic Numbers, absolute/relative
Referenzen), Datenformatregeln (typisierte Werte statt Strings,
locale-invariante Formatcodes), Edit-Disziplin (Bestandsformat studieren,
minimale lokale Änderung, Conditional-Formatting-Ranges miterweitern),
Domain-Guidance, Verifikationsregeln (Fehler-Scan `#REF!/#DIV/0!/...`,
visueller Pass über alle Sheets).

`excel-live-control` (Office.js gegen laufendes Excel) wird nicht portiert.

## Mapping: pdf-Skill → CTOX

Am wenigsten gekoppelt, aber auch am wenigsten Substanz (84 Zeilen). Zwei Wege:

- **Lesen/Prüfen:** Rendering über Poppler bleibt fachlich richtig; die
  Bereitstellung wechselt von „`uv pip install` / `brew install` zur Laufzeit"
  (verboten) zu CTOX-provisioniertem Tooling. Anknüpfung an den bestehenden
  Report-Pfad (`src/core/report/render/`), der Poppler-Referenzen schon kennt.
- **Erzeugen:** Nicht reportlab portieren. PDF-Erzeugung läuft in CTOX
  perspektivisch als Export-Pfad der Documents-Engine (DOCX → PDF beim
  Euro-Office-Port ohnehin nötig für Druck) plus dem bestehenden
  Report-Renderer. Der Skill beschreibt dann: „erzeuge das Dokument als DOCX
  über den doc-Skill, exportiere als PDF, verifiziere gerendert".

Qualitätskontrakt (keine Auslieferung mit Clipping/Overlap/kaputten Glyphen,
ASCII-Bindestriche, menschenlesbare Zitate) portiert unverändert.

## CTOX-Idiome, die Codex-Plumbing ersetzen

- **Persistenz/Auslieferung:** Deliverables sind Business-OS-Records bzw.
  Desktop-Files (RxDB-repliziert), nicht Dateipfade in `outputs/<thread>/`.
  Skills nennen als Abschlusskriterium den persistierten Record.
- **Zitate/Verweise:** `:codex-file-citation{...}` → Business-OS-Deep-Links
  (Konvention existiert bereits im `ctox-business-os-mcp`-Skill).
- **QA-Evidenz:** Codex behandelt Render-PNGs als wegwerfbare Intermediates.
  CTOX macht sie zu **Prozess-Evidenz**: Render-Ergebnisse der Verify-Schleife
  werden als Outcome-Evidence persistiert — das Render-Gate wird damit
  durchsetzbar (Review kann prüfen, ob wirklich gerendert wurde) statt nur
  Prompt-Appell zu sein.
- **Kein Environment-Gefrickel:** Alle „falls Tool fehlt, installiere
  X"-Passagen entfallen; fehlende Fähigkeit = Blocker mit klarer Meldung,
  Runtime-Konfiguration über die bestehenden Stores.

## Gating: Skills folgen der Feature-Matrix

Skill-Fähigkeiten werden pro Task-Playbook an Feature-Gruppen aus
`features.json` gebunden (deklarativ im Playbook-Frontmatter, z. B.
`requires_features: ["document.comments-track-changes"]`). Ein Task ist erst
nutzbar, wenn seine Gruppen `shipped` sind; `differential_passed` erlaubt
Nutzung hinter dem gleichen Rollout-Flag wie der Editor selbst. Damit können
die Skill-Texte vollständig vorbereitet werden, ohne dass ein Skill
Operationen verspricht, die es nicht gibt.

Heute bedeutet das: Documents-Tasks sind vorbereitbar (12/12
differential_passed), Spreadsheet-Authoring bleibt auf Lesen/Analysieren/
Rendern beschränkt, bis `formulas-references` u. a. landen.

## Rückwirkungen auf den Office-Port (Gegenprobe)

Die Skills decken Operationen ab, die in der 24-Gruppen-Matrix fehlen —
Kandidaten für neue Feature-Gruppen oder explizite Ebene-B-Entscheidungen:

1. Content Controls / Forms (SDTs)
2. Fußnoten/Endnoten
3. Protection / Restrict Editing
4. Dokument-Merge
5. Wasserzeichen
6. A11y-Audit (nur Ebene B, kein Editor-Feature)
7. TOC-/Feld-Materialisierung für deterministisches Rendering

## Phasenplan

- **Phase 1 (sofort, unabhängig vom Editor-Port):**
  a) `pdf`-Skill v2 portieren (geringste Kopplung).
  b) Ebene-B-Operationen in `office_engine.rs` beginnen: `privacy_scrub`,
     `redact`, `a11y_audit`, `merge`, `protection`, `comments_extract` —
     reine OOXML-Ops mit CLI + business_commands, testbar gegen die
     vorhandenen `tests/fixtures/office/`. **Abstimmung mit dem laufenden
     Office-Port-Agenten nötig** (Datei ist in-flight, uncommitted).
  c) Skill-Texte v2 für doc/spreadsheet schreiben (Schichten 1+2 portieren,
     Ausführungsabschnitte gegen die CTOX-Flächen formulieren, Gating-
     Frontmatter einführen).
- **Phase 2 (wenn Editor-Gruppen shipped):** Editor-Flow-Fläche als typisierte
  Headless-API freigeben (Oracle-Flow-Infrastruktur generalisieren);
  documents-Tasks scharf schalten.
- **Phase 3:** Spreadsheet-Authoring-Tasks in dem Takt aktivieren, in dem die
  Spreadsheet-Gruppen durch die Matrix wandern; Workbook-Read/Render-API
  analog artifact-tool anbieten.

## Nicht portieren

- `excel-live-control` (Office.js, setzt laufende Excel-Instanz voraus)
- Google-Docs/Google-Sheets-Zielpfade (Codex-Plugin-spezifisch)
- `presentations`-Plugin: erst relevant, wenn eine Slides-Engine existiert;
  bis dahin bleibt das bestehende `slides`-Skill unverändert
- `template-creator`: erneut prüfen, wenn der Template-Store
  (`src/apps/business-os/template-store/`) im Port stabil ist

## Handoff an den Office-Port-Agenten

Direkte Bitten des Skills-/Ops-Strangs an den Strang, der `office_engine.rs`,
`business_os/server.rs` und `features.json` besitzt. Die Batch-Ops in
`src/core/office-engine/src/ops.rs` sind zustandslose reine Funktionen über
Paket-Bytes und vollständig getestet — sie warten nur auf deine Flächen.

1. **`business_commands`-Registrierung der Batch-Ops** (bitte in deiner
   nächsten Server-Welle): Vorschlag Command-Familie `business_os.office.*`
   mit 1:1-Mapping auf die `ops.rs`-Funktionen, z. B.
   `office.comments_extract`, `office.tracked_changes_accept`,
   `office.privacy_scrub`, `office.redact`, `office.a11y_audit`,
   `office.merge_append` usw. (vollständige Liste + Semantik:
   `src/skills/packs/content/doc/references/execution-surfaces.md`).
   Payload: Referenz auf Desktop-File/Record + Op-Parameter; Ergebnis als
   neues Desktop-File/Record plus JSON-Report im Command-Status. Policy:
   Modul-/Collection-Write-Grant des Ziel-Records, Audit-Reason pro Op;
   Einträge in `business_command_inventory.json` nicht vergessen. Ausführung
   bounded + persisted (kein Hintergrundprozess).
2. **`features.json`-Disziplin:** Nach jedem Statuswechsel bitte
   `node src/scripts/check-office-skill-gating.mjs` laufen lassen (läuft
   jetzt auch als eigener CI-Workflow `office-skill-gating`). Der Guard
   erwartet, dass die Gating-Tabellen der Skills jedem Statuswechsel folgen —
   bei deiner Re-Baseline heute hat er korrekt angeschlagen. Wenn du Gruppen
   umbenennst oder ergänzt, schlagen unreferenzierte Gruppen ebenfalls auf.
3. **Feature-Matrix-Lücken, Entscheidung bei dir:** Content Controls (SDTs)
   und Fußnoten/Endnoten fehlen als Editor-Feature-Gruppen. Protection,
   Merge, Wasserzeichen und A11y sind inzwischen auf Paket-Ebene abgedeckt
   und brauchen m. E. keine eigenen Editor-Gruppen mehr.
4. **Phase-2-Bedarf der Skills (Render-Evidenz):** Die Render-Verify-Schleife
   der doc/spreadsheet-Skills braucht eine headless aufrufbare
   Render-Fläche (Seiten/Ranges → PNG) auf demselben Codepfad wie
   `document.open-render-zoom` / `spreadsheet.open-render-sheets`, plus einen
   Weg, die finalen Renders als Prozess-Evidenz zu persistieren. Die
   Oracle-Flow-Infrastruktur (`fake-runtime.mjs`, `oracle/flows/*`) scheint
   dafür der richtige Generalisierungspunkt. Sobald das existiert, schalten
   die Skills ihr Render-Gate von "Fallback: strukturell prüfen und
   Limitation nennen" auf verpflichtend.
5. **Stabilitätszusage erbeten:** Die CLI-Verben von `ctox-office-engine`
   (inspect|export|…|table-import) sind in den Skill-Texten referenziert.
   Bitte nicht umbenennen ohne die beiden `execution-surfaces.md` und die
   SKILL.md-Zeilen mitzuziehen.
