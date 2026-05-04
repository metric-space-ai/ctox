---
name: interactive-browser
description: Use a real browser through js_repl-backed Playwright when CTOX needs live DOM state, client-side JavaScript execution, auth/session behavior, screenshots, or true UI interaction instead of search, source reading, or durable scraping.
metadata:
  short-description: Real browser interaction through js_repl and Playwright
cluster: communication
---

# Interactive Browser

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


For CTOX mission work, browser findings become durable knowledge only when they are reflected in the the CTOX runtime store, such as communication records, verification state, ticket knowledge, continuity, or other runtime store records. Screenshots and notes alone do not count as durable knowledge.

Use this skill when the task requires a real browser session:

- click, type, scroll, or navigate through a real page
- inspect client-rendered DOM state after JavaScript runs
- validate auth or session-bound flows
- capture screenshots as compact evidence
- debug local web apps or browser-backed portals
- derive a reviewed browser observation before deciding whether work should stay one-off or become a durable scraper

Do not use this skill for normal current-information lookup. Prefer the cheaper CTOX web paths first:

- `WebSearch` for current discovery and query planning
- `WebRead` for concrete source reading, `open_page`, `find_in_page`, PDF evidence, and GitHub/docs/news adapters
- `WebScrape` for repeatable extraction with registry state, revisions, latest-state materialization, and scheduled reruns

This skill is the fourth path: use it only when the browser itself is the source of truth.

## Preconditions

- `js_repl` must be enabled for the Codex session.
- A Playwright reference workspace should exist under `runtime/browser/interactive-reference/`.
- If the reference is missing, prefer the native CTOX bridge:

```sh
ctox browser install-reference
ctox browser doctor
```

- When browser binaries are missing, install Chromium from the reference workspace:

```sh
ctox browser install-reference --install-browser
```

## Operating Model

Treat browser work as reviewed capability, not as prompt sludge.

- Keep the browser session persistent through `js_repl`.
- Keep long traces out of the main prompt.
- Store screenshots or compact artifacts on disk when they matter.
- Reuse live handles across iterations instead of relaunching on every step.
- Use browser observation to inform later `WebRead` or `WebScrape` work when that becomes the cheaper stable path.

## Core Workflow

1. Decide whether the task really needs a real browser.
2. Verify the reference with `ctox browser doctor`.
3. In a `js_repl` session, dynamically import `playwright`.
4. Launch Chromium headed unless a headless pass is clearly better.
5. Reuse `browser`, `context`, and `page` handles across iterations.
6. Capture compact evidence.
7. If the behavior must recur, convert the observation into a `universal-scraping` target instead of repeating ad hoc browser work forever.

## Minimal Bootstrap

Use the native bootstrap helper if you need a reminder:

```sh
ctox browser bootstrap
```

Equivalent `js_repl` snippet:

```javascript
var chromium;
var browser;
var context;
var page;
({ chromium } = await import("playwright"));
browser ??= await chromium.launch({ headless: false });
context ??= await browser.newContext({
  viewport: { width: 1600, height: 900 },
});
page ??= await context.newPage();
await page.goto("http://127.0.0.1:3000", { waitUntil: "domcontentloaded" });
console.log("Loaded:", await page.title());
```

## Guardrails

- Do not default to browser work when `WebSearch` or `WebRead` is enough.
- Do not turn one-off UI inspection into a scrape target too early.
- Do not leave repeated browser-backed extraction as tribal knowledge in chat history.
- Do not dump raw browser traces into the main agent context.
- Prefer screenshots, concise notes, and durable target scripts over bulky logs.

## Completion Gate

Do not report this path as ready until:

- `ctox browser doctor` reports `node`, `npm`, and `npx` available
- the reference workspace exists
- `playwright` is installed in the reference workspace
- the Codex session receives `features.js_repl=true`

If a task also needs durable repeated extraction, hand it off to `universal-scraping` before claiming the browser path alone solved the long-run operating need.
