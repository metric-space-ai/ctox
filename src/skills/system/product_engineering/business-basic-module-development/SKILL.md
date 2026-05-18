---
name: business-basic-module-development
description: "Use when CTOX must add or repair a module in templates/business-basic by first cloning and reading real open-source implementations, deriving the RFC from that evidence, implementing against the RFC in the Business Basic tech stack, and then improving UI/UX through paired manual/CTOX user stories."
metadata:
  short-description: Build usable Business Basic modules from OSS evidence, RFCs, real mutations, browser proof, and paired CTOX/manual stories
cluster: product_engineering
---

# Business Basic Module Development

## Use When

- A new `templates/business-basic` module or submodule is requested.
- An existing Business Basic module is fake, list-only, UI-only, or not usable.
- The user asks whether a module is ready for implementation.
- The user asks to continue from RFC to M0/M1/story coverage.

## Hard Stop Rules

Stop and report the blocker instead of coding when any of these is true:

```text
fewer than 3 relevant OSS repos were cloned and read
no RFC section names the central object, owned tables, states, commands, and UI work surface
the UI reference supplied by the user was not inspected and mapped into the new domain
no paired manual/CTOX stories exist for the workflow being implemented
an action is visible in the UI but has no command, runtime mutation, persistence, smoke assertion, and browser assertion
drag/drop, right-click, drawer, modal, or toolbar affordances are present but only update local decoration or status text
browser proof was not run after the last UI/runtime change
the app's global context menu bridge captures right-clicks and no module handler consumes its direct actions
smoke data can collide across repeated runs
```

Do not say "ready", "done", "usable", or "implemented" while any hard stop is active.

## Fake UI Ban

Treat every visible affordance as a contract. Before rendering a button, menu item,
draggable card, editable field, drawer action, modal action, or CTA, define the full
chain:

```text
visible affordance
-> UI event handler
-> command name
-> runtime/API mutation
-> durable state change
-> audit/event or CTOX payload when relevant
-> smoke assertion
-> browser assertion
```

If the full chain does not exist, remove the affordance. Never ship placeholder
actions, status-only handlers, decorative drag handles, disabled-looking fake controls,
or buttons that only set a message.

Smoke tests must be repeatable. Do not create fixed-date/fixed-time demo rows that
collide with previous runs. Either clean all dependent rows touched by the test or use
unique dates/ids per run. A test that passes only on an empty local store is not proof.

Required implementation check:

```sh
rg -n "onClick|onSubmit|onDrop|onDragStart|onPointerDown|context-action|data-context|button|draggable" templates/business-basic/apps/web/components templates/business-basic/apps/web/app
rg -n "\"<command>\"|case \"<command>\"|execute.*Command|mutate\\(" templates/business-basic/apps/web/lib templates/business-basic/apps/web/app/api templates/business-basic/apps/web/scripts
```

For each visible command in the module, the second search must show the runtime/API/test path.

## Reference UI Transplant

When the user provides an existing UI/code reference, inspect it before designing:

```sh
sed -n '1,260p' <reference-file>
rg -n "modal|drawer|contextmenu|drag|drop|right|left|bottom|score|match|kanban|status|action|toolbar|data-|onClick|onDrop|onDrag" <reference-file> <related-files>
```

Write an analogue map before implementation:

```text
Reference object -> Business Basic object
Reference left rail -> New left rail
Reference center work object -> New center work object
Reference right rail -> New right rail
Reference bottom drawer -> New bottom drawer
Reference left-bottom modal -> New left-bottom modal
Reference right modal -> New right modal
Reference score/match model -> New score/check model
Reference kanban/status transitions -> New workflow transitions
Reference context-menu actions -> New context-menu actions
Reference AI/CTOX prompt path -> New CTOX prompt path
What must be removed because the new domain does not need it
```

Do not implement a new layout until this map exists in the implementation map.

## Output Files

Use these exact files for module `<module>`:

```text
rfcs/<nnnn>_business-basic-<module>.md
templates/business-basic/docs/<module>-oss-implementation-notes.md
templates/business-basic/docs/<module>-implementation-map.md
templates/business-basic/docs/<module>-user-stories.md
templates/business-basic/docs/<module>-acceptance-matrix.md
```

Do not create duplicate files when one already exists.

The implementation map must include these concrete sections:

```text
OSS decisions
Reference UI analogue map
Central object lifecycle
Visible affordance inventory
Command-to-affordance matrix
Data ownership and adjacent handoffs
Browser proof plan
Regression story set
```

## Sequence

### 1. Find Existing CTOX Work

Run:

```sh
rg -n "<module>|<submodule>" rfcs templates/business-basic/docs templates/business-basic/modules templates/business-basic/apps/web templates/business-basic/packages
find templates/business-basic/modules -maxdepth 2 -type f | sort
sed -n '1,260p' templates/business-basic/packages/ui/src/navigation/model.ts
```

If a relevant RFC, story file, matrix, or implementation map exists, update it.

### 2. Find Real Open-Source Implementations

Use GitHub search first. Prefer mature repos with schema, workflows, tests, and
domain code.

```sh
gh search repos "<module> ERP open source" --limit 20 --json fullName,description,stargazersCount,url,updatedAt,primaryLanguage
gh search repos "<business process> open source <domain keyword>" --limit 20 --json fullName,description,stargazersCount,url,updatedAt,primaryLanguage
```

If `gh` is unavailable or weak, use CTOX web search:

```json
ctox_web_search({
  "query": "site:github.com open source <module> ERP <business process> schema workflow",
  "search_context_size": "high",
  "include_sources": true
})
```

Select at least 3 repos. Prefer repos that expose:

```text
database schema or migrations
domain services
state/status fields
workflow transitions
API endpoints
tests
UI screens
```

### 3. Clone And Read Repos

Clone into runtime research space:

```sh
mkdir -p runtime/research/business-basic-<module>/repos
git clone --depth 1 <repo-url> runtime/research/business-basic-<module>/repos/<repo-name>
```

Read each repo with `rg`, not by browsing random files:

```sh
rg -n "class .*<Object>|table|migration|status|state|workflow|transition|reservation|fulfillment|invoice|stock|order|shipment|payment|audit|event|outbox" runtime/research/business-basic-<module>/repos/<repo-name>
rg -n "<central-object>|<domain-keyword>|status|state|workflow|api|controller|service|schema|migration|test" runtime/research/business-basic-<module>/repos/<repo-name>
find runtime/research/business-basic-<module>/repos/<repo-name> -maxdepth 4 -type f | sort | sed -n '1,240p'
```

Write findings to:

```text
templates/business-basic/docs/<module>-oss-implementation-notes.md
```

Required table:

```text
Repo | Files read | Objects | Tables/schema | States | Transitions | APIs/services | UI pattern | Tests | Business Basic decision
```

Stop if fewer than 3 repos were actually read.

Do not summarize repositories from memory or from README files only. At least one
schema/migration/model file and one service/controller/workflow file must be read for
each selected repo, unless the repo truly lacks one; record that absence explicitly.

### 4. Derive RFC From OSS Evidence

Update:

```text
rfcs/<nnnn>_business-basic-<module>.md
```

RFC sections:

```text
OSS evidence summary
Business Basic scope
Central object
Owned tables
Read-only adjacent data
Outbound handoffs
State model
Commands/mutations
Idempotency and audit
API/runtime contract
UI work surface
Right-click actions
CTOX prompt payload
M0 scope
M1 scope
Rejected OSS patterns
Deferred OSS patterns
Acceptance evidence
```

Do not implement before the RFC names:

```text
central object
owned tables
states
commands
UI work surface
CTOX context payload
M0 slice
M1 slice
```

The RFC must include an explicit "Not a mockup" checklist:

```text
central object persisted:
create/edit mutation:
move/transition mutation:
right-click direct action:
CTOX prompt payload:
bottom drawer object editor:
browser proof target:
```

### 5. Implement M0 Against RFC

M0 is one end-to-end workflow in the Business Basic stack.

Touch the needed files:

```text
templates/business-basic/modules/<module>/module.json
templates/business-basic/modules/<module>/README.md
templates/business-basic/packages/ui/src/navigation/model.ts
templates/business-basic/packages/ui/src/theme/theme.css
templates/business-basic/apps/web/lib/<module>-runtime.ts
templates/business-basic/apps/web/app/api/<module>/...
templates/business-basic/apps/web/components/<module>-*.tsx
templates/business-basic/apps/web/app/app/[module]/[submodule]/page.tsx
templates/business-basic/apps/web/scripts/<module>-smoke.mjs
```

M0 proof:

```text
route renders
durable data loads
central object selectable
one create/edit mutation persists
reload keeps mutation
right-click opens actions
Prompt CTOX sees module/submodule/record context
visible affordance inventory has no unimplemented action
smoke command passes
browser proof passes
```

### 6. Implement M1 Against RFC

M1 is the smallest usable module, not a full enterprise system.

M1 requires:

```text
create/edit/rename/duplicate/archive on central master data where relevant
main workflow can move through real states
left/center/right/bottom work surface when the workflow needs it
actions at the object, not detached toolbar-only controls
drag/drop or direct in-place move when movement is a core workflow
right-click for object actions
Prompt CTOX from selected object
visible missing/blocked/ready/done states
handoff to adjacent module represented
browser coverage for main workflow
DB/API smoke for mutations
reload persistence for every core mutation
```

For product workflow modules, default surface:

```text
left rail: intake/backlog/source material or inbound work
center: the primary work object map/board/timeline where direct manipulation happens
right rail: outgoing queue, exceptions, approval, handoff, or review pressure
bottom drawer: selected object editor, evidence, score/checks, and allowed actions
left-bottom drawer: master data/setup for the selected left context
right drawer: selected outgoing/review/handoff detail
```

Only remove a rail/drawer when the RFC says why the workflow does not need it.

### 7. Write 50 Paired User Stories After M0

Update:

```text
templates/business-basic/docs/<module>-user-stories.md
```

Story groups:

```text
01-05 setup/master data
06-10 intake/source work
11-20 core workflow
21-25 move/transition/drag-drop
26-30 edit/rename/duplicate/archive/delete
31-35 right-click actions
36-40 CTOX-assisted actions
41-44 blocker/exception recovery
45-47 cross-module handoff
48-50 report/export/audit/regression
```

Every story has both blocks:

```text
US-<n> Manual
As <actor>, when <specific trigger>, I <operate central object> so that <business result>.
UI path:
1. route:
2. select:
3. action:
4. result:
Done when:
- UI:
- DB:
- event/audit:

US-<n> CTOX
From <right-click/drawer/selected object>, ask CTOX to <prepare/validate/execute>.
Context payload:
- module:
- submodule:
- recordType:
- recordId:
- selectedFields:
- allowedAction:
Done when:
- CTOX result:
- persisted state:
- approval/recovery:
```

### 8. Optimize UI/UX Through Stories

Create/update:

```text
templates/business-basic/docs/<module>-acceptance-matrix.md
```

Columns:

```text
Story | Manual UI | CTOX path | DB | API/runtime | UI file | Context menu | Test | Browser proof | Status | Blocker
```

Allowed status:

```text
missing
partial
needs proof
done
blocked
```

Each iteration must:

```text
pick one missing/partial story
implement DB/runtime/API/UI/context together
run smoke/browser proof
update matrix
re-test early stories if shared UI/runtime changed
```

The acceptance matrix may not mark a story as `done` unless all of these cells are
non-empty and evidence-backed:

```text
Manual UI
CTOX path
DB
API/runtime
UI file
Context menu
Test
Browser proof
```

Re-test these after large changes:

```text
US-01 setup
US-02 create/edit
US-11 first core workflow
US-31 first right-click action
US-36 first CTOX-assisted action
```

### 9. Required CTOX Context

Every central object element must expose:

```tsx
data-context-module="<module>"
data-context-submodule="<submodule>"
data-context-record-type="<recordType>"
data-context-record-id="<recordId>"
data-context-label="<label>"
data-context-skill="product_engineering/business-basic-module-development"
```

Every context-menu direct action must be consumed by the module:

```tsx
window.addEventListener("ctox:context-action", onContextAction);
```

The handler must call the same command path as the visible UI action. It must not only
select the object or set explanatory text.

In `templates/business-basic`, `ContextMenuBridge` listens to `contextmenu` in capture
phase for every `data-context-*` object. Do not assume a component-level
`onContextMenu` will win. If the object has `data-context-*`, add direct actions in
`context-menu-bridge.tsx` and consume `ctox:context-action` inside the module. Browser
proof must right-click the object and execute at least one direct action from the
visible CTOX menu.

For drag/drop, do not rely on decorative HTML5 `draggable` alone. If the browser proof
cannot complete the drag reliably, implement pointer-based drag release detection that
finds the target work cell under the cursor and calls the same move command. Browser
proof must show the object inside the target cell after the drag and a saved state.

`Prompt CTOX` payload:

```json
{
  "prompt": "<allowed action> for <record label>",
  "action": "<allowedAction>",
  "items": [
    {
      "moduleId": "<module>",
      "submoduleId": "<submodule>",
      "recordType": "<recordType>",
      "recordId": "<recordId>",
      "label": "<label>",
      "href": "/app/<module>/<submodule>?recordId=<recordId>",
      "skill": "product_engineering/business-basic-module-development"
    }
  ]
}
```

### 10. Required Proof Commands

Run the project-specific equivalents:

```sh
pnpm --dir templates/business-basic test
pnpm --dir templates/business-basic --filter @ctox-business/web build
pnpm --dir templates/business-basic --filter @ctox-business/web exec node apps/web/scripts/<module>-smoke.mjs
```

For UI work, also run:

```sh
pnpm --dir templates/business-basic --filter @ctox-business/web typecheck
rg -n "TODO|placeholder|mock|fake|not implemented|coming soon|setMessage\\(|alert\\(" templates/business-basic/apps/web/components/<module>-*.tsx templates/business-basic/apps/web/lib/<module>-runtime.ts templates/business-basic/apps/web/app/api/<module>
```

Any hit must be either removed or explained in the acceptance matrix as blocked.

Browser proof:

```javascript
// ctox-browser: timeout_ms=30000
await ctoxBrowser.goto("http://localhost:3001/app/<module>/<submodule>?locale=de&theme=light");
const initial = await ctoxBrowser.observe({ limit: 80 });
await ctoxBrowser.click("<central object target>");
const selected = await ctoxBrowser.observe({ limit: 80 });
await page.locator("[data-context-record-id='<recordId>']").click({ button: "right" });
const menu = await ctoxBrowser.observe({ limit: 80 });
return {
  route: initial.url,
  objectVisible: initial.text.includes("<object label>"),
  selectedVisible: selected.text.includes("<selected label or drawer label>"),
  promptCtoxVisible: menu.text.includes("Prompt CTOX")
};
```

Browser proof must cover the core interaction, not only route rendering:

```text
open route
select central object
open bottom drawer
edit a field and save
reload and confirm persistence
right-click the same object
execute one direct context action
drag/drop or move the object when movement is a core story
confirm visible state changed where the user performed the action
confirm no console errors
```

### 11. Queue Remaining Work

If the current turn cannot finish a story:

```sh
ctox queue add \
  --title "Business Basic <module>: US-<n>" \
  --thread-key "business-basic/<module>" \
  --skill "product_engineering/business-basic-module-development" \
  --priority "normal" \
  --prompt "Implement US-<n> for <module>. Read RFC, OSS notes, implementation map, user stories, and acceptance matrix. Implement DB/runtime/API/UI/context-menu/CTOX path together. Run smoke and ctox_browser_automation proof. Update the acceptance matrix."
```

### 12. Completion Gate

Do not say ready unless:

```text
OSS notes include at least 3 cloned/read repos
RFC derives decisions from OSS notes
implementation map includes the reference UI analogue map when a reference exists
M0 proof exists
M1 proof exists
50 paired stories exist
acceptance matrix has no core missing/partial/needs proof
smoke command passed
browser proof passed
right-click Prompt CTOX works
all visible affordances have command/runtime/persistence/test/browser evidence
early story regression check passed
```

Final answer must include:

```text
OSS notes:
RFC:
implementation map:
user stories:
acceptance matrix:
M0 proof:
M1 proof:
tests:
browser proof:
done stories:
blocked stories:
```
