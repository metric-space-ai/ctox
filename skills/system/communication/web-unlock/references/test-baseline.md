# Detection Test Baseline

**Last verified:** 2026-05-16, commit `4c4ad318`.

This is the known-good state of CTOX's browser stealth against the four
canonical probes. Any regression below this baseline is a stealth break.

---

## bot.sannysoft.com

URL: `https://bot.sannysoft.com/`

**Expected:** All 29 tests `passed` / `ok`.

Headless tests (table 1, 10 rows):

| Row | Expected |
|---|---|
| User Agent (Old) | passed (UA contains Chrome/146, **not** HeadlessChrome) |
| WebDriver (New) | `missing (passed)` |
| WebDriver Advanced | `passed` |
| Chrome (New) | `present (passed)` |
| Permissions (New) | `prompt` |
| Plugins Length (Old) | `5` |
| Plugins is of type PluginArray | `passed` |
| Languages (Old) | matches host locale (`de-DE` on de-DE hosts, `en-US` on en-US hosts) |
| WebGL Vendor | `Google Inc. (Apple)` on macOS, `Google Inc. (Intel)` on Linux, `Google Inc. (NVIDIA)` on Windows |
| WebGL Renderer | `ANGLE (Apple, Apple M1, OpenGL 4.1)` on macOS (varies) |
| Broken Image Dimensions | `16x16` |

Fingerprint tests (table 2, 19 rows — fpscanner-style):

All `ok`:
PHANTOM_UA, PHANTOM_PROPERTIES, PHANTOM_ETSL, PHANTOM_LANGUAGE,
PHANTOM_WEBSOCKET, MQ_SCREEN, PHANTOM_OVERFLOW, PHANTOM_WINDOW_HEIGHT,
HEADCHR_UA, HEADCHR_CHROME_OBJ, HEADCHR_PERMISSIONS, HEADCHR_PLUGINS,
HEADCHR_IFRAME, CHR_DEBUG_TOOLS, SELENIUM_DRIVER, CHR_BATTERY,
CHR_MEMORY, TRANSPARENT_PIXEL, SEQUENTUM, VIDEO_CODECS.

**Note:** sannysoft's two tables have 30 rows total, 29 actual tests
(one is a redundant marker). All must pass.

---

## arh.antoinevastel.com

URL: `https://arh.antoinevastel.com/bots/areyouheadless`

**Expected verdict text:** "You are not Chrome headless".

If the page text contains "You are Chrome headless" (no "not"), the
fix has regressed and the most likely cause is a re-introduced
`HeadlessChrome` in the User-Agent — check that
`tools/web-stack/src/browser.rs::build_browser_runner_script`'s
`contextOptions.userAgent` is set and that the launch arg
`--user-agent=...` is present.

---

## bot.incolumitas.com

URL: `https://bot.incolumitas.com/`

**Expected:** 37/37 tests OK across three blocks.

### Block `new-tests` (9/9 OK)

```
puppeteerEvaluationScript: OK
webdriverPresent: OK
connectionRTT: OK
refMatch: OK
overrideTest: OK
overflowTest: OK
puppeteerExtraStealthUsed: OK  (= NOT detected as puppeteer-extra-stealth)
inconsistentWebWorkerNavigatorPropery: OK
inconsistentServiceWorkerNavigatorPropery: OK
```

### Block `detection-tests.intoli` (6/6 OK)

```
userAgent: OK
webDriver: OK
webDriverAdvanced: OK
pluginsLength: OK
pluginArray: OK
languages: OK
```

### Block `detection-tests.fpscanner` (22/22 OK)

```
PHANTOM_UA, PHANTOM_PROPERTIES, PHANTOM_ETSL, PHANTOM_LANGUAGE,
PHANTOM_WEBSOCKET, MQ_SCREEN, PHANTOM_OVERFLOW, PHANTOM_WINDOW_HEIGHT,
HEADCHR_UA, WEBDRIVER, HEADCHR_CHROME_OBJ, HEADCHR_PERMISSIONS,
HEADCHR_PLUGINS, HEADCHR_IFRAME, CHR_DEBUG_TOOLS, SELENIUM_DRIVER,
CHR_BATTERY, CHR_MEMORY, TRANSPARENT_PIXEL, SEQUENTUM, VIDEO_CODECS
— all OK
```

### Service Worker / Web Worker probes

The `serviceWorkerRes` and `webWorkerRes` blocks must show:
- `userAgent: ...Chrome/146...` (NOT HeadlessChrome)
- `appVersion: ...Chrome/146...`
- `language` and `languages` matching the page (typically `de-DE` on de-DE hosts, `en-US` on en-US hosts)

---

## abrahamjuliot.github.io/creepjs

URL: `https://abrahamjuliot.github.io/creepjs/`

**Expected:** Headless score around 33%.

CreepJS reports the score in two places:
- "Like Headless" pattern: ~44%
- "Headless" specific pattern: 33% (commit baseline)

**This score is a structural ceiling for userland-only stealth.** Reducing
it further requires Camoufox or another binary-level approach. As long
as the score is in the 30-50% range, the stealth stack is operating
correctly. Above 60% would indicate a regression.

The presence of `HeadlessChrome` string anywhere in the dumped fingerprint
(via `botRelated` field of the probe output) is a hard failure — that
means a UA leak somewhere.

---

## How to run the baseline

```sh
ctox web browser-automation --script-file skills/system/communication/web-unlock/agents/probe-scripts/sannysoft.js --timeout-ms 60000
ctox web browser-automation --script-file skills/system/communication/web-unlock/agents/probe-scripts/areyouheadless.js --timeout-ms 60000
ctox web browser-automation --script-file skills/system/communication/web-unlock/agents/probe-scripts/incolumitas.js --timeout-ms 120000
ctox web browser-automation --script-file skills/system/communication/web-unlock/agents/probe-scripts/creepjs.js --timeout-ms 90000
```

Pipe each through `jq '.result'` (or similar) to read the structured output.

---

## Historic progression

| Date | Commit | Sannysoft | Antoinevastel | Incolumitas | CreepJS |
|---|---|---|---|---|---|
| 2026-05-15 | (pre-unlock) | ~10 FAIL | "headless" | n/a | 100% |
| 2026-05-16 | `55d73ce` | 2 FAIL | "headless" | n/a | 100% |
| 2026-05-16 | `880d60c` | 2 FAIL | "headless" | n/a | 100% |
| 2026-05-16 | `746f783` | 2 FAIL | "headless" | n/a | 100% |
| 2026-05-16 | `9b990d0` | 0/29 | not headless | n/a | 67% |
| 2026-05-16 | `b1e0f79` | 0/29 | not headless | 4 FAIL | 33% |
| 2026-05-16 | `e8501ba` | 0/29 | not headless | 1 FAIL | 33% |
| 2026-05-16 | `3bb1ea9` | 0/29 | not headless | 0 FAIL (new-tests), 1 FAIL (fpscanner WEBDRIVER) | 33% |
| 2026-05-16 | `4c4ad318` | 0/29 | not headless | 37/37 OK | 33% |
