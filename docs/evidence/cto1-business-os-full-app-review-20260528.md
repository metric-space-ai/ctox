# CTO1 Business OS Full App Review, 2026-05-28

Scope: CTO1 live Business OS at `https://cto1.kunstmen.com`, build line `b480189`, shell asset `APP_BUILD=20260528-cto1-repair3`.

Evidence:

- Browser inventory: `output/cto1-app-inventory-20260528-repair3/inventory.json`
- Focused post-fix DOM verification: `output/cto1-app-inventory-20260528-repair3/postfix-verification.json`
- Screenshots: `output/cto1-app-inventory-20260528-repair3/*.png`
- App-specific subagent reviews: one app per Business OS app, plus earlier dedicated app reviews for Tickets, Spreadsheets, Matching, Outbound, Customers, and Reports.

The review covers 19 surfaces: CTOX plus the 18 Business OS apps visible in the current CTO1 shell.

## Executive Summary

CTO1 is not failing because of one isolated visual bug. The main failure pattern is:

1. Apps render from local browser RxDB before the required collections have actually replicated.
2. Several VPS/seed/mock documents do not match the current browser schemas or UI normalizers.
3. Missing collections are often swallowed as empty arrays, so a data-plane or schema failure is displayed as "Noch keine Daten".
4. Many modules implement their own context menus, resizers, empty states, and source/edit affordances instead of using one shared Business OS contract.
5. The global shell status can still hold an offline error while hiding the warning in the UI.

This explains why apps such as Tickets, Customers, Matching, Knowledge, and parts of Outbound show no data even though the VPS SQLite contains rows. The data can exist on the VPS and still be invisible in the app if the collection did not replicate, if the schema rejected the rows, if a normalizer drops top-level fields, or if the UI selects an empty fallback scope.

## Global P0/P1 Issues

### G1 - Fake empty states before data readiness

Several apps call their initial `refresh/load` path on mount and immediately render "Keine ..." if local rows are not present. The shell now starts module sync earlier, but the apps still need a real readiness contract:

- initial state: loading
- replicated state: data or true empty
- failed state: exact collection/data-plane error

No app should show "Keine Tickets", "Keine Kunden", "Keine Anforderungen", or "Keine Knowledge-Eintraege" until the relevant collections have either completed initial sync or failed explicitly.

### G2 - Data contracts do not match schemas

Known mismatches:

- Matching v0/v1 rows mix top-level fields and `data` payload fields.
- Customers test/mock rows omit required schema fields such as status/search/delete/timestamps.
- Reports/CTOX generated docs can omit required `updated_at_ms` and `kind`.
- Outbound sources use `name/url` while the schema expects `title/source_type/payload/updated_at_ms`.
- Spreadsheets require `spreadsheet_versions.model_json`; parent spreadsheet rows alone are not enough.
- Knowledge `knowledge_tables` rows are not surfaced unless linked through visible `knowledge_items`.

This is the core reason "Daten sind im VPS, aber nicht in der App" can happen.

### G3 - Context menu contract is inconsistent

Requirement: every fullscreen app and every column/record surface should support right-click at pointer position with:

- `Mit Daten arbeiten`
- `App modifizieren`
- module id, column id, record id, selection/cell/range context
- submitted prompt appears visually in the bottom chat bar

Current state:

- Some modules use bespoke context menus.
- The global capture listener can shadow module-specific menus.
- Many modules block native right-click on inputs/textareas/buttons.
- Some rows expose `data-context-record-id`, while the global shell expects other attributes such as `data-record-id`.
- Calendar and Browser do not provide a useful app-specific right-click contract.

### G4 - Header, column, resize, and dock layout are not standardized

Common failures:

- fixed pane widths squeeze center workspaces at 1280px
- bottom chat/dock overlaps content in Calendar, Research, Shiftflow, and other workspaces
- table columns are fixed or squeezed, with no per-column resize
- top navigation tabs overlap with Window/DE/Dark/user controls
- resizer handles are invisible or app-specific
- labels clip in German and English

### G5 - Source Editor and File Viewer are only partially recovered

File Viewer and Source Editor are back in launch targets, but the behavior is not coherent:

- Source Editor deep links often open only `#module`, not a specific source file or selected record.
- Some apps claim source editability in registry while `module.json` says the opposite.
- File Viewer cannot preview DOCX, even though Documents uses SuperDoc internally.
- App-specific source context is missing for Calendar events, App Store cards, Browser sessions, and several right-click flows.

### G6 - Technical/debug copy leaks into production UX

Examples found in screenshots or code paths:

- `coding agent execute`, `cmd_*`
- `rows`
- `RxDB`, `WebRTC`, `N/A`
- `Loading workspaces...`
- hidden CTOX offline text while the warning is not visible
- module-specific "Seed", "Clear Frames", diagnostic terms

These should be hidden behind a developer mode or converted to concrete user-facing state.

### G7 - Desktop/start menu/app registry mismatch

The app set depends on a mix of registry entries, default-installed flags, desktop layout, and taskbar pins. That explains why apps can appear on the Desktop but not in the start menu, or vice versa. The registry should be the canonical source, and desktop/start menu/taskbar should be projections with explicit installed/hidden/pinned state.

## App Matrix

| App | Data status on CTO1 | Highest issue | Screenshot |
|---|---|---:|---|
| CTOX | Partial: queue visible, flow projection missing | P1 | `output/cto1-app-inventory-20260528-repair3/ctox.png` |
| Tickets | Empty despite VPS ticket rows | P0 | `output/cto1-app-inventory-20260528-repair3/tickets.png` |
| Documents | Renders, but no selected document/version | P1 | `output/cto1-app-inventory-20260528-repair3/documents.png` |
| Spreadsheets | Shell visible, version/model dependency fragile | P1 | `output/cto1-app-inventory-20260528-repair3/spreadsheets.png` |
| Knowledge | Empty despite knowledge/table rows | P0 | `output/cto1-app-inventory-20260528-repair3/knowledge.png` |
| Web Research | Data visible | P1 | `output/cto1-app-inventory-20260528-repair3/research.png` |
| Matching | Empty despite matching rows | P0 | `output/cto1-app-inventory-20260528-repair3/matching.png` |
| Conversations | Empty/syncing | P1 | `output/cto1-app-inventory-20260528-repair3/conversations.png` |
| Outbound | Partial: table rows visible, counters/scope unreliable | P1 | `output/cto1-app-inventory-20260528-repair3/outbound.png` |
| Customers | Empty despite customer/account data | P0 | `output/cto1-app-inventory-20260528-repair3/customers.png` |
| Shiftflow | Data visible | P1 | `output/cto1-app-inventory-20260528-repair3/shiftflow.png` |
| Notes | Data visible, but seed/security problems | P0 | `output/cto1-app-inventory-20260528-repair3/notes.png` |
| App Creator | Workbench visible, no real source preview | P1 | `output/cto1-app-inventory-20260528-repair3/creator.png` |
| App Store | Catalog visible, drawer broken | P1 | `output/cto1-app-inventory-20260528-repair3/app-store.png` |
| Browser | Remote browser unusable | P0 | `output/cto1-app-inventory-20260528-repair3/browser.png` |
| Buchhaltung | Data visible, accounting actions broken | P0 | `output/cto1-app-inventory-20260528-repair3/buchhaltung.png` |
| Calendar | Data visible | P1 | `output/cto1-app-inventory-20260528-repair3/calendar.png` |
| Coding Agents | Loading/offline state, credential risk | P0 | `output/cto1-app-inventory-20260528-repair3/coding-agents.png` |
| Reports | Post-fix data visible, status/schema still fragile | P1 | `output/cto1-app-inventory-20260528-repair3/reports.png` |

## Per-App Findings

### CTOX

Status: task queue data appears, but live flow/timeline/metrics are not working.

- P1: Main flow falls back to empty harness/projection state even while queue rows are visible.
- P1: CTOX-specific context menu can be shadowed by the global capture listener, losing task/flow/timeline context.
- P1: Source evidence is inconsistent: `business_module_source_files` exists in schema/module metadata, but the registry/app path does not expose it coherently.
- P2: Task labels leak raw command names and IDs such as `coding agent execute` and `cmd_*`.
- P2: `0 aktiv` conflicts with visible blocked/waiting work.
- P2: Left resizer is app-specific while shell resizers are hidden for this layout.

Required fix direction: show a real red CTOX offline/projection error when the flow projector is absent, normalize command labels, and route CTOX context through the shared right-click contract.

### Tickets

Status: empty on CTO1, despite known ticket rows on the VPS.

- P0: Tickets reads local RxDB only and returns `[]` if collections are missing, so sync/schema failure becomes "Noch keine Tickets".
- P0: Ticket collections are not part of the critical warmup gate.
- P1: The module can mount and render empty before ticket replication has started or completed.
- P1: `pending_sync` command behavior can hang for up to 45s and then fail without useful progress.
- P2: Ticket row context uses attributes not consumed by the global context menu, so right-click lacks the selected ticket id.
- P2: Fixed panes and min widths can clip the header/detail/context columns.
- P2: Registry and `module.json` disagree on source editability.

Required fix direction: gate rendering on `ctox_ticket_*` readiness, show exact collection errors, and make ticket rows use the global record/column context schema.

### Documents

Status: Documents app renders, but current screenshot/DOM shows no selected document or version.

- P1: Empty workspace is ambiguous: no document, no version, and no sync/error state are clearly distinguished.
- P1: DOCX editor toolbar has max-height/overflow rules that can clip controls in narrow center panes.
- P1: Source Editor "App oeffnen" only jumps to `#documents`, not to the specific source/file context.
- P2: Context menu intercepts host/left/right, including fields where native text actions should work.
- P2: "Word-Dokument erstellen" opens a runbook/research style dialog, not a direct blank document creation.
- P2: Runbook copy says `RUNBOOKS KEINE` while runbook options are still visible, which reads as contradictory.
- P2: File Viewer cannot preview DOCX while Documents itself can.

Required fix direction: separate document-empty, version-missing, and sync-error states; deep-link file/source context; reuse one context menu implementation with native input opt-out.

### Spreadsheets

Status: spreadsheet shell visible, but reliable rendering requires `spreadsheet_versions.model_json`.

- P1: Parent `spreadsheets` rows alone do not render a sheet. A valid `spreadsheet_versions` row with `model_json` is required.
- P1: Initial load can show "no saved version" or empty selection before sync is ready.
- P2: Missing/corrupt model data can fall back to sample/default grid data, hiding real data corruption.
- P2: Registry promises native XLSX, but implementation supports CSV/JSON import only.
- P2: Explorer deep links such as `#spreadsheets?record=` are stripped by shell routing.
- P2: Column resize changes are not reliably persisted.
- P2: Context menu omits selected cell/range/model context and can suppress spreadsheet-native context actions.

Required fix direction: validate spreadsheet parent plus version data together, remove sample fallback in production, and pass selected cell/range into CTOX chat.

### Knowledge

Status: empty on CTO1, despite known knowledge tables/items on the VPS.

- P0: Pure `knowledge_tables` rows are not shown. The UI renders tables only when linked through visible `knowledge_items` with the expected kind/flags.
- P0: `loadLocalKnowledgeRecords()` can unwrap payloads in a way that drops top-level metadata such as id/kind/table flags.
- P1: Empty state does not distinguish no rows, missing collection, failed sync, or filtered-out rows.
- P1: Header repeats "Knowledge" across shell, app, and pane while the main workspace is empty.
- P1: Resize logic mixes absolute resizer layout and older ratio/localStorage keys.
- P1: Context menu overrides native right-click on inputs/textareas/buttons.
- P2: Data-tab pager can advance into empty pages.
- P2: Markdown rendering is too weak for real skills/runbooks/tables.
- P2: Source filter classifies only a narrow subset of system sources as System.

Required fix direction: make `knowledge_tables` a first-class visible source, preserve metadata during payload normalization, and expose collection-level sync errors.

### Web Research

Status: data visible. Inventory shows dashboards, sources, measurements, and completed status.

- P1: Registry does not list `knowledge_tables` although module/schema depend on it, risking incomplete sync/install state.
- P1: Header can clip `Research fortsetzen` at 1280px due fixed pane widths.
- P1: Source scoring looks generic, with many sources shown as `D/Risiko` despite many rows.
- P2: Bottom chat/dock overlaps the workbench because the module lacks enough bottom inset.
- P2: Context menu blocks native input/select/button right-click.
- P2: Source details are read-only JSON drawers, not a real evidence editor/writeback flow.
- P2: Some selectable cards use `div[data-action]` instead of buttons, hurting keyboard/accessibility.

Required fix direction: add registry collection dependencies, repair score inputs/copy, and standardize headers/dock-safe layout.

### Matching

Status: empty on CTO1, despite known matching data.

- P0: v0/v1 data mapping can drop required top-level fields when `row.data` exists.
- P0: Matches are filtered out if `requirementId`, `objectId`, or source ids are not normalized correctly.
- P1: Registry says v0.1/default-installed while module manifest says v1/default false.
- P1: Mock/source rows can lack required `updated_at_ms`.
- P1: After a local DB reset, the module renders empty before sync/projected data is available.
- P2: Three-column grid has hard min widths and clips on narrow workspaces.
- P2: Source Editor save does not clearly update the currently running module revision.

Required fix direction: unify v1 schema/registry, normalize top-level plus `data` fields, and render sync/error states before empty states.

### Conversations

Status: renderable but empty/syncing. Screenshot shows no accounts and communication sync text.

- P1: App depends on `communication_accounts`, `communication_threads`, and `communication_messages`; missing or not-yet-synced collections render as no conversations.
- P1: Direction filter in the left list is visible but only applies later to timeline messages after selection.
- P1: Registry/module entry points can send source editors to `index.html` first while real logic is in `index.js`.
- P2: Header repeats "Conversations" and uses the unclear kicker "AUDIT AUS CTOX-SICHT".
- P2: Right context disappears under breakpoint instead of becoming an accessible drawer/tab.
- P2: Context menu exists only on message bubbles, so the empty state has no app-specific recovery action.

Required fix direction: surface communication collection readiness, fix list filters, and add app-level right-click recovery/source context.

### Outbound

Status: partial. Post-fix DOM shows table rows, but counters still show zero in places and campaign scope remains fragile.

- P1: Default empty campaigns can mask populated seeded campaigns if selected before the data-bearing campaign.
- P1: Recovery logic does not move valid seeded rows into the selected default campaign.
- P1: Seeded `outbound_sources` docs do not match the schema.
- P1: No general loading state for campaign/company hydration.
- P2: `pending_sync` is not consistently classified as waiting/running.
- P2: Contact field filters can target `contact_field.*` ids that table value code does not resolve.
- P2: CRM table uses fixed layout with many columns and no per-column resizing.
- P2: Registry/module manifest disagree on install/edit metadata.

Required fix direction: choose the first populated campaign deterministically, fix source schema, and implement real table column resize and filter mapping.

### Customers

Status: empty on CTO1, despite known customer/account/contact/opportunity data on the VPS.

- P0: Missing collections are swallowed as empty arrays and rendered as "Noch keine Kunden".
- P0: Local test/mock customer rows omit required fields and are not valid seed contracts.
- P1: Schema/hash drift can block replication while the UI looks empty.
- P1: Registry marks Customers as store/default false; rebuilds from default-installed can drop it from installed app sets.
- P2: Stored pane widths and fixed table layout can break headers and columns.
- P2: Context/source editing is registry-driven but not app-context aware.

Required fix direction: show missing collection as error, validate customer seed data against schema, and mark install/default state consistently.

### Shiftflow

Status: data visible. Screenshot includes employees, projects, shifts, assistant, and conflicts.

- P1: Auto-plan can delete published shifts for the week while comment implies draft cleanup.
- P1: Shift form does not validate end time before start time or model overnight shifts cleanly.
- P1: Three-column layout is cramped and bottom dock overlaps scheduler rows at 1280px.
- P2: Context menu overrides every right-click target, including text/input actions.
- P2: "Ausfall/Ersatz planen" is only an alert.
- P2: Drag/drop creates a published default 08:00-16:00 Service shift too aggressively.
- P2: Manual new shifts are published immediately, so "Dienstplan veroeffentlichen" may have nothing to do.

Required fix direction: fix destructive planning semantics, validate schedule times, and make draft/published workflows explicit.

### Notes

Status: data visible, three notes. Data appears seed/example-like.

- P0: Notes render stored HTML through `innerHTML` paths and rehydrate via DOMParser/Lexical, creating an unsanitized HTML/XSS risk.
- P1: Lock/zero-knowledge UX is false: default PIN is visible and note passcodes are stored in records.
- P1: If collection is empty, the app writes sample notes, masking real empty or sync failure.
- P1: Context menu misclassifies sidebar interactions because it checks `ctx.left` after rendering sidebar inside host.
- P2: Header/list title clips, e.g. `Alle N...`.
- P2: Resizers are narrow and mostly invisible.
- P2: Module claims markdown, but the editor stores raw HTML and has no controlled raw/markdown recovery mode.

Required fix direction: sanitize or remove unsafe HTML, remove seed writes from production, and replace cosmetic lock with real encrypted or honest UX.

### App Creator

Status: workbench visible, custom-app state empty.

- P1: App creation reports success immediately after dispatching commands, without verifying persisted catalog/source files.
- P1: Generated templates interpolate user-controlled `appTitle` into HTML strings.
- P1: There is no visible file list, diff, source preview, or edit-before-install path despite registry copy claiming code projection.
- P2: Quickstart templates make installation possible while the "Spezifikation optimieren" flow remains visually primary.
- P2: Fixed side panes can squeeze center content before the responsive breakpoint.
- P2: Context menu intercepts form field/text area native actions.
- P2: Right pane says no own apps without distinguishing empty catalog from missing sync.

Required fix direction: add pre-install source preview/diff, verify command completion against the catalog, and make creation flow honest.

### App Store

Status: catalog visible, 19 GitHub modules, 13 installed, but detail drawer state is visually broken.

- P1: Empty detail drawer is visible and overlays the catalog because HTML/JS/CSS use mismatched class names.
- P1: Install/manage relies on command dispatch and polling with generic timeout messaging and no per-card retry/error detail.
- P2: Mixed German/English labels: `Marketplace`, `Installed`, `Everything`, `Available`, `Source path`, `Installer archive`.
- P2: Sidebar resize reads `localStorage` directly after resizer init and may avoid normal clamping.
- P2: Context menu intercepts all host right-clicks, including buttons/links/inputs.
- P2: "Bearbeiten" routes to Creator upgrade instead of a verifiable source editor/diff flow.

Required fix direction: fix drawer class contract, localize UI, and add card-level install status/error/retry.

### Browser

Status: app renders but remote browser is not usable. The screen is an offline/no-session state.

- P0: No active remote frame/data channel, and the user sees no concrete visible error despite hidden CTOX offline warning.
- P1: Viewer activity/heartbeat is disabled by an early return, so presence is never written.
- P1: Sidebar/header in screenshot does not match the expected module controls, indicating shell/module layout drift.
- P1: Three-column layout and inspector behavior clip in desktop workspace.
- P1: Keyboard handling ignores Ctrl/Meta combinations, so browser-like shortcuts and copy/paste are incomplete.
- P2: Right-click is forwarded only as mouse down/up, not a useful context menu.
- P2: `Seed Frame` and `Clear Frames` are product-visible dev/test controls in module markup.

Required fix direction: restore remote session handshake/frame display, show exact channel error, and remove dev-only controls from production UI.

### Buchhaltung

Status: strongest data rendering. Tables for chart of accounts, journal, receipts, bank, assets, and travel are visible.

- P0: Draft bookings cannot be posted because the UI calls `postEntryDirectly(...)`, which does not exist.
- P1: Bank reconciliation links `reconciled_entry_id` to receipt id instead of journal entry id.
- P1: Posting paths can write journal lines without a central double-entry validation and with empty `account_id` fallbacks.
- P1: DATEV export uses hardcoded counter accounts and exports each ledger row too naively.
- P1: Asset toolbar actions are visible but have no event bindings.
- P2: Drag/drop import is promised but only file input change is implemented.
- P2: Context menu uses wrong field names for receipts/bank rows, so CTOX context can be empty or wrong.
- P2: Fixed 300/360px side panes plus table min-widths cause horizontal clipping at 1280px.

Required fix direction: repair posting/reconciliation before UI polish, then standardize accounting context payloads and table sizing.

### Calendar

Status: data visible, two events, two calendars, one booking page.

- P1: Calendar body is overlapped by the bottom dock, making lower time slots hard to reach.
- P1: EventCalendar view switcher appears visually broken/unreadable and duplicates the custom header.
- P1: Drag/resize of recurring occurrences patches the original event instead of creating an exception.
- P2: Header has two competing navigation/view control layers.
- P2: Resizer handles are hard to see and sidebars disappear hard at breakpoints.
- P2: No app-specific context menu for event/calendar/booking.
- P2: Event selection uses an extra capture-click match by title/time, which can select the wrong event.
- P2: Calendar adapter forces German locale while mount supports language switching.

Required fix direction: reserve dock-safe space, unify calendar header controls, and handle recurring events with instance exceptions.

### Coding Agents

Status: inventory shows loading/offline state: `Loading workspaces...`, `Verbindung getrennt`, no workspace selected.

- P0: Login/password values can be sent as command payloads and therefore persisted/synced/logged through `business_commands`.
- P1: Session create includes workspace, but session list/get/prompt are scoped only by app, so sessions can leak across selected workspaces.
- P1: Permission bypass uses a hardcoded Codex workspace path and treats any grant as active for Codex.
- P2: Loading/offline state is generic and warning text is hidden; user sees a disabled screen without a precise recovery path.
- P2: Agent selection can become stale because a closed-over `mappedApp` is used after selector changes.
- P2: No app-specific context menu or source editor.
- P2: Critical lifecycle/bypass/credential controls are hidden behind generic `System Settings`.

Required fix direction: remove credential command payloads, scope every session command by workspace, and show exact backend/command-bus state.

### Reports

Status: post-fix verification shows feature-request rows visible. Earlier screenshot before repair showed empty.

- P1: Generated report/bug docs can be rejected by schema if required fields such as `updated_at_ms` and `kind` are missing.
- P1: Reports can still render empty if `refreshReports()` runs before the relevant collections have synced.
- P1: Status/kind mapping is lossy: `pending`, `pending_sync`, `queued`, and `in_progress` collapse into broad open states.
- P2: Module-specific context menu is likely shadowed by the global context menu in full-workspace capture mode.
- P2: `module.json` and registry disagree on editability.
- P2: Detail header can overflow with long titles next to action buttons.

Required fix direction: validate report docs against schema, wait for collection readiness, and unify report context menu with global context.

## Business OS App Quality Checklist

Every Business OS app should pass this before being treated as working.

### 1. Data Contract

- Registry collections, `module.json` collections, schema exports, and VPS SQLite tables match.
- Seed/mock data validates against the same schema as production data.
- Required fields include timestamps, delete flags, search fields, status/kind, and ids.
- UI normalizers merge top-level fields and payload fields without dropping ids.
- App can explain collection states: not started, syncing, synced empty, synced with rows, sync failed.

### 2. Loading And Empty States

- First render is a real loading/sync state, not a fake empty state.
- Empty state appears only after initial sync/readiness.
- Data-plane error states name the collection and cause.
- No production UI says vague "prüfen" when the status is actually offline.
- No app writes sample rows just to avoid an empty state.

### 3. Header And Shell Fit

- Header fits at 1280x720, 1440x900, and mobile width.
- German and English labels both fit.
- App title, pane title, filters, and primary action do not overlap.
- Global dock/chat does not cover the workbench.
- Status colors are semantic: red for offline/error, yellow for waiting/degraded, green for ready.

### 4. Tables, Columns, And Resizing

- Every data table has stable header layout.
- Every column can be right-clicked with column context.
- Per-column resize is present where tables can have many columns.
- Horizontal scroll, if needed, is intentional and visible.
- Pane resizers use shared `CtoxResizer` behavior and clamp stale localStorage values.
- Selected row/record state is obvious.

### 5. Right-Click CTOX Menu

- Right-click popup opens at click position.
- It has `Mit Daten arbeiten` and `App modifizieren`.
- Context payload includes module, view, column, row, record id, selected text, cell/range when applicable.
- Submit dispatches to CTOX and creates a visible chat item in the bottom bar.
- Native text/input/menu behavior is preserved where appropriate.
- There is one shared implementation, not many incompatible module copies.

### 6. App-Specific Workflow

For each app, verify:

- create/new
- select/open detail
- edit/save
- delete/archive/complete where applicable
- search/filter/sort
- import/export
- command dispatch and completion/error state
- source/file viewer path if relevant
- keyboard and mouse interaction
- error recovery

### 7. Source Editor And File Viewer

- App source button opens the correct module source, not just a module hash.
- Source editor can deep-link to a file and preserve selected module context.
- Editable policy is consistent across registry and `module.json`.
- File Viewer supports the file types the app creates or explicitly routes them to the app viewer.
- Source edits produce a visible revision/reload path.

### 8. Copy And Debug Hygiene

- No `pending_sync`, `cmd_*`, `rows`, `RxDB`, `WebRTC`, `N/A`, "Seed", "Diagnose", or raw adapter names in normal user mode.
- Technical diagnostics are behind developer mode.
- Empty/loading/error copy is exact and operational.
- Buttons describe what they actually do.

### 9. Visual Consistency

- Shared panes, controls, icon buttons, tabs, segmented controls, drawers, status chips, tables, and context menus are reused.
- No app invents a different modal/context/menu style without a domain reason.
- Icons follow the same sizing/tone rules.
- Cards are used only for actual records/items, not every container.
- Dark/light/language modes preserve layout.

### 10. Evidence Required Per App

- Screenshot: desktop 1280x720.
- Screenshot: larger desktop.
- Screenshot: mobile/narrow, where supported.
- DOM inventory: row counts, empty/loading flags, visible warnings.
- Console/network failures.
- DB row counts for declared collections.
- Right-click test on header, column, row, empty surface.
- One app-specific interaction test.
- One command dispatch test if the app uses CTOX commands.

## Skill Compliance For Building Business OS Apps

The Business-OS-style frontend rules are clear: inspect adjacent modules, reuse shared primitives, preserve stable workbench topology, ship real context menus, resizers, chat affordances, loading/error states, and avoid fake interactions.

Current CTO1 violates that standard in multiple ways:

- modules duplicate their own context menu implementations
- right-click behavior is not consistent or record-aware everywhere
- empty states mask sync and schema errors
- resizers and bottom dock safety are app-specific
- source editor and file viewer paths are not context preserving
- several controls imply real work but dispatch nothing, only queue blindly, or use placeholder/sample behavior

The build skill itself is directionally correct. The app implementations are not consistently following it.

## Fix Order

1. Shell contract first: data readiness gate, visible red CTOX status, shared context menu payload, dock-safe layout.
2. Data visibility P0s: Tickets, Customers, Matching, Knowledge, Browser, Coding Agents.
3. App-specific P0 safety: Notes HTML/passcode, Buchhaltung posting/reconciliation, Coding Agents credentials.
4. Partial-data apps: Outbound, Spreadsheets, Reports, Documents, Conversations.
5. UI system pass: headers, resizers, column resize, labels, icon/style parity.
6. App-specific workflows: Calendar recurring edits, Shiftflow planning, App Store install retry, Creator preview/verify.

