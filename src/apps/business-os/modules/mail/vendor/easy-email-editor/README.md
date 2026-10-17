# CTOX Easy Email editor bundle

This managed Business OS browser asset is a genuine source build of
[Easy Email](https://github.com/zalify/easy-email-editor), pinned to commit
`16bb02926a20af20dc6dc473c72619f4a0b4f64b` (upstream version 4.17.1).
The checked-in bundle contains the upstream React editor, block palette,
drag-and-drop canvas, history, attribute/layer/source panels, MJML renderer,
CodeMirror integration, and their browser dependencies. Business OS does not
load npm packages, a CDN, or a remote editor at runtime.

The editor runs in a same-origin local iframe so its React context remains
isolated from the host module. Layout, Properties, and Source are simplified
right-hand drawers inside that frame. The Mail module owns persistence and its
separate nested conditional-logic editor.

## Public bridge

`index.mjs` exports `createEmailDocument`, `createEasyEmailEditor`, and the
immutable `EASY_EMAIL_UPSTREAM` pin. The returned handle supports document and
HTML/MJML access, selection events, merge data, internal panel activation, and
a non-persisting `setLogicPreview` visualization.

`setDocument` is transactional: it resolves only after the frame has accepted
the new document snapshot. `getHtml` filters blocks whose CTOX
`data.value.logic` evaluates false against test/merge data, without adding
presentation flags to the stored document.

## Rebuild

The build-only sources live outside all runtime modules:

```sh
node src/scripts/vendor-builds/easy-email-editor/build-upstream.mjs
```

The script clones and verifies the exact git revision, creates an isolated
upstream workspace, and writes only browser-ready output here under `bundle/`.
It needs Git, Node.js, and pnpm for the build; none are runtime dependencies.

## Verification

```sh
node --test src/apps/business-os/modules/mail/vendor/easy-email-editor/tests/*.test.mjs
node src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs mail
```
