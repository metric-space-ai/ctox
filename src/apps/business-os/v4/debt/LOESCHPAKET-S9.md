# Shell v4 – Löschpaket S9 (Strang DEL1)

Stand: Do 15.10.2026 (Berlin), Klon `~/Documents/ctox-shell-v4`. Vorbereitet, **nicht ausgeführt**. Das Paket löscht
die Shell-Dateien aus `LOESCHPLAN.md`, die heute noch an der alten Shell (`app.js`, `index.html`, `mobile-host`,
Slot-Pipeline), an CI-Workflows oder an Kern-Gegenstellen hängen. Es läuft in S9 (Mo 19.10. 05:00), sobald ctox sync v4
die Kern-Gegenstellen entfernt hat und die v4-Shell der Regelweg ist.

Skript: `v4/debt/apply-s9.sh` (Trockenlauf ohne Argument, Löschen nur mit `--apply`, Löschen per `git rm`).

## 1. Bedingung (im Skript erzwungen)

Für jede Datei der Löschmenge sucht das Skript per `git grep -E` über das **ganze Repo** (inkl. `src/core`, `.github`,
`tests`, Wurzel-`scripts`, `src/scripts`, `src/apps/business-os-desktop`, `src/skills`) nach Verweisen, ohne `*.md`,
`vendor/`, `dist/`, `v4/debt/`, `v4/tools/guards/` (Löschlisten-Muster), `v4/manifests/` und
`v4/tools/manifest/out/` (generiert). Ein Verweis zählt nicht, wenn er in einer Datei der Löschmenge selbst steht (die
Menge fällt gemeinsam). Bleibt ein Treffer, gibt das Skript ihn mit Datei:Zeile aus, bricht mit Exit 1 ab und löscht
nichts. Damit ist jede Kern-, CI- und Shell-Gegenstelle unten eine harte Vorbedingung.

Trockenlauf am 15.10.2026: **66 Dateien, 26 blockiert, 40 frei** (frei = nur noch aus der Löschmenge selbst
referenziert, z. B. `shared/maintenance-state.js` nur aus `app.js`).

Zusätzliche Vorbedingungen ohne Dateiverweis:
- v4-Loader-Seite ist Regelweg: `index.html` startet `startShell` (v4/shell) statt `app.js`; die Altteile laut L-02
  (`?v=`-Buster, `shell-release-status.js`, `mobile-host.*`, `shell-topbar.css`) sind aus `index.html` entfernt. Das ist
  ein Umbau von `index.html`, kein Löschen, und steht deshalb nicht im Skript.
- Workjet-Hostbrücke umgezogen (DEBT D-047, ctox sync v4 Phase 11).
- Kern liefert `app.js` nicht mehr aus (K-01/K-02, `server.rs`, `shell_assets.rs`, `shell_update.rs`).

## 2. Löschmenge und Gegenstellen (Datei:Zeile, gemessen 15.10.2026)

`bos/` = `src/apps/business-os/`. „Kern“ = `src/core/**`, `bos/rxdb/**`, `shared/sync.js|db.js|rxdb-runtime.js`;
„CI“ = `.github/workflows/*`. Die Shell-Gegenstellen stellt Shell v4 in S9 selbst um.

| Nr. | Shell-Dateien (werden gelöscht) | Kern-/CI-Gegenstellen (Bedingung) | Shell-Gegenstellen (in S9 umstellen/löschen) |
|---|---|---|---|
| L-01 | `app.js` (inkl. L-08 `OFFLINE_FALLBACK_CATALOG`) | Kern `server.rs:3624,4928,5009,5051,5052`, `shell_assets.rs:218-300`, `shell_update.rs:30,676,1284,1288`, `store.rs:46445-46509`, `install/mod.rs:3510`, `rxdb/tools/browser_rust_smoke.js:739,823,2960,4617,4628,16070,16162,16230,16371`, `business_os_connection_modes_smoke.js:311`, `desktop_pin_reload_probe.js:18`, `bos/rxdb/tests/{browser-launch-config-secret-free:4,collection-invalidation-only:5,context-chat-submission:5,data-plane-guard:117,151,live-sync-facade:4,module-sync-lifecycle:5,non-destructive-reconnect-repair:4,query-api:228,shell-webrtc-readiness:7,workjet-device-control:15}-smoke.mjs`; CI `native-sync.yml:21,46`; sonst `contracts/binary_bundle_manifest.txt:11`, `scripts/installed-sync-acceptance.mjs:207-250`, `scripts/prepare-installed-sync-tenant.py:17`, `tests/business_os_shell_asset_sync_smoke.sh:34,82,109,143` | `index.html:251`; Workjet-Tests Phase 11 (`shared/workjet-*.test.mjs`, `*.browser.mjs`); Prüfskripte, die `app.js` lesen und CI-Aufrufer haben: `assert-rxdb-only.mjs` (ci.yml:302, release.yml:132, iot-soak.yml:110, crew-ui-acceptance.yml:58), `assert-module-collection-allowlists.mjs`, `assert-module-context-contract.mjs`, `assert-production-release-docs.mjs`, `assert-context-menu-lifecycle.mjs` (native-sync.yml), `test-shell-window-cache-startup.mjs` (`src/scripts/native-integration-artifact.py`), `audit-business-os-content.mjs` (Liste `SHELL_SURFACE_FILES` kürzen, nicht löschen), `assert-global-context-menu-policy.mjs`/`assert-managed-business-os-assets.mjs` (package.json), `assert-db-isolation-inventory.mjs` (assert-module-conformance), `command-consumer-inventory.json` |
| L-01t | 24 Tests/Skripte, die nur den Quelltext von `app.js` prüfen und keinen CI-Aufrufer haben (Liste im Skript, Block `L-01t`) | – | – (fallen mit `app.js`) |
| L-03/04 | `mobile-host.js`, `mobile-host.css`, `mobile-host.test.mjs`, `qa/workjet-mobile-host-layout.test.mjs` | Kern `server.rs:3624`, `shell_update.rs:32,33`, `bos/rxdb/tests/workjet-device-control-smoke.mjs:16` | `index.html:53,250`; `scripts/assert-workjet-branding.mjs:14,15`; Phase 11: `scripts/workjet-shell-integration.test.mjs:61`, `shared/workjet-session-control.test.mjs:7`, `shared/workjet-session-events.test.mjs:7` |
| L-05 | `mobile-apps.json` | – | nur `mobile-host.*` (Löschmenge) |
| L-06 | `system-apps.json` | Kern `store.rs:168` (`include_str!`), `server.rs:3624`, `shell_update.rs:34`; Desktop-Hülle `src/apps/business-os-desktop/src/main/url-safety.cjs:26` (+ `test/url-safety.test.cjs:46-55`, `test/session-view.test.cjs:104-107`) | `scripts/assert-standard-app-bundle.mjs:9`, `business-os-app-inventory.mjs:9,30,42,45`, `generate-module-registry.mjs` (Löschmenge), `shared/shell-windowing-contract.test.mjs:31` |
| L-07 | `modules/registry.json` | Kern `server.rs:3680,4509`, `rxdb/tools/browser_rust_smoke.js:4463`, `business_os_ui_catalog_fixture.js:30`; Desktop `url-safety.cjs:27`; `tests/business_os_shell_asset_sync_smoke.sh:35,50,83,110,144`; Skill `src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs:48` | App-Tests `browser.test.mjs:250`, `cv-print-builder.test.mjs:10`, `documents.test.mjs:327`, `iot.test.mjs:116`, `research/test.mjs:948` (auf `v4/manifests/*.manifest.json` umstellen); Skripte `assert-managed-business-os-assets.mjs:16`, `assert-standard-app-bundle.mjs:10`, `assert-workjet-branding.mjs:145`, `audit-business-os-content.mjs:571`, `business-os-app-inventory.mjs:8,29`, `business-os-qa-baseline.mjs`, `validate-app-module.test.mjs`; `shared/{grok-shell-icon-selection,operator-icon-selection,shell-windowing,thesen-browser-release}-contract.test.mjs` |
| L-11/12/13 | `modules/conversations/schema.js`, `modules/ctox/schema.js`, `modules/outbound/schema.js` | Kern `bos/rxdb/tests/{crew-identity,workjet-computer-migration,workjet-project-configuration,workjet-project-migration}-smoke.mjs:3`, `rxdb/tools/browser_rust_smoke.js:8057,8252,11998`; CI `workjet-computer-migration.yml:13` | `app.js:967` und `app.js:6512` (`registerModuleSchemas` lädt `schema.js` jeder App – Löschmenge); Phase 11 `shared/workjet-project-control.test.mjs:686`, `shared/workjet-supervisor-luma-contract.test.mjs:5`; Kommentar `scripts/assert-module-conformance.mjs:12`; danach `gen-manifest.mjs` neu (Manifeste `conversations`, `ctox`, `outbound` führen `schema.js` in `files`) |
| L-16 | `shared/collection-freshness.js` | Kern `bos/rxdb/tests/current-pull-freshness-browser-smoke.mjs:15,40` | `shared/topbar-apps.test.mjs:5` (Fall entfernen, Rest prüft `topbar-apps.js`); `scripts/topbar-status.browser.mjs` (Löschmenge) |
| L-17 | `shared/maintenance-state.js` + Test | – | nur `app.js:70` |
| L-18 | `shared/shell-generation.js` + Test | `src/scripts/native-integration-artifact.py:226` (ruft den Test) | nur `app.js:90` |
| L-19 | `shared/shell-release-status.js` + Test | Kern `shell_update.rs:36` (Slot-Dateiliste) | `index.html:249`; `scripts/audit-business-os-content.test.mjs:44` |
| L-20 | `shared/startup-deadlines.js` + Test, `scripts/assert-startup-deadlines-browser.mjs` | – | nur `app.js:98` |
| L-21 | `shared/command-bus.js` + `command-bus.test.mjs`, `command-bus-receipt.test.mjs` | Kern `shared/sync.js:26`, `bos/rxdb/tests/{command-auth-resilience:11,command-bus-projection:20,command-plane-observability:4,command-submit-atomicity:12,data-plane-progress-health:17,collection-lease-lifecycle:7}-smoke.mjs` | `customer-modules/outbound-lead-generation/index.js:9676`, `v4-port.mjs:190` (prüfen: Kommentar/Scanner-Muster); `assert-app-command-contracts.mjs:40`, `assert-rxdb-only.mjs:265,362,388,391`, `workjet-command-native.browser.mjs:79,141`, `command-consumer-inventory.json:67`; Phase 11 `shared/workjet-project-command-timing.test.mjs:5` |
| L-24/25 | `scripts/assert-shell-v2-contract.mjs`, `scripts/shell-v2-geometry-lab.mjs` | CI `native-sync.yml:24,25,49,50,229,230`, `crew-ui-acceptance.yml:49,62`, `rxdb-frame-liveness.yml:57`; `src/scripts/native-integration-artifact.py:369,376`; Absatz „Shell-V2 deploy discipline“ in `AGENTS.md` (Owner) | Kommentar `modules/knowledge/index.js:3771` |
| L-26 | `scripts/generate-module-registry.mjs` | CI `ci.yml:303`, `crew-ui-acceptance.yml:57`, `workjet-project-chats.yml:41` | `app.js:11637` (Löschmenge) |
| L-27 | `scripts/build-shell-artifact.mjs` + Test, `scripts/sign-shell-release.mjs` + Test | CI `business-os-shell-release.yml:68`; Kern K-02 (Slots/Ed25519 in `shell_update.rs`) | `package.json:32,33,35` (Skripte `build`/`sign`/`test:shell-artifact` entfernen); `build-shell-sbom.mjs` + Test separat prüfen (nicht im Löschplan) |
| L-28 | `scripts/shell-generation-native-lab.mjs`, `scripts/shell-generation-native.browser.mjs` | Kern K-02 (Generationssperre `server.rs:3589-3669`) | `package.json:27` (`qa:shell-generation-native`) |
| L-29 | `scripts/assert-shell-release-chrome.mjs`, `scripts/topbar-status.browser.mjs` | CI `business-os-shell-release.yml:139` | – |
| L-30 | `modules/mail/tests/mail-browser-qa.mjs` (v3-Harness `mount(ctx)`) | CI `mail-ui-acceptance.yml:35` | `modules/mail/tests/mail.test.mjs:424` (liest die Datei) |
| L-32a | `design-lab.js`, `design-lab.html` | CI `crew-ui-acceptance.yml:53-54` | `scripts/assert-accessibility-contract.mjs:9`, `assert-design-system.mjs:9`, `capture-design-matrix.mjs:10`, `run-design-quality-gates.mjs:13,34` (auf `v4/frame/lab`) |
| L-32b | `shared/business-chat.js`, `shared/business-chat.test.mjs` | CI `crew-ui-acceptance.yml:37` (führt den Test); Kommentar `src/core/business_os/threads.rs:775` | `app.css:9319` (Kommentar), `modules/ctox/handlers/_lib/context-chat.js:17` (prüfen), `scripts/assert-business-chat-{behavior,layout}.mjs`, `assert-ctox-crew-map.mjs:182`, `assert-managed-business-os-assets.mjs:18`, `assert-module-collection-allowlists.mjs:101`, `assert-shell-chat-composition.mjs:571`, `command-consumer-inventory.json:46,66`, `shared/app-task-badges.test.mjs:5`, `shared/crew-renderer.test.mjs:3`, `shared/chat-open-persistence.test.mjs`, `shared/business-chat-startup-window.test.mjs`, `shared/business-chat-render-path-smoke.mjs`, `qa/ctox-desktop-shell.test.mjs:12` (Löschmenge); Phase 11 `workjet-shell-integration.test.mjs:49` |
| L-33 | `shared/permissions.js` (Fassade), `shared/permissions.test.mjs` | Kern `bos/rxdb/tests/crew-identity-smoke.mjs:4`, `module-demand-only-collections-smoke.mjs:4`, `rxdb/tools/browser_rust_smoke.js:5761,9680,10249,11249,12694,13104`; Kern projiziert `_rights` aus den Manifest-Rechten (debt-abgleich Nr. 9, G8b) | `shared/react-settings.js:7` (auf `business-permissions.js`), `shared/shell-permissions-ui.test.mjs:8`, `shared/app-lifecycle.test.mjs:4`; Kommentare `modules/app-store/index.js:2521`, `modules/ctox/index.js:5911`, `modules/tickets/index.js:10` |

Nicht im Skript, aber S9-relevant:
- `shared/bugReporter.js` ist in DEL1 schon gelöscht (s. LOESCHPLAN L-37).
- ctox-sync-v4-eigene Dateien ohne Produktiv-Aufrufer (nur ihr eigener Test), laut `dateigrenzen.md` Kern-Besitz –
  Shell v4 löscht sie nicht, Meldung an ctox sync v4: `shared/business-data.js` (0 Verweise),
  `shared/command-lifecycle.js` (nur `command-lifecycle.test.mjs`), `shared/task-history-native-changes.js` (nur Test),
  `shared/browser-surface-binding.js` (nur Test; Phase 11).
- `vendor/app-importer/` (sucrase/react-ESM): einziger Repo-Verweis war `shared/app-transcode.test.mjs` (gelöscht);
  bleibt, weil bereits importierte Kunden-Apps zur Laufzeit `../../vendor/app-importer/*.mjs` laden können – das ist
  im Repo nicht messbar. Löschen erst nach Messung auf den Mandanten (installierte Module ohne `vendor/app-importer`).
- `scripts/build-shell-sbom.mjs` + Test hängen an `package.json:35` (`test:shell-artifact`) und der Slot-Pipeline;
  nicht im Löschplan, mit L-27 messen.

## 3. Ablauf in S9

```
cd ~/Documents/ctox-shell-v4             # bzw. main-Checkout
bash src/apps/business-os/v4/debt/apply-s9.sh          # Trockenlauf: jede BLOCKT-Zeile nennt die Restverweise
# Shell-Gegenstellen aus §2 umstellen (index.html, package.json, Skripte, Tests, Kommentare)
# Kern-/CI-Gegenstellen müssen von ctox sync v4 entfernt sein (Skript bricht sonst ab)
bash src/apps/business-os/v4/debt/apply-s9.sh --apply  # git rm der 66 Dateien
```

Danach im Generator `v4/tools/manifest/gen-manifest.mjs` `SHELL_SHARED_PATTERNS` kürzen: `app.js`, `index.html`
(falls die Loader-Seite anders heißt), `mobile-host.*`, `business-chat.js`, `permissions.js`.

## 4. Prüfkommandos danach

```
cd src/apps/business-os
node v4/tools/guards/guard-v4.mjs; echo $?                       # Exit 0
node v4/tools/isolation/check-isolation.mjs --summary            # R1 (modules/ctox/schema.js) muss weg sein, R4 ≤ heute
node v4/debt/ratchet.mjs && node v4/debt/ratchet.mjs --update    # nur sinkende Zähler
node v4/tools/manifest/gen-manifest.mjs && node v4/tools/manifest/gen-manifest.mjs --check
(cd ../../.. && node src/apps/business-os/v4/frame/lab/frame.test.mjs mock)
export CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/7706796
for t in modules/*/tests/v4-api.test.mjs customer-modules/*/tests/v4-api.test.mjs desktop-apps/*/tests/v4-api.test.mjs; do
  node --import ./v4/tools/node/ctox-shared.mjs --test "$t" || echo "ROT $t"; done
bash v4/debt/apply-s9.sh                                          # alle Einträge "WEG"
git grep -nE "business-os/app\.js|mobile-host\.|system-apps\.json|modules/registry\.json|command-bus\.js|shared/permissions\.js" -- ':/' ':!*.md'   # leer
```
Die v4-api-Tests schreiben `tests/shots/*.png` neu; vor dem Lauf sichern und danach zurückkopieren, wenn keine neuen
Aufnahmen gewollt sind.

### Nachtrag 15.10. 19:25 – command-lifecycle
`shared/command-lifecycle.js` (+ `.generated.js`, `.test.mjs`): **blockiert bis Kern-Testumzug (K-1c, #823)** – Kerninvarianten (Zustände/Übergänge, genau ein Endzustand je Befehl, keine Rückwärtsübergänge, Idempotenz bei Wiederzustellung; Quelle `src/core/rxdb/tests/fixtures/business-command-lifecycle-v2.json`) ziehen nach `kernel/tests`. Shell-Seite: Rahmen-Labortest „jeder Endzustand genau einmal je client_seq in der Zone“. Freigabe, wenn der Kern die 1:1-Abdeckung meldet.
