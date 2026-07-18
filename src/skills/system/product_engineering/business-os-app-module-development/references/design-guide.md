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
