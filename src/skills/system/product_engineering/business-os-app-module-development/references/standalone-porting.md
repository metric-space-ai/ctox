# Standalone To Business OS Porting

Use this when a user starts with a standalone vanilla app and asks an agent to
port it into CTOX Business OS.

## Portable Shape

Write standalone apps so the Business OS port is mostly wiring, not a rewrite:

```js
export async function mount(ctx) {
  const root = ctx.host;
  const records = ctx.db.collection('your_module_records');
  const cleanup = [];
  // render, subscribe, dispatch commands
  return () => cleanup.forEach((fn) => fn());
}
```

The app may run standalone by passing a mock `ctx`, but production Business OS
gets the real `ctx` from the shell.

Required `ctx` boundary:

- `ctx.host`: DOM element owned by the shell.
- `ctx.db.collection(name)`: declared module collection handle.
- `ctx.commandBus.dispatch(command)`: automation/CTOX task dispatch.
- `ctx.preferences.theme`: current `dark` or `light` mode.
- `ctx.preferences.branding`: active workspace token payload when available.

## Standalone Rules

- Use browser ESM and relative imports only.
- Vendor every executable dependency into the module as local browser ESM.
  Remote `script`/stylesheet URLs, HTTP(S) `import`/`import()`, import maps that
  resolve to a CDN, remote workers, and runtime package-manager/CDN loaders are
  release blockers. A domain API or media stream may remain remote only when it
  is an explicit product requirement; it must never supply executable code.
- A one-file HTML import is source material, not a production URL. Preserve its
  complete behavior in local module files and never read the operator's
  original path after the immutable import snapshot was materialized.
- Keep persistence calls behind `ctx.db.collection(...)`.
- Keep automation behind `ctx.commandBus.dispatch(...)`.
- Use Business OS tokens in CSS even in standalone mode.
- Load `assets/standalone/business-os-tokens.css` in standalone previews to
  mimic the default shell tokens.
- Use `assets/standalone/mock-business-os-context.mjs` for local demos and
  tests, then remove the mock from the Business OS runtime bundle.

## Porting Steps

1. Run the source app and record a behavior inventory before editing: every
   visible surface, control, dataset size, interaction, persistence path,
   animation, canvas/WebGL behavior, and responsive breakpoint. Capture the
   source at the same viewports used for the port acceptance proof.
2. Create `module.json`, `collections.schema.json`, `schema.js`, `index.html`,
   `index.css`, `index.js`, one local `icon.svg` or `icon.png`, and focused
   tests.
3. Move the standalone app's root render into `mount(ctx)`.
4. Replace direct storage, localStorage, IndexedDB, REST, or in-memory stores
   with `ctx.db.collection('<module_scoped_collection>')`.
5. Replace automation/follow-up calls with `ctx.commandBus.dispatch(...)`.
6. Add record right-click annotations to every row/card/tree node.
7. Set root `launch_kind` to `"desktop-app"`, write the canonical root
   `presentation` object, and retain `layout.shell: "windowed"` only as the
   compatibility hint.
8. Validate with `ctox business-os app validate <module-id> --installed` or
   `--source`, then smoke in the real shell.
9. Inspect the mounted runtime resource list. Fail the port if any executable
   script, stylesheet, module, worker, or imported source came from another
   origin.

For a substantial canvas/WebGL one-file app, an isolated `srcdoc` compatibility
surface is acceptable only when a direct `mount(ctx)` rewrite would materially
change behavior. Store the entire source locally behind static relative ESM
imports, inject only the shell persistence bridge, assign `srcdoc` with the
standard load watchdog, and prove the rendered canvas plus primary interactions
in the real Shell V2 host. A loaded iframe or a passing `load` event is not
render proof.

## What Not To Port

- Package manager setup, bundlers, dev servers, framework bootstraps.
- Full HTML documents with `<html>`, `<head>`, `<body>`, scripts, or styles.
- App-owned auth, HTTP APIs, database sync, or server storage.
- Standalone mock data as production persistence.
- CSS root palettes or forced `color-scheme`.

## Acceptance Proof

Before claiming the port is done:

- Standalone mock still mounts for local inspection.
- The port matches the recorded source behavior inventory. It does not replace
  a real dataset with a short fixture, simplify an interactive visualization
  into a decorative approximation, or silently drop controls and workflows.
- Business OS mount works with shell-provided `ctx`.
- Records persist through declared collections.
- Automation commands persist through `business_commands`.
- Visual proof covers light, dark, and one custom-brand fixture.
- A real host reload/reopen preserves state and the primary workflow still
  succeeds.
- Canvas/WebGL apps show non-empty representative pixels at default, 640×480,
  and 360px layouts; no blank lower region, clipped toolbar, or off-canvas
  control is accepted.
- The runtime resource audit contains no remote executable dependencies.
