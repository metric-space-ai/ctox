# Shell v4 – Löschplan (Strang DEBT)

Stand: Do 15.10.2026 16:02 (Berlin, gemessen), Klon `~/Documents/ctox-shell-v4` (HEAD 67bf593b4 + Arbeitsbaum).
Jede Altlast-Datei bzw. jeder Altlast-Block, der mit v4 verschwinden muss, steht hier mit Besitzer (laut
`~/.local/state/ctox-sync-v4/docs/dateigrenzen.md`), heutigen Importeuren/Aufrufern, Löschbedingung und Reihenfolge.
Offene Punkte ohne Datei stehen in `DEBT.md`; der Zahlenwächter ist `ratchet.mjs`.
**Fortschreibung Strang DEL1 (Do 15.10.2026 19:2x)**: Status je Eintrag in §4; S9-Paket in `LOESCHPAKET-S9.md` +
`apply-s9.sh`. Die Nummern L-32 und L-33 sind doppelt vergeben; §4 nennt sie L-32a/L-32b und L-33a/L-33b.

**Importeure sind gemessen, nicht geschätzt**: `git grep -n -I --untracked -E <Muster>` über den ganzen Klon, ohne
`*.md`, `vendor/`, `dist/`, `docs/`, `tmp/`, `v4/tools/guards/` (Wächter-Fixtures) und `v4/debt/`; Selbstverweise der
Datei ausgenommen. Die Spalte nennt die Laufzeit-Importeure; die **vollständige Fundliste je Eintrag mit Datei:Zeile
steht im Anhang** (generiert, `bos/` = `src/apps/business-os/`). Muster-Treffer können Fehltreffer enthalten (z. B.
`desktop-apps/code-editor/*.test.mjs` meint die eigene `app.js`); vor jedem Löschen die Zeile öffnen.

**Reihenfolge** (Phasen, an die Meilensteine gekoppelt):
- **P1 sofort / S7** – Dateien ohne Produktiv-Importeur (nur Tests/Manifest-Listen).
- **P2 nach App-Port / S7–S8** – Dateien, deren letzte Importeure App- oder Testcode sind, der beim Port umgestellt wird.
- **P3 mit dem Shell-Wechsel / S9** – alles, was am alten `app.js`/`index.html` hängt; fällt in einem Schritt mit
  `app.js`, sobald die v4-Shell der Regelweg ist und die Workjet-Hostbrücke (Phase 11) umgezogen ist.
- **P4 Kern** – Gegenstücke in `src/core/**` und Kern-eigene Tests; nur als Verweis, Ausführung bei ctox sync v4.

## 1. Einträge

| Nr. | Pfad | Besitzer | Heutige Importeure/Aufrufer (Laufzeit; alle Fundstellen im Anhang) | Bedingung für das Löschen | Phase |
|---|---|---|---|---|---|
| L-01 | `app.js` (17 795 Z.; Schichten siehe §2) | Shell v4 (außer `createModuleContext` 6839 = Vorlage ctox sync v4; Workjet-Brücke 13362–15924 = ctox sync v4 Phase 11) | `index.html:251` (einziger Lade-Einstieg); 231 Muster-Treffer in 102 Dateien, davon Kern: `server.rs:3624,4928,5009,5051-5052`, `shell_assets.rs:218-300`, `shell_update.rs:30,676,1284,1288`, `store.rs:46445-46509`, `install/mod.rs:3510`; ~45 `shared/*.test.mjs` und `scripts/assert-*.mjs` lesen den Quelltext | v4-Shell ist Regelweg (Loader startet `startShell`), Workjet-Hostbrücke umgezogen (DEBT D-047), alle Tests/Skripte im Anhang L-01 umgestellt oder gelöscht, Kern liefert `app.js` nicht mehr aus (P4) | P3 |
| L-02 | `index.html` – Altteile: `?v=`-Buster (Z. 8, 51-59, 249-251), `shared/shell-release-status.js` (249), `mobile-host.*` (53, 250), `shell-topbar.css` (59), Katalog-Snapshot-Injektion des Servers (`server.rs:3868-3883`, D-native §Was v4 ersetzt) | Shell v4 | Server liefert sie aus (`server.rs` Shell-Assets); guard-v4 Gruppe `shell/index.html` 20 Treffer | v4-Loader-Seite ersetzt sie; Kern entfernt die Katalog-Injektion | P3 |
| L-03 | `mobile-host.js`, `mobile-host.css` | Shell v4 | `index.html:53,250`; Tests `mobile-host.test.mjs:11-12`, `qa/workjet-mobile-host-layout.test.mjs:8`, `shared/workjet-session-control.test.mjs:7`, `shared/workjet-session-events.test.mjs:7`, `scripts/workjet-shell-integration.test.mjs:61`, `rxdb/tests/workjet-device-control-smoke.mjs:16`; Listen `scripts/build-shell-artifact.mjs:24-25`, `scripts/assert-workjet-branding.mjs:14-15`, `v4/manifests/shell.manifest.json:30-31`; Kern `server.rs:3624`, `shell_update.rs:32-33` | Schmale Darstellung als Layout der v4-Shell (Entwurf §10) abgenommen; Workjet-Session-Tests auf v4 umgestellt (Phase 11) | P3 |
| L-04 | `mobile-host.test.mjs` | Shell v4 | – (Test) | mit L-03 | P3 |
| L-05 | `mobile-apps.json` | Shell v4 | `mobile-host.js:250` (fetch), `mobile-host.test.mjs:38` | mit L-03 | P3 |
| L-06 | `system-apps.json` | Shell v4 (Datei) / ctox sync v4 (`include_str!`) | `app.js:11548` (fetch); Kern `store.rs:168` (`include_str!`), `server.rs:3624`, `shell_update.rs:34`; Desktop-Hülle `business-os-desktop/src/main/url-safety.cjs:26`; Skripte `assert-standard-app-bundle.mjs:9`, `build-shell-artifact.mjs:26`, `business-os-app-inventory.mjs:9`, `generate-module-registry.mjs:64`; Tests `appsec-pentest.test.mjs:12`, `module-catalog-cache.test.mjs:99`, `shell-windowing-contract.test.mjs:31`, `mobile-host.test.mjs:39` | Katalog = `business_apps` des Kernels (Entwurf §9); Kern entfernt `include_str!` (P4); Skripte/Tests umgestellt | P3 + P4 |
| L-07 | `modules/registry.json` | Shell v4 | `app.js:174,181` (fetch); Kern `server.rs:3680,4509`; Desktop `url-safety.cjs:27`; Generator `scripts/generate-module-registry.mjs:153`; 6 App-Tests (`appsec-pentest`, `browser:250`, `cv-print-builder`, `documents:482`, `iot:105`, `research/test.mjs:943`), 4 shared-Tests, 6 Skripte | Katalog nur über Sync; App-Tests prüfen gegen `v4/manifests/*.manifest.json` (wie customers schon); L-26 gelöscht | P3 |
| L-08 | Block `OFFLINE_FALLBACK_CATALOG` in `app.js` 11637–13190 (1 552 Z., generiert) | Shell v4 | `app.js:11638,13190`; Generator `generate-module-registry.mjs:148`; Tests `grok-shell-icon-selection-contract.test.mjs:19`, `module-catalog-cache.test.mjs:14` | mit L-01; Generator L-26 vorher entfernen | P3 |
| L-09 | `customer-modules/outbound-lead-generation/schema.js` | Shell v4 | kein Importeur gefunden | sofort; `v4/manifests/outbound-lead-generation.manifest.json` `files` neu generieren | P1 |
| L-10 | `modules/app-store/schema.js` | Shell v4 | kein Importeur gefunden | sofort (D-071); Manifest neu generieren | P1 |
| L-11 | `modules/conversations/schema.js` | Shell v4 | `modules/ctox/schema.js:1` | conversations portiert (D-069), danach L-12 | P2 |
| L-12 | `modules/ctox/schema.js` | Shell v4 | `app.js:967`; Kern-/Sync-Tests `rxdb/tests/{crew-identity,workjet-computer-migration,workjet-project-configuration,workjet-project-migration}-smoke.mjs:3`; `shared/workjet-project-control.test.mjs:686`, `shared/workjet-supervisor-luma-contract.test.mjs:5`; `scripts/assert-module-conformance.mjs:12`; `.github/workflows/workjet-computer-migration.yml:13`; Kern `browser_rust_smoke.js:8252` | Tests lesen `modules/ctox/collections.schema.json` (inhaltsgleich, ctox P:124); Workflow-Pfadfilter umgestellt; L-01 weg | P2 |
| L-13 | `modules/outbound/schema.js` | Shell v4 | `modules/conversations/schema.js:19`; Kern `browser_rust_smoke.js:8057,11998` | nach L-11 („nach Port von conversations entfällt der Import von outbound/schema.js“, outbound P:106) | P2 |
| L-14 | Verweise auf **schon gelöschte** `schema.js` (desktop, tickets, invoices, iot, appsec-pentest …) | Shell v4 / ctox sync v4 (Kern-Dateien) | `app.js:968`; `modules/appsec-pentest/appsec-pentest.test.mjs:5` (Import bricht); `scripts/assert-rxdb-only.mjs:685,706`; `scripts/installed-sync-acceptance.mjs:307`; Kern `browser_rust_smoke.js:5342,8032,8082,8097,8251,11928,11962,12025`, Kommentare `src/core/iot/projector.rs:27,48` | Test/Skripte umstellen oder löschen (L-30); Kern-Rauchtest bei ctox sync v4 (DEBT D-127) | P2 + P4 |
| L-15 | `v4/manifests/*.manifest.json` mit `schema.js` in `files` | Shell v4 | Generator-Ausgabe (`REPORT-M2.md:25`: 8 vorhanden / 2 nicht ladbar; `reports.manifest.json` → gelöschtes `threads/schema.js`, threads P:152) | nach jedem schema.js-Löschen `gen-manifest.mjs` neu laufen lassen | P1/P2 |
| L-16 | `shared/collection-freshness.js` | Shell v4 (Löschliste Dateigrenze) | `app.js:17`; `rxdb/tests/current-pull-freshness-browser-smoke.mjs:15,40`; `scripts/topbar-status.browser.mjs:21,178`; `shared/topbar-apps.test.mjs:5` | mit L-01; Sync-Smoke (ctox sync v4) umgestellt | P3 |
| L-17 | `shared/maintenance-state.js` | Shell v4 (Löschliste) | `app.js:70`; Test `shared/maintenance-state.test.mjs:10` | mit L-01 | P3 |
| L-18 | `shared/shell-generation.js` | Shell v4 (Löschliste) | `app.js:90`; Test `shared/shell-generation.test.mjs:8` | mit L-01 | P3 |
| L-19 | `shared/shell-release-status.js` | Shell v4 (Löschliste) | `index.html:249`; Kern `shell_update.rs:36`; Skripte `build-shell-artifact.mjs:91`, `topbar-status.browser.mjs:22`; Tests `qa/ctox-desktop-shell.test.mjs:15`, `audit-business-os-content.test.mjs:44`, `build-shell-artifact.test.mjs:159`, `shell-generation.test.mjs:45`, `shell-release-status.test.mjs:13` | mit L-02; Kern-Slotliste (P4) | P3 |
| L-20 | `shared/startup-deadlines.js` | Shell v4 (Löschliste) | `app.js:98`; `scripts/assert-startup-deadlines-browser.mjs:12`; Test `shared/startup-deadlines.test.mjs:13` | mit L-01 | P3 |
| L-21 | `shared/command-bus.js` | ctox sync v4 nennt es, Shell v4 löscht bei null Aufrufern | `app.js:959`; `customer-modules/outbound-lead-generation/index.js:11145`; `shared/sync.js:26` (Sync-Kern); Sync-Smokes `rxdb/tests/command-{auth-resilience,bus-projection,plane-observability,submit-atomicity}-smoke.mjs`, `data-plane-progress-health-smoke.mjs:17`; Skripte `assert-app-command-contracts.mjs:40`, `assert-rxdb-only.mjs:265,362,388,391`, `workjet-command-native.browser.mjs:79,141`, `command-consumer-inventory.json:67`; Tests `command-bus.test.mjs`, `command-bus-receipt.test.mjs`, `workjet-project-command-timing.test.mjs` | OLG-UI portiert (D-072), `sync.js` ohne Import (ctox sync v4), L-01 weg; guard `commandBus` = 0 | P3 |
| L-22 | `shared/list-state.js` (Zustandstext „Daten werden …“, Entwurf §2) | Shell v4 | **kein Produktiv-Importeur** mehr: nur `shared/list-state.test.mjs:4`, Listen `v4/manifests/shell.manifest.json:52`, `v4/tools/manifest/gen-manifest.mjs:280` | sofort: Datei + Test löschen, Shell-Liste im Generator kürzen | P1 |
| L-23 | `desktop-apps/code-editor/app.js` (Altform `mount(container, ctx)`), übrige `desktop-apps/`-Reste | Shell v4 | `app.js:3055` (Legacy-Shell); `v4/source/editor.js:2,8,16,17` (nur `resolveMonacoBaseUrl`); `scripts/command-consumer-inventory.json:7,53`; eigene Tests | Code-Editor als Shell-Bibliothek unter `v4/source/` (D-064), L-01 weg; `desktop-apps/` leer → Verzeichnis und Server-Auslieferung (`server.rs:3429-3845`, P4) entfernen | P3 |
| L-24 | `scripts/assert-shell-v2-contract.mjs` | Shell v4 | CI `.github/workflows/native-sync.yml:24,49,229`, `crew-ui-acceptance.yml:49`, `rxdb-frame-liveness.yml:57`; `src/scripts/native-integration-artifact.py:369`; Kommentar `modules/knowledge/index.js:3718`; `AGENTS.md` (Shell-V2-Deploy-Disziplin) | v4-Rahmenvertrag + `v4/tools/geometry/geometry-v4.mjs` in denselben Workflows grün; AGENTS.md-Absatz umgeschrieben (Owner) | P3 |
| L-25 | `scripts/shell-v2-geometry-lab.mjs` | Shell v4 | CI `native-sync.yml:25,50,230`, `crew-ui-acceptance.yml:62`; `native-integration-artifact.py:376`; `AGENTS.md` | Geometrie-Labor gegen v4-Rahmen gelaufen (D-060), Workflows auf `geometry-v4.mjs` | P3 |
| L-26 | `scripts/generate-module-registry.mjs` | Shell v4 | CI `ci.yml:303`, `crew-ui-acceptance.yml:57`, `workjet-project-chats.yml:41`; `app.js:11637` (Kommentar des generierten Blocks) | Manifeste kommen aus `gen-manifest.mjs`; L-07/L-08 weg | P3 |
| L-27 | Slot-Pipeline `scripts/build-shell-artifact.mjs`, `sign-shell-release.mjs` (+ Tests) | Shell v4 | CI `business-os-shell-release.yml:68`; `package.json:32,33,35`; `shell-generation-native-lab.mjs:15` | Shell als Core-signiertes Sync-Artefakt (D-native §Was v4 ersetzt: Slots/Ed25519 ersetzen) | P3 + P4 |
| L-28 | `scripts/shell-generation-native-lab.mjs`, `shell-generation-native.browser.mjs` | Shell v4 | `package.json:27`; gegenseitig | Generationssperre entfällt (Generation = Artefakt-Hash) | P3 |
| L-29 | `scripts/assert-shell-release-chrome.mjs`, `assert-startup-deadlines-browser.mjs`, `topbar-status.browser.mjs`, `shell-v2-data-probe.mjs`, `shell-v2-live-probe.mjs` | Shell v4 | CI `business-os-shell-release.yml:139` (release-chrome); sonst keine Aufrufer gefunden | mit L-19/L-20/L-16; Probes ohne Aufrufer sofort | P1 (Probes) / P3 |
| L-30 | Tote Tests auf v3-`ctx` (testen entfernte Hooks/Dateien) | Shell v4 (App-Ports) | `modules/appsec-pentest/appsec-pentest.test.mjs` (Import `./schema.js` bricht, Z. 5), `appsec-pentest.browser.test.mjs`; `modules/coding-agents/tests/coding-agents.test.mjs`; `modules/documents/documents.test.mjs`, `mail-merge-recipient-switch.test.mjs`, `documents-layout.browser.test.mjs`; `modules/spreadsheets/spreadsheets.test.mjs`, `spreadsheets-version.browser.test.mjs`; `modules/mail/tests/mail-browser-qa.mjs`; ~45 `shared/*.test.mjs` und `scripts/*` aus Anhang L-01 (lesen `app.js`) | je Datei: Fachprüfungen nach `tests/v4-api.test.mjs` überführen, Rest löschen (DEBT D-038); shared-Tests sterben mit L-01 | P2/P3 |
| L-31 | `shared/universal-importer.js/.css` – **v3-Teil** (`ctx.db`, `ctx.sync`, `commandBus`, `?v=`) | Shell v4 | `customer-modules/outbound-lead-generation/index.js:7`; `modules/mail/index.js:1,1961`; `modules/outbound/index.js:8,6611`; `modules/matching/ui/businessOsControls.js:5`, `matching/index.css:1389`; Kern `server.rs:5274`; Skripte/Tests 6 | Port auf App-API 4.0 (kein Löschen der Datei, nur der v3-Wege; DEBT D-067) | P2 |
| L-32 | `design-lab.js`, `design-lab.html` (Shell-Wurzel) | Shell v4 | `scripts/assert-accessibility-contract.mjs:9`, `assert-design-system.mjs:9`, `capture-design-matrix.mjs:10`, `run-design-quality-gates.mjs:13,34`, `build-shell-artifact.test.mjs:83,234`; CI `crew-ui-acceptance.yml:53-54` | v4-Designprüfung über `v4/frame/lab` ersetzt; Gates umgestellt | P3 |
| L-33 | v4-eigene Löschlisten-Treffer (guard Gruppe `v4`: 62, u. a. `vocab:de-laedt` 41, `ref-legacy-catalog-file` 11) | Shell v4 | `ratchet-baseline.json` → `guard.byGroup.v4` | Zustandstexte der neuen Shell nur über die Zone; Labor-Dateien vom Shell-Artefakt ausnehmen | P2 |

Gemessene Summen für die Phasen: P1 = L-09, L-10, L-22, Probes aus L-29 (sofort möglich, kein Produktiv-Importeur);
P2 = L-11 → L-13 → L-12, L-14, L-15, L-30, L-31, L-33; P3 = L-01 … L-08, L-16 … L-21, L-23 … L-29, L-32 in einem
Schritt; P4 = Kern-Verweise unten.

## 2. `app.js` nach Schicht (Quelle: `~/.local/state/shell-v4/analysis/A1-appjs-teil1.md:10-19`, `A2-appjs-teil2.md:10-20`)

Zeilenzahlen gelten für den unveränderten Stand (app.js im Klon = HEAD, 17 795 Zeilen, `git diff --stat` leer).

| Schicht | Teil 1 (1–8914) | Teil 2 (8901–17795) | Summe | v4-Ziel | Besitzer |
|---|---:|---:|---:|---|---|
| SHELL | 4 757 | 2 055 | 6 812 | Neubau unter `v4/frame`, `v4/shell` | Shell v4 |
| WEGFALL_SYNC | 1 619 (Advanced Status 3793–4653 ≈ 860) | 716 | 2 335 | ersatzlos | Shell v4 |
| LADEWEG | 772 | 2 284 (davon `OFFLINE_FALLBACK_CATALOG` 11637–13188: 1 552) | 3 056 | SW + Kernel-Katalog | Shell v4 |
| HUELLE | 743 | 962 | 1 705 | Rahmen v4 | Shell v4 |
| RECHTE | 645 (Grant-Regelwerk 7648–7670) | 115 | 760 | Spiegel `api.actor`, Entscheid im Core | Shell v4 / Kern entscheidet |
| ZUSTANDSTEXT | 86 | 408 | 494 | ersatzlos, Zone | Shell v4 |
| SONSTIGES | 292 | 2 341 (Workjet-Hostbrücke 13362–15924) | 2 633 | Phase 11 | ctox sync v4 |
| (FREMD-Rest) | – | 14 | 14 | – | – |

Einzelblöcke mit Löschlisten-Bezug (Zeilen im heutigen app.js): globaler `fetch`-Wrapper 129–148 (lädt bei neuer
Generation neu), `globalThis.workjet*`-Exporte 677–680, `createModuleContext` 6839 (Vertrag 6857–6984 → Vorlage
App-API, ctox sync v4), `wireShellV2ModuleTitle` 2826, Schema-Importe 967–968, `modules/registry.json`-Abruf 174–181,
`system-apps.json`-Abruf 11548, Code-Editor-Import 3055. guard-v4 zählt in `shell/app.js` 674 Treffer
(`ratchet-baseline.json`).

## 3. Kern-Seite (nur Verweis, Ausführung ctox sync v4)

Quelle: `~/.local/state/shell-v4/analysis/D-native.md:374-401` („Was v4 hier ersetzt/löscht“). Keine Detailanalyse hier.

| Nr. | Kern-Gegenstück | Bezug zu Shell-Einträgen |
|---|---|---|
| K-01 | Statische Auslieferung von App-Code/Manifesten/Icons, `installed-modules/`, `local-modules/`, `modules/registry.json`, `desktop-apps/` (`server.rs:3429-3845`) | L-01, L-03, L-06, L-07, L-23 |
| K-02 | Shell-Slots per HTTPS + Ed25519 im Binary, Generationssperre 409/`-shell-v2-` (`shell_update.rs`, `server.rs:3589-3669`) | L-19, L-27, L-28 |
| K-03 | Katalog-Snapshot in `index.html` injiziert (`server.rs:3868-3883`), Katalogdokument `module-catalog` + 3-s-Polling | L-02, L-07, L-08 |
| K-04 | `include_str!` auf App-Teile: `system-apps.json` (`store.rs:168`), 12 App-Starter-Dateien, `modules/ctox/collections.schema.json` ×2, Schema-Vertrag ×3, Slide-Validator | L-06, L-12 |
| K-05 | 199 einkompilierte Collections, 5-s-Schema-Watch + Peer-Respawn, Migration nur beim Peer-Start | L-09 … L-15 |
| K-06 | 29 tote `/api/business-os/*`-Arme, `/mail/track/*` im Server | – |
| K-07 | Doppeltes Grant-Regelwerk Browser (`app.js:7648-7670`) ↔ nativ (`rxdb_peer.rs:2981-3001`), `migration.sync.*` | L-01 (RECHTE) |
| K-08 | Kern-Tests/Werkzeuge mit Altpfaden: `src/core/rxdb/tools/browser_rust_smoke.js` (schema.js, app.js), `business_os_connection_modes_smoke.js:311`, `desktop_pin_reload_probe.js:18`, `tests/business_os_shell_asset_sync_smoke.sh:34,82,109,143`, `src/apps/business-os/rxdb/tests/*` (Anhang) | L-01, L-12, L-14, L-21 |
| K-09 | Desktop-Hülle `src/apps/business-os-desktop/src/main/url-safety.cjs:26-27` erlaubt `system-apps.json`/`modules/registry.json` | L-06, L-07 |

## Anhang – Fundstellen je Eintrag (generiert 15.10.2026 16:02, `git grep`, Rohliste)

### L-01 · app.js (Shell v3/V2) — 231 Fundstellen in 102 Dateien

Muster: `git grep -nE 'business-os/app\.js|['\"/]app\.js(\?|['\"])|\.\./app\.js'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `.github/workflows/native-sync.yml`: 21, 46
- `contracts/binary_bundle_manifest.txt`: 11
- `scripts/installed-sync-acceptance.mjs`: 207, 208, 216, 234, 250
- `scripts/prepare-installed-sync-tenant.py`: 17
- `bos/desktop-apps/code-editor/code-editor.test.mjs`: 8, 10, 43
- `bos/desktop-apps/code-editor/tests/v4-api.test.mjs`: 38
- `bos/index.html`: 251
- `bos/modules/browser/browser.test.mjs`: 251
- `bos/office-engine/office-engine.test.mjs`: 1279, 1281
- `bos/output/crew-shots/cdp.mjs`: 14
- `bos/output/crew-shots/probe.mjs`: 17
- `bos/qa/ctox-desktop-shell.test.mjs`: 13
- `bos/rxdb/tests/browser-launch-config-secret-free-smoke.mjs`: 4
- `bos/rxdb/tests/collection-invalidation-only-smoke.mjs`: 5
- `bos/rxdb/tests/context-chat-submission-smoke.mjs`: 5
- `bos/rxdb/tests/data-plane-guard-smoke.mjs`: 117, 151
- `bos/rxdb/tests/live-sync-facade-smoke.mjs`: 4
- `bos/rxdb/tests/module-sync-lifecycle-smoke.mjs`: 5
- `bos/rxdb/tests/non-destructive-reconnect-repair-smoke.mjs`: 4
- `bos/rxdb/tests/query-api-smoke.mjs`: 228
- `bos/rxdb/tests/shell-webrtc-readiness-smoke.mjs`: 7
- `bos/rxdb/tests/workjet-device-control-smoke.mjs`: 15
- `bos/scripts/assert-app-migration-inventory.mjs`: 12
- `bos/scripts/assert-context-menu-lifecycle.mjs`: 13
- `bos/scripts/assert-db-isolation-inventory.mjs`: 19, 150, 267, 277
- `bos/scripts/assert-global-context-menu-policy.mjs`: 9
- `bos/scripts/assert-managed-business-os-assets.mjs`: 15, 170, 175
- `bos/scripts/assert-module-collection-allowlists.mjs`: 14
- `bos/scripts/assert-module-context-contract.mjs`: 20
- `bos/scripts/assert-permissions-ui.mjs`: 10
- `bos/scripts/assert-production-release-docs.mjs`: 22, 27
- `bos/scripts/assert-rxdb-only.mjs`: 10, 68, 74, 86, 89, 92, 98, 279, 396, 399, 402, 405, 408, 421, 426, 436, 439, 442, 525, 532, 535, 548, 561, 565, 568, 571, 574, 577, 580, 583, 586, 589, 613, 628, 640, 675, 688, 689, 779, 783, 795, 798, 803, 809, 811, 816, 818, 821, 825, 829, 834, 838
- `bos/scripts/assert-settings-readiness-race.mjs`: 4
- `bos/scripts/assert-workjet-branding.mjs`: 12
- `bos/scripts/audit-business-os-content.mjs`: 91, 119
- `bos/scripts/audit-business-os-content.test.mjs`: 56, 61, 77
- `bos/scripts/build-shell-artifact.mjs`: 22
- `bos/scripts/build-shell-artifact.test.mjs`: 194, 323, 338
- `bos/scripts/business-os-interactive-window-qa.contract.test.mjs`: 12
- `bos/scripts/command-consumer-inventory.json`: 5, 7, 8, 51, 53, 54
- `bos/scripts/generate-module-registry.mjs`: 154
- `bos/scripts/shell-asset-routing.browser.mjs`: 9, 64, 68
- `bos/scripts/shell-generation-native-lab.mjs`: 51, 57, 111
- `bos/scripts/smoke-app-module.contract.test.mjs`: 9
- `bos/scripts/test-shell-page-exit-browser.mjs`: 7
- `bos/scripts/test-shell-page-exit.mjs`: 6
- `bos/scripts/test-shell-window-cache-startup.mjs`: 6
- `bos/scripts/workjet-command-native.browser.mjs`: 19
- `bos/scripts/workjet-shell-integration.test.mjs`: 19
- `bos/shared/app-launch-lifecycle.test.mjs`: 7
- `bos/shared/app-task-badges.test.mjs`: 323
- `bos/shared/audience-route-denial.test.mjs`: 6
- `bos/shared/context-approval-receipt.test.mjs`: 6
- `bos/shared/ctox-collection-access.test.mjs`: 6
- `bos/shared/data-plane-startup.test.mjs`: 6
- `bos/shared/grok-shell-icon-selection-contract.test.mjs`: 14
- `bos/shared/jour-fixe-narration.test.mjs`: 150
- `bos/shared/module-catalog-cache.test.mjs`: 5
- `bos/shared/module-icon-startup.test.mjs`: 4
- `bos/shared/module-navigation.test.mjs`: 6
- `bos/shared/module-schema-dependencies.test.mjs`: 6
- `bos/shared/module-stylesheet-cache.test.mjs`: 4
- `bos/shared/operator-icon-selection-contract.test.mjs`: 86
- `bos/shared/requested-module-startup.test.mjs`: 6
- `bos/shared/scoped-system-data-facade.test.mjs`: 5
- `bos/shared/settings-drawer-sync.test.mjs`: 4
- `bos/shared/shell-crew-health.test.mjs`: 6
- `bos/shared/shell-generation.test.mjs`: 29, 47, 54
- `bos/shared/shell-responsive-header.test.mjs`: 9
- `bos/shared/shell-v2-version-menu-contract.test.mjs`: 5
- `bos/shared/shell-windowing-contract.test.mjs`: 27
- `bos/shared/startup-deadlines.test.mjs`: 56, 63, 75
- `bos/shared/startup-inventory-diagnostics.test.mjs`: 5
- `bos/shared/sync-room-circuit.test.mjs`: 78
- `bos/shared/taskbar-pins.test.mjs`: 43
- `bos/shared/thesen-browser-release-contract.test.mjs`: 7
- `bos/shared/window-geometry-startup.test.mjs`: 6
- `bos/shared/window-recovery.test.mjs`: 6
- `bos/shared/workjet-computer-control.test.mjs`: 6
- `bos/shared/workjet-exit-model-control.test.mjs`: 6
- `bos/shared/workjet-project-command-timing.test.mjs`: 7
- `bos/shared/workjet-project-control.browser.mjs`: 6
- `bos/shared/workjet-project-control.test.mjs`: 14
- `bos/shared/workjet-projection-readiness.test.mjs`: 6
- `bos/shared/workjet-provider-native.test.mjs`: 142
- `bos/shared/workjet-session-control.test.mjs`: 6
- `bos/shared/workjet-session-events.test.mjs`: 6
- `bos/shared/workjet-supervisor-route-native.test.mjs`: 135
- `bos/shared/workjet-supervisor-route.browser.mjs`: 8
- `bos/v4/manifests/shell.manifest.json`: 28
- `bos/v4/source/editor.js`: 8, 17
- `bos/v4/tools/manifest/gen-manifest.mjs`: 277
- `src/core/business_os/server.rs`: 3624, 4928, 5009, 5051, 5052
- `src/core/business_os/shell_assets.rs`: 218, 221, 224, 225, 257, 258, 259, 260, 266, 277, 284, 291, 293, 295, 298, 300
- `src/core/business_os/shell_update.rs`: 30, 676, 1284, 1288
- `src/core/business_os/store.rs`: 46445, 46455, 46459, 46462, 46472, 46476, 46498, 46509
- `src/core/install/mod.rs`: 3510
- `src/core/rxdb/tools/browser_rust_smoke.js`: 739, 823, 2960, 4617, 4628, 16070, 16162, 16230, 16371
- `src/core/rxdb/tools/business_os_connection_modes_smoke.js`: 311
- `src/core/rxdb/tools/desktop_pin_reload_probe.js`: 18
- `src/scripts/vendor-builds/build-ctox-office.mjs`: 170, 282, 285, 301, 311
- `tests/business_os_shell_asset_sync_smoke.sh`: 34, 82, 109, 143

### L-03 · mobile-host.js / mobile-host.css — 21 Fundstellen in 14 Dateien

Muster: `git grep -nE 'mobile-host\.(js|css)'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/index.html`: 53, 250
- `bos/mobile-host.test.mjs`: 11, 12
- `bos/qa/workjet-mobile-host-layout.test.mjs`: 8
- `bos/rxdb/tests/workjet-device-control-smoke.mjs`: 16
- `bos/scripts/assert-workjet-branding.mjs`: 14, 15
- `bos/scripts/build-shell-artifact.mjs`: 24, 25
- `bos/scripts/shell-generation-native-lab.mjs`: 81
- `bos/scripts/workjet-shell-integration.test.mjs`: 61
- `bos/shared/shell-generation.test.mjs`: 43, 46
- `bos/shared/workjet-session-control.test.mjs`: 7
- `bos/shared/workjet-session-events.test.mjs`: 7
- `bos/v4/manifests/shell.manifest.json`: 30, 31
- `src/core/business_os/server.rs`: 3624
- `src/core/business_os/shell_update.rs`: 32, 33

### L-05 · mobile-apps.json — 2 Fundstellen in 2 Dateien

Muster: `git grep -nE 'mobile-apps\.json'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/mobile-host.js`: 250
- `bos/mobile-host.test.mjs`: 38

### L-06 · system-apps.json — 28 Fundstellen in 15 Dateien

Muster: `git grep -nE 'system-apps\.json'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `src/apps/business-os-desktop/src/main/url-safety.cjs`: 26
- `src/apps/business-os-desktop/test/session-view.test.cjs`: 104, 105, 106, 107
- `src/apps/business-os-desktop/test/url-safety.test.cjs`: 46, 48, 54, 55
- `bos/app.js`: 11548
- `bos/mobile-host.test.mjs`: 39
- `bos/modules/appsec-pentest/appsec-pentest.test.mjs`: 12
- `bos/scripts/assert-standard-app-bundle.mjs`: 9
- `bos/scripts/build-shell-artifact.mjs`: 26
- `bos/scripts/business-os-app-inventory.mjs`: 9, 30, 42, 45
- `bos/scripts/generate-module-registry.mjs`: 11, 64, 66, 80, 101
- `bos/shared/module-catalog-cache.test.mjs`: 99
- `bos/shared/shell-windowing-contract.test.mjs`: 31
- `src/core/business_os/server.rs`: 3624
- `src/core/business_os/shell_update.rs`: 34
- `src/core/business_os/store.rs`: 168

### L-07 · modules/registry.json — 38 Fundstellen in 26 Dateien

Muster: `git grep -nE 'modules/registry\.json|\.\./registry\.json|'registry\.json''` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `src/apps/business-os-desktop/src/main/url-safety.cjs`: 27
- `src/apps/business-os-desktop/test/url-safety.test.cjs`: 47, 49
- `bos/app.js`: 174, 181
- `bos/modules/appsec-pentest/appsec-pentest.test.mjs`: 11
- `bos/modules/browser/browser.test.mjs`: 250
- `bos/modules/cv-print-builder/tests/cv-print-builder.test.mjs`: 10
- `bos/modules/documents/documents.test.mjs`: 482
- `bos/modules/iot/iot.test.mjs`: 105
- `bos/modules/research/test.mjs`: 943
- `bos/scripts/assert-managed-business-os-assets.mjs`: 16
- `bos/scripts/assert-standard-app-bundle.mjs`: 10
- `bos/scripts/assert-workjet-branding.mjs`: 145
- `bos/scripts/audit-business-os-content.mjs`: 572
- `bos/scripts/business-os-app-inventory.mjs`: 8, 29
- `bos/scripts/business-os-qa-baseline.mjs`: 883, 941
- `bos/scripts/generate-module-registry.mjs`: 2, 153, 170
- `bos/scripts/validate-app-module.test.mjs`: 27, 247
- `bos/shared/grok-shell-icon-selection-contract.test.mjs`: 13
- `bos/shared/operator-icon-selection-contract.test.mjs`: 16
- `bos/shared/shell-windowing-contract.test.mjs`: 30
- `bos/shared/thesen-browser-release-contract.test.mjs`: 10
- `src/core/business_os/server.rs`: 3680, 4509
- `src/core/rxdb/tools/browser_rust_smoke.js`: 4463
- `src/core/rxdb/tools/business_os_ui_catalog_fixture.js`: 30
- `src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs`: 48
- `tests/business_os_shell_asset_sync_smoke.sh`: 35, 50, 83, 110, 144

### L-08 · OFFLINE_FALLBACK_CATALOG — 6 Fundstellen in 5 Dateien

Muster: `git grep -nE 'OFFLINE_FALLBACK_CATALOG'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/app.js`: 11638, 13190
- `bos/scripts/generate-module-registry.mjs`: 148
- `bos/shared/grok-shell-icon-selection-contract.test.mjs`: 19
- `bos/shared/module-catalog-cache.test.mjs`: 14
- `bos/v4/tools/port/scan.mjs`: 29

### L-12 · modules/ctox/schema.js — 10 Fundstellen in 10 Dateien

Muster: `git grep -nE 'ctox/schema\.js'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `.github/workflows/workjet-computer-migration.yml`: 13
- `bos/app.js`: 967
- `bos/rxdb/tests/crew-identity-smoke.mjs`: 3
- `bos/rxdb/tests/workjet-computer-migration-smoke.mjs`: 3
- `bos/rxdb/tests/workjet-project-configuration-smoke.mjs`: 3
- `bos/rxdb/tests/workjet-project-migration-smoke.mjs`: 3
- `bos/scripts/assert-module-conformance.mjs`: 12
- `bos/shared/workjet-project-control.test.mjs`: 686
- `bos/shared/workjet-supervisor-luma-contract.test.mjs`: 5
- `src/core/rxdb/tools/browser_rust_smoke.js`: 8252

### L-13 · modules/outbound/schema.js — 3 Fundstellen in 2 Dateien

Muster: `git grep -nE 'outbound/schema\.js'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/modules/conversations/schema.js`: 19
- `src/core/rxdb/tools/browser_rust_smoke.js`: 8057, 11998

### L-14 · modules/conversations/schema.js — 1 Fundstellen in 1 Dateien

Muster: `git grep -nE 'conversations/schema\.js'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/modules/ctox/schema.js`: 1

### L-15 · gelöschte schema.js (desktop, tickets, invoices, iot, documents, threads, appsec-pentest …) — 15 Fundstellen in 6 Dateien

Muster: `git grep -nE '(desktop|tickets|invoices|iot|documents|threads|appsec-pentest|support|research|mail|cv-print-builder|reports|knowledge)/schema\.js|['\"]\./schema\.js'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `scripts/installed-sync-acceptance.mjs`: 307
- `bos/app.js`: 968
- `bos/modules/appsec-pentest/appsec-pentest.test.mjs`: 5
- `bos/scripts/assert-rxdb-only.mjs`: 685, 706
- `src/core/iot/projector.rs`: 27, 48
- `src/core/rxdb/tools/browser_rust_smoke.js`: 5342, 8032, 8082, 8097, 8251, 11928, 11962, 12025

### L-16 · shared/collection-freshness.js — 6 Fundstellen in 4 Dateien

Muster: `git grep -nE 'collection-freshness\.js'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/app.js`: 17
- `bos/rxdb/tests/current-pull-freshness-browser-smoke.mjs`: 15, 40
- `bos/scripts/topbar-status.browser.mjs`: 21, 178
- `bos/shared/topbar-apps.test.mjs`: 5

### L-17 · shared/maintenance-state.js — 2 Fundstellen in 2 Dateien

Muster: `git grep -nE 'maintenance-state\.js'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/app.js`: 70
- `bos/shared/maintenance-state.test.mjs`: 10

### L-18 · shared/shell-generation.js — 2 Fundstellen in 2 Dateien

Muster: `git grep -nE 'shell-generation\.js'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/app.js`: 90
- `bos/shared/shell-generation.test.mjs`: 8

### L-19 · shared/shell-release-status.js — 9 Fundstellen in 9 Dateien

Muster: `git grep -nE 'shell-release-status\.js'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/index.html`: 249
- `bos/qa/ctox-desktop-shell.test.mjs`: 15
- `bos/scripts/audit-business-os-content.test.mjs`: 44
- `bos/scripts/build-shell-artifact.mjs`: 91
- `bos/scripts/build-shell-artifact.test.mjs`: 159
- `bos/scripts/topbar-status.browser.mjs`: 22
- `bos/shared/shell-generation.test.mjs`: 45
- `bos/shared/shell-release-status.test.mjs`: 13
- `src/core/business_os/shell_update.rs`: 36

### L-20 · shared/startup-deadlines.js — 3 Fundstellen in 3 Dateien

Muster: `git grep -nE 'startup-deadlines\.js'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/app.js`: 98
- `bos/scripts/assert-startup-deadlines-browser.mjs`: 12
- `bos/shared/startup-deadlines.test.mjs`: 13

### L-21 · shared/command-bus.js — 20 Fundstellen in 15 Dateien

Muster: `git grep -nE 'command-bus\.js'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/app.js`: 959
- `bos/customer-modules/outbound-lead-generation/index.js`: 11145
- `bos/rxdb/tests/command-auth-resilience-smoke.mjs`: 11
- `bos/rxdb/tests/command-bus-projection-smoke.mjs`: 20
- `bos/rxdb/tests/command-plane-observability-smoke.mjs`: 4
- `bos/rxdb/tests/command-submit-atomicity-smoke.mjs`: 12
- `bos/rxdb/tests/data-plane-progress-health-smoke.mjs`: 17
- `bos/scripts/assert-app-command-contracts.mjs`: 40
- `bos/scripts/assert-rxdb-only.mjs`: 265, 362, 388, 391
- `bos/scripts/command-consumer-inventory.json`: 67
- `bos/scripts/workjet-command-native.browser.mjs`: 79, 141
- `bos/shared/command-bus-receipt.test.mjs`: 7
- `bos/shared/command-bus.test.mjs`: 16, 20
- `bos/shared/sync.js`: 26
- `bos/shared/workjet-project-command-timing.test.mjs`: 5

### L-22 · shared/list-state.js — 3 Fundstellen in 3 Dateien

Muster: `git grep -nE 'list-state\.js'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/shared/list-state.test.mjs`: 4
- `bos/v4/manifests/shell.manifest.json`: 52
- `bos/v4/tools/manifest/gen-manifest.mjs`: 280

### L-23 · desktop-apps/code-editor (Altform) — 10 Fundstellen in 5 Dateien

Muster: `git grep -nE 'desktop-apps/code-editor'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/app.js`: 3055
- `bos/desktop-apps/code-editor/code-editor.test.mjs`: 43
- `bos/desktop-apps/code-editor/tests/v4-api.test.mjs`: 3, 38
- `bos/scripts/command-consumer-inventory.json`: 7, 53
- `bos/v4/source/editor.js`: 2, 8, 16, 17

### L-24 · scripts/assert-shell-v2-contract.mjs — 8 Fundstellen in 6 Dateien

Muster: `git grep -nE 'assert-shell-v2-contract'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `.github/workflows/crew-ui-acceptance.yml`: 49
- `.github/workflows/native-sync.yml`: 24, 49, 229
- `.github/workflows/rxdb-frame-liveness.yml`: 57
- `bos/modules/knowledge/index.js`: 3718
- `bos/scripts/assert-shell-v2-contract.mjs`: 11
- `src/scripts/native-integration-artifact.py`: 369

### L-25 · scripts/shell-v2-geometry-lab.mjs — 7 Fundstellen in 4 Dateien

Muster: `git grep -nE 'shell-v2-geometry-lab'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `.github/workflows/crew-ui-acceptance.yml`: 62
- `.github/workflows/native-sync.yml`: 25, 50, 230
- `bos/scripts/shell-v2-geometry-lab.mjs`: 6, 16
- `src/scripts/native-integration-artifact.py`: 376

### L-26 · scripts/generate-module-registry.mjs — 9 Fundstellen in 5 Dateien

Muster: `git grep -nE 'generate-module-registry'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `.github/workflows/ci.yml`: 303
- `.github/workflows/crew-ui-acceptance.yml`: 57
- `.github/workflows/workjet-project-chats.yml`: 41
- `bos/app.js`: 11637
- `bos/scripts/generate-module-registry.mjs`: 7, 8, 22, 49, 172

### L-27 · Slot-Pipeline build-shell-artifact / sign-shell-release — 9 Fundstellen in 7 Dateien

Muster: `git grep -nE 'build-shell-artifact|sign-shell-release'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `.github/workflows/business-os-shell-release.yml`: 68
- `bos/package.json`: 32, 33, 35
- `bos/scripts/build-shell-artifact.mjs`: 595
- `bos/scripts/build-shell-artifact.test.mjs`: 25
- `bos/scripts/shell-generation-native-lab.mjs`: 15
- `bos/scripts/sign-shell-release.mjs`: 162
- `bos/scripts/sign-shell-release.test.mjs`: 11

### L-28 · Shell-Generation-Labor — 6 Fundstellen in 3 Dateien

Muster: `git grep -nE 'shell-generation-native'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/package.json`: 27
- `bos/scripts/shell-generation-native-lab.mjs`: 19, 30, 32
- `bos/scripts/shell-generation-native.browser.mjs`: 16, 104

### L-29 · assert-shell-release-chrome / assert-startup-deadlines-browser / topbar-status.browser — 1 Fundstellen in 1 Dateien

Muster: `git grep -nE 'assert-shell-release-chrome|assert-startup-deadlines-browser|topbar-status\.browser'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `.github/workflows/business-os-shell-release.yml`: 139

### L-31 · shared/universal-importer.* — 18 Fundstellen in 13 Dateien

Muster: `git grep -nE 'universal-importer\.(js|css)'` (ohne *.md, vendor/, dist/, docs/, v4/tools/guards/, tmp/, v4/debt/; Selbstverweise ausgenommen)

- `bos/customer-modules/outbound-lead-generation/index.js`: 7
- `bos/modules/mail/index.js`: 1, 1961
- `bos/modules/matching/index.css`: 1389
- `bos/modules/matching/ui/businessOsControls.js`: 5
- `bos/modules/outbound/index.js`: 8, 6611
- `bos/scripts/assert-rxdb-only.mjs`: 687, 748, 752
- `bos/scripts/command-consumer-inventory.json`: 48
- `bos/scripts/tests/outbound-import-app.test.mjs`: 15
- `bos/scripts/tests/outbound-scoped-reload.test.mjs`: 140 (gelöscht in OLG2)
- `bos/scripts/tests/universal-importer-header.test.mjs`: 3
- `bos/shared/universal-importer.js`: 1145
- `bos/v4/manifests/shell.manifest.json`: 71, 72
- `src/core/business_os/server.rs`: 5274


## Nachtrag Strang SHARED (R4, Do 15.10.2026) – Weiterleitungen nur für die alte Shell

v4-Apps laden diese Dateien nicht mehr (gemessen: Importhülle aller `ctox:shared/`-Importe aus modules/,
customer-modules/, desktop-apps/ samt relativer Folgeimporte; R4-Treffer darin = 0). Sie bleiben nur, weil die alte
Shell und Alt-Tests sie laden.

| Nr. | Pfad | Besitzer | Heutige Importeure/Aufrufer | Bedingung für das Löschen | Phase |
|---|---|---|---|---|---|
| L-32 | `shared/business-chat.js` (ganze Datei) inkl. Weiterleitung `export { CREW_READING_WINDOW_MS, CREW_LEARNING_WINDOW_MS, crewMemberExpression, crewMemberExpressionTtlMs, crewCreatureHtml }` auf `shared/crew-presence.js` | Shell v4 | `app.js:9714` (dynamischer Import); Tests `shared/business-chat.test.mjs`, `shared/app-task-badges.test.mjs`, `shared/crew-renderer.test.mjs:3`, `shared/chat-open-persistence.test.mjs`, `shared/business-chat-startup-window.test.mjs`, `shared/business-chat-render-path-smoke.mjs`; Skripte `assert-business-chat-layout.mjs`, `assert-business-chat-behavior.mjs`, `assert-shell-chat-composition.mjs`, `assert-ctox-crew-map.mjs:182`, `assert-managed-business-os-assets.mjs:18`, `assert-module-collection-allowlists.mjs:101`, `workjet-shell-integration.test.mjs:49`; `qa/ctox-desktop-shell.test.mjs:12`; Listen `v4/tools/manifest/gen-manifest.mjs` SHELL_SHARED_PATTERNS. Die ctox-App importiert seit R4 `ctox:shared/crew-presence.js`. | mit `app.js` (L-01); Tests/Skripte gelöscht oder auf `crew-presence.js` umgestellt | P3 |
| L-33 | `shared/permissions.js` (Fassade: `export *` aus `shared/business-permissions.js` plus `roleMayReadCollection`, `ADMIN_ONLY_COLLECTIONS`, `USER_PRIVATE_COCKPIT_COLLECTIONS`) | Shell v4 | `app.js:35-36` (`roleMayReadCollection`), `shared/react-settings.js:7`; Tests `shared/permissions.test.mjs`, `shared/shell-permissions-ui.test.mjs:8`, `shared/app-lifecycle.test.mjs:4`, `rxdb/tests/module-demand-only-collections-smoke.mjs:4`, `rxdb/tests/crew-identity-smoke.mjs:4` (Sync-Kern, nur ctox sync v4 darf umstellen). v4-Apps nutzen `ctox:shared/business-permissions.js`; die Lese-Rechte stehen als `collections.<name>.rights` in `v4/manifests/ctox.manifest.json` / `nachweise.manifest.json` (Quelle `DOC_COLLECTION_RIGHTS` in gen-manifest.mjs, V4-HANDLERS.md „Rechte“). | `app.js` gelöscht; Kern projiziert `_rights` aus den Manifest-Rechten (debt-abgleich Nr. 9, G8b); rxdb-Tests auf `business-permissions.js` umgestellt | P3 + P4 |

## Nachtrag Strang LIBS2 (Kern §16.39 Nr. 1/2, Do 15.10.2026) – URL-Rückfall für Kernel ohne `artifacts.url`

Seit §16.39 Nr. 1 baut der Kernel die Artefakt-URL der App-Instanz (`kernel.artifacts.url(appId, ver, path, {shellVer?})`
→ `/~a/<app>@<ver>~shell@<sv>[~lib:<name>@<major>.<minor>]*/<pfad>`, Grammatik im Kopfkommentar von `v4/sw.js`). Die
Brücke reicht nur durch. Der Kern-Stub (Pin 7706796) und die Rahmen-Attrappe (`v4/frame/lab/kernel-mock.js`) haben
kein `artifacts.url`; für sie bleibt ein klar markierter Rückfall.

| Nr. | Pfad | Besitzer | Heutige Aufrufer | Bedingung für das Löschen | Phase |
|---|---|---|---|---|---|
| L-34 | `v4/shell/kernel-adapter.js` `legacyArtifactUrl` (ALTFORM; mit shellVer `/~a/<app>@<ver>~shell@<sv>/…` ohne Lib-Bindung, ohne shellVer ungebunden `/~a/<app>@<ver>/…`) samt Lücke `gaps: 'artifacts.url'`; Rückfall in `v4/shell/app-host.js` `appAssetUrl` für Nicht-Zeichenketten (Blob-Weg); im SW der ungebundene Zweig (`/~a/<app>@<ver>/…` ohne `~shell@`: `meta active:shell`, Antwort `no-cache`, `ctox:lib/` bleibt unaufgelöst); im Geometrie-Labor `LAB_LIB_BINDING` für ungebundene Stub-URLs | Shell v4 | `adaptKernel` für jeden Kernel ohne `artifacts.url` (Rahmen-Labor mock/stub, Geometrie-Labor, `lab/app-host/loader.js`); `api.shell.assetUrl` im Rahmen-Labor | Kern-Stub-Pin liefert `kernel.artifacts.url` mit Shell- und Lib-Bindung (§16.39 Nr. 1/2); Rahmen-Attrappe und Labor-Server (`frame/lab/serve.mjs`, `tools/geometry/geometry-v4.mjs`) verstehen die gebundene Wurzel; Shell-eigene Bibliotheksnutzung gebunden (heute `v4/source/editor.js` mit `ctox:lib/monaco@0/` im ungebundenen Shell-Code `/~a/shell@<sv>/…`: bleibt unaufgelöst, Rückfall Textarea; eine `~lib:`-Wurzel für die Shell selbst würde die Modul-Identität der `shared/`-Importe zwischen Rahmen und Apps trennen, also Monaco-Basis über eine kernelgebundene `lib:monaco`-URL holen) | P2 |

| L-35 | `modules/ctox/handlers/business_os.chat.task.js` + commands_in-Eintrag in `modules/ctox/v4-manifest.json` | Shell v4 | alte Clients, Core-Konsumenten harness_cockpit_chat.rs, guest_registry_command.rs, command_plane.rs (Kern-Abbau) | Harness-Abbau misst null Sender von business_os.chat.task | P3 |

## 4. Status (Strang DEL1, Do 15.10.2026 19:2x, neu gemessen)

Messung: `git grep -n -I --untracked -E <Muster>` über das **ganze Repo** (`:/`, inkl. `src/core`, `.github`, `tests`,
Wurzel-`scripts`, `src/scripts`, `src/skills`, Desktop-Hülle), ohne `*.md`, `vendor/`, `dist/`, `v4/debt/`,
`v4/tools/guards/`; jeder Treffer geöffnet. Zusätzlich gefunden: `app.js:6512` (`registerModuleSchemas`) lädt
`modules/<id>/schema.js` **jeder** App dynamisch – jede verbliebene `schema.js` hängt damit an der alten Shell.
Gelöschte Dateien (sha256 vor dem Löschen) liegen als Sicherung im Sitzungs-Scratchpad; Wiederherstellung über git.

| Nr. | Status | Messbeleg / Bedingung |
|---|---|---|
| L-01 | vorbereitet S9 | 255 Fundstellen in 113 Dateien; Kern/CI-Liste `LOESCHPAKET-S9.md` §2; 24 reine app.js-Quelltexttests als Block `L-01t` im Skript |
| L-02 | vorbereitet S9 (Umbau, kein Löschen) | `index.html` wird zur v4-Loader-Seite umgebaut; Kern `server.rs:3868-3883` (Katalog-Injektion) |
| L-03/L-04 | vorbereitet S9 | `index.html:53,250`; Kern `server.rs:3624`, `shell_update.rs:32,33`, `bos/rxdb/tests/workjet-device-control-smoke.mjs:16` |
| L-05 | vorbereitet S9 | nur `mobile-host.js:250`, `mobile-host.test.mjs:38` (fällt mit L-03) |
| L-06 | blockiert durch Kern `store.rs:168` (`include_str!`), `server.rs:3624`, `shell_update.rs:34`, Desktop `url-safety.cjs:26` | im Skript |
| L-07 | blockiert durch Kern `server.rs:3680,4509`, `browser_rust_smoke.js:4463`, `business_os_ui_catalog_fixture.js:30`, `tests/business_os_shell_asset_sync_smoke.sh` | im Skript; 5 App-Tests umstellen |
| L-08 | vorbereitet S9 | Block in `app.js` 11637–13190, fällt mit L-01 |
| L-09 | **gelöscht (vor DEL1)** | Datei fehlt; gelöscht in `8f0674d88` (OLGUI); `customer-modules/outbound-lead-generation/schema.js`: 0 Verweise |
| L-10 | **gelöscht (vor DEL1)** | Datei fehlt; gelöscht in `54497fd99` (UD6); `modules/app-store/schema.js`: 0 Verweise |
| L-11 | vorbereitet S9 | einziger Importeur `modules/ctox/schema.js:1` (L-12) plus `app.js:6512`; frei, sobald L-12 fällt |
| L-12 | blockiert durch Kern `bos/rxdb/tests/{crew-identity,workjet-computer-migration,workjet-project-configuration,workjet-project-migration}-smoke.mjs:3`, `browser_rust_smoke.js:8252`, CI `workjet-computer-migration.yml:13` | auch `app.js:967`; isolation R1 (einziger R1-Treffer) fällt mit |
| L-13 | blockiert durch Kern `browser_rust_smoke.js:8057,11998` | `conversations/schema.js` importiert nicht mehr (nur Kommentar Z. 11-12); `app.js:6512` |
| L-14 | blockiert (Kern) – keine Shell-Datei zu löschen | Restverweise auf gelöschte `schema.js`: `app.js:968` (desktop), `scripts/assert-rxdb-only.mjs:685,706` (CI ci.yml/release.yml/iot-soak.yml), Wurzel `scripts/installed-sync-acceptance.mjs:307`, Kern `browser_rust_smoke.js:5342,8032,8082,8097,8251,11928,11962,12025`, Kommentare `src/core/iot/projector.rs:27,48`; `appsec-pentest.test.mjs` bricht nicht mehr (prüft jetzt Z. 56 das Fehlen) |
| L-15 | **erledigt (nichts tot)** | `gen-manifest.mjs` neu gelaufen, `--check` Exit 0; nur noch `schema.js`-Einträge der existierenden Dateien `conversations`/`ctox`/`outbound` (`schema_js_present: 3`, `schema_js_load_errors: 0`); `reports.manifest.json` ohne `threads/schema.js` |
| L-16 | blockiert durch Kern `bos/rxdb/tests/current-pull-freshness-browser-smoke.mjs:15,40` | `app.js:17`, `shared/topbar-apps.test.mjs:5` |
| L-17 | vorbereitet S9 | nur `app.js:70` + eigener Test |
| L-18 | vorbereitet S9 | `app.js:90`; Test von `src/scripts/native-integration-artifact.py:226` aufgerufen |
| L-19 | blockiert durch Kern `shell_update.rs:36` | `index.html:249`, `audit-business-os-content.test.mjs:44` |
| L-20 | vorbereitet S9 | nur `app.js:98`, eigener Test, `assert-startup-deadlines-browser.mjs` (ohne Aufrufer) |
| L-21 | blockiert durch Kern `shared/sync.js:26` und 6 `bos/rxdb/tests/*-smoke.mjs` | guard `commandBus` bleibt bis dahin |
| L-22 | **gelöscht in DEL1** | `shared/list-state.js` (20 Z.) + `shared/list-state.test.mjs` (49 Z.); vorher einzige Verweise: eigener Test, `shell.manifest.json:54` (generiert), `gen-manifest.mjs:174` (Liste, gekürzt); Kommentar in `guards/vocabulary.json:10` angepasst; nachher 0 Verweise |
| L-23 | blockiert durch Shell-Bibliothek D-064 | `v4/source/editor.js:2,8,16,17` importiert `resolveMonacoBaseUrl` aus `desktop-apps/code-editor/app.js`; `app.js:3055`; nicht im S9-Skript, bis der Lader nach `v4/source/` gezogen ist |
| L-24/L-25 | blockiert durch CI `native-sync.yml:24,25,49,50,229,230`, `crew-ui-acceptance.yml:49,62`, `rxdb-frame-liveness.yml:57`, `src/scripts/native-integration-artifact.py:369,376`, AGENTS.md (Owner) | im Skript |
| L-26 | blockiert durch CI `ci.yml:303`, `crew-ui-acceptance.yml:57`, `workjet-project-chats.yml:41` | im Skript |
| L-27 | blockiert durch CI `business-os-shell-release.yml:68`, Kern K-02 | `package.json:32,33,35` |
| L-28 | vorbereitet S9 | nur `package.json:27` und gegenseitig; Kern K-02 (Generationssperre) |
| L-29 | **teilweise gelöscht in DEL1** | gelöscht: `scripts/shell-v2-data-probe.mjs` (119 Z.), `scripts/shell-v2-live-probe.mjs` (73 Z.) – vorher nur Selbstverweise (Kopfkommentar, Ausgabepfad), kein package.json-/CI-Aufruf. Rest blockiert: `assert-shell-release-chrome.mjs` durch CI `business-os-shell-release.yml:139`; `topbar-status.browser.mjs` (ohne Aufrufer, liest `collection-freshness.js`) fällt mit L-16 im Skript |
| L-30 | **erledigt bis auf 1 Datei** | OLDTEST hat die Alt-Suiten umgestellt (alle gelisteten Dateien grün, `OLDTEST.md`); Rest `modules/mail/tests/mail-browser-qa.mjs` blockiert durch CI `mail-ui-acceptance.yml:35` (+ `mail.test.mjs:424`), im Skript |
| L-31 | offen P2 (Port, kein Löschen) | 20 Fundstellen in 13 Dateien; Kern `server.rs:5274` |
| L-32a (design-lab) | blockiert durch CI `crew-ui-acceptance.yml:53-54` | 4 Design-Gate-Skripte umstellen; im Skript |
| L-32b (business-chat.js) | vorbereitet S9 | `app.js:9714`; CI `crew-ui-acceptance.yml:37` führt den Test; Shell-Gegenstellen in `LOESCHPAKET-S9.md` §2 |
| L-33a (guard-Gruppe v4) | offen P2 (Umbau, kein Löschen) | `guard.byGroup.v4` heute 61 (vorher 62): `vocab:de-laedt` 40, `ref-legacy-catalog-file` 11, `vocab:de-laeuft` 7, je 1 `de-veraltet-offline`/`de-vorbereitet`/`de-nicht-verfuegbar` |
| L-33b (permissions.js) | **nicht gelöscht, nur notiert** (app.js existiert) | `app.js:36`, `shared/react-settings.js:7`; Kern `bos/rxdb/tests/crew-identity-smoke.mjs:4`, `module-demand-only-collections-smoke.mjs:4`, `browser_rust_smoke.js` 6×; im Skript |
| L-34 | **erledigt (SWB, Pin 68b83a3 / K-1d)** | gelöscht: `legacyArtifactUrl`/`artifactUrl`, Blob-Ladeweg und `adaptStubKernel` in `kernel-adapter.js` (aus Bind-Klon c9cc9b8fc), `appAssetUrl`-Rückfall und `HOST_SHELL_VER` in `app-host.js`, ungebundener SW-Zweig (`meta active:shell`, `no-cache`), `LAB_LIB_BINDING`-Ersatz für ungebundene URLs im Geometrie-Labor. Ersatz: Kernel-`artifacts.bind` je Mount (genau ein bind, Rahmen-Labor zählt), `artifacts.url` ohne Shell-Version; Labore veröffentlichen Shell (+ Bibliotheken) im Stub. Fehlt `url`/`bind` im Kernel → `gaps` + `unavailable`, kein Ersatzweg. Nicht Teil: Monaco-Bindung der Shell selbst (`source/editor.js`, Rückfall Textarea bleibt) |
| L-35 | offen P3 | Harness-Abbau (Kern) |
| L-36 | **gelöscht in DEL1** (neu gefunden) | `shared/app-transcode.mjs` (294 Z.) + `shared/app-transcode.test.mjs` (206 Z.), Besitz Shell v4 laut `dateigrenzen.md`; vorher einziger Verweis eigener Test + Generator-Liste (gekürzt); 0 Verweise in `modules/importer`, `src/skills`, `src/core` |
| L-37 | **gelöscht in DEL1** (neu gefunden) | `shared/bugReporter.js` (1184 Z.), Besitz Shell v4; kein Import (auch nicht dynamisch, Suche `bug-?reporter` ohne Treffer außer `business-reporter.js`), nur Auditliste `scripts/audit-business-os-content.mjs:93` (Eintrag und Regex-Alternative entfernt) und Generator-Liste (gekürzt). Audit vorher und nachher gleich: 1 Fund in `modules/mail/vendor/easy-email-editor/…/frame-w8wiCvNe.js:61` (fremd, vorbestehend) |
| L-38 | **gelöscht in DEL1** (neu gefunden) | `shared/design-templates.js` (75 Z.) + `shared/design-templates.test.mjs` (55 Z.), Besitz Shell v4; kein Import (`app.js` setzt nur `window.CTOX_BUSINESS_OS_DESIGN_TEMPLATES` aus dem Startkontext, Z. 11020); Generator-Liste gekürzt |
| L-39 | Meldung an ctox sync v4 (Kern-Besitz) | ohne Produktiv-Aufrufer: `shared/business-data.js` (0), `shared/command-lifecycle.js` (nur Test), `shared/task-history-native-changes.js` (nur Test), `shared/browser-surface-binding.js` (nur Test, Phase 11) |
| L-40 | blockiert (nicht messbar im Repo) | `vendor/app-importer/` ohne Repo-Verweis seit L-36; importierte Kunden-Apps können ihn zur Laufzeit laden – erst nach Mandantenmessung |

**Abnahme DEL1** (nach allen Löschungen): guard-v4 Exit 0 (total 1482 → 1474); check-isolation unverändert 149
Treffer (R1 1, R4 148 – vorbestehend, Exit 1 schon vor DEL1); ratchet OK, nur gesunkene Zähler, `--update` geschrieben;
`gen-manifest.mjs` neu + `--check` Exit 0 (shell.manifest.json −4 Dateien, `shiftflow.manifest.json` durch Parallelstrang
mitgeneriert), `gen-manifest.test.mjs` 37/37; Rahmen-Labor mock 64/64; alle 42 `tests/v4-api.test.mjs` grün (395 Fälle,
0 rot; Aufnahmen vorher gesichert und byte-gleich zurückgelegt); guard-v4.test 66/66, check-isolation.test 12/12,
ratchet.test 11/11; `apply-s9.sh` Trockenlauf: 66 Einträge, 26 blockiert, 40 frei, nichts gelöscht.

## Nachtrag Strang ZONE45 (Kern §16.45, Do 15.10.2026) – V3-Rückfall des Verbindungszustands

| Nr. | Pfad | Besitzer | Heutige Aufrufer | Bedingung für das Löschen | Phase |
|---|---|---|---|---|---|
| L-41 | `v4/shell/sync-state.js`: V3-Zweig (`readV3`, `v3CollectionState`, Standardquelle `window.ctoxBusinessOsSyncDiagnostics` samt Ereignis `ctox-business-os-sync-diagnostics` und 1-s-Abfrage), Wächter-Ausnahme `// v4-allow: debt L-41`; Fälle „V3-…“ in `sync-state.test.mjs` und Z45-7 im Rahmen-Labor | Shell v4 | `createSyncState` (Rückfall nur, wenn `kernel.metrics.collections` fehlt) | **Löschdatum Sa 17.10.2026**: V4-Core (K-1e, `metrics.collections` + `reconnect`) auf welsch aktiv. Danach Zweig, Marker, V3-Tests löschen; Wächter `syncDiagnostics` für `v4/` muss 0 zeigen | S9 |

## Nachtrag Strang WJB-0 (E24, Fr 16.10.2026) – Workjet-V2-Kompatibilitätsschicht

| Nr. | Pfad | Besitzer | Heutige Aufrufer | Bedingung für das Löschen | Phase |
|---|---|---|---|---|---|
| L-42 | `v4/shell/workjet/` (`v2-compat.js`, `v2-helpers.js`, `actions/*.js`): V2-Namen `commandBus`, `requestNative`, `startCollection`, `requireRevision`, `until: 'terminal'` und wörtliche V2-Fehlertexte; Wächter-Ausnahmen `// v4-allow: debt L-42` bzw. `// v4-allow: code` | Shell v4 | Workjet main (`globalThis.workjetProjectControl` u. a., Schnittstelle unverändert nach E24) | Workjet ruft App-API 4 (`api.command`/`api.query`) direkt; dann Brücken, Marker und Orakel-Harness löschen; Wächter für `v4/shell/workjet` muss 0 zeigen | nach Cutover |
