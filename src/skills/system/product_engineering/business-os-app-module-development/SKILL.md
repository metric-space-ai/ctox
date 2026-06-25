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
- The shell also accepts any `data-*-id` as a fallback (record type derived from
  the attribute name), but the explicit trio above is required because it pins a
  clean record type and human label.
- Mark side panes with a `*-left` / `*-right` / `*-sidebar` class or
  `data-left-content` / `data-right-content` so the agent learns the column.
- Do not build a per-app context menu or a `ctox:context-action` bridge for this;
  the shell handles it. Full contract and shell internals:
  `references/module-contract.md` ("Agent Context (Right-Click)").

## Required Context

- Inspect three existing shipped Business OS apps selected for similar workflow, data model, and UI shape.
- Use `ctox business-os app references --json` when a local reference catalog is needed.
- Runtime-created apps live in `runtime/business-os/installed-modules/<module-id>/`.
- Source apps live in `src/apps/business-os/modules/<module-id>/` only when the task explicitly targets checked-in source.

## Validation

- Runtime app: `ctox business-os app validate <module-id> --installed`
- Source app: `ctox business-os app validate <module-id> --source`
- Browser proof: `ctox business-os app smoke <module-id> --installed`

Validation failures are app defects or contract defects. Fix the app or the contract; do not hide failures by weakening validation.

## References

Load only the files needed for the current task from `references/`.
