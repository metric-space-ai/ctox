# RFC 0009: Business OS Customers CRM

**Status:** Draft for product specification
**Date:** 2026-05-27
**Affects:** `src/apps/business-os/modules/customers/`,
`src/apps/business-os/modules/registry.json`,
`src/core/business_os/business_os_schema_contract.json`,
`src/core/business_os/store.rs`,
`src/core/business_os/importer.rs`

**Implementation Plan:** `docs/business-os-customers-crm-implementation-plan.md`

## 1. Decision

CTOX Business OS gets a first-party **Customers** app for classical CRM work
around existing customers and active sales relationships. The app is modeled
from Twenty's CRM product concepts, but it should be specified and implemented
as a native CTOX module, not as an embedded Twenty frontend.

Twenty is used as a reference system for:

- standard CRM objects and relationships
- table, kanban, calendar, and saved-view behavior
- record detail page composition
- notes, tasks, attachments, and timeline activity
- sales pipeline conventions
- data-model customization expectations

The first CTOX implementation should keep the surface narrower than Twenty:
focus on account management, contacts, opportunities, tasks, notes, and
interaction history. Generic no-code object customization can come later.

## 2. Twenty Reference Baseline

The reference checkout is outside the project root:

```text
/private/tmp/twenty-clone-yYTZPJ/twenty
```

Relevant Twenty source files inspected:

- `packages/twenty-server/src/modules/company/standard-objects/company.workspace-entity.ts`
- `packages/twenty-server/src/modules/person/standard-objects/person.workspace-entity.ts`
- `packages/twenty-server/src/modules/opportunity/standard-objects/opportunity.workspace-entity.ts`
- `packages/twenty-server/src/modules/task/standard-objects/task.workspace-entity.ts`
- `packages/twenty-server/src/modules/note/standard-objects/note.workspace-entity.ts`
- `packages/twenty-server/src/modules/timeline/standard-objects/timeline-activity.workspace-entity.ts`
- `packages/twenty-front/src/modules/object-record/`
- `packages/twenty-front/src/modules/views/`
- `packages/twenty-docs/user-guide/data-model/capabilities/objects.mdx`
- `packages/twenty-docs/user-guide/layout/capabilities/record-pages.mdx`
- `packages/twenty-docs/user-guide/views-pipelines/how-tos/set-up-a-sales-pipeline.mdx`

Twenty's core CRM model:

- `Company`: account/customer organization
- `Person`: contact, usually attached to a company
- `Opportunity`: deal or expansion opportunity, attached to company and point
  of contact
- `Task`: follow-up or action item linked to one or more records
- `Note`: rich text note linked to one or more records
- `TimelineActivity`: normalized activity stream over records
- `Attachment`: files linked to records
- `WorkspaceMember`: internal owner, account owner, assignee

Twenty's key UX pattern is metadata-driven object work:

- records are opened from object index pages
- index pages can be table, kanban, or calendar views
- views support filters, sorting, grouping, visible fields, aggregations, and
  workspace/personal visibility
- record pages are composed from tabs and widgets: fields, related records,
  emails, calendar, timeline, tasks, notes, files, charts, iframe, rich text
- opportunities are commonly shown as a kanban pipeline grouped by stage

## 3. Product Scope

The Customers app is the system of record for Bestandskunden and active sales
relationships after initial outbound handoff.

Final app identity:

- Launcher label: `Kunden`
- Module id: `customers`
- Internal product description: Customers CRM
- Primary language: German labels, with English i18n fallback

Included in v1:

- customer account list
- contact list
- opportunities / deals
- customer detail page
- contact detail page
- opportunity detail page
- saved views for customer segments and pipelines
- notes and tasks linked to accounts, contacts, and opportunities
- timeline of customer interactions
- import and deduplication hooks from Outbound
- ownership and next-action tracking

Deferred from v1:

- fully generic custom object builder
- arbitrary record-page layout editor
- full email/calendar provider sync inside Customers
- marketplace-like app extensibility
- AI autonomy that sends customer-facing communication without explicit user
  approval

## 4. Target Collections

The initial replicated collections are:

- `customer_accounts`
- `customer_contacts`
- `customer_opportunities`
- `customer_tasks`
- `customer_notes`
- `customer_activities`
- `customer_files`
- `customer_views`
- `customer_view_filters`
- `customer_view_sorts`
- `customer_import_batches`
- `customer_dedupe_candidates`
- `business_commands`

Existing cross-app collections should be reused where appropriate:

- `communication_messages` remains the durable communication timeline for sent,
  received, failed, and drafted messages.
- Outbound pipeline records should link to customers after qualification instead
  of duplicating a second account/contact lifecycle.

## 5. Data Model

### customer_accounts

Represents a business account or customer organization.

Required fields:

- `id`
- `created_at`
- `updated_at`
- `deleted_at`
- `name`
- `domain`
- `website_url`
- `linkedin_url`
- `x_url`
- `account_status`
- `customer_stage`
- `account_owner_id`
- `annual_recurring_revenue_cents`
- `currency`
- `employee_count`
- `industry`
- `address`
- `ideal_customer_profile`
- `source`
- `source_record_id`
- `last_activity_at`
- `next_action_at`
- `health_status`
- `search_text`

Recommended `account_status` values:

- `prospect`
- `active_customer`
- `at_risk`
- `former_customer`
- `partner`
- `vendor`

Recommended `customer_stage` values:

- `new`
- `onboarding`
- `active`
- `expansion`
- `renewal`
- `paused`
- `churned`

### customer_contacts

Represents a person at a customer account.

Required fields:

- `id`
- `created_at`
- `updated_at`
- `deleted_at`
- `account_id`
- `first_name`
- `last_name`
- `email`
- `phone`
- `job_title`
- `city`
- `linkedin_url`
- `x_url`
- `is_primary_contact`
- `contact_owner_id`
- `last_activity_at`
- `source`
- `source_record_id`
- `search_text`

### customer_opportunities

Represents sales, expansion, renewal, or commercial follow-up opportunities.

Required fields:

- `id`
- `created_at`
- `updated_at`
- `deleted_at`
- `name`
- `account_id`
- `primary_contact_id`
- `owner_id`
- `opportunity_type`
- `stage`
- `amount_cents`
- `currency`
- `close_date`
- `probability`
- `position`
- `last_stage_changed_at`
- `lost_reason`
- `source`
- `source_record_id`
- `search_text`

Recommended `opportunity_type` values:

- `new_business`
- `expansion`
- `renewal`
- `reactivation`
- `cross_sell`

Recommended default stages:

- `new`
- `qualified`
- `meeting`
- `proposal`
- `negotiation`
- `closed_won`
- `closed_lost`

### customer_tasks

Represents action items and follow-ups.

Required fields:

- `id`
- `created_at`
- `updated_at`
- `deleted_at`
- `title`
- `body`
- `status`
- `due_at`
- `assignee_id`
- `account_id`
- `contact_id`
- `opportunity_id`
- `position`
- `search_text`

Recommended `status` values:

- `todo`
- `in_progress`
- `done`
- `canceled`

### customer_notes

Represents rich text notes linked to customer records.

Required fields:

- `id`
- `created_at`
- `updated_at`
- `deleted_at`
- `title`
- `body`
- `author_id`
- `account_id`
- `contact_id`
- `opportunity_id`
- `search_text`

### customer_activities

Represents the normalized timeline for customer records.

Required fields:

- `id`
- `created_at`
- `happens_at`
- `activity_type`
- `name`
- `properties`
- `actor_id`
- `account_id`
- `contact_id`
- `opportunity_id`
- `communication_message_key`
- `linked_record_type`
- `linked_record_id`
- `linked_record_name`

Recommended `activity_type` values:

- `record_created`
- `record_updated`
- `note_created`
- `task_created`
- `task_completed`
- `opportunity_stage_changed`
- `email_sent`
- `email_received`
- `meeting_booked`
- `file_attached`
- `outbound_handoff`

## 6. Views

The Customers app should ship with opinionated default views rather than a full
generic view builder.

Default account views:

- `Alle Kunden`: table, sorted by `updated_at_ms desc`
- `Meine Kunden`: table, filter `account_owner_id = me`
- `Gefaehrdet`: table, filter `health_status = at_risk`
- `Renewals`: table/calendar, filter by upcoming renewal or `customer_stage =
  renewal`
- `ICP Accounts`: table, filter `ideal_customer_profile = true`

Initial account table columns:

- `name`
- `account_status`
- `customer_stage`
- `health_status`
- `account_owner_id`
- `domain`
- `annual_recurring_revenue_cents`
- `last_activity_at_ms`
- `next_action_at_ms`

Default contact views:

- `Alle Kontakte`
- `Primaerkontakte`
- `Meine Kontakte`
- `Ohne neue Aktivitaet`

Initial contact table columns:

- `first_name` / `last_name`
- `account_id`
- `email`
- `phone`
- `job_title`
- `is_primary_contact`
- `contact_owner_id`
- `last_activity_at_ms`

Default opportunity views:

- `Sales Pipeline`: kanban grouped by `stage`, aggregation `sum(amount_cents)`
- `Meine Pipeline`: kanban, filter `owner_id = me`
- `Abschluss diesen Monat`: table, filter `close_date_ms` in current month and open
  stages only
- `Renewals`: table/calendar, filter `opportunity_type = renewal`
- `Closed Won`
- `Closed Lost`

Initial opportunity table columns:

- `name`
- `account_id`
- `primary_contact_id`
- `stage`
- `amount_cents`
- `currency`
- `close_date_ms`
- `owner_id`
- `opportunity_type`
- `last_stage_changed_at_ms`

Initial pipeline card fields:

- opportunity `name`
- linked account name
- `amount_cents` + `currency`
- `close_date_ms`
- `owner_id`
- `primary_contact_id`

View capabilities for v1:

- table view
- kanban view for opportunities
- field visibility
- simple filters
- simple sorting
- stage column drag and drop
- column count and amount aggregation
- personal vs shared view flag

Deferred view capabilities:

- arbitrary nested filter groups
- formula fields
- chart widgets
- fully generic calendar view over every date field

## 7. Record Pages

The v1 record pages should be fixed, not user-customizable. This preserves the
Twenty mental model while keeping implementation scope controlled.

### Account Page

Tabs:

- `Overview`
- `Contacts`
- `Opportunities`
- `Activity`
- `Files`

Overview widgets:

- account fields
- owner and health
- next action
- open tasks
- recent notes
- recent activity

Contacts tab:

- related contacts table
- primary contact marker
- add/link contact action

Opportunities tab:

- related opportunity table
- active pipeline summary
- create opportunity action

Activity tab:

- timeline
- linked communication messages
- notes
- tasks

### Contact Page

Tabs:

- `Overview`
- `Activity`
- `Opportunities`

Widgets:

- contact fields
- linked account
- open tasks
- recent communication
- related opportunities where contact is primary contact

### Opportunity Page

Tabs:

- `Overview`
- `Activity`
- `Files`

Widgets:

- opportunity fields
- stage control
- amount and close date
- linked account
- primary contact
- open tasks
- notes
- stage-change timeline

## 8. Commands

Initial command surface:

- `customers.account.create`
- `customers.account.update`
- `customers.account.archive`
- `customers.contact.create`
- `customers.contact.update`
- `customers.contact.archive`
- `customers.opportunity.create`
- `customers.opportunity.update`
- `customers.opportunity.move_stage`
- `customers.opportunity.close_won`
- `customers.opportunity.close_lost`
- `customers.task.create`
- `customers.task.update`
- `customers.task.complete`
- `customers.note.create`
- `customers.note.update`
- `customers.activity.record`
- `customers.view.save`
- `customers.import.from_outbound`
- `customers.dedupe.resolve`

All commands must be idempotent and write deterministic command outcomes into
`business_commands`.

## 9. Outbound Handoff

Outbound remains responsible for Neukundengewinnung. Customers becomes the
system of record once a company/contact is qualified or a commercial
opportunity is created.

Handoff rules:

- Outbound companies can create or link `customer_accounts`.
- Outbound contacts can create or link `customer_contacts`.
- Qualified pipeline items can create `customer_opportunities`.
- The original outbound record IDs are preserved in `source_record_id`.
- Dedupe uses normalized domain for accounts and normalized email for contacts.
- Handoff writes a `customer_activities` entry of type `outbound_handoff`.

## 10. Cross-App Deep Links

Customers uses the existing hash/query pattern used by the shell and
Conversations. Links must be stable, optional-parameter friendly, and safe when
the target record has not synced yet.

Customers inbound links:

- Account: `#customers?object=account&account_id=<customer_account_id>`
- Contact: `#customers?object=contact&contact_id=<customer_contact_id>`
- Opportunity: `#customers?object=opportunity&opportunity_id=<customer_opportunity_id>`
- View: `#customers?view=<view_id>`
- Pipeline: `#customers?view=sales_pipeline&opportunity_id=<customer_opportunity_id>`
- Dedupe queue: `#customers?panel=dedupe&candidate_id=<customer_dedupe_candidate_id>`
- Outbound handoff review: `#customers?panel=handoff&source=outbound&source_record_id=<outbound_record_id>`

Outbound links from Customers:

- Campaign: `#outbound?campaign_id=<campaign_id>`
- Company: `#outbound?campaign_id=<campaign_id>&company_id=<outbound_company_id>`
- Pipeline item: `#outbound?campaign_id=<campaign_id>&pipeline_id=<outbound_pipeline_item_id>`

Conversations links from Customers:

- Thread: `#conversations?thread_key=<communication_thread_key>`
- Message: `#conversations?message_key=<communication_message_key>`
- Customer-linked conversation: `#conversations?account_key=<communication_account_key>&thread_key=<communication_thread_key>`

Calendar links from Customers:

- Date focus: `#calendar?date=<YYYY-MM-DD>&source=customers`
- Booking context: `#calendar?customer_account_id=<customer_account_id>&customer_contact_id=<customer_contact_id>`
- Opportunity close date: `#calendar?date=<YYYY-MM-DD>&opportunity_id=<customer_opportunity_id>`

Documents links from Customers:

- Document: `#documents?document_id=<document_id>&source=customers`
- Opportunity documents: `#documents?customer_opportunity_id=<customer_opportunity_id>`

Notes links from Customers:

- Note: `#notes?note_id=<note_id>&source=customers`
- Customer notes context: `#notes?customer_account_id=<customer_account_id>`

Spreadsheets links from Customers:

- Export/bulk analysis context: `#spreadsheets?source=customers&view=<view_id>`

When a target app does not yet implement a parameter, Customers should still use
the target module hash and show a clear fallback action such as "Open
Conversations" or "Open Calendar"; target-specific focus behavior can be added
incrementally.

## 11. Safety And Permissions

Customers may prepare internal notes, tasks, summaries, and suggested messages.
It must not send external customer communication directly.

External sends must continue to use the existing reviewed communication path or
the active Outbound approval gate once implemented.

Permission model:

- users can read shared accounts, contacts, opportunities, notes, and tasks
- owners can edit their assigned accounts and opportunities
- admins can edit all customer records and resolve dedupe conflicts
- personal views are visible only to the creator
- shared views are workspace-visible

## 12. Production Scope Boundary

Production v1 includes:

- native Business-OS module shell integration
- browser schemas and native schema contract
- backend commands for authoritative CRM transitions
- account/contact/opportunity/task/note/activity collections
- account/contact workbench
- opportunity table and kanban pipeline
- fixed record pages
- CRM-linked tasks and notes
- outbound handoff and dedupe flows
- cross-app links to Conversations, Calendar, Documents, Notes, Spreadsheets,
  and Outbound
- light/dark, German/English labels, empty/loading/error/sync states
- app-local tests and browser QA

Production v1 explicitly excludes:

- generic no-code object builder
- arbitrary record-page layout editor
- external customer-message sending
- full email/calendar sync implementation inside Customers
- standalone task manager
- full Notes/Documents/Spreadsheets editors inside Customers
- workflow builder
- general dashboard/chart builder

These exclusions are not quality reductions. They are app-boundary decisions:
the CRM app must be complete for customer relationship work while linking to
the specialized apps that already own adjacent domains.

## 13. UI/UX Feature Matrix

The Customers app should not try to become a second copy of every Twenty
surface. It should own the CRM-specific workflow and link out to specialized
Business OS apps where CTOX already has a stronger native surface.

Decision states:

- `Own`: implement directly in Customers.
- `Link`: surface lightweight context and deep-link/open the owning Business OS
  app.
- `Defer`: leave out of v1, but keep the data model extensible.
- `No`: intentionally not part of Customers.

| Area | Twenty feature | Customers decision | Customers UX | Owning/linked Business OS app | Reason |
|---|---|---:|---|---|---|
| Navigation | Sidebar entries for Companies, People, Opportunities, Tasks, Notes | Own | Left pane object switcher: Accounts, Contacts, Opportunities, Tasks, Notes, Views | Customers | Core CRM navigation belongs in the CRM app. |
| Navigation | Command menu navigation | Link | Register launcher/search actions for opening Customers, selected record, and common views | Desktop / shell command surfaces | Global command/menu behavior should stay shell-owned. |
| Accounts | Companies object | Own | Accounts table plus account detail page | Customers | This is the primary Bestandskunden object. |
| Contacts | People object | Own | Contacts table, contact detail, primary-contact marker, account relation | Customers | Customer relationship work needs contacts directly in CRM. |
| Deals | Opportunities object | Own | Opportunity table and kanban pipeline | Customers | This is the classical sales CRM surface missing today. |
| Pipeline | Kanban grouped by opportunity stage | Own | Full center-pane pipeline with drag/drop stage changes and amount totals | Customers | Critical daily sales workflow. |
| Pipeline | Stage configuration through data model settings | Defer | Fixed default stages in v1; later editable stage list | Customers | Avoids building Twenty's generic data-model editor first. |
| Pipeline | Column aggregations | Own | Count and sum of `amount_cents` in each stage header | Customers | High-value, small implementation scope. |
| Pipeline | Compact cards | Own | Dense card toggle showing account, amount, close date, owner | Customers | Useful for repeated sales work. |
| Pipeline | Track time in stage | Defer | Store `last_stage_changed_at_ms`; report duration later | Customers / Reports | Data should be captured now; reporting can come later. |
| Views | Table views | Own | Accounts, contacts, opportunities, tasks, notes as dense tables | Customers | Baseline CRM scanning and editing surface. |
| Views | Saved views | Own | Opinionated default views plus simple saved personal/shared views | Customers | Needed, but narrower than Twenty's generic view system. |
| Views | Advanced filters | Defer | v1 supports simple field/operator/value filters | Customers | Nested filter builder is useful but not essential for first CRM slice. |
| Views | Sorting | Own | Single or simple multi-sort controls per view | Customers | Required for table usability. |
| Views | Field visibility | Own | Per-view visible column toggles | Customers | Required for CRM table ergonomics. |
| Views | Grouping | Defer | Stage grouping for pipeline only in v1 | Customers | Generic grouping can wait. |
| Views | Calendar view | Link | Show due/close dates summary; open full date work in Calendar | Calendar | Calendar already owns scheduling, booking links, recurrence, availability. |
| Views | View visibility/access | Own | Personal vs shared view flag | Customers | Simple privacy model is enough for saved CRM views. |
| Record pages | Customizable tabs and widgets | Defer | Fixed tabs for Account, Contact, Opportunity pages | Customers | Twenty-like layout is useful, but layout editor is not v1 scope. |
| Record pages | Fields widget | Own | Fixed field sections with inline edit controls | Customers | Core record editing belongs in CRM. |
| Record pages | Related records widget | Own | Related contacts/opportunities/tasks/notes tables inside detail pages | Customers | Essential CRM context. |
| Record pages | Timeline widget | Own | Unified customer activity stream in right pane and Activity tab | Customers + Conversations | Customers owns CRM events; Conversations owns complete message audit. |
| Communication | Email history on records | Link | Recent communication preview with "Open thread" action | Conversations | Conversations already owns cross-channel audit over `communication_messages`. |
| Communication | Calendar events on records | Link | Meeting preview and next booking/action link | Calendar | Avoid duplicating calendar grid and booking-page logic. |
| Communication | Send email from CRM | No | Customers may prepare internal drafts/suggestions only | Outbound / Conversations / reviewed channel path | Sending must stay approval-gated and channel-owned. |
| Outbound | Prospecting and campaign qualification | Link | Handoff panel showing originating campaign/source and import evidence | Outbound | Neukundengewinnung remains Outbound's domain. |
| Outbound | Lead/contact research | Link | Display imported research summary; open source pipeline item | Outbound | Research workflow already exists in Outbound. |
| Outbound | Customer handoff | Own | `Import from Outbound` / `Link existing account` flow with dedupe | Customers + Outbound | Boundary between prospecting and account management. |
| Tasks | Tasks object linked to records | Own | Inline task list, due date, assignee, status, quick complete | Customers | CRM follow-up should be visible where the record is. |
| Tasks | Standalone task manager | Defer | Customers task list limited to CRM-linked tasks | Customers / future Tasks app | Avoid creating a general personal productivity app inside CRM. |
| Notes | Notes object linked to records | Own | Lightweight CRM note composer and note list | Customers | Meeting/account notes need to live with customer context. |
| Notes | Full rich note workspace | Link | "Open in Notes" for long-form or standalone notes | Notes | Notes app already owns rich note workspace behavior. |
| Files | Attachments on records | Link | File list preview and attach/link metadata | Documents / shell file viewer | Documents owns document editing/versioning; Customers stores links/context. |
| Documents | Proposal or contract documents | Link | Related document cards on account/opportunity | Documents | Avoid embedding DOCX editor in CRM. |
| Spreadsheets | Spreadsheet-like data editing/import | Link | Import/export table data; open bulk analysis in Spreadsheets | Spreadsheets | Spreadsheet app owns XLSX grids and formulas. |
| Dashboards | Charts and dashboard widgets | Defer | Pipeline summary numbers only in v1 | Reports / future dashboard | Keep v1 operational; analytics can be cross-app. |
| Data model | Standard objects | Own | Fixed CRM schemas for accounts, contacts, opportunities, tasks, notes | Customers | This is the app's native domain model. |
| Data model | Custom fields | Defer | `payload`/`custom_fields` escape hatch, no UI builder in v1 | Customers | Preserve extensibility without building admin UI first. |
| Data model | Custom objects | No | Not inside Customers v1 | App Store / future builder | Generic object builder is a platform feature, not CRM v1. |
| Data model | Relation fields | Own | Fixed relations: account-contact, account-opportunity, opportunity-contact, record-task/note | Customers | CRM needs these relationships directly. |
| Import | CSV import | Own | Account/contact import using universal importer patterns | Customers | CRM onboarding needs bulk import. |
| Import | CRM migration guide | Defer | Keep import batches and dedupe records; no full migration wizard in v1 | Customers / Spreadsheets | Useful later, but first scope is simpler imports. |
| Dedupe | Merge duplicate records | Own | Dedupe queue for domain/email conflicts | Customers | Necessary when linking Outbound and imports. |
| Search | Object search | Own | Search input over account/contact/opportunity fields | Customers | Core usability. |
| Global search | Cross-app search | Link | Expose selected records to shell/global search later | Desktop / shell | Global indexing should be platform-owned. |
| Permissions | Workspace roles and object permissions | Defer | Basic owner/admin conventions in v1 | Core Business OS governance | Fine-grained permission matrix is platform-level work. |
| Collaboration | Mentions/comments | Defer | Activity entries and notes only | Conversations / future collaboration | Avoid duplicating a comment system before needed. |
| AI | AI summaries and suggested next actions | Link | Right-pane CTOX assistant prompt context from selected record | CTOX / Knowledge / Outbound | Agent orchestration is cross-app; Customers supplies record context. |
| AI | Autonomous CRM actions | No | No external send or destructive action without command approval | Core command validation | Customer-facing work needs explicit gates. |
| Automation | Workflows | Defer | Command hooks only: import, dedupe, stage move, task creation | Core workflow / Outbound | Twenty-style workflow builder is beyond CRM v1. |
| Audit | Timeline and field updates | Own | `customer_activities` for CRM-domain changes | Customers | Required for trust and account history. |
| Audit | Full channel audit | Link | Link to communication thread and outbound approval evidence | Conversations / Outbound | Existing apps already own communication provenance. |
| Layout | Full workspace shell | Own | `layout.shell = full-workspace`, 3-pane CRM workbench | Customers | Matches existing Business OS modules. |
| Layout | Arbitrary widget layout editor | Defer | Fixed layouts with stable tabs | Customers | Reduces build risk and keeps v1 coherent. |
| Mobile/responsive | Dense CRM on small screens | Own | Responsive columns, drawers for detail panes | Customers | Required for shell/browser usability. |
| Offline/local-first | Local RxDB state and WebRTC sync | Own | Use `ctx.db.raw` and module schemas, no HTTP fallback | Business OS data plane | Mandatory Business OS runtime contract. |
| Backend validation | Authoritative commands | Own | Write command docs for state transitions requiring validation | Core Business OS store | Browser UI is not trusted as enforcement boundary. |

## 14. Business OS Implementation Shape

Customers must be implemented as a normal native Business OS module, matching
the existing app contract in `src/apps/business-os/README.md` and
`src/apps/business-os/ARCHITECTURE.md`.

Required module directory:

```text
src/apps/business-os/modules/customers/
  module.json
  schema.js
  index.html
  index.css
  index.js
  customers.test.mjs
```

The app must remain no-build:

- direct ESM from `index.js`
- static `index.html` loaded with `fetch(new URL('./index.html', import.meta.url))`
- module-scoped CSS loaded by appending a `<link>`
- all durable browser state through CTOX DB collections supplied on `ctx.db`
- backend work through `ctx.commandBus.dispatch(...)` into `business_commands`
- no embedded Twenty frontend and no copied Twenty source files

The app should follow the existing module lifecycle:

- export `async function mount(ctx)`
- load labels via `loadModuleMessages`
- inject module HTML into `ctx.host`
- use `ctx.left`, `ctx.right`, and drawers intentionally
- wire direct DOM event handlers
- wire RxDB realtime subscriptions with debounced rendering
- return an unmount function that removes subscriptions, timers, resizers, and
  context menus

Reference modules:

- `tickets`: clean mount, realtime, right-pane context, command status patterns
- `outbound`: Sales-domain workbench, importer behavior, campaign handoff,
  command-driven automation
- `calendar`: scheduling and booking should be linked, not duplicated
- `conversations`: communication timeline should be linked, not duplicated
- `notes`, `documents`, `spreadsheets`: rich editing surfaces should be linked,
  not reimplemented inside Customers

## 15. Legal Constraint

Twenty is AGPL-licensed. If CTOX copies Twenty source code or produces a
derivative work from Twenty implementation code, AGPL obligations may attach.
For this RFC, Twenty is used only as a behavioral and data-model reference.

Implementation should:

- avoid copying Twenty source files
- reimplement UI and backend behavior natively in CTOX style
- keep source references in design notes for traceability
- preserve attribution where required if any Twenty-derived material is used

## 16. Implementation Plan

Phase 1: Specification and schema

- add `customers` module folder
- define module metadata and icon
- define collection schemas
- add registry entry
- add backend command validation stubs

Phase 2: Read/write CRM core

- implement account/contact/opportunity/task/note CRUD
- implement timeline activity writer
- implement search fields
- add basic tests for schema and commands

Phase 3: UI v1

- accounts table
- contacts table
- opportunity pipeline kanban
- detail pages with fixed tabs
- task and note side panels
- saved default views

Phase 4: Outbound integration

- implement `customers.import.from_outbound`
- add dedupe candidate generation
- link outbound pipeline items to customer records
- show handoff activity on customer timeline

Phase 5: Hardening

- permission enforcement
- import conflict handling
- large-table performance pass
- accessibility pass
- smoke tests and browser QA baseline

## 17. Open Questions

- Are former customers managed here, or should churned accounts move to a
  separate archive/reporting surface?
- Should opportunities include subscription renewal dates in v1, or should
  renewals be represented as a separate object later?
- Which existing CTOX user/member collection should own `account_owner_id`,
  `owner_id`, and `assignee_id`?
- Should customer health be manual in v1, calculated from signals, or both?
