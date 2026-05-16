# Detection Vectors — Known Probes and Their Fixes

Each entry has: **Vector** (what the site checks) → **Probe** (the exact JS predicate, when known) → **Fix** (the CTOX patch).

Append new entries as you discover them. Keep this table current; it's the skill's working memory.

---

## 1. HeadlessChrome in User-Agent

**Vector:** Default Chromium with `headless: true` emits a UA containing `HeadlessChrome/<ver>`. Trivial to detect.

**Probes:**
- `sannysoft` "User Agent (Old)" — substring match
- `antoinevastel` page-level: substring match
- `creepjs` Headless heading
- `fpscanner` HEADCHR_UA: `/HeadlessChrome/.test(fingerprint.userAgent)`
- `incolumitas` page-level fp

**Fix:** Set explicit `userAgent` in context options of both runners. Use a platform-aware string (darwin → Mac, win32 → Win, else → Linux). **In addition**, pass `--user-agent=<UA>` as a Chromium launch arg so Service/Web Worker contexts also see Chrome/<n> (CDP-only override leaves Workers stale).

**Patched in:**
- `tools/web-stack/src/browser.rs` (build_browser_runner_script): `defaultUserAgent` + `userAgent: defaultUserAgent` + `args: [`--user-agent=...`]`
- `tools/web-stack/assets/google_browser_runner.mjs`: `defaultUserAgent()` + `userAgent: cfg.userAgent || defaultUserAgent()` + `args: [..., `--user-agent=${launchUserAgent}`]`

---

## 2. `navigator.webdriver` exists

**Vector:** `--enable-automation` mode injects a `webdriver` property on `Navigator.prototype` whose value is `true`. fpscanner checks the **existence** of the property, not its value.

**Probes:**
- `fpscanner` WEBDRIVER: `'webdriver' in navigator`
- `sannysoft` WebDriver (New): same idea
- `intoli.webDriver`: checks `navigator.webdriver` value
- `incolumitas.webdriverPresent`: `navigator.webdriver` value

**Fix:** Two parts. (a) `ignoreDefaultArgs: ['--enable-automation']` in launchOptions of both runners suppresses injection. (b) In `stealth_init.js`, `delete Navigator.prototype.webdriver` and `delete navigator.webdriver`. Fall back to a getter that returns `undefined` only if both deletes fail (non-configurable descriptor).

**Patched in:** `tools/web-stack/assets/stealth_init.js` navigator.webdriver block.

---

## 3. `navigator.plugins` empty or fake-shaped

**Vector:** Headless Chrome has zero plugins. Old stealth scripts return `[1,2,3,4,5]` — itself a tell because plugins should be Plugin objects with name/filename/description.

**Probes:**
- `sannysoft` Plugins Length (Old), Plugins is of type PluginArray
- `intoli.pluginsLength`, `intoli.pluginArray`
- `incolumitas.refMatch` — `navigator.plugins[0][0].enabledPlugin === navigator.plugins[0]`
- `incolumitas.overflowTest` — `navigator.plugins.item(2**32) === navigator.plugins[0]`

**Fix:** Build a realistic PluginArray with 5 PDF Viewer plugins (real Chrome 2024+ shape, no Native Client). Each plugin is `Object.create(Plugin.prototype)`. MimeTypeArray with `application/pdf` + `text/pdf`. Critical: `item(i)` must use **uint32-truncate** (`i >>> 0`) so `item(2**32)` wraps to `item(0)` — matches real Chrome behavior.

**Patched in:** `tools/web-stack/assets/stealth_init.js` navigator.plugins / mimeTypes block.

---

## 4. `window.chrome` missing or incomplete

**Vector:** Headless lacks the `chrome` global; or only has `chrome.runtime = {}`. Real Chrome has `chrome.app`, `chrome.csi()`, `chrome.loadTimes()`, `chrome.runtime.OnInstalledReason` etc.

**Probes:**
- `sannysoft` Chrome (New): `present (passed)`
- `fpscanner` HEADCHR_CHROME_OBJ
- `incolumitas` hasChrome boolean

**Fix:** Construct a full `window.chrome` object: `{ app: {...}, csi: fn, loadTimes: fn, runtime: { OnInstalledReason, OnRestartRequiredReason, PlatformOs, PlatformArch, PlatformNaclArch, RequestUpdateCheckStatus } }`. `chrome.runtime.id` returns `undefined` per real Chrome.

**Patched in:** `tools/web-stack/assets/stealth_init.js` window.chrome block.

---

## 5. `permissions.query` notification leak

**Vector:** `navigator.permissions.query({name: 'notifications'})` returning `{state: 'denied'}` while `Notification.permission === 'default'` is a known headless inconsistency.

**Probes:**
- `sannysoft` Permissions (New)
- `fpscanner` HEADCHR_PERMISSIONS
- `incolumitas.puppeteerExtraStealthUsed` — also probes this for stealth-pattern matching

**Fix:** Override `navigator.permissions.query` for `{name: 'notifications'}` to return `{state: 'prompt'}` when `Notification.permission === 'default'`. **Also** patch `Notification.permission` getter to return `'default'` when it's `'denied'`.

**Patched in:** `tools/web-stack/assets/stealth_init.js` permissions.query and Notification.permission blocks.

---

## 6. Sec-CH-UA client hints leak HeadlessChrome

**Vector:** Even with `userAgent` override set in context options, the HTTP `Sec-CH-UA`, `Sec-CH-UA-Mobile`, `Sec-CH-UA-Platform` headers still announce the original browser identity. CreepJS and FingerprintJS read these from HTTP, not from JS.

**Probes:**
- `creepjs` Headless brand detection
- Various HTTP-layer fingerprinters

**Fix:** Set `extraHTTPHeaders` on the context with:
- `Sec-CH-UA: '"Chromium";v="146", "Google Chrome";v="146", "Not.A/Brand";v="24"'`
- `Sec-CH-UA-Mobile: '?0'`
- `Sec-CH-UA-Platform: '"macOS"'` (or `'"Windows"'` / `'"Linux"'`)

**Plus** the JS-side counterpart: mock `navigator.userAgentData` with matching brands, `mobile: false`, `platform`, and a working `getHighEntropyValues()` that returns `architecture`, `bitness`, `fullVersionList`, etc.

**Patched in:** both runners' contextOptions `extraHTTPHeaders` field + `stealth_init.js` navigator.userAgentData block.

---

## 7. Service Worker / Web Worker UA inconsistency

**Vector:** Page-level CDP `Network.setUserAgentOverride` does NOT reach Service Worker or Web Worker JS contexts. Worker `navigator.userAgent` still shows HeadlessChrome. incolumitas compares them.

**Probes:**
- `incolumitas.inconsistentServiceWorkerNavigatorPropery`
- `incolumitas.inconsistentWebWorkerNavigatorPropery`

**Fix:** Pass `--user-agent=<UA>` as Chromium launch arg. This is browser-process-global and covers Workers. The context-level override stays in place too for layered defense.

**Patched in:** `args: [..., `--user-agent=${UA}`]` in launchOptions of both runners.

---

## 8. Worker locale inconsistency

**Vector:** Page locale set via context.locale uses CDP override. Workers read OS locale directly. If `LANG=de_DE.UTF-8` on host but context locale is `en-US`, page and worker diverge.

**Probes:**
- `incolumitas.inconsistentWebWorker/ServiceWorker` (locale arm)
- `incolumitas.inconsistentLanguages`

**Fix:** Derive locale from `process.env.LC_ALL || LC_MESSAGES || LANG`. Set context.locale AND `--lang=<locale>` launch arg to the same derived value. If env is empty, leave both unset — Chromium and Worker then fall back to OS default in lockstep.

**Patched in:** `hostLocale` computation in `build_browser_runner_script` + `--lang=${hostLocale}` launch arg.

---

## 9. `navigator.connection.rtt === 0`

**Vector:** Headless reports `rtt: 0` while real Chrome rounds rtt to nearest 25ms with privacy budget. 0 means "unmeasured" and is a tell.

**Probes:**
- `incolumitas.connectionRTT`: `connectionRtt === 0 ? FAIL : OK`

**Fix:** Unconditionally mock `navigator.connection` with `{rtt: 50, downlink: 10, effectiveType: '4g', saveData: false, type: 'wifi'}` plus EventTarget methods. **Important:** don't condition on `rtt === 0` at init time — the value flips to 0 after page load on real pages, missing the patch window.

**Patched in:** `tools/web-stack/assets/stealth_init.js` navigator.connection block (unconditional override).

---

## 10. WebGL renderer/vendor swiftshader leak

**Vector:** Headless reports `Google SwiftShader` / `SwiftShader` as renderer. Real Chrome on macOS reports `ANGLE (Apple, Apple M1, OpenGL 4.1)`, on Linux `ANGLE (Intel, ...)`, on Windows `ANGLE (NVIDIA, ...)`.

**Probes:**
- `sannysoft` WebGL Vendor / WebGL Renderer

**Fix:** Patch `WebGLRenderingContext.prototype.getParameter` (and WebGL2) to return platform-mapped values for `UNMASKED_VENDOR_WEBGL` (37445) and `UNMASKED_RENDERER_WEBGL` (37446). Map from `navigator.platform`.

**Patched in:** `tools/web-stack/assets/stealth_init.js` WebGL block.

---

## 11. `Function.prototype.toString` reveals patched functions

**Vector:** `myOverrideFn.toString()` returns `"function (...) { ... my code ... }"` instead of `"function name() { [native code] }"`. Reveals every Object.defineProperty override.

**Probes:**
- Generic — read any property getter via `Object.getOwnPropertyDescriptor(...).get.toString()`
- `incolumitas.puppeteerExtraStealthUsed` (in conjunction with permissions.query shape)

**Fix:** Proxy `Function.prototype.toString` with `apply` trap. Maintain a WeakMap `patchedFns` mapping our overrides to their fake `[native code]` strings. Helper `asNative(name, fn)` registers each override.

**Patched in:** Top of `stealth_init.js` — must be the very first thing.

**Caveat:** CreepJS detects the *pattern* of having a Function.prototype.toString proxy via double-toString chaining. Cannot be fully bypassed in userland — it's the reason CreepJS sits at ~33% headless and won't budge.

---

## 12. `Element.scrollLeft` not int-clamped

**Vector:** Real Chrome clamps scrollLeft setter to int >= 0. Headless variants sometimes preserve floats or accept negative values.

**Probes:**
- Sometimes folded into broader overflow checks (incolumitas `overflowTest` is a DIFFERENT test — see entry #3)

**Fix:** Override `Element.prototype.scrollLeft` setter to `Math.max(0, Math.floor(value))`.

**Patched in:** `tools/web-stack/assets/stealth_init.js` overflow block.

---

## 13. `navigator.hardwareConcurrency` too low

**Vector:** Headless containers often report `hardwareConcurrency: 1`. Real devices have 4+ in 2024+.

**Probes:**
- Generic fingerprinters
- `incolumitas` page-level fp

**Fix:** If `< 4`, override getter on `Navigator.prototype` to return `8`.

**Patched in:** `tools/web-stack/assets/stealth_init.js` hardwareConcurrency block.

---

## 14. `outerHeight / outerWidth === 0`

**Vector:** Headless reports both as 0. Real headed Chrome reports `innerHeight + browser-chrome` and `innerWidth`.

**Probes:**
- `fpscanner` PHANTOM_WINDOW_HEIGHT
- `sannysoft` Browser dimension tests

**Fix:** If 0, override to `innerHeight + 85` / `innerWidth`.

**Patched in:** `tools/web-stack/assets/stealth_init.js` outerHeight/outerWidth block.

---

## 15. `iframe.contentWindow` lacks chrome global

**Vector:** Anti-bots open an `about:blank` iframe and probe `iframe.contentWindow.chrome` — if missing, the page-level chrome mock didn't propagate.

**Probes:**
- `fpscanner` HEADCHR_IFRAME
- `sannysoft` Chrome iframe test

**Fix:** Override `HTMLIFrameElement.prototype.contentWindow` getter to propagate `window.chrome` into the contentWindow if not already there.

**Patched in:** `tools/web-stack/assets/stealth_init.js` iframe block.

---

## 16. `document.hasFocus() === false`

**Vector:** Headless contexts don't get focus by default — `hasFocus()` returns false. Real users on a foregrounded tab return true.

**Fix:** Override `document.hasFocus` to `() => true || origHasFocus.call(document)`.

**Patched in:** `tools/web-stack/assets/stealth_init.js` hasFocus block.

---

## 17. `navigator.vibrate` missing

**Vector:** Modern Chrome desktop always has `navigator.vibrate` as a function. Some headless builds strip it.

**Fix:** Define on `Navigator.prototype` if missing — returns `true`.

**Patched in:** `tools/web-stack/assets/stealth_init.js` vibrate block.

---

## Pattern-detection cluster (NOT solvable per single vector)

These tell whether *any* stealth is present, not which property is patched:

- **CreepJS "Stealth" heading + "Like Headless"** scores aggregate the number of overrides. Adding more patches makes it WORSE (more evidence of tampering), removing patches makes the other probes fail. Stable structural ceiling ~33%.
- **`incolumitas.puppeteerExtraStealthUsed`** — looks at Function.prototype prototype chain to detect puppeteer-extra-stealth specifically. We already pass this (`OK` = not detected) because our shape doesn't match puppeteer-extra-stealth exactly. But future updates may catch us.
- **FingerprintJS Premium** — commercial, behavioral.

If a regression here lands, **document the diagnosis and stop**. Do not invent more patches for these — the gain is negative.

---

## Adding a new vector

When you discover a new failing test:

1. Add an entry below in this file using the same format.
2. Note the patch file and key terms (block heading).
3. Update `test-baseline.md` with the post-fix expected result.
4. Commit message should mention "New vector: <site>.<test>" for grep-ability.

---

## 18. (template for next entry)

**Vector:**

**Probes:**

**Fix:**

**Patched in:**
