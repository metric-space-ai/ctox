# Business OS Design Guide

Use this when styling a Business OS app or reviewing a port.

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

- Runtime-installed business apps use `layout.shell: "full-workspace"`.
- Put the primary workflow in the central workspace, not in generic shell side
  panes.
- Use dense, operational layouts for repeated business work: tables, split
  workbenches, lists, timelines, calendars, and detail drawers.
- Use cards only for repeated domain items or compact summaries. Do not nest
  cards or wrap whole page sections in decorative cards.
- Mark every record container with `data-context-record-id`,
  `data-context-record-type`, and `data-context-label`.
- Use right-click annotations instead of app-owned context menus.

## Anti-Patterns

- Landing-page hero instead of the actual app.
- A generic CRUD scaffold when the domain has a clear primary action.
- Empty or decorative side columns.
- Visible AI/queue buttons that do not dispatch real commands.
- Copying App Creator, App Store, Browser, CTOX, Credentials, or Coding Agents
  as a business-app UI template.
- Root CSS palettes, forced dark/light themes, or unreadable status colors
  under a custom brand.
