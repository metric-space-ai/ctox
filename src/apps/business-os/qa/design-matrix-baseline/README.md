# Business OS Design Matrix Baseline

Baselines are separated by `process.platform` because Chromium text rendering
is operating-system dependent. CI and Release use the reviewed `linux/`
baseline; macOS development uses `darwin/` at the same strict mismatch limit.

Generate only through `npm run qa:visual-baseline` on the target platform and
commit only after human visual review. Ordinary QA selects the matching
immutable platform baseline and never updates it implicitly. A platform without
a reviewed baseline fails closed.
