---
name: business-os-app-module-development
description: Use whenever CTOX, Business OS, App Creator, App Store, chat, CLI, or an inbound Business OS workflow asks an agent to build, modify, repair, review, or install a CTOX Business OS app/module. The agent builds the app itself as no-build vanilla HTML/CSS/browser ESM, persists through the shell-provided CTOX DB/RxDB handle, sends automation through commandBus, studies shipped Business OS app examples, and validates the result with CTOX app validation.
metadata:
  short-description: Build runnable CTOX Business OS app modules with vanilla ESM, CTOX DB persistence, and command-bus automation.
---

# Business OS App Module Development

This file is a resource index. Use the linked contracts and checklists as the working source of truth.

## Resource Index

- `references/module-contract.md`: file layout, manifest, schema, mount contract, persistence contract, automation contract.
- `references/dos-and-donts.md`: short rules for correct Business OS app implementation.
- `references/green-checklist.md`: finalization checklist before a task can be considered done.
- `references/architecture-translation.md`: mapping from familiar web app patterns to CTOX Business OS app patterns.

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
