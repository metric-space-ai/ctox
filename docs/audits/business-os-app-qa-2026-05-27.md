# Business OS App QA Audit - 2026-05-27

Scope: VPS `vps-63daac02.vps.ovh.net` / `51.210.246.120`, native Business OS served through local SSH tunnel at `http://127.0.0.1:18765/`.

This audit exists because the first pass was only a visual inventory. The corrected pass uses one subagent per Business-OS app and requires app-specific browser interactions. This audit is now complete for all 18 Desktop apps. Each app section is based on a dedicated single-app browser QA pass; where a core flow was not executed because it would write/import/delete data, that limitation is stated explicitly.

Evidence roots:

- Full screenshot inventory: `/private/tmp/ctox-business-os-inventory-20260527/`
- DOM inventory: `/private/tmp/ctox-business-os-inventory-20260527/inventory-dom.json`
- Start menu screenshot: `/private/tmp/ctox-business-os-inventory-20260527/start-menu.png`
- CTOX interactive QA artifact: `/private/tmp/ctox-ctox-app-interactive-qa-20260527/README.md`

Per-app screenshot inventory:

| App | Screenshot |
|---|---|
| CTOX | `/private/tmp/ctox-business-os-inventory-20260527/01-ctox.png` |
| Bugs & Features | `/private/tmp/ctox-business-os-inventory-20260527/02-bugs-and-features.png` |
| Documents | `/private/tmp/ctox-business-os-inventory-20260527/03-documents.png` |
| Knowledge | `/private/tmp/ctox-business-os-inventory-20260527/04-knowledge.png` |
| Web Research | `/private/tmp/ctox-business-os-inventory-20260527/05-web-research.png` |
| Matching | `/private/tmp/ctox-business-os-inventory-20260527/06-matching.png` |
| Conversations | `/private/tmp/ctox-business-os-inventory-20260527/07-conversations.png` |
| Outbound | `/private/tmp/ctox-business-os-inventory-20260527/08-outbound.png` |
| Einsatzplanung | `/private/tmp/ctox-business-os-inventory-20260527/09-einsatzplanung.png` |
| Spreadsheets | `/private/tmp/ctox-business-os-inventory-20260527/10-spreadsheets.png` |
| Notizen | `/private/tmp/ctox-business-os-inventory-20260527/11-notizen.png` |
| App Store | `/private/tmp/ctox-business-os-inventory-20260527/12-app-store.png` |
| Buchhaltung | `/private/tmp/ctox-business-os-inventory-20260527/13-buchhaltung.png` |
| Kalender | `/private/tmp/ctox-business-os-inventory-20260527/14-kalender.png` |
| Coding Agents | `/private/tmp/ctox-business-os-inventory-20260527/15-coding-agents.png` |
| Files | `/private/tmp/ctox-business-os-inventory-20260527/16-files.png` |
| Source Editor | `/private/tmp/ctox-business-os-inventory-20260527/17-source-editor.png` |
| App Creator | `/private/tmp/ctox-business-os-inventory-20260527/18-app-creator.png` |

Evidence provenance caveat: screenshot state, DOM state, console state, and later interactive state are not always identical. Where a review subagent found a conflict or evidence gap, this report now calls it out explicitly rather than treating all artifacts as one synchronized state.

## Executive Findings

The main problem is not missing mock data. The VPS RxDB/SQLite contains many records, but several apps render empty states because the browser runtime is failing to synchronize or materialize app collections. The global shell status also shows `CTOX ARBEITET NICHT` on every app, while some inner panels still claim `Rxdb Webrtc verbunden`, creating contradictory status messaging.

The UI system is also not yet unified. Apps render their own versions of headers, toolbars, filter bars, icon buttons, tab strips, table headers, empty states, dialogs, and resizers. Main content views can remain app-specific, but the reusable Business-OS interaction surfaces need one shared component contract.

## Data Reality vs UI

Observed non-empty collections on the VPS include:

| Collection | Count | UI Symptom |
|---|---:|---|
| `documents` | 2 | Documents shows `No documents` |
| `document_versions` | 2 | Documents has no selectable document/version |
| `spreadsheets` | 32 | Spreadsheets shows `Keine Tabellen` |
| `spreadsheet_versions` | 32 | No sheet/grid visible |
| `knowledge_items` | 65 | Knowledge shows no entries |
| `matching_objects` | 230 | Matching shows no objects |
| `matching_requirements` | 136 | Matching shows no requirements |
| `matching_results` | 312 | Matching table/list empty |
| `outbound_companies` | 365 | Outbound shows `0 Firmen` |
| `outbound_messages` | 415 | Outbound message/pipeline data not visible |
| `calendar_events` | 211 | Calendar week view shows no events |
| `calendar_bookings` | 211 | Calendar bookings panel empty |
| `notes` | 102 | Notes shows `Keine Notizen` |
| `business_module_reports` | 110 | Bugs & Features shows `Keine Reports gefunden` |
| `ctox_bug_reports` | 110 | Bugs & Features shows no reports |
| `planning_*` | populated | Einsatzplanung does render data |
| `accounting_*` | populated | Buchhaltung does render data |

For communication apps, collection availability is not yet proven by count evidence in this table. Conversations reports replication failures for `communication_accounts` and `communication_messages`, but verified DB counts for those collections were not captured here. Treat the Conversations empty state as a confirmed UI/sync failure only after those collection counts are verified.

Repeated browser-console symptoms reported by subagents:

- `WebRTC replication failed for ...`
- `failed to load module versions: Command ... wurde nicht synchronisiert`
- Affected examples include `document_versions`, `business_module_reports`, `research_runs`, `research_notes`, `desktop_files`, `ctox_runtime_settings`, `business_commands`, `desktop_icons`, `ctox_queue_tasks`, and `ctox_bug_reports`.

## Registry / Launch Surface Findings

Desktop apps observed: 18.

`CTOX`, `Bugs & Features`, `Documents`, `Knowledge`, `Web Research`, `Matching`, `Conversations`, `Outbound`, `Einsatzplanung`, `Spreadsheets`, `Notizen`, `App Store`, `Buchhaltung`, `Kalender`, `Coding Agents`, `Files`, `Source Editor`, `App Creator`.

Start menu does not match Desktop:

- `Files` is missing from Start menu.
- `Conversations` is missing from Start menu.
- `Outbound` is missing from Start menu.
- `App Creator` appears twice.
- Desktop, Start menu, and App Store are not using a single authoritative app registry.

Acceptance criterion: Desktop icons, Start menu entries, App Store entries, taskbar pins, app tabs, icon identity, app category, and launch target must all be derived from one registry record per app.

## Shared UI Contract Needed

The following interaction surfaces should be shared Business-OS components, not reimplemented per app:

- App header: app icon, title, count/status, primary action, secondary icon actions.
- Filter/search bar: consistent input, select/menu, chips, active-filter state, reset affordance.
- Icon buttons: common size, icon set, tooltip, disabled state, pressed/selected state.
- Tabs/segmented controls: same visual states and keyboard/ARIA behavior.
- Table/list header: sticky header, column labels, sort state, filter row, empty state linkage.
- Dialog/sidepanel: `role=dialog`, focus trap, Escape/close, submit validation, destructive confirmation.
- Resizers: full-height hit target, min/max bounds, visible affordance, persistent/session contract.
- Empty state: distinguish `no data`, `filtered no results`, `loading`, `sync failed`, `no selection`.
- Bottom shell bar: must not cover app content, tables, editor footers, or detail panes.

## App Reports

### 1. CTOX

Status: completed by single-app subagent.

Evidence:

- `/private/tmp/ctox-business-os-inventory-20260527/01-ctox.png`
- `/private/tmp/ctox-business-os-inventory-20260527/inventory-dom.json`
- `/private/tmp/ctox-ctox-app-interactive-qa-20260527/README.md`
- `/private/tmp/ctox-ctox-app-interactive-qa-20260527/01-ctox-initial.png`
- `/private/tmp/ctox-ctox-app-interactive-qa-20260527/10-final-ctox.png`
- `/private/tmp/ctox-ctox-app-interactive-qa-20260527/07-flow-zoom-report.json`
- `/private/tmp/ctox-ctox-app-interactive-qa-20260527/08-flow-node-timeline-report.json`
- `/private/tmp/ctox-ctox-app-interactive-qa-20260527/09-resize-report.json`
- `/private/tmp/ctox-ctox-app-interactive-qa-20260527/10-diagnostics.json`

Tested interactions:

- Opened CTOX from Desktop.
- Clicked `Web Stack aktualisieren`.
- Opened `Source von CTOX öffnen`.
- Tested flow zoom `+`, `-`, `Reset`.
- Clicked flow nodes `Running` and `Passed`.
- Inspected timeline slider.
- Dragged task-column resize handle.
- Chat/FAB was only visually inspected.

Bugs and risks:

- Global shell shows `CTOX ARBEITET NICHT`.
- Status is contradictory: center says `Rxdb Webrtc verbunden`, left panel continues `Verbindung zum CTOX-Backend wird hergestellt...`.
- `Web Stack aktualisieren` is clickable but remains `Web Stack 0/0 konfiguriert`, `Quellen: 0`.
- Source Editor opens but stays at `Lade Source...` and reports projection/sync failure.
- Flow zoom `+` works from `100%` to `112%`; zoom `-` did not reduce from `112%`; `Reset` works.
- Flow nodes are clickable but do not change timeline/detail state.
- Timeline slider has `min=0`, `max=0`, `value=0`, so it is visually present but functionally inert.
- Task resize handle can be dragged but measured left/main widths did not change.
- Console has WebRTC failures for `desktop_files`, `ctox_runtime_settings`, `business_commands`, `desktop_icons`, `ctox_queue_tasks`, `ctox_bug_reports`.

UI/UX issues:

- Diagnostic, task, and live-flow areas compete instead of following a standard app-header/content structure.
- Several controls look interactive but produce no state change.
- Bottom bar and FABs compete with app viewport.
- Global warning dominates the app and is not reconciled with inner `connected` status.
- The flow canvas is wider than the visible app area; right-side stages are partially clipped and the bottom shell bar overlaps the timeline/zoom area.
- `Web Stack projection is not available in RxDB` is visible in the app, but this is not surfaced as a structured app-level diagnostic state.

Acceptance criteria:

- `Web Stack aktualisieren` either refreshes real projection data or shows an actionable sync error.
- CTOX status model has one consistent source of truth for global warning, backend connection, and RxDB/WebRTC.
- Flow zoom controls all work symmetrically.
- Flow nodes update selected state/detail panel or are non-interactive.
- Timeline is hidden or disabled unless it has a valid range.
- Resize changes pane width and respects min/max bounds.
- Source Editor opens with source content or a specific collection/command sync error.
- No console errors for core CTOX collections in normal load.

### 2. Bugs & Features

Status: completed by single-app subagent.

Evidence:

- `/private/tmp/ctox-business-os-inventory-20260527/02-bugs-and-features.png`
- `/private/tmp/ctox-business-os-inventory-20260527/interactive/02-bugs-features-initial.png`
- `/private/tmp/ctox-business-os-inventory-20260527/interactive/02-bugs-features-after.png`

Tested interactions:

- Opened from Desktop; app route `#reports`.
- Opened from Start menu; existing app focused rather than duplicated.
- Clicked `Aktualisieren`.
- Entered search text.
- Changed type filter: `Alle Typen`, `Bugs`, `Features`.
- Changed status filter: `Alle Status`, `Offen`, `In Arbeit`, `Erledigt`, `Blockiert`.
- Report selection not testable because no reports were visible.
- Inspected detail empty state.
- Dragged column resizer; moved from x=333 to x=417 and back.
- Source button presence verified, not clicked.
- `Bug oder Feature an CTOX melden` was not opened/tested; report creation remains unverified.

Bugs and risks:

- Report list always shows `Keine Reports gefunden`.
- This blocks report selection, detail view, and report-specific workflows.
- `Aktualisieren` does not surface data or an actionable error.
- Console includes:
  - `WebRTC replication failed for business_module_reports`
  - `failed to load module versions: Command ... wurde nicht synchronisiert`
  - more failures for `business_module_releases`, `desktop_layout`, `ctox_runtime_settings`.

UI/UX issues:

- Header/toolbar is not standardized: global appbar, inner pane header, and `CTOX INTAKE` compete.
- Refresh is a text button while Source is an icon button; hierarchy is unclear.
- Search, type filter, and status filter are cramped and wrap poorly.
- Native inputs/selects look browser-default.
- Detail pane is huge and empty, with no distinction between `no report selected`, `no data`, and `sync failed`.
- `Waehle` appears instead of German `Wähle`.
- App tab truncates label as `Bugs & ...`.
- Screenshot evidence shows the search and select controls visually collide/truncate in the left pane; this is not only cramped, it is below usable toolbar quality.

Acceptance criteria:

- Reports from `business_module_reports` / `ctox_bug_reports` render.
- Empty state appears only when the collection is truly empty or filters remove all results.
- Refresh reloads data or presents visible diagnostics.
- Search and filters change visible report rows.
- Selecting a report fills detail pane with type, status, title, description, source/time, and actions.
- Header and toolbar use shared Business-OS components.
- Filter bar does not wrap or compress below usable widths.
- Console has no report/module-version sync errors.

### 3. Documents

Status: completed by single-app subagent.

Evidence:

- `/private/tmp/ctox-business-os-inventory-20260527/03-documents.png`
- `/private/tmp/ctox-business-os-inventory-20260527/interactive/03-documents-initial.png`

Tested interactions:

- Opened from Desktop; route `#documents`.
- Search field `Dokumente suchen` accepts input.
- Sort changed through options such as `Neueste zuerst`, `Titel A-Z`.
- Status filter changed through `Alle`, `Imported`, `Draft`, `Review`, `Final`.
- Tag filter changed through `Alle Tags`, `Ohne Tags`.
- Export verified disabled without selected document.
- Import button opened sidepanel with file, import mode, tags, prompt, runbook. No upload executed.
- Create button opened `Neues Dokument` sidepanel. No create submit executed.
- Runbook selection works, e.g. `Zusammenfassen`.
- Prompt field accepts input.
- `Runbook starten` stays disabled without selected document.
- Detail empty state inspected.
- Left/middle/right separators exist with `role=separator`; resize works basically but handles are poorly placed.

Bugs and risks:

- UI shows `No documents`; `docRows: []`.
- This contradicts known non-empty `documents` and `document_versions` collections.
- Console includes:
  - `WebRTC replication failed for document_versions`
  - `WebRTC replication failed for desktop_files`
  - `failed to load module versions: Command ... wurde nicht synchronisiert`
  - additional WebRTC failures for Desktop/Business-OS collections.
- Likely cause: Documents app receives an empty or failed RxDB/command projection.
- Core write flows were opened but not submitted; actual import/create persistence, validation after submit, and resulting document rendering remain unverified.

UI/UX issues:

- Empty state is false when mock data exists.
- Filter row is cramped; sort/status/tag controls are about 60-70px wide and barely readable.
- The left filter controls are visibly too narrow in the screenshot; labels such as `Neueste` and `Alle Tags` are clipped, making the filter state hard to read.
- Header controls are icon-only and visually equal despite different importance.
- Import/Create sidepanels lack proper `role=dialog` / `aria-modal`.
- Import submit is active even without a selected file.
- Create submit is active with default title and empty prompt; if valid, the UI must explain why.
- Resizers are only visibly/comfortably grabbable near the bottom, not full pane height.
- Right runbook pane becomes too narrow under resize.

Acceptance criteria:

- Existing `documents` / `document_versions` render in the list.
- Empty state only appears when post-filter collection is empty.
- Search/sort/status/tag filters visibly change rows.
- Export enables after document selection.
- Runbook starts only with valid selected document and validated prompt state.
- Import/Create sidepanels have dialog semantics, focus handling, Escape/Close, and disabled submit for missing required data.
- Resizers are full-height, bounded, and do not push controls offscreen.
- Header/toolbar uses shared component: title, count, primary action, secondary actions, filter bar, status.

### 4. Knowledge

Status: completed by single-app subagent.

Evidence:

- `/private/tmp/ctox-business-os-inventory-20260527/04-knowledge.png`
- `/private/tmp/ctox-business-os-inventory-20260527/interactive/04-knowledge-initial.png`
- `/private/tmp/ctox-business-os-inventory-20260527/interactive/04-knowledge-after.png`

Tested interactions:

- Opened from Desktop.
- Search tested with `skill` and `zz-no-hit`, then cleared.
- Source tabs tested: `User`, `System`, `Alle`.
- View tabs tested: `Skill`, `Runbooks`, `Data`.
- Buttons opened or were inspected up to non-destructive dialog state: `Knowledge Book erstellen`, `Knowledge konfigurieren`, `Knowledge Book importieren`, `Knowledge Books exportieren`.
- `Bearbeiten` clicked with no selection.
- Resize handle tested.
- Detail empty state inspected.
- Console warning/error checked.

Bugs and risks:

- No Knowledge entries show in `User`, `System`, or `Alle`.
- Search only filters the empty state.
- Source tabs visually respond.
- Detail tabs are inconsistent: clicking `Runbooks` and `Data` is possible, but `Skill` can remain `[pressed]`.
- `Data` can become the active view even though no Knowledge entry is selected; the UI then shows an entry-specific empty table state, which incorrectly implies a selected entry exists.
- `Bearbeiten` is enabled with no selected Knowledge item and does nothing.
- Create/Import/Export/Config dialogs expose enabled primary submit buttons with empty required fields:
  - `Erstellen lassen`
  - `Import starten`
  - `Export starten`
  - `An CTOX geben`
- Resize can be dragged over/near the left edge; layout bounds are weak.
- Console shows sync/projection errors including WebRTC replication failures and failed module-version command sync.
- Create/Edit/Import/Export/Config were not submitted; persistence and post-submit validation remain unverified.

UI/UX issues:

- Icon-only toolbar lacks visible hierarchy/tooltips in main flow.
- Empty state does not distinguish no data from failed sync.
- Bottom bar overlays lower app area.
- Lists/detail/resizers are app-specific rather than shared components.
- Dialog validation is too permissive.

Acceptance criteria:

- Existing Knowledge data appears in `Alle`; `User` and `System` filter correctly.
- Search shows matches and a true no-results state only when appropriate.
- Selecting an entry fills the detail pane.
- `Skill`, `Runbooks`, and `Data` either switch visible content or are disabled without selection.
- `Bearbeiten` disabled without selection.
- Create/Import/Export/Config dialogs validate required fields before enabling submit.
- Resize has min/max bounds.
- No console sync/projection errors on normal open.

### 5. Web Research

Status: completed by single-app subagent.

Evidence:

- `/private/tmp/ctox-business-os-inventory-20260527/05-web-research.png`
- `/private/tmp/ctox-business-os-inventory-20260527/interactive/05-web-research-initial.png`
- `/private/tmp/ctox-business-os-inventory-20260527/interactive/05-web-research-after.png`

Tested interactions:

- Opened from Desktop; route `#research`.
- Clicked `Daten neu laden`.
- Opened `Research anlegen`.
- Filled form non-destructively, then cancelled.
- Checked `Research starten` disabled/enabled state.
- Inspected left task list, ranking, center dashboard, right context panel.
- Dragged left and right resizers.
- Checked scroll areas.
- Console warning/error checked.

Bugs and risks:

- No Research/Knowledge data visible:
  - `Keine Knowledge-basierte Research-Aufgabe gefunden`
  - `Noch keine Quellen geladen`
  - `Keine Domain`
  - `Sources 0`
- `Daten neu laden` leaves state unchanged.
- Console includes:
  - `WebRTC replication failed for research_runs`
  - `WebRTC replication failed for research_notes`
  - `WebRTC replication failed for business_module_catalog`
  - `failed to load module versions: Command ... wurde nicht synchronisiert`
- `Research starten` is technically disabled but visually looks active: `opacity: 1`, `cursor: pointer`, `tabIndex: 0`.
- The screenshot shows `Research starten` styled as an active primary action even while no valid task/domain exists; this should be treated as a visual-state bug, not only a disabled-state implementation bug.
- Create dialog allows empty submit state: `Anlegen` enabled with empty form.
- Knowledge Domain is exposed as free input/combobox-like field, not a real populated selection/autocomplete from Knowledge data.

UI/UX issues:

- Header/toolbar icon-only controls do not follow a clear standard.
- Reload/Create live in left panel while `Research starten` is in a different panel; action hierarchy is inconsistent.
- Disabled state is not visually distinct.
- Empty states lack diagnostic distinction between no data, no selection, loading, and sync failure.
- Bottom chat bar reduces usable workspace.
- Resizer affordance is weak.

Acceptance criteria:

- Existing Knowledge domains and Research dashboards appear.
- `Daten neu laden` either updates visible data or displays collection/sync diagnostics.
- `Research anlegen` validates title, domain, and task before enabling submit.
- Knowledge Domain is a real selection/autocomplete backed by Knowledge data.
- `Research starten` only enables for a valid selected research task and visually looks disabled when disabled.
- Left list, ranking, center dashboard, and right details update consistently after selection.
- No console errors for `research_runs`, `research_notes`, or `business_module_catalog`.

### 6. Matching

Status: completed by single-app subagent.

Evidence:

- `/private/tmp/ctox-business-os-inventory-20260527/06-matching.png`
- `/private/tmp/ctox-business-os-inventory-20260527/interactive/06-matching-initial.png`
- `/private/tmp/ctox-business-os-inventory-20260527/interactive/06-matching-after.png`

Tested interactions:

- Opened via Desktop icon.
- Filled requirements search with `crm`, then cleared via keyboard selection/backspace.
- Changed requirements sort to `Gruppieren: Bereiche` and `Neueste zuerst`.
- Filled object search with `crm`, then cleared.
- Changed object sorting to `Name` and back to `Bester Match`.
- Clicked Matches `Liste` / `Tabelle` toggle.
- Inspected match filter.
- Tried requirements/matches/objects config/import/export buttons through normal UI clicks without upload/write.
- Dragged both column resizers.
- Checked browser console.
- Read VPS SQLite counts for matching collections.

Bugs and risks:

- P0: SQLite has Matching data, but UI shows none:
  - `ctox_business_os__matching_requirements__v1`: 136
  - `ctox_business_os__matching_objects__v1`: 230
  - `ctox_business_os__matching_results__v1`: 312
- UI still shows:
  - `Keine Anforderungen in der Datenbank gefunden.`
  - `Keine Matches im aktuellen Matrix-Filter.`
  - `Keine Objekte in der Datenbank gefunden.`
- P0: HTTP Pull returns 0 for visible unversioned collections:
  - `/api/business-os/rxdb/pull?collection=matching_requirements`
  - `/api/business-os/rxdb/pull?collection=matching_objects`
  - `/api/business-os/rxdb/pull?collection=matching_results`
- Strong likely cause: records exist in `v1`, but Web/API/UI replication reads effectively empty `v0` or fails the schema/replication handshake.
- P0: Console shows replication failures for `matching_requirements`, `matching_objects`, `matching_results`.
- P1: Middle `Matches` column has width `0`; match search/filter exist in DOM but are invisible.
- The general screenshot shows the middle `Matches` column visible, but earlier measured interaction data reported a collapsed/0-width state for some controls/resizer targets. Distinguish visible pane layout from zero-width or inaccessible controls.
- P1: Both resizers are at `x = -5`, overlapping at the left edge.
- P1: Config/Import/Export icon buttons have width `0`, so normal UI click targets are invisible/unusable.
- P1: `Liste` / `Tabelle` toggle does not visibly switch; `Tabelle` remains active after clicking `Liste`.
- P2: Programmatic `fill("")` did not reliably clear fields; keyboard select/backspace worked.
- P2: Detail selection could not be tested because no rows render.

UI/UX issues:

- Three-column layout collapses and makes the app look like two broken panes.
- Header/toolbar icon buttons are not robust clickable surfaces.
- Invisible 0px buttons break affordance and accessibility.
- Empty states are misleading because data exists.
- Global `CTOX ARBEITET NICHT` aligns with the data break but gives no collection-specific diagnosis.

Acceptance criteria:

- UI renders the existing 136 requirements, 230 objects, and 312 match results.
- API/Pull and browser RxDB read the same active schema version as SQLite (`v1` for these collections).
- No console errors for matching collections.
- Three columns keep stable minimum widths; middle column never collapses to `0px`.
- Resizers sit between columns and change widths reproducibly.
- List/Table toggle updates active state and visible representation.
- Button labels/toggles such as `Liste` and `Tabelle` render as distinct segmented controls with clear active state, spacing, and keyboard focus behavior.
- Config/Import/Export buttons have at least 28x28px click targets and open dialogs without immediate side effects.
- Empty states appear only when the relevant collection is truly empty.

### 7. Conversations

Status: completed by single-app subagent.

Tested interactions:

- Opened from Desktop.
- Search tested with `termin`, `zz-no-hit`, then cleared.
- Channel chips tested: `Alle Channels`, `WhatsApp`, `E-Mail`, `Jami`, `MS Teams`, `Meeting`.
- Account filter inspected.
- Direction filter tested: `Eingang`, `Ausgang`, back to `Alle Richtungen`.
- Time filter tested: `Heute`, `Letzte 7 Tage`, `Letzte 30 Tage`, back to `Alle Zeiträume`.
- Conversation list, timeline/detail pane, and right context column inspected.
- Left and right resize handles dragged.
- Console warning/error checked.

Bugs and risks:

- Communication data is not visible. App stays at `Keine Konversationen`.
- Inventory DOM marks `resize=false`; no explicit app resize separators were detected in the captured artifact, even though the pane boundaries are visually present. The resize result is therefore inconclusive unless a real app-owned handle is identified in a focused run.
- `Konversationen` listbox has 0 options.
- Detail pane remains at `Keine Konversation ausgewählt`; timeline cannot be tested because there is no selectable conversation.
- Right context column remains empty: `Kontaktdaten und verknüpfte Datensätze erscheinen hier.`
- Account filter only contains `Alle Accounts`; no real communication accounts are available.
- Channel chips visually react but do not expose `aria-pressed`.
- Search field accepts text but only filters empty state; hit vs no-hit is indistinguishable.
- Console includes:
  - `WebRTC replication failed for communication_accounts`
  - `WebRTC replication failed for communication_messages`
  - additional Business-OS replication failures and failed module-version sync.

UI/UX issues:

- Global `CTOX ARBEITET NICHT` remains visible.
- Bottom chat/date bar overlaps app area.
- Header is minimal and not equivalent to a shared Business-OS app chrome.
- Search, chips, and selects look app-specific and cramped.
- The screenshot shows the search placeholder clipped (`Kontakt, Inhalt, Accou...`).
- Empty state is misleading when backend/DB communication data exists.
- Right context column consumes significant width without a useful diagnostic state.
- The right context column occupies a large blank area without a typed diagnostic state.
- Channel chips use colored dots rather than standardized channel icons.

Acceptance criteria:

- `communication_accounts` and `communication_messages` replicate without console errors.
- Existing conversations appear in the list.
- Account filter lists real accounts.
- Channel, direction, time, and search filters reduce visible conversations correctly.
- Null-result empty state appears only for real zero-result filter state.
- Selecting a conversation fills timeline/detail pane.
- Right column shows contact, account, linked records, and metadata.
- Channel chips expose `aria-pressed` or equivalent semantics.
- App header uses shared Business-OS header/toolbar component.
- Bottom bar does not overlap app content.
- Resize handles maintain readable minimum widths.

### 8. Outbound

Status: completed by single-app subagent.

Tested interactions:

- Opened from Desktop.
- Clicked campaign selection.
- Opened/closed Outreach toggle.
- Set status filter `Offen`, tag filter `Entscheider`, then reset.
- Toggled `Kompakte Ansicht`.
- Toggled `Versteckte Firmen`.
- Clicked pipeline/stage cards: `Input`, `Research offen`, `Unternehmen qualifiziert`.
- Sorted table by `Unternehmen`.
- Used column filters for `Unternehmen` and `Status`.
- Opened `Research-Felder einstellen` dialog without saving.
- Opened `Importjob anlegen` dialog without starting/uploading.
- Opened Campaign edit state without saving.
- Opened Campaign delete confirmation and cancelled.
- Tested resize handle.
- Checked console warning/error.

Bugs and risks:

- Only one campaign renders: `Outbound Firmenqualifizierung`.
- Existing outbound company data is not visible:
  - UI shows `0 Importjobs · 0 Firmen · 34 Spalten`.
  - UI shows `Noch keine Unternehmen in dieser Campaign`.
- Pipeline counts are all `0 / 0`; research/pipeline actions disabled.
- Compact mode works: 34 columns reduce to 11 and back.
- Table filters and status/tag filters are operable but not meaningfully testable because no rows render.
- Import dialog opens, but `Importjob starten` is enabled with empty required fields.
- Campaign edit is inline, not clearly a dialog; `Speichern` immediately enabled.
- Delete confirmation is present and was cancelled.
- Research fields dialog does not close on Escape; only `x` works.
- Resize drag has no visible effect: navigation stayed `360px`, workbench `882px`.
- Console includes:
  - `WebRTC replication failed for outbound_pipeline_items`
  - `selected campaign knowledge setup failed`
  - `Command ... wurde nicht synchronisiert`
  - additional Business-OS replication failures.
- Captured screenshot evidence has `hasSearch=false`; this report should not imply a global Outbound search was tested. Only status/tag filters and table column filters were tested.

UI/UX issues:

- Header/action vocabulary is inconsistent: `+`, icon-only runbook/edit/delete, text `Import`, `Outreach` toggle, and shell icons use different patterns.
- Status/tag selects lack visible labels and ARIA labels.
- `Versteckte Firmen 0` looks button-like but lacks clear active/pressed state.
- Disabled pipeline action labels say `Abgeschlossen`, but the real state is more like `no open items` or `no data`.
- Outreach toggle can briefly leave normal Outbound workbench content missing, which feels unstable.
- Bottom bar competes with viewport.
- The screenshot shows a large unused/right workbench area while the left campaign/table area is cramped and partly covered by the bottom shell bar.
- `Research Details` only shows a minimal `Details` heading and close button, not usable company/research context.

Acceptance criteria:

- Existing `outbound_companies` render as rows in selected campaign.
- Campaign counters, pipeline stages, and table rows match DB/projection.
- Filters, sorting, compact mode, and hidden-company toggle affect real rows.
- Selecting a company opens detail pane with research/outreach/lead context.
- Import/Edit validate required fields; submit disabled until valid.
- Delete remains confirmation-gated.
- Research fields dialog closes via `x`, Escape, and cancel consistently.
- Resize visibly changes panes within min/max widths.
- Console has no Outbound/projection errors on normal open.

### 9. Einsatzplanung

Status: completed by single-app subagent.

Tested interactions:

- Opened from Desktop; route `#shiftflow`.
- Employee search: `Lisa`, `zz-no-hit`, then cleared via keyboard.
- Department filter: `Küche`, `Service`, back to `Alle Abteilungen`.
- Tabs: `Dienstplan`, `Zeiterfassung`, `Abrechnung`.
- Week navigation: next week, then previous week.
- Timeline toggle: `Mitarbeiter-Timeline`, `Projekt-Timeline`.
- Selected shift `Küchenleitung Catering`.
- Opened detail/edit drawer and closed with `Abbrechen` without saving.
- Clicked `Dienstplan veröffentlichen`.
- Checked scroll areas: schedule grid, left sidebar, time tracking, payroll.
- Dragged left column resizer.
- Console warning/error checked.

Results:

- Planning data renders: 4 employees, 3 projects, several shifts, one time-tracking approval, two payroll rows.
- This app is a positive control in this audit: unlike most apps, it renders domain data and supports meaningful app-specific interactions. Remaining issues are primarily action safety, ARIA state, pane/drawer layout, and shell overlap.
- Department filter works and filters left list plus schedule.
- Week navigation works: KW 22 has shifts, KW 23 shows empty plan cells.
- Timeline toggle changes view from employee to project.
- Tabs change content correctly.
- Shift selection opens a right edit drawer with employee, project, date, department, start, end, notes, `Abbrechen`, `Löschen`, `Speichern`.
- Left resize works: left pane from about `300px` to `364px`, center shrinks.
- Schedule grid and left sidebar scroll.

Bugs and risks:

- `Dienstplan veröffentlichen` is enabled and looks productive but is inert: no confirmation, toast, or visible publish state. Source reportedly has no click handler bound for `btnPublishSchedule`.
- Employee search filters only the left employee list, not the schedule. This is likely incomplete or unclear.
- `fill("")` did not clear search; keyboard select/backspace was needed.
- Tab ARIA is wrong: after switching to `Zeiterfassung` or `Abrechnung`, `Dienstplan` remains `aria-pressed="true"` while visual active class changes.
- Timeline toggles lack `aria-pressed`.
- Right standard pane is broken/invisible: `.shiftflow-right-pane` and right resizer measure `0x0`; app seems to contain a right pane in HTML but grid only defines two columns.
- A bottom drawer `Schicht-Details` exists but only shows a header and competes with global chat/date bar.
- Shift selection opens edit/delete/save immediately; a safe read-only detail view should come first.
- Console still has shell/sync issues for `desktop_notifications`, `channel_pairing_state`, `desktop_files`, `ctox_queue_tasks`, `ctox_runtime_settings`, `desktop_icons`, and module-version sync. No planning-specific replication error was seen.

UI/UX issues:

- Functionally stronger than many apps, but right companion/detail area is unusable.
- Bottom bar and bottom drawer overlap/compete.
- Publish CTA implies final action without protection or effect.
- Search/filter behavior is ambiguous: employee search and department filter affect different scopes.
- Shift cards are clickable but not semantically buttons.
- Publish CTA uses emoji rather than standardized icon.
- App-specific tabs/toggles should use shared components so visual and ARIA states cannot diverge.

Acceptance criteria:

- Planning data renders from `planning_*` without console errors.
- Employee search scope is clear and either filters schedule too or explicitly only filters sidebar.
- Search field clears reliably through normal empty value.
- Department filter semantics are explicit.
- `Dienstplan veröffentlichen` has a real handler with confirmation or is disabled/removed.
- Tabs and timeline toggles expose correct ARIA/role states.
- Right pane/resizer is visible and functional or removed.
- Shift selection opens a safe detail view first; edit/delete/save are secondary.
- Resize has min/max bounds and persistence/session behavior is clear.
- Bottom bar does not cover app drawers or work areas.

### 10. Spreadsheets

Status: completed by single-app subagent.

Tested interactions:

- Opened from Desktop.
- Search `Tabellen suchen` with `budget`.
- Sort changed to `Titel A-Z`.
- Status filter changed to `Imported`.
- Tag filter changed to `Ohne Tags`.
- Filters reset to defaults.
- Clicked `Neue Tabelle erstellen`.
- Inspected Sheet/Grid: demo grid with `Sheet1`, columns `A-F`, rows `1-10`.
- Clicked cell `Premium Widget`.
- Tested runbook cards: `Finanzielle Risikoanalyse`, `Formeln auditieren`, `Tabelle zusammenfassen`.
- Tested prompt field and `Senden` disabled/enabled state.
- Opened `Tabelle importieren` panel, no upload, closed with `Abbrechen`.
- Dragged left and right resize separators.
- Console warning/error checked.

Bugs and risks:

- Existing Spreadsheet data is not visible initially; left panel shows `Keine Tabellen`.
- The inventory screenshot confirms the initial state: left list `Keine Tabellen`, center `Keine Tabelle ausgewählt`, while runbook prompts are visible on the right. This is distinct from the later generated demo table state after `Neue Tabelle erstellen`.
- Search/sort/status/tag filters technically react but cannot visibly affect list because no tables render.
- P0/P1 product risk: `Neue Tabelle erstellen` immediately creates/persists a demo table and marks it `Gespeichert`; it does not open a safe dialog/preview.
- After creating a table, main grid appears but left list still says `Keine Tabellen`.
- Export becomes enabled after generated table exists.
- Import panel has `Importieren` enabled despite no file selected.
- `Senden` correctly disabled without a table, enabled with selected/generated table.
- Prompt text can accidentally land in a grid cell after grid focus unless scoped to right-pane prompt. This is a focus/editor risk.
- Resize is unstable: left separator stayed at `x=256`; right separator jumped to `x=1276`.
- Console includes:
  - `WebRTC replication failed for business_module_catalog`
  - `WebRTC replication failed for desktop_files`
  - `WebRTC replication failed for ctox_runtime_settings`
  - `WebRTC replication failed for desktop_layout`
  - `failed to load module versions: Command ... wurde nicht synchronisiert`

UI/UX issues:

- Empty state is misleading when data exists or a table is open.
- Filter bar is too cramped and uses native-looking controls.
- Resizers are only visible/grabbable near bottom, not full pane height.
- Bottom chat/date bar competes with workspace.
- Sheet selection/edit states are not visually clear enough.
- German UI contains ASCII transliterations such as `ausgewaehlte`, `auffaellige`, `Ausreisser`.
- Create/Import/Export icon actions are visually equal despite very different consequences.

Acceptance criteria:

- Existing spreadsheet mock data appears in the left list on open.
- Empty state only appears when post-filter collection is truly empty.
- Search/sort/status/tag filters visibly affect table list.
- `Neue Tabelle erstellen` opens dialog/confirm/preview, or is clearly labeled as immediate persistence.
- Newly created/selected table appears in the left list.
- Import submit disabled until a file is selected.
- Prompt input cannot write into grid cells.
- Resizers are full-height, bounded, and move predictably.
- Console has no sync/projection errors for relevant collections.

### 11. Notizen

Status: completed by single-app subagent.

Tested interactions:

- Opened from Desktop; route `#notes`.
- Search with `meeting`, `zz-no-hit`, then cleared by keyboard.
- Sidebar navigation: `Favoriten`, `Papierkorb`, back to `Notizen`.
- Notebook/tag meta controls: `Tags verwalten`, `Kein Notizbuch`.
- `Neue Notiz` clicked up to editor/draft without typing content.
- Sort/filter popover opened; `Kompakt-Modus` tested.
- Editor toolbar format/callout buttons inspected.
- Lock/Favorite/Delete enabled/risk states inspected, not mutating clicked.
- App `Sperren` clicked and visible effect checked.
- Both vertical resizers dragged.
- List/editor scroll checked.
- Console warning/error checked.

Bugs and risks:

- Existing Notes data does not appear: UI shows `Notizen 0`, `Favoriten 0`, `Papierkorb 0`, `Keine Notizen`.
- At 1280px width the primary `Neue Notiz` action is visibly clipped at the right edge; this is a responsive/layout defect independent of the missing data issue.
- Search field works but only filters empty state; `fill("")` did not clear, keyboard select/backspace did.
- `Favoriten` and `Papierkorb` switch visually but remain empty.
- P1/P0 product risk: `Neue Notiz` immediately creates/saves a draft. After click, app shows `Notizen 1`, `HEUTE / Neue Notiz`, editor `Gespeichert`.
- `Tags verwalten` opens dropdown with `Keine Tags erstellt`.
- `Kein Notizbuch` dropdown opens but outside click and Escape do not close it; only button click closes.
- Sort/filter popover opens; compact mode toggles list class but meaningful behavior is limited with only empty auto-draft.
- Format/callout dropdowns exist in DOM but remain `hidden=true` after click.
- Resizers are visible/full-height but dragging does not change column widths.
- `Notiz löschen` is enabled after draft and appears to move directly to trash without first-step confirmation.
- `Favorit umschalten` is enabled and would save immediately.
- `Notiz verschlüsseln` is enabled and enters password/save flow.
- App `Sperren` showed no visible effect; lock screen stayed hidden.
- Console had 17 warn/error entries, including failed module-version sync and WebRTC failures for `ctox_runtime_settings`, `desktop_files`, `desktop_layout`, `business_commands`, `desktop_notifications`.

UI/UX issues:

- Empty state is false if mock notes should exist.
- Sidebar categories exist but real notebook/tag navigation is empty.
- Bottom bar/FABs overlay lower editor footer.
- The editor footer/status area is partially covered by the global bottom shell bar, so word count, sync state, and lower editor controls are not reliably usable.
- Dropdown closing behavior is inconsistent.
- Toolbar is overloaded and partly inert.
- Appbar, Notesnook header, list header, and editor header feel like separate systems.
- Primary action `Neue Notiz` sits in editor header while search/filter lives elsewhere.

Acceptance criteria:

- Existing Notes mock data visible on open.
- Empty state only when filtered collection is truly empty.
- Search filters visible notes and clears reliably.
- Notebook/tag navigation shows real notebooks/tags and counts.
- `Neue Notiz` has draft/undo/cancel semantics before persistence, or is clearly labeled as instant create.
- Favorite/Lock/Delete have risk-appropriate disabled/confirmation/undo behavior.
- Dropdowns close via button, outside click, and Escape.
- Editor toolbar actions visibly work or are disabled.
- Resizers change widths within min/max bounds.
- No sync/projection console errors during normal operation.

### 12. App Store

Status: completed by single-app subagent.

Tested interactions:

- Opened from Desktop.
- Catalog search: `buch`, `zz-no-hit`, then cleared.
- Categories tested: `Marketplace`, `Templates`, `Installed`, `Starter`, `Local Modules`, `System`, `All`.
- Grid/List toggle tested.
- `Refresh GitHub` tested once enabled.
- Opened `Details` for `Buchhaltung`.
- Tested safe `Öffnen` on App Store's own entry.
- GitHub buttons inspected but not clicked to avoid external navigation.
- `App von Scratch erstellen` followed to form without saving.
- Compared visible Start menu / Desktop / App Store registry.
- Console warning/error checked.

Results:

- App Store opens under `#app-store`.
- Search works: `buch` reduces to `1 App`; `zz-no-hit` shows `0 Apps`.
- Categories visually react and change lists.
- Grid/List toggle changes active state.
- `Refresh GitHub` works when enabled; after refresh it reports `18 GitHub Module gefunden`.
- `Details` for `Buchhaltung` shows metadata and collections.
- `Öffnen` on App Store entry remains internal to `#app-store`.
- `App von Scratch erstellen` navigates directly to `#creator` and shows a form.
- Screenshot artifact shows `GH Marketplace 18`, `Templates 1`, `Installed 0`, while visible app cards are marked `Installed`. The later observed `Installed 17` and `All 36` state is post-refresh/post-category interaction state.

Bugs and risks:

- Counts are unstable: initially `GH Marketplace 0`, `Installed 0`, `All 18`; after discovery `GH Marketplace 18`, `Installed 17`, `All 36`.
- App Store has a registry contradiction inside its own UI: category count `Installed 0` conflicts with cards displaying an `Installed` badge.
- No clear loading/sync state explains count changes.
- `All Everything 36` contains duplicates: GitHub/Installed/local/system entries for the same apps.
- Details drawer initially shows placeholder `Application Title` even without real selected app.
- `App von Scratch erstellen` leaves App Store directly for `#creator` without a clear dialog/back context.
- Console includes sync/projection issues for `desktop_files`, `desktop_icons`, `ctox_runtime_settings`, `desktop_notifications`, and failed module-version sync.

UI/UX issues:

- Start menu, Desktop, and App Store are not the same visible registry:
  - Start menu lacks `Files`, `Conversations`, `Outbound`.
  - `App Creator` appears twice in Start menu.
  - App Store shows additional/different labels such as `Browser`, `Desktop`, `Notes` while Desktop has `Notizen`, `Files`, `Source Editor`.
- `Refresh GitHub` can be disabled without explanation.
- `GitHub`, `Öffnen`, and `Details` buttons look similarly weighted despite external vs internal behavior.
- Grid/List buttons are icon-only with small 28x20px targets.
- Category icons mix text/symbols (`GH`, `+`, check, star, etc.) instead of a standard icon set.
- App cards pack status, category, provenance, and actions too tightly.

Acceptance criteria:

- One app registry is source of truth for Desktop, Start menu, App Store, taskbar, app tabs, icon, route, category, and install state.
- No duplicate app entries in `All` unless explicitly grouped by Marketplace vs Installed.
- GitHub discovery has visible loading/sync state.
- Details drawer shows real selection or a clear empty state.
- External GitHub actions are visually marked as external.
- `App von Scratch erstellen` is either an App Store dialog or a deliberate navigation with clear return path.
- No console errors for module versions, desktop registry, or RxDB/WebRTC sync during normal open/refresh.

### 13. Buchhaltung

Status: completed by single-app subagent.

Tested interactions:

- Opened app at `#buchhaltung`.
- Navigation tested: `Kontenrahmen`, `Journal & Hauptbuch`, `Belege`, `Bankabgleich`, `Reisekosten`, `Fahrtenbuch`, `Bilanz / GuV / UStVA`, `Anlagenspiegel`.
- Kontenrahmen search with `Bank`.
- Clicked table rows in Kontenrahmen and Anlagenspiegel.
- Companion tabs tested: `Beleg-Vorschau`, `CTOX AI Agent`.
- `Kontenrahmen neu initialisieren` not executed; DOM/code path checked as destructive after `confirm(...)`.
- `Neue manuelle Buchung` opened bottom drawer; closed it.
- Report tabs tested: `HGB-Bilanz`, `HGB-GuV`, `Umsatzsteuer-Voranmeldung`, `DATEV EXTF-Export`.
- Scrolled Kontenrahmen.
- Tested left/middle/right resize.
- Console warning/error checked.

Results:

- Accounting data partially visible:
  - Kontenrahmen shows 42 accounts.
  - Anlagenspiegel shows 3 assets.
  - Journal, receipts, bank reconciliation, travel expenses, mileage log are empty.
- Kontenrahmen search works for `1200 Bank (Girokonto)`.
- Navigation works.
- Right companion switches between preview and AI agent.
- Booking editor opens/closes.

Bugs and risks:

- Global `CTOX ARBEITET NICHT` remains.
- Console still shows sync/projection failures for `ctox_runtime_settings`, `desktop_notifications`, `desktop_files`, `channel_pairing_state`, `business_module_catalog`, plus failed module-version sync.
- Table row click produces no clear selection state: no `aria-selected`, no visible highlight/detail binding.
- Screenshot evidence shows the bottom booking editor/drawer is partly occluded by the global bottom shell bar. This is stronger than visual competition: the editor area can become practically inaccessible at 720px height.
- Evidence gap: the screenshot inventory only corroborates the `Kontenrahmen` view. Claims about Journal, Belege, Bankabgleich, Reisekosten, Fahrtenbuch, reports, and Anlagenspiegel come from the dedicated interactive subagent run, not from the static screenshot.
- Anlagenspiegel `Plan` button has no visible effect.
- Search/filter state can diverge: after navigation, search value `Bank` remained once while all 42 rows were visible.
- Kontenrahmen table header scrolls away; not sticky.
- `Neue manuelle Buchung`: submit `Als Entwurf buchen` enabled with empty booking text and amount.
- Booking editor lacks proper `role=dialog` semantics.
- Booking editor defaults Soll and Haben to the same account `SKR03_0410`, dangerous unless validated.
- `Kontenrahmen neu initialisieren` is prominently enabled and destructive after confirm; needs stronger safety.
- Start menu remains present in accessibility/DOM snapshot after being closed.

UI/UX issues:

- Stronger than many apps functionally, but visually very app-specific.
- Sidebar, toolbar, buttons, and companion do not use shared Business-OS components.
- Bottom chat/date bar competes with app space.
- Empty states do not explain whether data is truly missing, not imported, or sync failed.
- Tables lack sticky headers and row selection affordance.
- Navigation uses emoji icons rather than shared icon components.
- Destructive init, create, auto-matching, export, and test buttons are too similar in weight.

Acceptance criteria:

- No global red shell status during normal operation.
- No sync/module projection console errors on open.
- Search/filter and visible counts remain synchronized.
- Table rows show clear selection and update detail/companion.
- Table headers are sticky during scroll.
- Journal/receipts/bank/travel/mileage either show mock data or diagnostic empty states.
- Booking editor validates date, text, Soll/Haben, and amount; submit disabled until valid.
- Destructive init uses explicit consequence dialog and cannot run without confirmation.
- Tabs/toggles have correct ARIA semantics.
- Header, icons, toolbar, resizer, and empty states use shared Business-OS components.

### 14. Kalender

Status: completed by single-app subagent.

Tested interactions:

- Opened from Desktop.
- Toggled calendars `Persönlich` and `Arbeit` off/on.
- Opened `Neuer Termin` form, did not save.
- Tested `Heute`, previous, next navigation.
- Tested view switch: month, week, day, list, back to week.
- Opened booking page `+ Seite` form, did not save.
- Clicked existing booking page `30 Min. Erstgespräch`.
- Checked temporary holds and confirmed bookings.
- Clicked visible event.
- Dragged left/right resizers.
- Tested calendar wheel scroll.
- Console warning/error checked.

Results:

- Calendar data visible: 6 events in week `25.-31. Mai 2026`, including `Tägliches Standup` and `Mittagessen mit Michael`.
- Evidence conflict: the dedicated interactive run reported 6 visible events in week `25.-31. Mai 2026`, but `/private/tmp/ctox-business-os-inventory-20260527/14-kalender.png` shows the week grid without visible event blocks. Treat this as either timing/state drift or a rendering/projection instability until a refreshed screenshot captures the loaded event state.
- Calendar checkbox filters work:
  - `Persönlich` off: 6 -> 5 events.
  - `Arbeit` off: 6 -> 1 event.
- Navigation works: next to `1.-7. Juni 2026`, previous back, today returns correctly.
- View switch works: month `Mai 2026`, day `27. Mai 2026`, list, week.
- Bookings/holds not visible: right panel shows no active holds or confirmed bookings.
- Read-only RxDB pulls for likely collections return 0: `calendar_events`, `calendar_bookings`, `booking_pages`, `booking_slots`, `calendar_holds`, `booking_holds`. This makes it unclear whether visible events come from expected projection or separate app seed/state.

Bugs and risks:

- `Neuer Termin`: `Speichern` enabled with empty title.
- `Neuer Termin`: form lacks `role=dialog`.
- `Abbrechen` does not close event form; only `x` works.
- `+ Seite`: `Speichern` enabled with empty title/slug.
- Booking page form also lacks dialog semantics.
- `Abbrechen` does not close booking page form; only `x` works.
- Event selection is broken: clicking a visible event opens an empty `Neuer Termin` form instead of populated event details.
- Existing booking page click has no visible effect: no selected state, detail panel, or booking data.
- Calendar wheel scroll did not react despite scrollable grid.
- Console shows global sync/projection errors for `desktop_notifications`, `channel_pairing_state`, `desktop_layout`, and failed module-version sync.

UI/UX issues:

- Calendar widget is very bright in dark theme and feels foreign.
- View-switch buttons are tiny, unlabeled/empty, and lack `aria-label`.
- Duplicate navigation exists: app-level `< Heute >` and internal FullCalendar toolbar.
- Bottom chat/date bar visually overlays lower app area.
- Resizer behavior leaves asymmetric widths; bounds/reset/persistence unclear.
- `Persönlich` / `Arbeit` checkboxes lack explicit ARIA labels.

Acceptance criteria:

- Calendar events/bookings are visible from expected RxDB/SQLite projection and counts are consistent.
- QA evidence includes a screenshot after event data has loaded, with visible event labels and corresponding collection/projection status.
- Event selection opens populated event detail/edit form.
- Event and booking-page forms have dialog semantics, focus handling, Escape/Close, and validated submit.
- `Abbrechen` closes forms without persistence.
- Booking page selection shows selected state and fills right context/bookings.
- Holds/confirmed bookings appear when data exists; empty states only when truly empty.
- View switches have visible labels/tooltips and `aria-label`.
- Unified header/toolbar replaces duplicate calendar navigation.
- Calendar grid scroll works with wheel/trackpad.
- Console has no sync/projection errors on open.

### 15. Coding Agents

Status: completed by single-app subagent.

Tested interactions:

- Opened from Desktop.
- Observed workspace loading: `Loading workspaces...` later becomes `Workspaces konnten nicht geladen werden (Backend antwortet nicht).`
- Clicked `Erneut versuchen`: returns to loading, then ends at `No workspaces authorized yet. Click "+" above.`
- Clicked sidebar `+`: opens `Add Workspace` panel; closed via `x` without adding.
- Clicked `System Settings`.
- Inspected `Active Session` dropdown: disabled, only `No Active Sessions`.
- Inspected `+ New Session`: disabled, no dialog possible.
- Inspected instruction field and `Send Instruction`: disabled.
- Checked workspace/session selection: none visible.
- Dragged resize handle between sidebar and workbench.
- Checked scroll in sidebar/workbench.
- Console warning/error checked.

Bugs and risks:

- P0: Workspace load fails or ends in contradictory state; app is not usable.
- No workspaces or sessions visible.
- Retry changes from backend error to `No workspaces authorized yet` without clear diagnosis.
- Add Workspace submit `Authorize & Add Workspace` is enabled with empty path.
- `System Settings` is clickable but inert.
- `Add Workspace` is not a proper accessible dialog; snapshot only shows panel with heading/close.
- Workbench says `No active sessions found for this workspace` even though no workspace is selected.
- Hidden/zero-size controls exist in DOM (`Launch GUI App`, `Spawn Headless`, `Terminate App`).
- `+ New Session` is semantically disabled in the DOM, but the screenshot presents it with primary enabled styling. Disabled and enabled action states are visually indistinguishable or contradictory.
- The bottom instruction composer is visibly clipped/covered by the global shell bar in the screenshot, making the disabled instruction state harder to inspect and potentially blocking normal use when enabled.
- Console includes WebRTC failures for `channel_pairing_state`, `desktop_notifications`, `desktop_layout`, and failed module-version sync.

UI/UX issues:

- Empty states do not clearly distinguish loading, backend unavailable, no authorized workspaces, and no selected workspace.
- Disabled states are functionally correct for session/instruction, but the next valid step is unclear.
- Instruction field at bottom competes with bottom shell bar.
- Sidebar `+` is too small and unlabeled as the primary action.
- `+ New Session` is disabled without explaining why.
- Header lacks shared Business-OS pattern with icon, title, status/count, primary action.

Acceptance criteria:

- Workspaces load or show concrete backend/endpoint/permission diagnostic.
- Retry resolves to visible workspaces or the same clear error state, not a different ambiguous empty state.
- Add Workspace validates required absolute path before enabling submit.
- After workspace selection, sessions load and `Active Session`, `+ New Session`, instruction/context areas activate correctly.
- `+ New Session` opens form/dialog and starts nothing without confirmation.
- `System Settings` opens visible panel/dialog or is disabled/removed.
- Header, sidebar add, session toolbar, and disabled states use shared Business-OS components.
- Console is free of replication/projection errors on open/retry.

### 16. Files

Status: completed by single-app subagent.

Tested interactions:

- Opened Files from Desktop.
- Checked root file-system data and Desktop icons.
- Switched sidebar locations: `FS Files`, `Documents`, `Spreadsheets`, `Knowledge`, `Matching Objects`, `Outbound`.
- Used search with matching query `Desk`, no-hit query `zz-no-hit`, then cleared it.
- Tested view toggle `Details` / `Icons`.
- Clicked sort headers `Name` and `Art`.
- Selected row `Documents` and inspected detail pane.
- Used `Öffnen` on folder, then `Eine Ebene höher`.
- Clicked `Aktualisieren`.
- Checked shell back state.
- Tested `Neuer Ordner`, then removed the accidentally created folder via context menu `In Papierkorb`.
- Clicked `Upload` and dismissed without file import.
- Tested maximize, restore, minimize, and reopen.
- Checked console warnings/errors.

Results:

- `FS Files` shows data: `Desktop`, `Documents`, `Downloads`, `Spreadsheets`.
- Search works on FS data: `Desk` returns `1 Objekt`, `zz-no-hit` returns `0 Objekte`.
- Row selection updates detail pane with location, size, modified date, ID, and `Öffnen`.
- Folder navigation via `Öffnen` and `Eine Ebene höher` works.
- `Aktualisieren` is clickable but produces no visible status/result.
- Maximize/restore works.
- Minimize works; restore is possible through the visible top tab, but locator semantics are ambiguous because there are duplicate `Files` buttons.
- No Files-specific browser console warnings/errors were observed in this run.
- The broader inventory run still captured global `[business-os] failed to load module versions` errors while Files was open, so the console should not be treated as globally clean.
- Evidence gap: the static DOM inventory for Files appears to capture mostly the Desktop/background tree, while `/private/tmp/ctox-business-os-inventory-20260527/16-files.png` visually confirms the Files window and root list. ARIA/layout conclusions need a focused DOM capture of the active Files window.

Bugs and risks:

- Critical: Business-OS sidebar locations show no rows but still display `4 Objekte`; affected: `Documents`, `Spreadsheets`, `Knowledge`, `Matching Objects`, `Outbound`.
- `Neuer Ordner` persists immediately. There is no dialog, name step, cancel, or undo before creation.
- `Upload` opens no visible app-level dialog and appears to rely only on the native file picker.
- `Icons` is visible but disabled, so the view switch promises functionality that is unavailable.
- Shell back remains disabled; Files has only hierarchy-up navigation, not clear history back navigation.
- `.app-explorer-row` was visually clickable, but automation/accessibility measurement intermittently reported `0x0` bounds, which points to an accessibility/layout measurement defect.

UI/UX issues:

- Sidebar locations with empty rows and stale counts are misleading.
- `Neuer Ordner` needs a dialog or inline rename before persistence.
- `Upload` needs a visible import/dropzone dialog with cancel state.
- Empty states do not distinguish empty folder, broken projection, unavailable collection, and no permission.
- Header/toolbar feels app-specific rather than shared Business-OS chrome.
- Toolbar items `⌃`, `↻`, `Neuer Ordner`, `Upload`, search, and view toggle have no clear primary/secondary hierarchy.
- Sidebar badges `FS`, `DOC`, `XLS`, `KNO`, `MAT`, `OUT` are text abbreviations rather than standardized icons.

Acceptance criteria:

- Every sidebar location shows either correct rows or a typed empty/error state.
- Count must always match visible rows.
- `Neuer Ordner` opens dialog/inline rename with `Abbrechen`; persistence only after confirmation.
- `Upload` opens a visible app dialog/dropzone and supports cancel.
- `Icons` is either implemented or hidden.
- Back/history and up/hierarchy navigation are visually and functionally distinct.
- File rows expose stable clickable bounds and useful ARIA roles.
- Console remains free of Files/projection errors during open, search, navigation, and sidebar switching.

### 17. Source Editor

Status: completed by single-app subagent.

Tested interactions:

- Opened Source Editor from Desktop.
- Inspected module/source list.
- Clicked `App öffnen` without module selection.
- Opened and closed `Diff`.
- Clicked `Neu laden`.
- Checked `Speichern` state, did not click.
- Focused Monaco editor and checked line numbers.
- Tested window maximize and restore.
- Tested window resize via right edge.
- Console warning/error checked.

Results:

- Source Editor opens as shell window.
- Left source navigation is empty: 0 children, no module, no source files.
- Status remains `Kein Modul ausgewählt.`
- No visible loading state or error in Source Editor.
- `App öffnen` is enabled but no-op without module selection.
- `Diff` is enabled and opens right diff panel with `Keine Datei ausgewählt.`
- `Neu laden` is enabled but leaves modules empty with no visible result/error.
- Monaco renders with line number 1 and focus lands in Monaco textarea.
- Editor content is empty because no file loaded.
- `Speichern` is enabled despite no module/file selected.
- Maximize/restore and edge resize work.
- Console had no warn/error entries in this run, only module-catalog sync info; shell still shows `CTOX ARBEITET NICHT`.
- Dedicated Source Editor run reported no Source-specific warn/error entries, but the inventory run captured global module-version sync errors while Source Editor was open. Distinguish app-specific console cleanliness from global Business-OS console failures.
- Screenshot evidence confirms the empty module list and enabled toolbar actions (`App öffnen`, `Diff`, `Neu laden`, `Speichern`) despite no selected module/file.

Bugs and risks:

- Critical: Source modules/files do not display; app is not usable.
- Critical: `Speichern` enabled without file/module is a persistence risk.
- `App öffnen`, `Diff`, `Neu laden` enabled without valid context.
- `Neu laden` has no visible loading, success, or failure state.
- Empty state gives no diagnosis: no modules, sync missing, projection failed, or load failed.
- Source selection flow cannot be tested because list is empty.

UI/UX issues:

- Empty state only says `Kein Modul ausgewählt`, not why module list is empty.
- Empty Monaco surface suggests editability despite no loaded file.
- Diff panel reachable without file and only shows passive empty state.
- Window resize handles are very narrow and weakly visible.
- Bottom/shell surfaces compete with floating app window.
- Toolbar actions look equal even though `Speichern` is risky.
- Source Editor uses text buttons while many other apps use icon buttons; interaction language is inconsistent.

Acceptance criteria:

- Module/source list shows available Business-OS apps and source files.
- Empty list distinguishes loading, no data, sync/projection error.
- `App öffnen`, `Diff`, `Neu laden`, `Speichern` disabled or diagnostic without valid selection.
- `Speichern` enabled only when a file is loaded and changed.
- Module selection loads file tree and first file or a clear empty state.
- File selection fills Monaco with content, path, and status.
- `Diff` displays meaningful diff only with file/changes.
- `Neu laden` shows visible loading/result/error.
- Resize/maximize stable with clear affordance.
- Console remains free of source/projection/sync errors.

### 18. App Creator

Status: completed by single-app subagent.

Tested interactions:

- Opened App Creator with route `#creator`.
- Selected template dropdown item `Notizbuch`.
- Filled and cleared the prompt field.
- Ran `Spezifikation optimieren & anwenden`; intentionally did not click `App jetzt generieren & installieren`.
- Opened and closed advanced settings.
- Checked main install button state before and after prompt changes.
- Inspected status, logs, and installed-apps area.
- Tested left/right resize handles.
- Scrolled advanced settings and checked console errors.

Results:

- Template dropdown works and fills prompt/spec-related fields (`appId=notizen`, `appTitle=Notizen`).
- Prompt plus optimize generates spec/log entries without installing an app.
- Installed-apps area remains `Noch keine eigenen Apps installiert`.
- Status stays `System bereit`.
- Logs include domain/layout/RxDB/SUCCESS messages.
- Left resize works, observed from about 320px to 380px.
- Right resize handle is inert.
- Console includes `failed to load module versions: Command ... wurde nicht synchronisiert`.
- Global shell still shows `CTOX ARBEITET NICHT`.
- `/private/tmp/ctox-business-os-inventory-20260527/18-app-creator.png` corroborates the initial invalid state: empty prompt, optimize button visible/enabled, install button visible/enabled, and `System bereit`.
- Evidence gap: the static DOM inventory for App Creator appears to capture mostly the Desktop/background tree, so App Creator-specific DOM/ARIA claims rely on the dedicated interactive subagent run rather than the shared DOM inventory.

Bugs and risks:

- Critical: `App jetzt generieren & installieren` is enabled with an empty prompt.
- Critical: `Spezifikation optimieren & anwenden` is enabled with an empty prompt.
- After clearing the prompt, optimize still produced `SUCCESS` with a previous/stale spec (`eine-sichere`), so stale state can be reused.
- Clearing via normal fill was unreliable; Meta+A plus Backspace was required.
- Advanced settings overflow below the viewport at 720px height, including `input-new-collection`, without usable scroll access.
- Advanced settings expose destructive/structural controls (`×`, `+`) without confirmation.
- Right resizer is visible but does not work.
- Logs communicate success even when prompt/state is invalid.

UI/UX issues:

- Primary install CTA is always active and has no preview/confirm step.
- Invalid, stale, generated, and installable states are not visually distinct.
- Advanced settings are too easy to expose but not safely usable.
- Header is not a shared app header; Source/Desktop icon hierarchy is unclear.
- Gear/emoji-style controls and text buttons do not match a standardized Business-OS icon/action system.
- Bottom shell bar competes with the App Creator workspace.
- It is unclear whether optimize is a temporary preview action or persistence-affecting action.

Acceptance criteria:

- Optimize is disabled until prompt/spec input is non-empty and valid.
- Install is disabled until there is a fresh, valid spec from the current prompt.
- Empty prompt must never produce success or reuse stale spec state.
- Install flow has preview/confirmation and clear persistence boundary.
- Prompt clearing is reliable by ordinary keyboard/input interactions.
- Advanced settings fit in viewport or have an internal scroll region.
- Destructive/structural advanced controls require confirmation or undo.
- Both visible resizers work or the inactive one is removed.
- Logs distinguish validation, spec generation, install, deploy, and backend errors.
- Header/buttons/icons use shared Business-OS components.
- Console is free of module-version/projection errors during App Creator open and optimize.

## Required Remediation Program

1. Bring VPS runtime to a verifiable `main` build and record build commit in the served app.
2. Fix global RxDB/WebRTC sync/projection failures before polishing individual apps.
3. Add a visible per-app data diagnostics strip in dev/QA mode: collection name, expected count, local count, sync status, last error.
4. Replace per-app header/toolbars with shared Business-OS components.
5. Replace false empty states with typed states: loading, sync failed, no selection, no data, no filtered results.
6. Unify Desktop, Start menu, App Store, taskbar, icon, and launch route from one app registry.
7. Add app-specific interaction smoke tests for every app and require screenshots/DOM/console evidence.
8. Add min/max bounded full-height resizers and ensure bottom shell bar never covers app content.
9. Add one QA checklist per app with required app-specific flows, including create/edit/import/export/start actions, and mark destructive or write flows as simulated, cancelled, or executed in a disposable dataset.
10. Make QA evidence stateful: every app report must include screenshots after data-load, after primary interaction, after empty/filter state, and after any dialog/drawer opens.
11. Split console findings into `global Business-OS errors`, `app-specific errors`, and `expected warnings`; never mark an app console-clean while global module-version/projection errors are present in the same run.
12. Add automated registry consistency tests: Desktop, Start menu, App Store categories, installed badges, app tabs, routes, and icons must agree for every app.
13. Add destructive-action safety rules for all apps: create/import/delete/install/publish actions must have validation, confirmation, undo, or disabled states as appropriate.
14. Add an action-state contract: disabled controls must be semantically disabled and visually disabled; enabled-looking primary buttons must always be actionable and valid for the current state.
