# APPSTORE-V2 — Kampagnen-Board

**Headline:** P0/P1/P3A/P5 gelandet; P2+P4+P6T2 fertig im Arbeitsbaum und warten
auf EINEN gruenen cargo check — kritischer Pfad = fremder cargo-test-Lock auf
/Volumes/tmp/ctox-check-target (PID 58382), danach commit-tree-Commit.

Zielbild (entscheidungsfrei bis auf OWNER-Karten unten):
https://claude.ai/code/artifact/00f7ce23-1cb4-450d-bb26-a1f1f5fc2898
Phasen: 0 Boden · 1 Katalog/Loader · 2 Origin+Core-Repair · 3 Store-Kanal appstore.ctox.dev · 4 Nutzer-Apps-Pipeline · 5 Code-Modus Shell · 6 Rubriken.

---

## Done

- **[P4-Rest] Importer-Bypass GESCHLOSSEN — von Parallelsitzungen, nicht von uns.**
  Gemessen 02.09.: der Browser-Importer oeffnet den Verzeichnis-Picker nur noch
  `mode: 'read'` und schickt `ctox.business_os.app.create` ueber den Kommandobus;
  nativ materialisiert `materialize_business_os_app_import_source` in einen
  Snapshot unter `runtime/business-os/app-imports/<command_id>/` und verlangt
  `install_target=runtime-installed-module`. KEIN Schreibpfad nach local-modules/
  mehr (grep ueber alle business_os-Rustdateien: nur Lesen, Loeschen, Tests).
  Commits fremd: `04f689358` (durable harness jobs), `574df1d7e` (native ack),
  `6734be3d1`. Damit ist Befund L der Ausgangsanalyse (Importer umgeht Policy,
  Versionsledger und Provenance) erledigt.
- **[F1] SICHERHEITSBEFUND in P2 gefunden UND behoben (Kimi-Review 31.08., von Fable am
  Code bestaetigt)** — die GitHub->lokal-Abkuerzung leitete das Quellverzeichnis aus dem
  REQUEST ab (`root.join(subpath)`, nur gegen `..` gefiltert). Ein Nutzer mit Founder-Rechten
  auf eine eigene App haette sein eigenes Verzeichnis als `verified:true /
  ctox-first-party-source` installieren koennen -> Umgehung genau des Trust-Gates, das
  Zip/URL/Fremd-GitHub blockiert, plus `origin:official` in der Projektion. NIE COMMITTET
  (starb im Arbeitsbaum). Fix: `first_party_local_catalog_source` nimmt kein `root` mehr,
  leitet den Pfad allein aus der Modul-ID ab (`<source_app_root>/modules/<id>`); ein
  Request-Subpath entscheidet nur noch OB, nie WELCHES Verzeichnis. Regressionstest
  `first_party_local_source_ignores_requester_supplied_paths` mit 3 feindlichen Pfaden.
- **KORREKTUR: P6 Teil 1 wurde von fremdem Commit `128aed748` zurueckgedreht** (thesen-
  outbound-Sitzung committete Title-Case-Kategorien ueber die Slugs, in 38 Manifesten +
  registry.json + app.js-Fallback). Im Arbeitsbaum wiederhergestellt (Diff nachgemessen:
  ausschliesslich `"category":`-Zeilen), geht mit dem naechsten Commit erneut rein.
- **[P1.3] Store liest nur noch die Server-Projektion** — GitHub-Discovery komplett raus
  (api.github.com/raw-Fetches, mergeMarketplace, DESKTOP_APPS-Merge), Refresh lädt die
  Projektion; Kopf/Locales „Offizieller Katalog"; Wächter auf neuen Vertrag. Visuell im
  Geometrie-Labor nachgeprüft. Commits `93228b38a`, `72770889e`, `23e7ed6a0`.
- **[P6 Teil 1] Rubrik-Slugs kanonisch** — 39 Manifeste auf die 16 Workjet-Slugs,
  Generator erzwingt Slugs, Store zeigt lokalisierte Labels. Teil 2 (imported=origin:user)
  wartet auf P2. Commit `31016bf1b`.
- **[P5] Code-Modus ausgesprochen** — Mechanik existierte (integrierte Fenster-Modi
  app|source|versions inkl. Agent-Panel); Titelmenü-Eintrag heißt jetzt „Code-Modus",
  desktop-apps/README.md dokumentiert code-editor als Shell-Komponente. Labor 37/37.
  Commit `14a347e6d`. OFFEN als Restlast: Klick-Durchstich auf laufender Instanz
  (Labor mountet Apps einzeln, nicht das Titelmenü).
- **BEFUND (Labor-Screenshot): explorer-Leerzustand** zeigt überlappende
  Kopf-Fragmente („Geändert"/„Details") ohne Daten — Kosmetik im Sol-Port, App sonst OK.


- **[P1.2b] explorer + file-viewer als Core-Module gelandet** — Sol-Port (Run
  `123104Z-d4712c24`, integrated) + Fable-Shell-Umbau: DESKTOP_APPS geleert (IDs stabil,
  Auflösung über Modulkatalog), 21 System-Apps, Schema-Direktimporte der Eigentümer (9/9
  byte-identisch nachgemessen), Tests 17+6 grün, Shell-V2 37/37 OK. Legacy desktop-apps/
  bleibt (browser_rust_smoke.js importiert file-viewer in 4 Smokes — Löschmessung hat
  Entfernung gestoppt). APP_BUILD → v328. DEPLOY-PFLICHT: Binary + Slot zusammen (include_str).
  Commit `4ab89e374`.
- **[P3A] Appstore-Publisher gelandet** — build-appstore-index.mjs + Testsuite (Sol-Run
  `122315Z-09c728d1`, integrated): 18 signierbare per-App-Bundles, Index v1, Determinismus
  und Signatur-Roundtrip im Hauptbaum verifiziert (5/5 Tests, Proof 37 Dateien).
  Commit `3b434daf5`.
- **[P1.3a] Store-Overlays ins Fenster + actionIcon-Fallback** — 2 vorbestehend rote
  app-store-Wächter grün gemacht (echte Defekte: Modals auf document.body). 27/27 Tests.
  Commit `8475b65e5`.
- **[P0.1] Bundle-Wächter grün** — `coding-agents` Manifest: `deletable=false`, Store-Block auf
  `installable:false / editable_after_install:false / distribution:system-module` (wie registry
  und alle System-Apps). Verifiziert: `node src/apps/business-os/scripts/assert-standard-app-bundle.mjs`
  → `standard_app_bundle_ok=1 selected=17`. Commit `f1f5d84d0`.
- **[P0.2] Registry-Generator gelandet** — `scripts/generate-module-registry.mjs` projiziert
  die 36 Manifeste deterministisch nach `modules/registry.json` UND in den markierten
  `OFFLINE_FALLBACK_CATALOG`-Block in app.js (jetzt alle 18 System-Apps statt 10);
  `--check` = Drift-Wächter; harte Validierung der Core-Mitgliedschaft. 7 Manifest-Versionen
  auf SemVer normalisiert. APP_BUILD → `20260831-ctox-appstore-registry-v326` (11 Träger).
  Verifiziert: generate --check, assert-standard-app-bundle, assert-shell-v2-contract
  (33/35, 2 Restbefunde calendar/importer sind vorbestehend), node --check app.js — grün.
  Commit `393ba88ce`.
- **[P5-Messung] Shell-Kartierung Code-Modus** — KORREKTUR zweier Annahmen aus der Codekarte:
  (1) `code-editor` ist HEUTE SCHON keine startbare App — er fehlt in DESKTOP_APPS
  (app.js:4346 enthält nur explorer+file-viewer) und wird ausschließlich als integrierte
  Source-Ansicht ins App-Fenster gemountet (`ensureIntegratedModuleToolSession`, Modi
  app|source|versions, app.js:2598ff; Mount app.js:2713). Der Fenster-Modus-Umschalter,
  den OWNER 2 fordert, existiert im Kern bereits. (2) `creator` ist eine TOTE Referenz:
  kein Modul, kein DESKTOP_APPS-Eintrag, aber der App-Store ruft `ctx.openApp('creator')`
  → der „Neue App per KI-Prompt"-Weg über den Store ist vermutlich defekt (in P1.2 messen).
  Coding läuft heute als separates Fenster (coding-agents) aus dem Titel-Versionsmenü
  (app.js:2450); code-editor hat aber ein eingebautes Agent-Panel mit `ctox.coding.turn`
  (desktop-apps/code-editor/app.js:890). P5 = Umschalter sichtbar machen + Agent-Panel im
  integrierten Modus + Reste (`DESKTOP_APP_DB_COLLECTIONS` browser/creator, app.js:6531).
- **[P0-Messung] kundenpipeline-Doppelgänger vermessen** — `src/apps/business-os/installed-modules/`
  ist gitignored (.gitignore:111) und wird von keinem Loader gelesen (Serving-Root ist
  `<state>/runtime/business-os`). Kein Laufzeitkonflikt. ABER: die ignorierte Kopie (0.3.1) ist
  gegenüber der Core-Kopie (0.1.0) echt divergiert (`core/`-Verzeichnis, records.test.mjs,
  audit-scenarios.json nur dort; alle Kernddateien differieren). → OWNER-Karte unten.

- **[P1.2a] creator als System-App gelandet** — modules/creator/ war komplett, nur ohne
  module.json; App-Store-Knopf „Neue App per KI-Prompt" lief gemessen als 'unknown app' ins
  Leere. Manifest + Aufnahme in beide System-Listen (19 Apps), Registry+Fallback regeneriert,
  APP_BUILD → v327. Wächter grün (creator Shell-V2-konform, creator.test 23 pass).
  Commit `22c959be9`. HINWEIS: Core-Wirkung nativ erst nach Binary-Rebuild
  (system-apps.json ist include_str!) — bis dahin stuft ein Alt-Binary creator als store ein.
- **BEFUND: app-store.test.mjs 2x rot auf HEAD** (vorbestehend, nicht von uns):
  Regex-Wächter `actionIcon/getActionIcon` und `(els.root || state.ctx?.host)?.append(overlay)`
  matchen modules/app-store/index.js nicht; Datei trägt stale Buster v141. Fällig in P1.3.

## Working

- **[P2+P4+P6T2] fertig im Arbeitsbaum, wartet auf cargo check** — ALLES geschrieben und
  im Worktree (unkommittiert, ueberlebte den Sitzungsabbruch verifiziert):
  * P2 origin/repair/local-catalog (Sol-Lauf `135757Z-50418a77`, integrated)
  * F1-SICHERHEITSFIX (siehe Done)
  * P4 Nutzer-Apps: zip/url/fremd-github installieren als `untrusted-user-source`,
    aber NUR mit instanzweiter apps.install-Autoritaet (Modul-Grant reicht nicht);
    Manifest-Selbstdeklaration von app_source/origin/trust_model wird vor dem
    Stempeln entfernt; local-modules sind jetzt deletable und `uninstall_app_module`
    raeumt beide Wurzeln.
  * P6 Teil 2: workjetCategoryForModule nutzt `origin` statt PUBLIC_DISTRIBUTIONS.
  Dateien: module_lifecycle.rs (+506/-77), module_manifest_loader.rs (+34/-3),
  store.rs (+141/-20), command_plane.rs, business_command_inventory.json,
  app-store/{index.js,index.css,index.html,module.json,locales/*}, workjet-theme.js.
  Log: scratchpad/cargo-p2p4.log. FERTIG = cargo check gruen -> commit-tree-Commit
  (Fremd-WIP in server.rs/mcp_channel.rs/store_catalog_projections.rs NICHT mitnehmen).
  UMGEBUNG: fremde cargo-test-Sitzung (PID 58382) haelt den Lock auf
  /Volumes/tmp/ctox-check-target; eigener Check haengt in "Blocking waiting for
  file lock on build directory" und laeuft automatisch an.

## To-Do
- **[P3] Store-Kanal V1** (CI-Publisher + nativer Client + Kimi-Cyber-Review). TRIGGER: nach P2;
  DNS appstore.ctox.dev = OWNER-Handgriff.
- **[P3B] nativer Store-Client** — `appstore`-Source-Kind: Index+Bundle von
  appstore.ctox.dev holen (SSRF-geschuetzt), Ed25519-Signatur gegen gepinnten Store-Key
  pruefen, sha256 des Bundles pruefen, als origin:official installieren. Faellt ohne
  gepinnten Key FAIL-CLOSED aus. TRIGGER: nach P2/P4-Commit. Owner-Gate: DNS + Key.


## Backlog + Owner

- **OWNER: kundenpipeline-Fork auflösen.** Ignorierte Kopie 0.3.1 (installed-modules/, mit
  core/-Dir + Zusatztests) vs. Core-Kopie 0.1.0 (modules/). Welche ist die Wahrheit? Bis zur
  Entscheidung wird nichts gelöscht; Serving nutzt die Core-Kopie.
- **OWNER: DNS/Subdomain appstore.ctox.dev anlegen** (Voraussetzung P3-Deploy; Deploy-Mechanik
  wie ctox.dev, siehe Memory project_ctox_dev_deploy).
- **OWNER 3 (Default läuft): Rubrikliste** = 16 Workjet-Slugs, „REM Capital" wird
  Kunden-Sichtbarkeit statt Rubrik. Veto möglich bis P6-Start.
- **Beobachtung:** `src/apps/business-os/runtime/qa/` (untracked Screenshots) verletzt
  „src/ ist nur Quellcode"; nicht Teil dieser Kampagne, nichts angefasst.
- **Beobachtung:** rem-* Apps liegen in gitignoriertem installed-modules/ ohne
  customer-app-binding.json — würden selbst am richtigen Ort von authorize_runtime_module
  abgelehnt. Gehört zu P4/K5-Kontext.

---

## Umgebungsfallen

- **STEMPEL-HOHEIT ABGEGEBEN (17:0x):** die Parallelsitzung hat alle Cache-Buster auf ihr
  Schema `20260831-shell-v2-unified-v325` vereinheitlicht und dabei auch meine v326–v330
  ersetzt sowie den GEPINNTEN `workjet-theme.js?v=20260826-workjet-ui-contract-v1` überschrieben
  → 2 rote Wächter in workjet-shell-integration.test.mjs (deren Kampagne, nicht unsere).
  Regel: KEINE eigenen APP_BUILD-Bumps mehr; Straggler v330 wurde auf unified-v325 angeglichen.
- **PARALLELE SITZUNG IM BAUM (seit ~14:42):** jemand editiert live modules/calendar
  (calendar-view-adapter.js, index.js) und modules/importer (index.css/html/js, Kopfband-
  Umbau). Diese Dateien NICHT anfassen, NIE mitcommitten. Ihre Arbeit hat nebenbei die
  zwei alten Shell-V2-Befunde (calendar hardcoded-accent, importer media-query) behoben.
- **Vorbestand im Baum (seit Sitzungsstart, unkommittiert):** server.rs (+203,
  Shell-Generation-Wächter), mcp_channel.rs (+201), store_catalog_projections.rs (+7),
  pi_sidecar.rs, install/mod.rs, skills/, AGENTS.md u.a. — vor jedem Commit auf diesen
  Pfaden Hunks strikt trennen; ggf. Vorbestand als eigenen, beschrifteten Commit übernehmen.

- Worker-Worktrees können kein Rust bauen (gitignorierte Artefakte fehlen) → Diffs selbst im
  Hauptbaum kompilieren.
- Workjet-Snapshots: nur HEAD, kein origin, detached HEAD; 64-MiB-Archivlimit → ggf. Launchpad
  unter ~/.local/state/workjet-launchpads/ (nie Scratchpad). Ergebnisse via
  `workjet result import <run-id>` als refs/workjet/<run-id>.
- Kein absoluter Repo-Pfad in Briefs; Briefs prüfen Baum-Identität über Dateiinhalt, nicht Refs.
- Shell-Arbeit: Slots nur aus main; scripts/shell-v2-geometry-lab.mjs +
  scripts/assert-shell-v2-contract.mjs für shell-wirksame Änderungen.
- `src/apps/business-os/installed-modules/` ist gitignored — Änderungen dort landen nie in git.

## Fehlermuster

1. Prämisse vor dem Auftrag messen (kundenpipeline: vermeintlicher Laufzeitkonflikt war keiner).

## Evidenzkarte

- Board: diese Datei (docs/dev/appstore-v2-board.md, committed) + stabile Artifact-URL:
  https://claude.ai/code/artifact/e9ffba86-1126-4663-ad64-0213db76ceb3
  (Rendering via Scratchpad-Skript render-board.mjs; bei Board-Updates neu publizieren).
- Zielbild-Artifact: https://claude.ai/code/artifact/00f7ce23-1cb4-450d-bb26-a1f1f5fc2898
- Codekarte (18 Befunde A–R): Explorer-Report, kondensiert im Zielbild §2.
- Wächterlauf P0.1: lokal reproduzierbar via assert-standard-app-bundle.mjs.
