# Business OS Impeccable Preflight

Run this before frontend design, app creation, UI refactoring, or CSS changes.
It specializes the Impeccable Product-register preflight for CTOX Business OS.

## Context

1. Read repository-root `PRODUCT.md` completely.
2. Read repository-root `DESIGN.md` completely.
3. Read `.impeccable/design.json` and use its components/narrative as the
   machine-readable extension.
4. Inspect three shipped Business OS apps with the closest workflow and pane
   shape. Do not copy an internal shell/developer app as the visual template.

## Required Shape Decision

Write down before implementation:

- primary repeated task and first visible working surface;
- pane model at wide, compact, and 360px mobile;
- exact owner of vertical/horizontal scrolling at each size;
- routine utility controls that must remain compact;
- the zero-or-one signature AI automation, including its business verb,
  typed command, permission/approval state, progress, result, abort, and retry;
- record elements carrying Context v2 right-click attributes;
- 640×480 floating-window behavior and ≤600px mobile-sheet behavior;
- light/dark/custom-brand token use and German/English expansion.

## Hard Rejections

- landing-page hero, metric mosaic, generic AI dashboard, glass/glow, nested
  cards, oversized radii, decorative panes;
- more than one visually dominant action per visible work surface;
- large colored Create/Save/Export/Settings buttons masquerading as automation;
- viewport-only breakpoints inside a resizable app window;
- off-screen 640px window on mobile;
- hidden panes without a visible tab/drawer/stack/back route;
- direct style mutation as resize evidence;
- app-owned version/source chrome or right-click menu.

## Required Evidence

- real Start/Desktop single click, mouse window resize, pane resize,
  maximize/restore, minimize/restore through the upper app bar, and close;
- 1180px, 960px, 640×480, 390×844, and supported 360px renderings;
- light/dark, German/English, coarse/fine pointer, Reduced Motion, keyboard;
- shell Start, upper app bar, header metadata/actions, and chat remain
  reachable; no second bottom app switcher is rendered;
- long app names stay within the fixed two-line desktop icon cell;
- signature automation dispatches a typed command and displays trackable state.

Before mutation, emit exactly:

`IMPECCABLE_PREFLIGHT: context=pass product=pass command_reference=pass shape=pass image_gate=pass mutation=open`

If any field is not proven, keep `mutation=blocked`, state the missing evidence,
and do not claim the app is ready.
