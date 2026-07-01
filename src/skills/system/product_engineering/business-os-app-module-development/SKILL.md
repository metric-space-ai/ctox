---
name: business-os-app-module-development
description: Use whenever CTOX, Business OS, App Creator, App Store, chat, CLI, or an inbound Business OS workflow asks an agent to build, modify, repair, review, or install a CTOX Business OS app/module. The agent builds the app itself as no-build vanilla HTML/CSS/browser ESM, persists through the shell-provided CTOX DB/RxDB handle, sends automation through commandBus, studies shipped Business OS app examples, and validates the result with CTOX app validation.
metadata:
  short-description: Build runnable CTOX Business OS app modules with vanilla ESM, CTOX DB persistence, and command-bus automation.
---

# Business OS App Module Development

This file is a resource index. Use the linked contracts and checklists as the working source of truth.

## Tool Boundary

- Product entry points may use `ctox business-os app create --instruction <text>`
  or `ctox business-os app modify <module-id> --instruction <text>` to enqueue
  a real Business OS app task.
- These tools route the request to the Business OS command/queue/policy path;
  they do not generate app files, derive schemas, choose layouts, or write
  templates.
- App workers build or modify the app themselves, using the resources below and
  the three chosen reference apps.
- Use the agent's structured file-edit tool for app files. In CTOX Codex runs
  this is `apply_patch`; keep shell commands for inspection, validation, and
  tests.
- During app creation or modification, do not run `ctox stop`, `ctox start`,
  `ctox upgrade`, `launchctl`, `systemctl`, or service lifecycle commands. The
  running CTOX service is the required runtime for validation and browser proof.

## Resource Index

- `references/module-contract.md`: file layout, manifest, schema, mount contract, persistence contract, automation contract, agent right-click context.
- `references/dos-and-donts.md`: short rules for correct Business OS app implementation.
- `references/green-checklist.md`: finalization checklist before a task can be considered done.
- `references/architecture-translation.md`: mapping from familiar web app patterns to CTOX Business OS app patterns.

## Required Agent Context (Right-Click)

Mandatory for every app and every record -- not optional, not deferrable. The
shell, not the app, owns the right-click menu; the app's job is to label its
records so the agent knows what was clicked.

- Put `data-context-record-id`, `data-context-record-type`, and
  `data-context-label` on the outermost element of every record (list row, card,
  table row, tree node). The shell's global "Chat to CTOX" right-click menu reads
  them and tells the agent which record the user clicked. Without them the agent
  gets only loose text and cannot act on the record.
- Always set the explicit trio. The shell will also resolve a bare `data-*-id` as
  a safety net (deriving the type from the attribute name and guessing a label),
  but that is NOT a substitute -- do not skip the trio just because a `data-*-id`
  already exists on the element.
- Mark side panes with a `*-left` / `*-right` / `*-sidebar` class or
  `data-left-content` / `data-right-content` so the agent learns the column.
- Do not build a per-app context menu or app-owned right-click event bridge for
  this; the shell handles it. Full contract and shell internals:
  `references/module-contract.md` ("Agent Context (Right-Click)").

## Required Context

- Inspect three existing shipped Business OS apps selected for similar workflow, data model, and UI shape.
- Use `ctox business-os app references --query "<workflow data keywords>" --json --limit 8` when a local reference catalog is needed.
- Runtime-created apps live in `runtime/business-os/installed-modules/<module-id>/`.
- Source apps live in `src/apps/business-os/modules/<module-id>/` only when the task explicitly targets checked-in source.
- Runtime-created business apps must own the visible workspace inside
  `ctx.host`: set `module.json` `layout.shell` to `full-workspace`, do not rely
  on the generic shell `Kontext`/`Themen` side panes, and do not duplicate those
  panes inside the app.
- The Business OS shell already shows app identity, navigation, version/source
  controls, account state, and global chat. A normal business app may have at
  most one compact app-level command/header row for its own filters and primary
  actions. Do not stack a hero/title block, version bar, day strip, metrics row,
  and filter bar before the work surface.
- Design the primary workflow around the user's common action. For booking,
  scheduling, shift, parking, availability, or other date/slot domains, provide
  a calendar/date-strip view and one-click actions for claim/release/book flows;
  do not force a modal or form unless the user must supply extra data.
- Encode the domain invariant behind the one-click path. For physical-resource
  apps such as parking, rooms, desks, devices, or shifts, prevent impossible
  duplicate claims such as the same person, vehicle, or asset being booked into
  two slots at the same time.
- Do not add generic "Report to CTOX", "An CTOX melden", queue, AI, or
  command-bus buttons by default. Add visible automation only when the user asks
  for it or the workflow clearly needs it, and only when it dispatches a real
  command and shows a trackable result.
- Use the Business OS shell tokens for colors and controls. App CSS must inherit
  light/dark theme state from the shell through tokens such as `--bg`,
  `--surface`, `--surface-2`, `--text`, `--muted`, `--line`, and `--accent`.
  Do not force `color-scheme`, hard-code a dark-only palette, or declare an app
  finished before checking both light and dark themes.

## Validation

- Runtime app: `ctox business-os app validate <module-id> --installed`
- Source app: `ctox business-os app validate <module-id> --source`
- Browser proof: `ctox business-os app smoke <module-id> --installed`
- Visual proof: inspect the mounted app in the real Business OS shell at a
  desktop viewport and a narrow viewport, in light and dark theme. The app must
  use the central workspace, avoid useless side columns, keep text readable, and
  complete the primary workflow without clipped controls.

Validation failures are app defects or contract defects. Fix the app or the contract; do not hide failures by weakening validation.

## References

Load only the files needed for the current task from `references/`.
