# Business OS Design Guide

Use this when styling a Business OS app or reviewing a port.

The strategic and visual source of truth is the repository-root `PRODUCT.md`
and `DESIGN.md`. This guide translates those contracts into the current
Business OS class and validator vocabulary.

## Token Contract

Business OS owns the workspace palette. Apps consume semantic CSS tokens and
must not define the root palette themselves.

Use these tokens for all app UI:

```css
--bg;
--surface;
--surface-2;
--line;
--text;
--text-strong;
--muted;
--accent;
--accent-soft;
--accent-foreground;
--danger;
--warning;
--success;
--focus-ring;
```

Map app semantics onto those tokens:

- Page/workspace background: `var(--bg)`.
- App panels and repeated records: `var(--surface)` or `var(--surface-2)`.
- Borders and separators: `var(--line)`.
- Normal text: `var(--text)`.
- Headings, primary labels, key values: `var(--text-strong)`.
- Secondary metadata: `var(--muted)`.
- Primary action, selected state, links: `var(--accent)`.
- Low-emphasis accent surfaces: `var(--accent-soft)`.
- Text on solid accent buttons: `var(--accent-foreground)`.
- Status states: `var(--danger)`, `var(--warning)`, `var(--success)`.
- Focus outlines: `var(--focus-ring)`.

## Custom Branding Rules

Workspace admins can change the Business OS light and dark token values. Apps
must keep working when those tokens change.

- Do not set `:root`, `html`, or `body` token values in app CSS.
- Do not set `color-scheme`; inherit the shell's active light/dark mode.
- Do not hard-code root surfaces such as `body { background: #111; }`.
- Do not encode text contrast by assuming dark mode or light mode.
- Keep domain-specific colors secondary. They may tint badges or chart series,
  but normal surfaces, text, borders, controls, and focus states must use
  Business OS tokens.
- Test the app in light, dark, and one custom-brand fixture before delivery.

## Component Kit (shared/base.css)

The shell preloads `src/apps/business-os/shared/base.css` into every app
document. It is the construction set: build the app frame and all recurring
controls from these classes, and keep app CSS for what is genuinely unique.
Do not rebuild any of these locally.

- Frame: `.ctox-workspace` (+ `--two-pane`, `--single`) with `.ctox-pane`
  columns and shell-owned `.ctox-column-resizer` (`data-resizer`,
  `data-resizer-var`, `data-resizer-min/max`); `.ctox-pane-body`,
  `.ctox-pane-scroll`, `.ctox-pane-band`.
- Pane header: `.ctox-pane-header` + `.ctox-pane-title-row` +
  `.ctox-pane-titles` (`.ctox-pane-kicker`, `.ctox-pane-title`) +
  `.ctox-pane-actions` on the right, then a `.ctox-pane-tools` row with
  `.ctox-pane-search` and `.ctox-pane-filter`.
- Actions: `.ctox-pane-icon` (30px icon button) in pane headers;
  `.ctox-button` / `.ctox-button.is-primary` / `.is-danger` and
  `.ctox-icon-button` in toolbars, forms, and modal footers.
- Data: `.ctox-table-wrap` + `.ctox-table` (sticky column headers, `.is-num`
  for numeric columns, `.is-selected` rows); `.ctox-list`/`.ctox-list-item`
  for row lists; `.ctox-fields` (dl) for key/value details; `.ctox-card` for
  inspector sections; `.ctox-badge` (`.is-success/.is-warning/.is-danger`)
  for status; `.ctox-chip` (+`.ctox-chip-count`) for filter pills;
  `.ctox-avatar` (+`--sm`/`--lg`) for people.
- Forms: `.ctox-input`, `.ctox-select`, `.ctox-textarea`,
  `.ctox-field-label`, `.ctox-choice-group`/`.ctox-choice`.
- Overlays: `.ctox-modal` + `.ctox-modal-card` (`-header/-title/-body/
  -footer`, `--wide`), toggled with `[hidden]`; `.ctox-empty` for empty
  states. App-level toasts use `ctx.notifications`, never app-owned toasts.

## Icon Rules

- Primary actions are elegant, compact icon buttons — never large text
  buttons in headers or on top of lists.
- Every icon button carries `aria-label` and `title`.
- Action glyphs come from the shared set: `ctx.getActionIcon(name)` (names
  via `listActionIcons()` in `shared/icons.js`). Static SVGs follow the same
  style: `viewBox="0 0 24 24"`, `fill="none"`, `stroke="currentColor"`,
  stroke-width 1.8, round caps. Do not mix icon styles or invent new ones.
- The app tile icon (`icon.svg`) may use the gradient module-icon style; UI
  action icons stay monochrome so they inherit control states.

## UX Patterns

### Operational Density and Signature Automation

- Treat Ableton Live as a density and immediacy reference, never as a skin.
- Routine import, search, filter, sort, select, edit, save, export, navigation,
  status, and window controls stay flat, neutral, and compact.
- A visible work surface may elevate at most one domain-named automation action.
  It may be more colorful and slightly larger only when it dispatches a real
  typed command and exposes approval, queued/running, result, failure, abort,
  and retry state.
- Do not call ordinary Create, Save, Export, Settings, or generic "Ask AI"
  controls hero actions. A signature control names the business outcome.

- New runtime-installed business apps use root `launch_kind: "desktop-app"`
  plus the canonical `presentation` object; minimum size is 640×480.
  `layout.shell: "windowed"` remains a compatibility hint.
- Floating desktop windows support real mouse resize down to 640×480. At a
  shell width of 600px or below they switch to a mobile-sheet presentation
  supported down to 360px; they do not retain an off-screen 640px canvas.
- Two-/three-pane apps use shell-owned resizers while wide. When compact, every
  hidden pane needs a visible tab/drawer/stack and return path. Stacking panes
  with one scroll owner is acceptable; silently dropping a context pane is not.
- The shell owns app name, version/status, Source, Versions, and window controls.
  App CSS must leave that header usable and must not duplicate it.
- `full-workspace` is a legacy compatibility value. Do not select it for a new
  generated app.
- Put the primary workflow in the central workspace, not in generic shell side
  panes.
- Use dense, operational layouts for repeated business work: tables, split
  workbenches, lists, timelines, calendars, and detail drawers.
- Use cards only for repeated domain items or compact summaries. Do not nest
  cards or wrap whole page sections in decorative cards.
- Mark every record container with `data-context-record-id`,
  `data-context-record-type`, and `data-context-label`.
- Use right-click annotations instead of app-owned context menus.

### Progressive Disclosure (hide until needed)

The default state of a surface is the minimal, most-common view. Optional,
secondary, or rarely-used elements are not permanently visible — reveal them on
demand. Decluttering never means deleting the feature; it means hiding it until
asked for.

- Keep permanently visible only the common case: the primary content, the
  primary action, and primary navigation/search. Everything situational is
  revealed on demand.
- Mechanisms, in rough order of preference: a collapsible pane (root class
  `is-<x>-hidden` + a header toggle `data-toggle-...` with `aria-pressed`, the
  threads idiom), a native `<details>/<summary>` disclosure, the native
  `[hidden]` attribute, or a `⋯` overflow menu for secondary per-row/per-pane
  actions.
- **A select-driven detail or inspector pane may be hidden by default ONLY if
  selecting a record auto-reveals it.** Use the model
  `visible = hasSelectedRecord && !userCollapsed`, recomputed on every render:
  the empty state shows no inspector, clicking a record reveals it, and the user
  can still collapse it (the `outbound` app is the reference). Shipping a
  default-hidden detail pane *without* the auto-reveal is a regression — the user
  selects a row and sees nothing.
- A deliberately-opened *panel* not tied to a selection (assistant, runbooks,
  holds, a rights/settings form) can simply be default-hidden behind its toggle;
  no auto-reveal needed.
- Only render a section or card when it has data. Do not emit empty
  "nicht gesetzt" placeholder cards; collapse an empty state to a single muted
  `.ctox-empty` line, never a bordered card.
- Only show a reveal control when there is something to reveal (e.g. hide the
  inspector toggle when nothing is selected).

### Canonical Column Grammar (binding — learned on Knowledge/Threads, 2026-07)

Every list/selection column follows ONE template, top to bottom. The same
grammar repeats fractally: what holds for the pane holds for every element
inside it (header + title left, action icons top-right).

1. **Header row**: kicker (micro caps) + title on the left; action icons
   (create/import/export …) collected top-right. Nothing else.
2. **Filter section** — one dense row `[search][view-toggle][filter-icon]`,
   never wrapping into extra rows. The filter icon expands a collapsed tray
   (one recessed fill): scope/secondary views as **dropdowns** (never
   full-width button banks), sort as a **dropdown plus an up/down direction
   toggle** (more than 3 sort fields, each reversible), content-type chips
   with an explicit `✓` when active, a reset (`↺`) icon in the dropdown row,
   and an **accent dot on the filter icon whenever any filter is active**.
   Scope (User/System/Alle …) is a filter — never a permanently visible row.
3. **View switcher band**: the 3–4 primary views as equal segmented tabs with
   **counts in parentheses, zeros included** — `Handeln (3)`, `Skill (1)`,
   `Runbooks (0)`. A second switcher level uses smaller type, equal segments,
   ellipsis + full title in the tooltip. Sibling labels drop their common
   ` · ` prefix so truncation never renders identical labels.
4. **Element well**: the list sits in a recessed zone under a divider line —
   toolbar and content never merge into one surface. Shards are flat surface
   cards (fill, no border); selection = accent fill + a flat 2px `::before`
   accent bar (no `box-shadow: inset`, forbidden by the kit guard).
5. **Footer**: one line, `N Einträge · <scope/filter label>`.

Rules that go with it:

- **Where there are shards there is a list view.** Every view that lists
  elements — the left column AND the main view — offers cards ↔ compact
  rows via the two view-toggle icons.
- **A shard is a pure selector**: title + ONE muted meta line
  (`KICKER · Kind (n) · Kind (n)`). No inline expansion inside selection
  lists — the content pane's tabs + second-level switcher are the only
  navigation into a group. Expanding in the list duplicates navigation and
  breeds dead controls.
- **Element actions are icons, collected top-right of the element**: pencil,
  trash, save (`✓`), cancel (`✗`), execute (`▶`). Never text buttons in the
  content flow. The one big text button is reserved for THE essential action
  of a form/composer (submit).
- **Reactive only**: no manual refresh button. Data updates via
  subscriptions or a bounded poll. If a refresh button feels needed, the
  data wiring is wrong.
- **No standing status badges** (`bereit`, `synchronisiert`, error text as a
  pill). Status is transient (toast) or contextual, never permanent chrome.
- Sync/maintenance notices float as a toast — they must never displace the
  app layout.
- **Hub/inbox contract (Threads reference)**: a decision surface links the
  object it decides about (deep link + decision banner at the record); inbox
  rows answer WHAT / WHY ME / HOW URGENT / FROM WHOM in one glance (unread
  dot, sender initial, relative time, derived why-me line, foreign-message
  preview); humans pick people from the roster (`business_users`) — never
  type user ids; @-mentions in the composer become `target_user_ids` and
  notify; delegate and ask-back are first-class actions next to approve.
- **Timeline rule**: system events render as compact protocol lines (one
  line, muted, human-readable head first), never as chat bubbles — only
  people and AI messages get bubbles. EVERY reference in a timeline is a
  link (source object, command, task — `#<module>?record_id/command_id/
  task_id`), and every failure line carries a one-click follow-up action
  that dispatches real work (rework via `threads.ai.request`), not dead text.
- **Personal-inbox rule**: an inbox view shows only what needs THIS user now
  — my pending reviews, my mentions, my unread human threads, work assigned
  to me. Admins are not a firehose: someone else's review queue lives under
  team/approvals views. Machine work (kind `ctox_task`) enters the inbox only
  when it escalates (blocked/failed) — a "finished" notification is a result,
  not a call to act. The pane header names whose inbox it is.
- **Pin workspace panes to explicit grid tracks** (`.app-left { grid-column: 1 }`,
  resizer 2, center 3, resizer 4, right 5). The kit hides column resizers on
  narrow windows; with auto-flow placement the center pane slides onto the
  empty 12px resizer track and the main view visually disappears.
- **Icon-rail variant for selector-only left panes.** When the left pane is a
  pure switcher (projects/apps, no metadata to scan), it may collapse to a
  56px icon rail BY DEFAULT: 40px app icons (module `icon.svg`, monogram
  fallback on load error), names as a floating hover chip (`position: fixed`
  on `document.body` so pane clipping can't swallow it; remove it on
  unmount), selection as an accent outline on the icon. Make the pane a size
  container (`container: <name> / inline-size`) and bring inline labels,
  kicker/title and footer text back via `@container (min-width: 150px)` —
  the shell resizer (min 56) is the expand affordance. Reference:
  coding-agents.
- **The primary column keeps a hard minimum; side panes yield.** Fixed/maxed
  side tracks (`minmax(220px, 280px) … minmax(260px, 380px)`) grow to their
  max BEFORE a `minmax(0, 1fr)` center gets anything — in a narrow window the
  main view collapses to 0px while both side panes render. Give the primary
  track a real minimum (`minmax(300px, 1fr)`) and add a
  `@container business-app-window (max-width: …)` rule that hides the least
  important pane (e.g. an artifact/detail pane) so the primary surface wins.
- Module CSS must carry the module JS cache-buster
  (`index.css?v=` from `import.meta.url`); fresh JS over a stale cached
  sheet produces phantom layout bugs.
- Module version is always three-part semver (`1.2.3`) in `module.json`.

## Anti-Patterns

- Landing-page hero instead of the actual app.
- A generic CRUD scaffold when the domain has a clear primary action.
- Empty or decorative side columns.
- A surface overloaded with secondary controls, filters, or reference cards that
  are needed only occasionally yet shown permanently.
- A default-hidden detail/inspector pane that does not auto-reveal when a record
  is selected.
- Visible AI/queue buttons that do not dispatch real commands.
- Copying App Creator, App Store, Browser, CTOX, Credentials, or Coding Agents
  as a business-app UI template.
- Root CSS palettes, forced dark/light themes, or unreadable status colors
  under a custom brand.
- Permanent walls of filter chips (10+ always-visible buttons) instead of the
  collapsed filter tray plus a counted primary-view band.
- Inline expansion (caret + sublists) inside a selection list — a shard
  selects; content is navigated in the main view only.
- A manual refresh/reload button on reactive data.
- Standing status badges (`bereit`/`synchronisiert`/error pills) in a pane
  header.
- Text buttons for element actions (Bearbeiten/Löschen/Freigeben) in the
  content flow instead of collected top-right icons.
- Toggles or view switchers placed top-right in the pane header action
  cluster — the top-right is for element actions; switchers live in their
  own band or the filter row.
- A sync/maintenance banner that displaces the layout instead of floating.
- Count badges as pill chrome costing extra rows — counts are inline text in
  parentheses, zeros included.
