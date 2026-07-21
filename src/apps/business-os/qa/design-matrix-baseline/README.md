# Business OS Design Matrix Baseline

Baselines are separated by `process.platform` because Chromium text rendering
is operating-system dependent. CI and Release use the reviewed `linux/`
baseline; macOS development uses `darwin/` at the same strict mismatch limit.

Generate only through `npm run qa:visual-baseline` on the target platform and
commit only after human visual review. Ordinary QA selects the matching
immutable platform baseline and never updates it implicitly. A platform without
a reviewed baseline fails closed.

## Matrix dimensions

Each platform baseline holds 3 widths x 2 themes x 2 locales x 2 brands = 24
captures per platform:

- widths: 640, 960, 1180 (design-lab frame)
- themes: `light`, `dark` (`data-theme`)
- locales: `de`, `en` (`renderDesignLabLocale`)
- brands: `default` (no suffix) and `custom` (`-brand` suffix)

File naming: `<width>-<theme>-<locale>.png` for the default brand and
`<width>-<theme>-<locale>-brand.png` for the custom-brand fixture.

## Custom-brand fixture

The `-brand` captures apply a fixed QA fixture identity (violet accent on warm
paper in light, on deep plum in dark) through the exact runtime mechanism of
`shared/branding.js`: the capture script builds the style block via
`workspaceBrandingStyleText()` and sets
`document.documentElement.dataset.workspaceBranding = 'custom'`, so the
`:root[data-workspace-branding="custom"]` cascade is identical to a real
workspace branding. The palette is deliberately non-default but readable in
both themes, so any token leaking past the branding whitelist shows up as an
obviously off-brand pixel diff. The fixture palette lives as `BRAND_FIXTURE`
in `scripts/capture-design-matrix.mjs`; `focus_ring` is intentionally unset
(the whitelist only accepts bare colors, but `--focus-ring` is a full
box-shadow value).

Note: the `linux/` baseline must be regenerated on a Linux/CI machine via
`npm run qa:visual-baseline` — Chromium text rendering differs per OS, so the
darwin-rendered branded captures must not be copied into `linux/`.
