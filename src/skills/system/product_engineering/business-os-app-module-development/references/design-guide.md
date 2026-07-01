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
