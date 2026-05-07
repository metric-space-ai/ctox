# CTOX Business OS Design Context

## Register

Product UI. Dense operational software, not brand or marketing.

## Visual Thesis

A quiet desktop-grade business workspace: low noise, crisp borders, compact text, restrained tinting, and clear working surfaces.

## Layout

- Global shell has exactly two navigation levels: module and submodule.
- Each submodule has one primary workbench view.
- Detail, edit, prompt, and bug-report surfaces open as contextual drawers, side panels, or bottom panels.
- Avoid nested cards. Use panes, dividers, tables, boards, maps, editors, and inspectors.
- Use available width aggressively for work surfaces, especially knowledge, campaigns, pipeline, and project planning.

## Typography

- Use the platform system font stack.
- Keep headings compact inside the app. Reserve large type for true document content or public pages.
- Dense tables and boards should favor readable 12-14px utility text with strong labels.

## Color

- Restrained default strategy: tinted neutrals plus one module accent.
- Accent use stays under control: active nav, primary action, selected record, status emphasis.
- Avoid one-note palettes and decorative gradients.

## Components

- Primary actions: one per visible work area when possible.
- Secondary actions: contextual and grouped, not repeated across every row.
- Row click opens the main contextual panel; duplicate `Open`, `Details`, or `Settings` buttons should be avoided.
- Status should be text plus subtle border/background state, not colored side stripes.
- Right-click menus must include CTOX prompt actions when record context exists.
- Bug reporting is always available and carries module/submodule context.

## Motion

- Drawers and bottom panels should slide/fade with short, restrained transitions.
- Do not animate layout-heavy properties.
- No bouncing or decorative motion.

## Accessibility

- No hidden critical actions behind tiny targets only.
- Interactive controls should be at least 32px high in dense desktop UI, with larger targets on mobile.
- Text must not overlap, clip meaningful labels, or require horizontal page scrolling. Inner work tables may scroll when the dataset requires it.

## Template Hygiene

- Business-basic must not include customer-specific names, secrets, or generated duplicate files.
- Starter data should demonstrate realistic workflows but remain generic.
- Template code should be the upstream source. Customer instances may diverge, but improvements should be upstreamed without leaking customer-specific context.
