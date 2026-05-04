# Operations Project-Management Port Parity

This file tracks the transplant of `/Users/michaelwelsch/Documents/ctox_projectmanagement/port` into the CTOX Business Basic Operations module.

## Transplanted Now

| Source capability | Source files | CTOX target |
| --- | --- | --- |
| Collapsible project tree | `src/components/projects/ProjectTree.tsx`, project `parentId/path` service logic | `OperationsProjectTreeTool` in the Projects submodule, backed by `parentProjectId` and member counts. |
| Work-package relations and richer detail fields | `src/components/work-packages/RelationsPanel.tsx`, work-package schema/service | Work item drawer sections for relations, custom fields, time entries, reminders, activity, and linked knowledge. |
| Kanban drag/drop surface | `src/components/boards/KanbanBoard.tsx`, board column/card model | `OperationsKanbanTool` in Boards with local drag/drop, WIP indicators, and CTOX context queue handoff. |
| Meeting agenda reorder/add | `src/components/meetings/AgendaList.tsx` | `OperationsAgendaTool` in Meetings with local reorder, drag/drop, and inline agenda add. |
| Soft modal navigation | CTOX-specific shell requirement | `ClientNavigationBridge` catches internal `/app/...` anchors and context-menu events, preserving deep links without document reloads. |

## High-Priority Remaining Parity

| Area | Missing CTOX work |
| --- | --- |
| Persistence for board moves and agenda reorder | Add Operations mutation endpoints for status moves, agenda updates, and audit/CTOX queue events. |
| Backlog/sprint buckets | Transplant backlog bucket model as Operations Intake, Validation, Scheduling, Handover, Done. |
| Calendar and Gantt | Use work item `start`, `due`, `doneRatio`, and `relations` for drag-to-reschedule, dependency visualization, and critical-path signals. |
| Team planner | Build a real capacity grid from people, roles, work items, due dates, and unassigned work. |
| Wiki/Markdown | Port sanitized Markdown rendering, work-item autocomplete, page tree, versions, comments, and attachments into Knowledge. |
| Attachments/storage | Add container-based file links, quota, virus/fulltext hooks, and external storage connections. |
| Activity/journal/comments | Generalize Work-Package journal to CTOX entities: projects, work items, meetings, knowledge, bugs, sync events. |
| Notifications/reminders | Extend reminders from work packages to all Operations records and wire notification preferences. |
| RBAC/admin | Replace OpenProject-oriented permissions with CTOX scopes: global, business unit, operation, client, entity. |
| Integrations/reporting/costs | Finish GitHub/GitLab webhooks, reporting pivots, budgets, costs, and time-entry actuals in Business/Operations sync. |

## Design Contract

- Keep one main view per submodule.
- Use drawers and bottom/side panels for details, create flows, prompts, bug reports, and record editing.
- Every interactive item must expose `data-context-*` metadata for right-click Prompt CTOX.
- Every drawer link must remain a deep link but must not hard-reload the document during normal app use.
- Module-specific functionality can be extended, but the shell, theme, locale, context menu, bug reporter, and CTOX queue handoff stay global.
