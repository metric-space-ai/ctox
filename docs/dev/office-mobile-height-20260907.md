# Mobile Shell V2 editor height

Read-only live geometry on Welsch (516px viewport) showed a 642px shell root,
but a 220px module-content and a 162px Word editor iframe. Both mobile shell
rules used `auto minmax(200px, auto) auto` with `align-content: start`, leaving
the middle track content-sized rather than filling the available space.

Both rules now use `auto minmax(auto, 1fr) auto`. The flexible maximum consumes
remaining space; the automatic minimum respects the center's existing explicit
minimum-height. Dock/safe-area padding, pane stacking and overflow declarations
are unchanged. No right Office column or app-specific fixed height was added.

The initial `minmax(200px, 1fr)` candidate fixed Office underfill but introduced
a measurable 20px overlap with long stacked panes: the center had a 220px
minimum while its track was 200px. An added non-overlap assertion caught it.
The final automatic minimum yields 173px / 220px / 173px tracks in that fixture
and preserves reachable scrolling in both side panes and the center.

## Verification

- Native embedded Pi proposal-only review completed with one model request:
  `ctox-dev/output/welsch-office-pi-proposal-1788749082036.json`. The final track
  minimum differs from the initial proposal because of the executed overlap
  regression above.
- New Chromium runner uses the complete real app.css, shared/base.css, each
  Office module's HTML/CSS, actual loaded-wrapper classes and a real iframe
  with deterministic fixture content. It does not mount the actual Office SDK,
  authenticate or exercise persistence.
- Final baseline: 8 failed / 5 passed. Repaired: all 13 passed. Widths
  390/516/720/1180, both editors, overlapping and standalone mobile-sheet rules,
  plus ordinary long content with empty/short/tall stacked panes are covered.
- The exact main-based candidate also passed all 13 cases. Unmodified static
  Shell V2 contract: 37/37 apps; existing Office geometry lab: 6/6 cases at
  window widths 1180/1000/720. That older lab clamps browser width to at least
  780px, so it cannot replace the new true mobile-viewport regression.
- At mobile widths, a 642px root now gives the center 566px after its existing
  76px dock reservation. The fixture's measured 74px header leaves a 492px
  iframe. Desktop remains 642px center / 568px iframe. The live header size
  differs with shell appearance tokens; live geometry must be measured again.
- One initial test assumed tall panes force outer scrolling; actual existing
  CSS scrolls the panes internally. It was corrected to require reachable
  inner overflow and actual clicks on end-of-content controls, with explicit
  non-overlap assertions. No pre-existing guard was weakened.
- CI runs this regression after installing pinned Chromium and before the
  broader Business OS suite. The pre-existing Explorer freeze violation is not
  changed or bypassed.
- The signed shell release workflow also runs the height and SDK save-lifecycle
  regressions before packaging, retaining its existing bridge/artifact gates.

This is a shell height repair, not production-readiness evidence for full Word
or Excel workflows. Signed main-based deployment and actual live editor
geometry/save/reopen acceptance remain required.
