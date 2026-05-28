# Business OS Live Forensic Inventory - 2026-05-27

## Scope

- Target: `https://cto1.kunstmen.com/`
- Tenant: `kunstmen` (`cto1.kunstmen.com`)
- VPS: `51.210.246.120`
- Browser QA artifacts: `/Users/michaelwelsch/Documents/ctox.nosync/output/playwright/business-os-qa-baseline-2026-05-27T12-46-20-459Z`
- Main QA JSON: `/Users/michaelwelsch/Documents/ctox.nosync/output/playwright/business-os-qa-baseline-2026-05-27T12-46-20-459Z/business-os-qa-baseline.json`
- Main QA report: `/Users/michaelwelsch/Documents/ctox.nosync/output/playwright/business-os-qa-baseline-2026-05-27T12-46-20-459Z/business-os-qa-baseline.md`
- Post-deploy QA artifacts: `/Users/michaelwelsch/Documents/ctox.nosync/output/playwright/business-os-qa-baseline-2026-05-27T15-03-25-870Z`
- Post-deploy QA report: `/Users/michaelwelsch/Documents/ctox.nosync/output/playwright/business-os-qa-baseline-2026-05-27T15-03-25-870Z/business-os-qa-baseline.md`

## What Was Wrong Before This Pass

- The earlier local screenshots were not sufficient evidence for `cto1.kunstmen.com`; they showed many empty states because the run was not authenticated against the live tenant.
- The live custom domain is served by the Vercel project `ctox-dev`, not by the standalone `kunstmen-business-os` project.
- `cto1.kunstmen.com` was login-gated and the Business OS web user `ubuntu` was missing or not set to the expected password in `tenant_business_os_users`.
- Production Business OS assets were already current, but the live tenant session was not usable, so the app shell stayed at the login gate and no app data could be validated.

## Actions Completed

- Added/updated the `ubuntu` Business OS tenant user for `kunstmen` in `ctox-dev` production DB.
- Verified HTTP login on `https://cto1.kunstmen.com/login` returns `303` with a Business OS session cookie.
- Verified authenticated root HTML injects both `window.CTOX_BUSINESS_OS_SESSION` and `window.CTOX_BUSINESS_OS_CONFIG`.
- Updated the QA harness to support authenticated live runs through `BUSINESS_OS_QA_LOGIN_USER` and `BUSINESS_OS_QA_LOGIN_PASSWORD`.
- Ran authenticated browser QA against `https://cto1.kunstmen.com/`; 18/18 expected apps opened and screenshots were captured.
- Ran one app-specific subagent per baseline app and consolidated the app-level findings below.
- Synced the fixed Business OS assets into `ctox-dev/public/business-os`.
- Built `ctox-dev` successfully with `npm run build`.
- Deployed `ctox-dev` to Vercel production, then redeployed after an explicit local resync so the public sync manifest points at the local source path. Final production deployment: `dpl_FDySBP5zgsubL148K7vi4T6cT11E`, production URL `https://ctox-bo49krcyx-metric-spaces-projects.vercel.app`.
- Verified `cto1.kunstmen.com` serves the new static assets for Source Editor, Creator, Calendar, and Buchhaltung.
- Ran post-deploy authenticated browser QA against `https://cto1.kunstmen.com/`; 18/18 expected apps opened. Notes screenshot timed out in the main harness and was captured separately into the same post-deploy artifact directory.
- After the final redeploy, spot-verified `cto1.kunstmen.com/business-os/.ctox-shell-sync.json` now reports source root `/Users/michaelwelsch/Documents/ctox.nosync/src/apps/business-os`.

## Platform-Level Findings

| Area | Status | Evidence | Impact |
|---|---|---|---|
| Login gate | Fixed for test user | Authenticated HTML contains launch session/config | Browser QA can now test real app surfaces. |
| App inventory | Inconsistent | Post-deploy QA: Runtime modules 19; Desktop icons 20; Start menu 20; App Store cards 21 | Desktop/Start/App Store still disagree. `tickets`, `customers`, and `create-scratch` appear in some surfaces but not the original 18-app baseline. |
| Required RxDB/WebRTC collections | Failing in browser | Advanced Status failures: `requiredCollectionsConnected`, `requiredCollectionsInitialSyncComplete`, `requiredCollectionsCheckpointEpochAdvertised`, `frameTransportRealtimeHealthy` | Browser-visible data is not fully proven to come from the VPS SQLite/native peer. Commands time out. |
| VPS SQLite health | Healthy locally on VPS | Native status reports required SQLite collections present and ready | Data exists on the VPS, but browser replication/signaling is not healthy. |
| Command bus | Failing | Repeated `Command ... wurde nicht synchronisiert` console errors | Actions that require native command acknowledgement are unreliable. |
| Monaco assets | Broken | `/vendor/monaco/vs/loader.js` returns HTML/404 and strict MIME error | Source Editor falls back to textarea instead of Monaco. |
| Header/status UX | Poor | Red `CTOX ARBEITET NICHT` banner remains while some local UI says platform active | Conflicting status language erodes operator trust. |

## App Inventory

| App | Screenshot | Data/Render State | Problems Found |
|---|---|---|---|
| CTOX | `01-ctox.png` | Opens, but shows no active work and Web Stack sync warning. | Web-stack projection missing in RxDB; status contradicts visible shell; needs real data proof. Subagent fixed two local CTOX bugs: refresh now preserves the precise projection-missing diagnostic, and empty task-step rendering no longer crashes on `null`. |
| Bugs & Features | `02-reports.png` | Opens with filters; empty reports state. | Live reports collections are empty; filters/search/refresh work but have no data. Subagent converted Refresh to an icon button, styled controls/focus states, added empty diagnostics with collection counts, and made JSON payload/client-context parsing robust. |
| Documents | `03-documents.png` | Opens with empty selection and no document rows. | No live document mock data reaches the browser. Subagent improved local UX: direct Import/Create actions in empty state, filter empty reset, less cramped filter controls, and Runbook header aligned with pane header pattern. |
| Knowledge | `04-knowledge.png` | Opens with controls; empty list. | No knowledge entries visible because live data does not reach the browser. Subagent fixed a local pointer/layout bug where the detail tabs could sit under the left pane/resizer, styled search/header controls, and added mobile stacking. |
| Web Research | `05-research.png` | Opens with diagnostic/empty dashboard state. | Live `knowledge_tables` has 0 rows, so there are no Research Tasks, Sources, Measurements, or dashboards. Subagent improved the no-data UX: standard center header, disabled workbench controls, visible validation, labeled icon buttons, and responsive empty state. |
| Matching | `06-matching.png` | Opens with controls; empty requirements. | `matching_requirements`, `matching_objects`, and `matching_results` show 0 live records. Subagent fixed local UI issues: grouping select is now wired/persisted, `Bereiche` renders area cards, `Neueste zuerst` sorts by timestamps, and the misleading `HTTP Pull` label becomes `Lokaler Spiegel` for local RxDB mirror diagnostics. |
| Conversations | `07-conversations.png` | Opens with filters; empty accounts/conversations. | No `communication_accounts`, `communication_threads`, or `communication_messages` data live. Subagent fixed diagnostics so `communication_threads` is required, connecting peers are treated as starting, peer timeouts are surfaced, and mobile layout keeps the filter/selection pane reachable. |
| Outbound | `08-outbound.png` | Opens with one campaign and pipeline shell. | Company/import tables are empty; native knowledge setup warnings indicate command acknowledgement failure. Subagent adjusted local header/mobile/resizer behavior, made hidden-firms action identifiable, and fixed the Outbound test harness to bundle Browser ESM. |
| Einsatzplanung | `09-shiftflow.png` | Opens with employee/shift mock data. | Live mock data is present: 4 employees, 6 shifts, 3 projects, 1 timesheet, 2 billing rows. Subagent fixed dark-mode shift-card contrast, pointer-click detail opening, action icon consistency, hover button overlap, and mobile stacking. |
| Spreadsheets | `10-spreadsheets.png` | Opens with empty selection; runbooks exist but sheet data is empty. | Live `spreadsheets`, `spreadsheet_versions`, and `spreadsheet_blob_chunks` are 0 while `spreadsheet_runbooks` has 3. Subagent fixed local filter grid/inline widths, JSpreadsheet `.jss_*` dark-theme styling, and test bundling. |
| Notizen | `11-notes.png` | Opens with notes/notebooks/tags data. | Live mock data is present: 3 notes, notebooks, tags, favorites, trash counters. Subagent fixed local accessibility/keyboard behavior, removed a noisy console log, prevented the lock button being covered by the chat dock, and hardened mobile layout/focus states. |
| App Store | `12-app-store.png` | Opens with 21 cards. | App Store count exceeds 18-app baseline; catalog surfaces include `browser`, `create-scratch`, `tickets`. |
| Buchhaltung | `13-buchhaltung.png` | Opens with SKR account data, 42 accounts visible. | SKR data is present, but Journal, Belege, Bankabgleich, Reisen, and Fahrtenbuch were empty live. Subagent fixed a local seeding race, added missing mock data, replaced emoji controls, and hardened responsive layout. |
| Kalender | `14-calendar.png`; extra local screenshots in `output/playwright/calendar-subagent-2026-05-27-local-after-fix/` | Opens with live data: 2 events, 2 calendars, 1 booking page. | Drawer actions could sit under/too close to the global dock; event clicks could open the wrong new-event drawer instead of editing the clicked event. Local fixes lift the drawer, improve mobile form layout, and harden EventCalendar event-id mapping/click fallback. |
| Coding Agents | `15-coding-agents.png`; extra screenshots in `output/playwright/coding-agents-subagent-2026-05-27T15-live/` | Opens, but no workspace/grant data is available live. | `Workspaces konnten nicht geladen werden`; command bus does not answer, so workspace select/session/new-session/prompt stay disabled. Local fixes remove duplicate `--app`, expose connection status, prevent chat-dock overlap, improve mobile layout, and fix tests. |
| Files | `16-explorer.png`; extra local screenshots in `output/playwright/explorer-local-after-fix-2026-05-27T14-18/` | Opens with locally seeded root folders; true VPS file/chunk data is not proven synced. | Live UI bug reproduced: table rows could run under the preview pane, causing preview to intercept double-clicks. Local fixes constrain grid columns, add ARIA/icon actions, and add responsive layout. Global file mock-data delivery remains blocked by RxDB/WebRTC sync health. |
| Source Editor | `17-code-editor.png` | Opens with module list; source file loading fails live. | Monaco loader used `/vendor/monaco/vs/loader.js`, which returns HTML/404/MIME error live. Local fix resolves Monaco under `/business-os/vendor/monaco/`, adds MIME preflight/retry, and keeps Save/Diff/Revert disabled until a real file is loaded. File loading still fails live through `ctox.source.load` command timeout. |
| App Creator | `18-creator.png`; extra screenshots in `output/playwright/creator-live-2026-05-27T13-creator/` | Opens with templates and prompt controls; no installed custom apps are shown live. | Right rail was hardcoded empty and did not read `business_module_catalog`/`business_commands`; mobile hid the prompt generator completely; direct test import was broken. Local Creator fixes now bind catalog/prompts, preserve manual spec validation, improve icons, and stack all panes on mobile. |

## Current Quality Checklist Draft

- App is reachable from runtime registry, desktop, Start menu, App Store, and taskbar without contradictory labels.
- App header uses the shared shell/module appbar pattern: consistent title, icon, secondary actions, and spacing.
- Controls use the shared visual grammar: icon buttons for tools, segmented controls/tabs for modes, selects/menus for option sets, toggles/checkboxes for binary settings, and clear primary actions.
- Empty states are actionable and distinguish `no data exists` from `sync unavailable`.
- Mock data appears for every app-specific primary list/table/panel before the app is considered QA-ready.
- Required app interactions work: filtering, selecting a row/card, opening details, creating/editing where applicable, refresh/reload, and responsive resizing.
- App-specific console/page errors are zero under authenticated live QA.
- Advanced Status is healthy or the app clearly marks itself degraded and blocks native-only actions.
- Desktop, Start menu, App Store, and module registry expose the same intentional app inventory.
- Tables and split panes have stable responsive sizing; labels do not wrap into broken words or overflow controls.

## Open Blockers

- Browser RxDB/WebRTC replication to the native peer is not healthy even though VPS SQLite required collections are present. Post-deploy Advanced Status still fails: `requiredCollectionsConnected`, `requiredCollectionsInitialSyncComplete`, `requiredCollectionsCheckpointEpochAdvertised`, `noStalledReconnect`, `frameTransportRealtimeHealthy`.
- Command acknowledgement is failing, which affects module version loading, Outbound knowledge setup, Coding Agents, Source Editor file loading, and other native-backed actions.
- Several apps still show empty states where mock data was expected.
- The app inventory is not unified across Desktop, Start menu, App Store, and baseline.
- Post-deploy QA is `OK` only because it verifies app reachability with console-fail disabled; console/page errors still occur in Knowledge, Outbound, Shiftflow, Notes, Source Editor, and App Creator and need a separate console-clean pass.

## Subagent Review Status

- Completed: all 18 baseline apps were handled by separate app-specific subagents.
- Completed app list: `ctox`, `reports`, `documents`, `knowledge`, `research`, `matching`, `conversations`, `outbound`, `shiftflow`, `spreadsheets`, `notes`, `app-store`, `buchhaltung`, `calendar`, `coding-agents`, `explorer`, `code-editor`, `creator`.
- Follow-up still required outside individual app modules: deploy/sync local asset fixes to `cto1.kunstmen.com`, fix browser RxDB/WebRTC/native command acknowledgement, and unify Desktop/Start/App Store inventory.

### CTOX Subagent Result

- Live interactions tested: login/open, Web-Stack refresh, flow zoom, flow node drawer, keyboard resizer, context menu, mobile width.
- Root data finding: no Tasks/Web-Stack data because browser RxDB/WebRTC has no active native peer evidence (`activePeerCount: 0`) and required collections are not initial-sync complete.
- Local fixes:
  - `src/apps/business-os/modules/ctox/index.js`: preserve actionable Web-Stack projection errors on refresh.
  - `src/apps/business-os/modules/ctox/index.js`: `taskSteps(null, state)` now returns `[]`.
  - `src/apps/business-os/modules/ctox/test.js`: added regression coverage.
- Verification: `node src/apps/business-os/modules/ctox/test.js` passes 6/6 checks.

### Matching Subagent Result

- Live interactions tested: source grouping (`source`, `area`, `recent`), requirement filter empty state, object filter empty state, object sort modes, table tab.
- Root data finding: Matching app collections are empty live; this is data delivery/mock seeding, not just rendering.
- Local fixes:
  - `src/apps/business-os/modules/matching/index.html`: source grouping select now has a stable ID and shorter labels.
  - `src/apps/business-os/modules/matching/ui/index.js`: grouping mode is wired, persisted, and renders area grouping cards.
  - `src/apps/business-os/modules/matching/index.css`: area grouping card styles.
  - `src/apps/business-os/modules/matching/index.js`: cache build key bumped.
- Verification reported by subagent: `node --check`, matching `node --test`, and `diff --check` passed.

### Research Subagent Result

- Live interactions tested: authenticated Research view, reload/diagnostics, empty state, create modal validation, responsive layout, header/icon control accessibility.
- Root data finding: `knowledge_tables` is 0 live, so app-specific data workflows cannot be fully validated until mock data replicates.
- Local fixes:
  - `src/apps/business-os/modules/research/index.js`: no-data state keeps the standard header and disabled workbench controls; create validation is clearer.
  - `src/apps/business-os/modules/research/index.css`: responsive empty state and center layout hardening.
  - `src/apps/business-os/modules/research/locales/de.json` and `en.json`: copy for diagnostics/validation.
  - `src/apps/business-os/modules/research/test.mjs`: regression coverage.
- Verification: `node --test src/apps/business-os/modules/research/test.mjs` passes 4/4 checks.

### Knowledge Subagent Result

- Live interactions tested: source tabs (`User`, `System`, `Alle`), search no-match, detail tabs, disabled runbooks/data/edit states, mobile width.
- Root data finding: no Knowledge data live because browser WebRTC/Frame-Transport has no active native peer and required collections are not connected.
- Local fixes:
  - `src/apps/business-os/modules/knowledge/index.css`: explicit grid placement prevents pane/resizer click interception; search/header/focus styling aligned; mobile stacked layout; resizer hidden on mobile.
- Verification: `node src/apps/business-os/modules/knowledge/test.mjs` passes 5/5 checks; local interaction screenshots passed.

### Reports Subagent Result

- Live interactions tested: type filter, status filter, search, refresh, empty state, responsive mobile controls.
- Root data finding: live `business_module_reports` and `ctox_bug_reports` are empty; refresh also exposes the no-HTTP-proxy/RxDB-WebRTC requirement.
- Local fixes:
  - `src/apps/business-os/modules/reports/index.html`: Refresh is an icon button; search/filter controls have ARIA labels.
  - `src/apps/business-os/modules/reports/index.css`: shared-looking icon/select/focus/empty/mobile styling.
  - `src/apps/business-os/modules/reports/index.js`: localized labels, visible empty diagnostics with collection counts, robust JSON field parsing.
  - `src/apps/business-os/modules/reports/locales/de.json`, `en.json`, `test.mjs`: coverage and copy.
- Verification: `node src/apps/business-os/modules/reports/test.mjs` passes 4/4 checks.

### Documents Subagent Result

- Live interactions tested: open app, search, sort, status/tag filters, import drawer validation, create drawer validation, empty state, detail/runbook panes, mobile split layout.
- Root data finding: no document rows live; Export and Runbook remain correctly disabled.
- Local fixes:
  - `src/apps/business-os/modules/documents/index.js`: empty state now offers direct Import/Create actions; filter no-result state offers reset.
  - `src/apps/business-os/modules/documents/index.css`: less cramped filters; pane/runbook header alignment.
  - `src/apps/business-os/modules/documents/locales/de.json`, `en.json`: copy for new states/actions.
- Verification: `node --test src/apps/business-os/modules/documents/documents.test.mjs` passes 4/4 checks.

### Conversations Subagent Result

- Live interactions tested: channel filter, direction filter, date filter, search, disabled account filter, empty state, mobile width.
- Root data finding: communication collections do not deliver data; sync diagnostics show peer/connectivity problems.
- Local fixes:
  - `src/apps/business-os/modules/conversations/index.js`: include `communication_threads` in required diagnostics; classify connecting peers and timeout lifecycle events correctly.
  - `src/apps/business-os/modules/conversations/index.css`: mobile layout keeps left pane reachable.
  - `src/apps/business-os/modules/conversations/conversations.test.mjs`: added connecting/timeout coverage.
- Verification: `node --test src/apps/business-os/modules/conversations/conversations.test.mjs` passes 6/6 checks.

### Outbound Subagent Result

- Live interactions covered by baseline/subagent: campaign shell, hidden firms, outreach/header controls, mobile/resizer behavior.
- Root data finding: native command acknowledgement is failing, so Outbound knowledge setup cannot complete; company/import rows remain empty.
- Local fixes:
  - `src/apps/business-os/modules/outbound/index.js`: build key bumped, outreach toggle uses regular button semantics, hidden-firms action has a stable data action.
  - `src/apps/business-os/modules/outbound/index.css`: mobile resizer hidden; header toggle sizing stabilized.
  - `src/apps/business-os/modules/outbound/outbound.test.mjs`: Browser ESM is bundled through esbuild for reliable Node tests.
- Verification: `node --test src/apps/business-os/modules/outbound/outbound.test.mjs` passes 3/3 checks.

### Notes Subagent Result

- Live interactions tested: notebooks, tags, favorite with undo, trash with undo, note selection, lock/encryption, sort/filter popover, header controls.
- Data finding: mock notes are present live.
- Local fixes:
  - `src/apps/business-os/modules/notes/index.js`: removed noisy console log; sidebar/note cards now expose role/tabindex/ARIA and keyboard activation.
  - `src/apps/business-os/modules/notes/index.css`: lock button no longer hidden by chat dock; mobile layout stacks; resizers hidden on mobile; focus states and accent strips refined.
  - `src/apps/business-os/modules/notes/notes.test.mjs`: Browser ESM bundled through esbuild for Node tests.
- Verification: `node --test src/apps/business-os/modules/notes/notes.test.mjs` passes 4/4 checks.

### Spreadsheets Subagent Result

- Live interactions tested: search, sort/status/tag filters, import drawer, new draft drawer, runbook selection.
- Root data finding: sheet records/chunks/versions are absent live; runbooks are present; browser frame transport still lacks active peer.
- Local fixes:
  - `src/apps/business-os/modules/spreadsheets/index.js`: removed brittle inline filter widths and added spreadsheet-specific filter classes.
  - `src/apps/business-os/modules/spreadsheets/index.css`: stable filter grid; `.jss_*` dark theme styles for JSpreadsheet.
  - `src/apps/business-os/modules/spreadsheets/spreadsheets.test.mjs`: Browser ESM bundled through esbuild for Node tests.
- Verification: `node --test src/apps/business-os/modules/spreadsheets/spreadsheets.test.mjs` passes 5/5 checks.

### App Store Subagent Result

- Live interactions tested: scope tabs, search, grid/list, details, responsive view.
- Data finding: App Store data exists but counts are inconsistent: GitHub Discovery 19, Marketplace 1 deduped app, Everything 21. Inventory mismatch across shell surfaces remains outside App Store scope.
- Local fixes:
  - `src/apps/business-os/modules/app-store/index.js`: `Installed` counts local/system/starter/installed apps instead of only `status=installed`.
  - `src/apps/business-os/modules/app-store/index.html` and `index.css`: moved inline controls to classes, ARIA pressed states, keyboard-focusable cards, responsive grid.
  - `src/apps/business-os/modules/app-store/app-store.test.mjs`: Browser ESM bundled through esbuild for Node tests.
- Verification: `node --test src/apps/business-os/modules/app-store/app-store.test.mjs` passes 6/6 checks.

### Shiftflow Subagent Result

- Live interactions tested: department filter, search, team/status, employee/project timeline, shift detail, timesheets, billing, conflict check, mobile.
- Data finding: Shiftflow mock data is present live.
- Local fixes:
  - `src/apps/business-os/modules/shiftflow/index.css`: dark-mode shift-card contrast, no side stripes, hover button no longer overlays shift cards, mobile layout stacks center/left/right.
  - `src/apps/business-os/modules/shiftflow/index.html`, `locales/de.json`, `locales/en.json`: emoji action prefixes replaced with mask-icon semantics.
  - `src/apps/business-os/modules/shiftflow/index.js`: cache build key bumped; project timeline row style updated.
- Verification: `git diff --check -- src/apps/business-os/modules/shiftflow` and locale JSON parse pass; local Playwright check passed. Existing `test.mjs` is not directly runnable in this checkout without the Browser-ESM bundling pattern.

### Buchhaltung Subagent Result

- Live interactions tested: SKR search and ledger drawer, Journal/manual booking validation, Belege, Bankabgleich, Reports tabs, AI-Agent input, Reise/Fahrtenbuch/Anlagen drawers, mobile layout.
- Data finding: SKR accounts are present live (42 accounts). Journal, Belege, Bankabgleich, Reisen, and Fahrtenbuch were empty because module mock seeding could run before `state.accounts` was fully loaded.
- Local fixes:
  - `src/apps/business-os/modules/buchhaltung/index.js`: seeding race fixed and mock data added for Journal, Belege, Bankabgleich, Reisen, and Fahrtenbuch.
  - `src/apps/business-os/modules/buchhaltung/index.html`: emoji controls replaced with token-style icons; Agent send control has ARIA/title.
  - `src/apps/business-os/modules/buchhaltung/index.css`: mobile one-column layout, resizer hiding, stable tabs/header, horizontal table scrolling.
- Verification: esbuild-bundled `test.js` passes 9/9; `node --input-type=module --check < index.js` passes; `git diff --check -- src/apps/business-os/modules/buchhaltung` passes.

### Calendar Subagent Result

- Live interactions tested: calendar list, calendar visibility toggle, booking page selection/edit, `+ Seite`, `Neuer Termin`, new calendar drawer, today/previous/next navigation, mobile layout.
- Data finding: Calendar mock data is present live: 2 events, 2 calendars, 1 booking page.
- Local fixes:
  - `src/apps/business-os/modules/calendar/index.css`: bottom drawer is lifted above the global dock and mobile drawer/form layout is stabilized.
  - `src/apps/business-os/modules/calendar/index.js`: fallback click handling for rendered EventCalendar events.
  - `src/apps/business-os/modules/calendar/calendar-view-adapter.js`: EventCalendar occurrence ids resolve more reliably back to Business OS event ids.
  - `src/apps/business-os/modules/calendar/calendar.test.mjs`: switched to the esbuild/Data-URL Browser ESM pattern.
- Verification: `node --test src/apps/business-os/modules/calendar/calendar.test.mjs` passes; calendar index and adapter esbuild browser bundles pass; `git diff --check -- src/apps/business-os/modules/calendar` passes.

### Coding Agents Subagent Result

- Live interactions tested: workspace list, system settings, add workspace validation, disabled new-session/safety/prompt states, mobile width.
- Data finding: no workspace/grant data is available live. The module reports command-bus/backend timeout, so native-backed actions correctly remain disabled.
- Local fixes:
  - `src/apps/business-os/modules/coding-agents/index.js`: duplicate `--app` in status commands removed; top-level debug log removed; command argument test hook added.
  - `src/apps/business-os/modules/coding-agents/index.html`: workbench connection status is visible.
  - `src/apps/business-os/modules/coding-agents/index.css`: 8px panel radii, safe-area spacing against chat dock overlap, mobile stacked layout, hidden mobile resizer.
  - `src/apps/business-os/modules/coding-agents/coding-agents.test.mjs`: switched to esbuild/Data-URL Browser ESM pattern.
- Verification: `node --test src/apps/business-os/modules/coding-agents/coding-agents.test.mjs` passes 8/8; browser esbuild bundle passes; `git diff --check -- src/apps/business-os/modules/coding-agents` passes.

### Explorer / Files Subagent Result

- Live interactions tested: path navigation, categories, new folder, upload enabled/disabled states, search, detail selection, header/ARIA, mobile layout.
- Data finding: Explorer shows locally seeded root folders. True VPS `desktop_files` / `desktop_file_chunks` delivery is still not proven in-browser because active native peer evidence is missing.
- Local fixes:
  - `src/apps/business-os/desktop-apps/explorer/app.js`: grid columns no longer run under preview; preview no longer intercepts row double-clicks; action buttons get icons/ARIA; source buttons/rows/sort state get ARIA; narrow/mobile layout is stabilized.
  - `src/apps/business-os/desktop-apps/explorer/explorer.test.mjs`: switched to esbuild/Data-URL Browser ESM pattern.
- Verification: `node --test src/apps/business-os/desktop-apps/explorer/explorer.test.mjs` passes 5/5; local Chromium harness passes; `git diff --check` passes.

### Source Editor Subagent Result

- Live interactions tested: authenticated open, module list, module/file load attempt, disabled save/diff/revert state, fallback editor behavior.
- Data finding: module list renders live, but file loading fails through `ctox.source.load` command acknowledgement timeout. Save/Diff/Revert remain correctly disabled because no source file is loaded.
- Local fixes:
  - `src/apps/business-os/desktop-apps/code-editor/app.js`: Monaco asset base resolves under `/business-os/vendor/monaco/`; MIME preflight added; failed Monaco load can retry; Format action added; JSON/low-risk whitespace formatting implemented; module/diff ARIA states and responsive layout improved.
  - `src/apps/business-os/desktop-apps/code-editor/code-editor.test.mjs`: switched to esbuild/Data-URL Browser ESM pattern.
- Verification: `node --test src/apps/business-os/desktop-apps/code-editor/code-editor.test.mjs` passes 6/6; `node --input-type=module --check < app.js` passes; `git diff --check -- src/apps/business-os/desktop-apps/code-editor/...` passes.

### App Creator Subagent Result

- Live interactions tested: authenticated open, preset loading (`Support Desk`), advanced settings, validation with empty title, custom CRM prompt optimization, install-confirmation path with cancel, context menu, mobile layout.
- Data finding: no installed custom apps are shown live. The module itself was not querying `business_module_catalog` or `business_commands`, so even available generated app metadata/prompts could never render in the right rail.
- Local fixes:
  - `src/apps/business-os/modules/creator/index.js`: right rail now subscribes to `business_module_catalog` and `business_commands`; installed custom apps and prompt suggestions are normalized/rendered; prompt adoption/open/upgrade actions are wired; manual advanced edits keep a fresh spec while surfacing validation errors.
  - `src/apps/business-os/modules/creator/index.html`: right rail gets real list containers; inline/text-only controls converted to icon controls with ARIA labels.
  - `src/apps/business-os/modules/creator/index.css`: mobile stacks left/center/right instead of hiding the generator, context menu is constrained on mobile, logs/cards wrap safely, and mini-card styling is standardized.
  - `src/apps/business-os/modules/creator/creator.test.mjs`: switched to the esbuild/Data-URL Browser ESM pattern and added right-rail/validation coverage.
- Verification: `node --test src/apps/business-os/modules/creator/creator.test.mjs` passes 8/8; `node --input-type=module --check < src/apps/business-os/modules/creator/index.js` passes; `git diff --check -- src/apps/business-os/modules/creator src/apps/business-os/desktop-apps/creator/app.js` passes.
