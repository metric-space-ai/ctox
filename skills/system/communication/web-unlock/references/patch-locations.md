# CTOX Stealth Patch-Locations

Map of every stealth-relevant file in CTOX. When a detection-vector lookup
in `detection-vectors.md` points here, this file gives you the line-level
anchors.

---

## `tools/web-stack/src/browser.rs`

Rust crate `ctox-web-stack` — generic browser-automation runner and
doctor/install pipeline.

### Generic runner script (`build_browser_runner_script`)

The runner is generated as a JS string and written to a temp file in the
reference dir, then spawned with node. The interesting bits live around
the `chromium.launch(...)` and `browser.newContext(...)` calls. Look for:

| Region | Purpose |
|---|---|
| `defaultUserAgent` IIFE | Platform-aware UA string for darwin / win32 / linux |
| `hostLocale` IIFE | OS-locale derivation from `LC_ALL` / `LC_MESSAGES` / `LANG` |
| `launchArgs` array | `--user-agent=<UA>`, `--lang=<locale>` (when non-empty) |
| `launchOptions` | `headless: true`, `ignoreDefaultArgs`, `args`, optional `executablePath` |
| `defaultClientHints` IIFE | Sec-CH-UA / Sec-CH-UA-Mobile / Sec-CH-UA-Platform |
| `contextOptions` | `viewport`, `userAgent`, optional `locale`, `extraHTTPHeaders` |
| `addInitScript({ path: ... 'stealth_init.js' })` | Loads the JS-property evasions |

### Module installers

| Function | Writes |
|---|---|
| `ensure_reference_package_json` | `package.json` declaring `patchright ^1.55.0` |
| `ensure_humanlike_module` | `humanlike.mjs` |
| `ensure_stealth_init_module` | `stealth_init.js` |

All three are called from `install_reference`, `run_browser_automation`,
and `capture_browser_transport`. The `include_str!("../assets/...")` bakes
the asset into the binary at compile time, then writes it to the reference
dir on each call (only when content differs).

### Doctor (`build_doctor_report`)

`runner_dependency_installed`, `runner_browser_installed`, etc. The doctor
runs a smoke test (`run_browser_smoke`) that launches Patchright, loads a
`data:text/html` page, and verifies `data-testid=ready`. If `automation_ready`
is false in your diagnosis, run `ctox web browser-prepare --install-reference
--install-browser` first.

---

## `tools/web-stack/assets/google_browser_runner.mjs`

The Google-search-specific runner. Used by `playwright_google` provider in
the cascade.

| Block | Purpose |
|---|---|
| `STEALTH_LAUNCH_ARGS` const | Static launch-arg list (mostly Chromium feature flags) |
| `defaultUserAgent()` fn | Same platform switch as generic runner |
| `defaultClientHints()` fn | Same Sec-CH-UA shape |
| `launchPersistentContext(stateDir, {...})` | Single launch + context call (persists cookies/state across runs in `runtime/google_browser_state/`) |
| `args` line | Includes `STEALTH_LAUNCH_ARGS` + `--user-agent=<launchUserAgent>` + `--lang=<launchLang>` |
| `addInitScript({ path: fileURLToPath(STEALTH_INIT_PATH) })` | Loads same `stealth_init.js` as the generic runner |
| `dismissConsent()` | DE/EN Google cookie-banner clicks |
| `extractResults()` | Pulls result h3/a anchors |

Provider name `"playwright_google"` is preserved as a stable identifier even
though the runtime is now Patchright.

---

## `tools/web-stack/assets/stealth_init.js`

Plain JS (not ESM) that's loaded via `addInitScript({ path })` in BOTH
runners. Runs once on every new document, before any page script.

Layout (in order):

1. **`Function.prototype.toString` proxy** — top of the IIFE, always first.
   `asNative(name, fn)` helper registers patches.
2. **`navigator.webdriver`** — `delete` first, fallback to undefined-getter
3. **`window.chrome`** — full mock with PlatformOs/PlatformArch enums
4. **`navigator.plugins / mimeTypes`** — realistic 5-plugin PDF set with
   uint32-truncated `item()`
5. **`Notification.permission`** — forced 'default' if 'denied'
6. **`navigator.permissions.query`** — notifications consistency
7. **`document.hasFocus`** — always true
8. **`navigator.vibrate`** — defined if missing
9. **WebGL `getParameter`** — UNMASKED_VENDOR/RENDERER platform-mapped
10. **`iframe.contentWindow`** — chrome propagation
11. **`navigator.hardwareConcurrency`** — clamp to >= 4
12. **`navigator.languages`** — fallback to ['en-US','en'] when empty
13. **`outerHeight / outerWidth`** — synthesise from inner
14. **`navigator.connection`** — unconditional rtt=50 mock
15. **`Element.prototype.scrollLeft`** — int >= 0 clamp
16. **`navigator.userAgentData`** — Chrome 146 brands triple + getHighEntropyValues

All wrapped in `try { ... } catch {}` so a single failure doesn't crash
the rest.

---

## `tools/web-stack/assets/humanlike.mjs`

Behavioral primitives. Not loaded via addInitScript; exposed as
`globalThis.humanlike` for skill code to call.

| Export | Purpose |
|---|---|
| `humanMouseMove(page, from, to, options?)` | Cubic Bezier with wobble + overshoot + burst pauses |
| `humanClickAt(page, xy, options?)` | Aim delay + hold timings |
| `humanType(locator, text, options?)` | Uniform-distribution delay + 2% mistype + CDP-trusted shift |
| `humanScroll(page, deltaY, options?)` | 3-phase burst-wheel inertia + overshoot |
| `humanClickLocator(page, locator, options?)` | ensureActionable + humanMouseMove + humanClickAt |
| `ensureActionable(locator, checks, options?)` | Pre-action gating (attached/visible/enabled/editable/pointerEvents) |
| `DEFAULT_HUMAN_CONFIG` | All tunable constants (mouseStepsDivisor, typingDelayMs, mistypeChance, etc.) |

---

## `tools/web-stack/src/web_search.rs`

`google_search()` near line 1781 — orchestrates the Google runner. If
the `playwright_google` provider needs adjustment (e.g. timeout, runtime
limits, payload shape), edit here.

The runner payload (JSON written to runner stdin) carries:
`{ query, language, region, stateDir, maxResults, timeoutMs, headless, userAgent }`.

---

## `install.sh`

`setup_browser_runtime()` near line 887. Runs
`npx patchright install chromium` and triggers `ctox web browser-prepare`.
Touch only if Patchright's CLI changes shape (new flags, different binary name).

---

## `runtime/browser/interactive-reference/`

Operator workspace at runtime. **Not in git.** Contains:

- `package.json` (generated by `ensure_reference_package_json`)
- `node_modules/patchright/` (from `npm install patchright`)
- `ms-playwright/chromium-XXXX/` (from `npx patchright install chromium`)
- `stealth_init.js` (from `ensure_stealth_init_module`)
- `humanlike.mjs` (from `ensure_humanlike_module`)
- `.ctox-browser-run-<pid>-<ts>.mjs` (temp runner per invocation, auto-deleted)
- `.ctox-browser-profile/` (per-context profile dir for generic runner)
- `runtime/google_browser_state/` (sibling — persistent Google session state)

To reset cleanly:

```sh
rm -rf runtime/browser/interactive-reference/{node_modules,package.json,package-lock.json,stealth_init.js,humanlike.mjs}
ctox web browser-prepare --install-reference --install-browser
```

---

## How a patch reaches the runtime

```
asset edit → cargo build -p ctox → fresh ctox binary
                                  ↓
                        operator invokes any of:
                          - web browser-prepare
                          - web browser-automation
                          - web browser-capture
                          - web search (Google provider)
                                  ↓
                        ensure_*_module(reference_dir) detects
                        asset-content drift and rewrites the file
                        in the reference dir
                                  ↓
                        runner spawns node with path to runner script
                          - context.addInitScript({ path: stealth_init.js })
                                  ↓
                        every new document gets the evasions before
                        any page script runs
```

**You do NOT need to `npm install` again after editing only assets.**
Cargo build alone is enough — the `include_str!` in browser.rs makes
the asset bytes part of the binary, and `ensure_*_module` writes them
out on every invocation.

You **do** need `npm install` (via `ctox web browser-prepare --install-reference`)
when:
- The Patchright version pinned in `ensure_reference_package_json` changes
- You wipe `node_modules`
- Chrome binary is missing (in which case also `--install-browser`)
