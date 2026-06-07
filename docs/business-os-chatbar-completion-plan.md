# Business OS Chatbar Completion Plan

Status legend: `todo`, `doing`, `blocked`, `done`.

Last updated: 2026-06-07
Audit baseline: `origin/main` at `72295a1a`
Audit artifacts:
- `/tmp/ctox-chat-thorough-audit.json`
- `/tmp/ctox-chat-thorough-audit.png`

## Working Rules

- Update this file after every implementation step and every browser test pass.
- Test on a clean `origin/main` worktree before pushing, not on the dirty local checkout.
- Do not count a fix as done until a headed browser story and a regression guard both pass.
- Keep UI feedback synchronous; persistence must not block visual state changes.
- Treat date navigation as read-only scope selection. It must not create chats or tasks.

## Target Model

The chatbar is a task and conversation dock, not a flat list of every task as a tab.

- Date scope shows existing work for that date.
- New work is created only by explicit `+`, submit, scheduling, or external command creation.
- The dock shows active, pinned, recent, and visible conversation threads.
- High-volume days use aggregation first: day, hour, status, module, source.
- Details use a virtualized drawer/list, not thousands of rendered chips or windows.

## User Stories

| ID | Status | Story | Required outcome |
| --- | --- | --- | --- |
| US-00 | done | User opens a day with zero chats/tasks. | Dock is compact, no empty full-width strip, no carousel controls. |
| US-01 | done | User changes date to a future day. | No phantom chat/task is created; future counts show only scheduled work. |
| US-02 | done | User has one chat. | Dock remains compact; no prev/next controls with nowhere to navigate. |
| US-03 | done | User has a few chats. | Chips and windows are side by side and directly clickable. |
| US-04 | done | User has many chats. | Dock scrolls, active/recent chips stay usable, overlap stays controlled. |
| US-05 | done | User has hundreds/thousands of tasks on one day. | Dock aggregates; detailed list is virtualized and filterable. |
| US-06 | done | User clicks maximize/minimize/delete/send on active chat. | Immediate visual feedback, no RxDB wait. |
| US-07 | done | User clicks a visible inactive/geared/tilted chat. | It selects reliably; visible controls are either actionable or hidden. |
| US-08 | done | User navigates by keyboard. | Focus never lands in inactive hidden controls; tab order is predictable. |
| US-09 | done | User scrolls messages. | Active message pane scrolls; dock wheel-scroll does not steal message scroll. |
| US-10 | done | User opens calendar/date picker. | Date workload heatmap shows task volume and selected-day intensity. |
| US-11 | done | User filters a busy day. | Filter by status, module, source, hour, text. |
| US-12 | done | User opens a day where one system created many related tasks, e.g. Web Research. | Related tasks collapse into deterministic groups while individual tasks remain selectable. |

## Implementation Plan

| Step | Status | Scope | Acceptance |
| --- | --- | --- | --- |
| 1 | done | Replace date navigation `ensureChat` calls with pure date-scope updates. | Navigating to tomorrow from zero chats keeps `chats.length === 0`. |
| 2 | done | Make empty and low-count dock compact. | `0/1` chat states no longer use full root width or empty strip. |
| 3 | done | Remove chat-window header `+` action. | Window header has no `data-chat-new`; dock/new entry remains explicit. |
| 4 | done | Render UI before persistence for header/chip/date interactions. | Simulated 180 ms DB delay keeps visual latency under 150 ms. |
| 5 | done | Fix inactive window interaction model. | No dead visible controls; body click selects; keyboard focus skips inactive controls. |
| 6 | done | Add workload summary model for days. | Counts derive from chats without creating records; queue/command-only task counts remain for a later data sync pass. |
| 7 | done | Add date workload heatmap/popover. | Calendar shows daily task count and selected-day intensity. |
| 8 | done | Add busy-day drawer with virtualized list. | 1000 tasks render without chip/window explosion. |
| 9 | done | Add filters. | Filter by hour, status, module, source, text. |
| 10 | done | Expand regression guards. | Static guard is in CI; browser guard covers `0/1/4/6/8/12/100/1000`, future dates, latency, keyboard, typing, scroll. |
| 11 | done | Final headed browser QA on clean main worktree. | Full story matrix passes before commit/push. |
| 12 | done | Add deterministic grouping for related high-volume task series. | `thread_key`, `group_key`, `record_id`, source and normalized title signatures create capped groups; Web Research series is browser-tested. |

## Regression Matrix

| Area | Status | Cases |
| --- | --- | --- |
| Counts | done | `0`, `1`, `4`, `6`, `8`, `12`, `100`, `1000` tasks/chats |
| Dates | done | today, tomorrow/future via date nav, arbitrary date via date picker panel |
| Viewports | done | 2048, 1440, 1024, 760, 390 |
| Interaction | done | click, wheel/message scroll, chip/window selection, keyboard tab, typing |
| Latency | done | DB delay `180` ms for max/min/chip interactions; immediate UI target <150 ms |
| Accessibility | done | focus order, aria labels, hidden inactive controls, touch target size |
| Data scale | done | Browser guard verifies 100/1000 source chats render max 12 chips/windows and max 80 busy-list rows |
| Grouping | done | Browser guard verifies 120 related Web Research tasks collapse into one group with max 80 rendered rows |

## Known Bugs From Audit

| ID | Severity | Status | Finding | Source |
| --- | --- | --- | --- | --- |
| BUG-01 | P0 | done | UI waits on persistence before visual feedback. | `persistChatState` awaited in handlers |
| BUG-02 | P0 | done | Visible inactive header controls are not clickable. | `.ctox-chat-window:not(.is-active) *` |
| BUG-03 | P1 | done | `0/1` chat dock is full-width. | `.ctox-chat-dock { width: 100%; }` |
| BUG-04 | P1 | done | Empty dock renders prev/next, strip, plus. | unconditional dock template |
| BUG-05 | P1 | done | Header `+` still creates chats. | `[data-chat-new]` inside `chatWindow` |
| BUG-06 | P1 | done | Keyboard focus lands inside inactive windows. | inactive controls remain tabbable |
| BUG-07 | P1 | done | Date navigation creates phantom future chats. | date handlers call `ensureChat` |
| BUG-08 | P2 | done | Single chat shows prev/next controls. | unconditional dock nav buttons |

## Test Log

| Time | Status | Command / scenario | Result |
| --- | --- | --- | --- |
| 2026-06-07 | done | Headed audit matrix on `3542faed` | 24 failures, documented in `/tmp/ctox-chat-thorough-audit.json` |
| 2026-06-07 | done | Future-date phantom chat reproduction | Date next from zero chats created phantom chats |
| 2026-06-07 | done | Implementation pass on `72295a1a` | Steps 1-5 implemented |
| 2026-06-07 | done | `node --check src/apps/business-os/shared/business-chat.js` | Syntax OK |
| 2026-06-07 | done | `node src/apps/business-os/scripts/assert-business-chat-layout.mjs` | Static regression guard OK |
| 2026-06-07 | done | `BUSINESS_CHAT_BEHAVIOR_HEADLESS=0 PLAYWRIGHT_MODULE_PATH=/tmp/ctox-chatbar-pw/node_modules/playwright node src/apps/business-os/scripts/assert-business-chat-behavior.mjs` | Browser matrix OK: 31 scenario entries |
| 2026-06-07 | done | `/tmp/ctox-chatbar-browser-matrix.json` | Measured 0 chats 267px, 1 chat 423px, 6/8 chats 1107px, 12 chats 1930px; max/min/chip latency ~1-3ms under 180ms DB delay |
| 2026-06-07 | done | `BUSINESS_CHAT_BEHAVIOR_HEADLESS=0 PLAYWRIGHT_MODULE_PATH=/tmp/ctox-chatbar-pw/node_modules/playwright node src/apps/business-os/scripts/assert-business-chat-behavior.mjs` | Browser matrix OK: 37 scenario entries including 100 and 1000 chats |
| 2026-06-07 | done | 100/1000 busy-day guard | 100 source chats render 12 chips/12 windows/80 panel rows + 20 remaining; 1000 source chats render 12 chips/12 windows/80 panel rows + 920 remaining |
| 2026-06-07 | done | `BUSINESS_CHAT_BEHAVIOR_HEADLESS=0 PLAYWRIGHT_MODULE_PATH=/tmp/ctox-chatbar-pw/node_modules/playwright node src/apps/business-os/scripts/assert-business-chat-behavior.mjs` | Browser matrix OK: 40 scenario entries including date heatmap, 100 and 1000 chats |
| 2026-06-07 | done | Date workload heatmap guard | 100-task day opens 28-day heatmap, selected day intensity `4`, summary `100 Tasks` |
| 2026-06-07 | done | Final headed browser matrix after rebase | Browser matrix OK: 48 scenario entries including viewports 2048/1440/1024/760/390 |
| 2026-06-07 | done | Viewport measurements | 1440: dock 1107px; 1024: 951px; 760: 724px; 390 one-chat: 354px |
| 2026-06-07 | done | Headed browser matrix after grouping implementation | Browser matrix OK: 51 scenario entries including 120 grouped Web Research tasks |
| 2026-06-07 | done | Grouped Web Research visual check | 120 related tasks render as one group, 80 visible rows, +40 group overflow, readable filter controls |
