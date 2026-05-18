# Interactive UX testing + Pattern 14 (subagent-parallel UX-fixing)

Unit tests with high coverage do NOT validate UI/UX. **They miss an entire defect class:** composition-layer wiring (form-action wiring, redirect logic, prop threading, ID resolving, server/client boundary violations, CSS-rendering issues). These bugs only surface when a real browser exercises the live app against a real DB.

This file documents the protocol that surfaced 16+ user-visible bugs in the OpenProject port that 1487 passing unit tests had missed.

## Setup

1. **Real Postgres locally.** `brew services start postgresql@18` or equivalent. The port's `.env` points at it. Run `pnpm db:migrate` + `pnpm db:seed`.
2. **Dev server.** `pnpm dev` in the background. Wait for `http://localhost:3000` to respond.
3. **Browser tool.** The orchestrator drives Chrome via the `claude-in-chrome` MCP (`mcp__Claude_in_Chrome__*` tools). The user must install + sign in to the Chrome extension. Without it, the methodology degrades to scripted curl tests (~50% of the value).

## User stories to exercise

A minimum viable test pass exercises:

| US | Flow | Catches |
|---|---|---|
| **Login** | Form submit + session cookie | BUG-1 (form not wired), BUG-12 (pool leak after multiple page loads) |
| **Dashboard redirect** | Land somewhere useful post-login | BUG-2 (skeleton root) |
| **TopBar state** | Shows logged-in user, not "Login" link | BUG-3 (layout passing user=null) |
| **Project list + tree** | Renders project tree with names | BUG-4 (raw IDs leaked) |
| **Project detail + tabs** | All ~8 tabs render without error | Tab i18n, tab routing |
| **WP detail rendering** | All sections render (pickers, time, reminder, attachments, relations, activity, comments) | Boundary violations (BUG-16), in-memory port silent fail (BUG-11) |
| **WP edit + save** | Picker change persists, activity stream updates | Form action wiring, in-memory ports |
| **Comment posting** | Comment + activity stream update + reset | BUG-7 (textarea not reset) |
| **Search live** | Debounce + dropdown + FTS bold | BUG-8 (HTML escaped in snippet) |
| **Wiki create + view** | Markdown headings + lists render | BUG-9 (typography not styled) |
| **Members add** | Add user + role → row in DB + visible in panel | BUG-11 (in-memory port losing writes) |
| **Admin pages** | Types/OAuth/FeatureFlags etc. render with DB content | BUG-13/14 (missing migrations / unwired Drizzle) |
| **Reporting page** | 3 SVG charts render with real DB data | Service composition |
| **CSV export** | Download returns valid CSV | Route handler |
| **Sidebar nav** | Each top-level link navigates | i18n coverage (BUG-5) |

## The defect class unit tests miss

In the OpenProject port (where 1487 passing unit tests existed pre-UX-test):

| Bug | Severity | Class | Why unit tests missed |
|---|---|---|---|
| BUG-1 Login form not wired | Blocker | Composition | Tests called the action directly, never via `<form>` |
| BUG-2 Root page skeleton | High | UX flow | No test asserted post-login redirect target |
| BUG-3 TopBar wrong state | High | Layout prop threading | Tests rendered TopBar with `user={mockUser}` directly |
| BUG-4 Raw IDs in WP detail | High | Display rendering | Tests asserted `wp.projectId` value, not displayed string |
| BUG-7 Comment textarea no reset | Low | Client state | No test simulated post-submit DOM state |
| BUG-8 Search snippet HTML escape | Medium | Render | Tests asserted snippet string, not HTML structure |
| BUG-9 Markdown not styled | Medium | CSS | Unit tests don't render real Tailwind |
| BUG-11 Members in-memory port | Blocker | Production composition | Tests use the same memory adapter intentionally |
| BUG-12 PG pool leak (React.cache) | Blocker | Composition lifecycle | Tests don't run for hours / many requests |
| BUG-13/14 Missing migrations | Blocker | Drizzle journal | Tests use `createTestContainer` not migration replay |
| BUG-16 Server/client boundary | Blocker | Module boundary | Tests don't trigger Next.js's server/client checker |

**Pattern.** Every one of these is a *composition-layer* bug. The business logic was correct; the wiring was wrong. Unit tests of services + components in isolation can't see wiring.

## Pattern 14 — Subagent-parallel UX-fixing

When the orchestrator is the only agent that can drive the browser (Chrome MCP) but bugs found are independent, dispatch subagents with focused fix specs.

### Workflow

1. **Orchestrator browser-tour.** Click through user stories. Capture screenshots. Note bugs in a numbered list.
2. **Triage.** Group bugs by independence. Bugs that touch the same file go to the same subagent.
3. **Dispatch fix subagents in parallel.** Each gets:
   - Symptom (1-2 sentences)
   - Root cause hypothesis (if known)
   - File paths to modify (Pattern 10 strict §2)
   - Acceptance: `pnpm typecheck` must pass
   - "Don't touch other files; report what you touched."
4. **Continue browser-tour while subagents work.** The orchestrator and subagents are working different axes.
5. **Verify each fix in browser** as it lands.
6. **Audit pass** if many bugs land — check for new sibling-cleanup events.

### Empirical (OpenProject port UX-test wave)

- 5 audit subagents in parallel produced findings reports.
- 8 fix subagents in parallel landed Drizzle adapters for 20 in-memory ports + boundary fixes + raw-ID resolves.
- 1 fixture-fix subagent corrected FK-violation tests.
- Total: 14 subagents in 3 sequential parallel waves.
- Wall-clock: ~30 min for what would have been 6+ hours serial.
- Result: 1495 tests passing, 0 regressions, 16+ user-visible bugs fixed.

### What the orchestrator does in parallel

While subagents fix, the orchestrator:
- Continues browser-tour to find more bugs
- Verifies fixes that have already landed
- Reads dev server logs for runtime errors
- Writes the cumulative bug-list document
- Updates the test report iteratively

The parallelism means the orchestrator's wall-clock is filled with browser-driving (which only it can do) while the subagents handle code-fix work in parallel.

## Browser-tool gotchas

### Form input via React-controlled inputs

`form_input` from the Chrome MCP sets the underlying `value` attribute but doesn't always trigger React's `onChange` synthetic event chain. Symptom: form submit produces empty field values.

**Fix.** Use `computer.left_click` on the field, then `computer.type` to type characters. This goes through the OS input stack and produces real keypresses that React picks up.

### Server actions vs GET forms

Next.js Server Actions render forms with hidden `$ACTION_ID` inputs and submit via POST. If your form falls back to GET (no action attribute), credentials/data leak into the URL.

**Detection.** Check the URL after submit. If you see your form values as query params, the form's not wired. (BUG-1.)

### The N badge on the Chrome extension

When N issues fire (errors in dev mode), the extension icon shows a red badge with the count. Read it before assuming success.

### Connection pool leaks

After ~30 page loads in dev mode, watch for `PostgresError: sorry, too many clients already` in the dev server log. If you see it, you have BUG-12 (composition root using `React.cache` instead of `globalThis`-symbol singleton).

## When the Chrome extension isn't connected

Fall back to scripted curl tests. They catch ~50% of the bugs Chrome would. The other 50% require real browser interaction.

```bash
# Cookie jar + login flow
CSRF=$(curl -s -c cookies.txt http://localhost:3000/api/auth/csrf | jq -r .csrfToken)
curl -s -b cookies.txt -c cookies.txt -X POST http://localhost:3000/api/auth/callback/credentials \
  -d "csrfToken=$CSRF" -d "login=admin" -d "password=changeme123" -d "json=true" \
  -H "Content-Type: application/x-www-form-urlencoded" -o /dev/null

# Authenticated requests
curl -s -b cookies.txt http://localhost:3000/api/v3/work_packages | python3 -m json.tool
```

## When to run UX testing

After:
1. All implementation waves landed.
2. Functional completion (build + tests green).
3. The audit pass from [audit-checklist.md](audit-checklist.md) has identified composition-layer bugs.

Before:
1. Declaring "production-ready."
2. Real-Postgres-deploy load testing.

The UX-test pass is what converts "tests pass" to "the app actually works for users."

## What's still NOT covered by interactive UX testing

Even after a thorough UX-test pass, the following remain untested:
- Real Vercel deployment behavior (env-var pickup, lambda cold-starts)
- Production Postgres under load
- Multi-user concurrent operations
- Email delivery via real Resend
- OAuth login flows (need real GitHub/Google credentials)
- Inngest cron firing (need Inngest cloud setup)
- Drag-and-drop in real browser (synthetic events vary)
- File upload to real Vercel Blob

These need a separate production-validation phase. The methodology in this skill ends at "the app works locally end-to-end against real Postgres." Production hardening is a different skill set.
