# Reviewed Linux design matrix

These 24 PNGs were captured on Linux, then imported with the existing
`qa:visual-baseline` updater (`--platform linux --actual <downloaded matrix>`).
No macOS-rendered pixels are included. Pixel threshold 0.08 and maximum mismatch
ratio 0.005 remain unchanged.

## Provenance and review (issue #178)

- Source PR head: b86610b9d7e8748347518750f2d8c6a0a7958974.
- Actual CI checkout: af5fc14 (merge into main 9dab76e73d2949bf011914e38a3becdb3db24267).
- Run: https://github.com/metric-space-ai/ctox/actions/runs/35471483616
- Job: 105973121291; artifact: 10593973993.
- Ubuntu 22.04.5, runner image 20260907.292.1.
- Chromium 148.0.7778.96, Playwright Chromium revision 1223.
- The source app.css, shared/base.css, shared/branding.js, design-lab.html,
  capture script and package-lock.json match the main base byte for byte.
- Prior baseline: c2e8a3e2e7e385d4655c03e1aa2562c08a012196 (2026-08-22).
- Canonical theme change: f0ae0bd1e6014ae652225998c8a69549f17277ab (2026-08-26).

Codex visually inspected all 24 before/after pairs on 2026-09-20: widths
640/960/1180, German/English, light/dark, default/custom brand. The default
palette moves from teal/blue-gray to the canonical Workjet blue/neutral tokens;
custom warm-paper/plum and violet accents remain intact. System typography is
wider/heavier, with corresponding button widths and table column allocation.
No clipped text, missing controls, overlapping content or broken responsive
stacking was observed. The 640 frame stacks panes; 960 and 1180 retain two panes.
All original capture overflow and accessible-button-name guards passed.
This is an agent review; human approval remains with the PR reviewer.

The font contract is app.css's system stack:
`-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif`, with
`font-synthesis: none`. The old artifact did not record resolved font files;
we do not infer their exact identity from pixels. The dedicated Linux validation
records its browser version, computed CSS font family, source revision, OS,
Fontconfig system-ui match and installed font inventory with the fresh captures.

This fixture review is not authenticated shell or live-tenant acceptance.
