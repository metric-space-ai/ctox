import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const qaDir = dirname(fileURLToPath(import.meta.url));
const businessOsDir = join(qaDir, '..');
const css = readFileSync(join(businessOsDir, 'themes', 'ctox-desktop-shell.css'), 'utf8');
const appCss = readFileSync(join(businessOsDir, 'app.css'), 'utf8');
const baseCss = readFileSync(join(businessOsDir, 'shared', 'base.css'), 'utf8');
const chatJs = readFileSync(join(businessOsDir, 'shared', 'business-chat.js'), 'utf8');
const appJs = readFileSync(join(businessOsDir, 'app.js'), 'utf8');
const shellReleaseStatusJs = readFileSync(
  join(businessOsDir, 'shared', 'shell-release-status.js'),
  'utf8',
);
const indexHtml = readFileSync(join(businessOsDir, 'index.html'), 'utf8');
const html = readFileSync(join(qaDir, 'ctox-desktop-shell.html'), 'utf8');
const js = readFileSync(join(qaDir, 'ctox-desktop-shell.js'), 'utf8');
const scope = 'html[data-desktop-host="ctox"]';
// Every assertion below inspects RULES, never prose. The host layer's own doc
// comment necessarily names :root, !important, @media and ctox-desktop-guest
// while explaining why none of them appear in it any more.
const cssRules = stripComments(css);
const indexHtmlMarkup = indexHtml.replace(/<!--[\s\S]*?-->/g, '');

function stripComments(source) {
  return source.replace(/\/\*[\s\S]*?\*\//g, '');
}

function styleRuleHeaders(source) {
  const stripped = stripComments(source);
  const headers = [];

  for (let index = 0; index < stripped.length; index += 1) {
    if (stripped[index] !== '{') continue;
    let start = index - 1;
    while (start >= 0 && !'{};'.includes(stripped[start])) start -= 1;
    const header = stripped.slice(start + 1, index).trim();
    if (header) headers.push(header);
  }

  return headers.filter((header) => !header.startsWith('@'));
}

function splitSelectors(header) {
  const selectors = [];
  let depth = 0;
  let start = 0;

  for (let index = 0; index < header.length; index += 1) {
    if ('(['.includes(header[index])) depth += 1;
    if (')]'.includes(header[index])) depth -= 1;
    if (header[index] === ',' && depth === 0) {
      selectors.push(header.slice(start, index).trim());
      start = index + 1;
    }
  }

  selectors.push(header.slice(start).trim());
  return selectors;
}

test('the host layer stays scoped to the desktop host attribute', () => {
  assert.match(html, /<html[^>]+data-desktop-host="ctox"/);
  const selectors = styleRuleHeaders(css).flatMap(splitSelectors);
  assert.ok(selectors.length > 0, 'host layer should still exist');
  for (const selector of selectors) {
    assert.ok(selector.startsWith(scope), `unscoped host-layer selector: ${selector}`);
  }
  assert.doesNotMatch(cssRules, /(^|[\s,{]):root\b/m);
});

// The whole point of the change this file guards: the desktop-adapted design
// IS the product design. The host layer may only change structural layout that
// follows from embedding the guest in Workjet. The moment it declares a colour,
// a radius, a font or a shadow again, browser and desktop have started drifting
// into two products.
test('the host layer declares no design of its own', () => {
  const declarations = stripComments(css)
    .split('}')
    .flatMap((block) => block.split('{').slice(1))
    .flatMap((body) => body.split(';'))
    .map((entry) => entry.trim())
    .filter(Boolean);

  assert.ok(declarations.length > 0, 'host layer should still declare something');

  const allowedProperties = new Set([
    'display', 'grid-template-rows', 'inset',
    'width', 'min-width', 'max-width',
    'height', 'min-height', 'max-height',
  ]);
  for (const declaration of declarations) {
    const property = declaration.slice(0, declaration.indexOf(':')).trim();
    assert.ok(
      allowedProperties.has(property),
      `host layer may only control embedded structure, found "${property}" in "${declaration}"`,
    );
  }

  assert.doesNotMatch(cssRules, /!important/, 'the host layer must not need to out-shout the shell');
  assert.doesNotMatch(cssRules, /--(bg|surface|line|text|muted|accent|panel-radius|control-radius|shadow)\b\s*:/,
    'primitive tokens belong on :root in app.css, not in the host layer');
  assert.doesNotMatch(cssRules, /\b(color|background|background-color|border|border-radius|box-shadow|font-family|font-size|backdrop-filter)\s*:/,
    'the host layer must not restyle anything');
  assert.doesNotMatch(cssRules, /gradient\s*\(/i);
  assert.doesNotMatch(cssRules, /url\s*\(/i);
  assert.doesNotMatch(cssRules, /(?:https?:)?\/\//i);
});

test('the signed desktop guest cannot render a second global menu', () => {
  assert.match(
    cssRules,
    /html\[data-desktop-host="ctox"\] \.topbar,\s*html\[data-desktop-host="ctox"\] \.ctox-maintenance-banner,\s*html\[data-desktop-host="ctox"\] \.shell-start-menu-panel\s*\{[^}]*display:\s*none;/,
    'the desktop host must suppress the shell topbar, status chrome and start menu',
  );
  assert.match(
    cssRules,
    /html\[data-desktop-host="ctox"\] \.app-shell\s*\{[^}]*grid-template-rows:\s*minmax\(0,\s*1fr\);/,
    'the app canvas must reclaim the row formerly reserved for global chrome',
  );
  assert.match(indexHtmlMarkup, /<header class="topbar">/,
    'standalone browser presentation must retain the normal shell topbar');
  assert.match(indexHtmlMarkup, /data-shell-start/,
    'standalone browser presentation must retain the normal start-menu entry');
});

test('desktop host keeps app canvas, windows and owned chat outside the chrome suppression rule', () => {
  assert.doesNotMatch(cssRules, /(?:\.workspace-frame|\.workspace-pane-center|\[data-module-host\]|\.ctox-chat-(?:root|dock))[^,{]*\{[^}]*display:\s*none/);
  assert.match(
    cssRules,
    /html\[data-desktop-host="ctox"\] \.shell-window-layer\s*\{[^}]*inset:\s*0;[^}]*width:\s*100%;[^}]*min-width:\s*0;[^}]*height:\s*100%;/,
    'app windows must use the full embedded host canvas after the topbar is removed',
  );
});

// It was linked from nowhere before, which is exactly how a browser tenant and
// the desktop ended up rendering differently: the desktop's pinned bundle had
// the theme, deployed tenants did not.
test('the host layer is linked unconditionally from the shell', () => {
  assert.match(indexHtml, /<link rel="stylesheet" href="themes\/ctox-desktop-shell\.css/);
  const appIndex = indexHtml.indexOf('href="app.css');
  const baseIndex = indexHtml.indexOf('href="shared/base.css');
  const themeIndex = indexHtml.indexOf('href="themes/ctox-desktop-shell.css');
  assert.ok(themeIndex > appIndex && themeIndex > baseIndex, 'host layer must load after the shell stylesheets');
  assert.doesNotMatch(indexHtmlMarkup, /data-desktop-host="ctox"/,
    'the shell must not set the host attribute itself — only the Electron host does');
});

// The palette that used to live in the host layer now lives once, on :root,
// and stays retintable by the desktop through --ctox-host-*.
test('the primitive palette is host-tintable on :root', () => {
  for (const token of ['--bg', '--surface', '--surface-2', '--surface-3', '--line', '--hairline', '--text', '--text-strong', '--muted', '--accent', '--accent-foreground', '--accent-soft']) {
    const pattern = new RegExp(`${token}:\\s*var\\(--ctox-host-${token.slice(2)},[^)]+\\)`, 'g');
    assert.equal((appCss.match(pattern) || []).length, 2,
      `${token} must read var(--ctox-host-*, <fallback>) on both :root blocks`);
  }
  assert.equal((appCss.match(/--panel-radius:\s*6px/g) || []).length, 2);
  assert.equal((appCss.match(/--control-radius:\s*4px/g) || []).length, 2);
});

// The flat chrome is the default, not an override: the shared topbar token and
// hairline separators must be declared by the shell itself.
test('the flat chrome is declared by the shell, unscoped', () => {
  assert.match(appCss, /--shell-topbar-height:\s*48px/);
  assert.match(appCss, /\.app-shell \{[^}]*grid-template-rows: var\(--shell-topbar-height\) minmax\(0, 1fr\)/);
  assert.match(appCss, /\.topbar \{[^}]*min-height: var\(--shell-topbar-height\)/);
  assert.match(appCss, /\.topbar \{[^}]*border-bottom: 1px solid var\(--hairline, var\(--line\)\)/);
  assert.match(chatJs, /\.ctox-chat-dock \{[^}]*border: 1px solid var\(--line\)/);
  assert.match(
    appCss,
    /:root\[data-shell-style="ctox"\] \.topbar \{\s*margin: 0;\s*border-radius: 0;[\s\S]*?backdrop-filter: none !important;/,
    'the final Workjet contract layer must flatten the legacy CTOX island',
  );
});

// Regression, twice over: an expanded dock holding no chat window reserved a
// fixed ~340px of the viewport above the 44px bar. The measured version of
// this lives in scripts/assert-business-chat-behavior.mjs; this is the cheap
// static guard that the collapsing class still exists and is still applied.
test('an empty chat stage collapses to zero height', () => {
  assert.match(chatJs, /const stagedWindowCount = dockCollapsed \? 0 : visibleWindowChats\.length;/);
  assert.match(chatJs, /stagedWindowCount === 0 \? 'is-empty'/);
  assert.match(chatJs, /\.ctox-chat-stage-inner\.is-empty \{[^}]*height: 0;/);
  assert.match(chatJs, /\.ctox-chat-stage-inner\.is-empty \{[^}]*padding-top: 0;/);
  assert.match(chatJs, /\.ctox-chat-stage-inner\.is-empty \{[^}]*padding-bottom: 0;/);
  assert.match(chatJs, /\.ctox-chat-stage-inner:not\(\.is-empty\) \{/,
    'the narrow breakpoint must exempt the empty stage instead of out-shouting it');
});

test('a scrubbed Workjet launch remains reloadable without storing another secret', () => {
  assert.match(appJs, /const CTOX_WORKSPACE_SCOPE_HINT_KEY = 'workjet\.businessOs\.workspaceScope';/);
  assert.match(appJs, /sessionStorage\.setItem\(CTOX_WORKSPACE_SCOPE_HINT_KEY, instanceId\)/);
  assert.match(appJs, /sessionStorage\.getItem\(CTOX_WORKSPACE_SCOPE_HINT_KEY\)/);
  assert.doesNotMatch(
    appJs,
    /sessionStorage\.setItem\([^\n]*(?:room_password|signaling_room_password|capability_token)/,
    'session storage may carry the opaque workspace hint, never pairing material',
  );
});

test('the static Workjet shell never probes a nonexistent HTTP maintenance plane', () => {
  assert.match(appJs, /if \(isWorkjetStaticShellLaunch\(\)\) \{[\s\S]*?return;[\s\S]*?\}/);
  assert.match(
    appJs,
    /document\.documentElement\.dataset\.workjetMobileHost === 'true'/,
    'the native RxDB\/WebRTC host must never probe the maintenance HTTP plane',
  );
  assert.match(appJs, /'local_daemon',[\s\S]*?'ssh_managed',[\s\S]*?'pairing_invite'/);
  assert.match(appJs, /fetchBusinessOsControlJson\('\/api\/business-os\/ctox\/maintenance'\)/);
  assert.match(
    shellReleaseStatusJs,
    /document\.documentElement\.dataset\.workjetMobileHost === 'true'\) return;/,
    'the native host owns shell-pack status and must not fetch a relative manifest',
  );
});

// Responsive behaviour is the shell's, and there is exactly one of it. The
// host layer used to re-implement the collapse as a parallel
// `ctox-desktop-guest` container layer; two cascades for one job is how narrow
// width broke last time.
test('there is a single responsive cascade and the host layer does not duplicate it', () => {
  assert.doesNotMatch(cssRules, /@container/);
  assert.doesNotMatch(cssRules, /@media/);
  assert.doesNotMatch(cssRules, /ctox-desktop-guest/);
  assert.match(baseCss, /@container business-app-window \(max-width: 768px\)/);
  assert.match(baseCss, /\.ctox-workspace > \[data-resizer\]/,
    'the single-column collapse must hide both resizer implementations');
  assert.match(appCss, /container-name: business-app-window/);
  assert.match(appCss, /@media \(max-width: 900px\)/);
  assert.match(appCss, /@media \(max-width: 600px\)/);
});

test('accessible focus and reduced motion remain shell defaults', () => {
  assert.match(baseCss, /--focus-ring:/);
  assert.match(baseCss, /:focus-visible/);
  assert.match(appCss, /@media \(prefers-reduced-motion:\s*reduce\)/);
  assert.match(appCss, /animation-duration:\s*0\.01ms\s*!important/);
});

test('fixture and fixture script use no remote assets', () => {
  const source = `${html}\n${js}`;
  assert.doesNotMatch(source, /(?:https?:)?\/\//i);
  assert.doesNotMatch(html, /<(?:img|iframe|video|audio|source)\b/i);
  assert.doesNotMatch(js, /\b(?:fetch|XMLHttpRequest|WebSocket|EventSource)\b/);
  for (const asset of html.matchAll(/(?:href|src)="([^"]+)"/g)) {
    assert.match(asset[1], /^\.\.\/|^\.\//, `asset must be local: ${asset[1]}`);
  }
});

test('fixture excludes duplicate desktop mode and instance controls', () => {
  assert.doesNotMatch(html, /Code\s*(?:\||\/|and)\s*Business OS/i);
  assert.doesNotMatch(html, /data-(?:mode-switch|product-mode|instance-picker|instance-sidebar)\b/i);
  assert.doesNotMatch(html, /class="[^"]*(?:instance-sidebar|desktop-sidebar|mode-switch)[^"]*"/i);
  assert.doesNotMatch(html, /aria-label="[^"]*(?:choose|select|switch) instance[^"]*"/i);
  assert.doesNotMatch(html, /traffic[- ]lights?/i);
});

test('fixture proves the production Business OS shell and workspace grammar', () => {
  const productionCss = `${appCss}\n${baseCss}`;
  for (const className of [
    'app-shell',
    'topbar',
    'module-nav',
    'module-tabs',
    'module-tab',
    'workspace-frame',
    'workspace-pane-center',
    'module-root',
    'module-content',
    'ctox-workspace',
    'ctox-pane',
    'ctox-pane-band',
    'ctox-pane-body',
    'ctox-action-strip',
    'ctox-run-control',
    'ctox-table',
    'ctox-card',
    'ctox-fields',
  ]) {
    assert.match(html, new RegExp(`class="[^"]*\\b${className}\\b`), `fixture must render .${className}`);
    assert.ok(productionCss.includes(`.${className}`), `fixture uses non-production class .${className}`);
  }

  assert.equal((html.match(/<(?:aside|section) class="ctox-pane"/g) || []).length, 3, 'fixture must render three workbench panes');
  assert.match(html, /aria-label="Inspector and activity"/);
  assert.match(html, />Assistant task</);
  assert.match(html, /data-resizer="left"/);
  assert.match(html, /data-resizer="right"/);
});

test('fixture-only script is deterministic and limited to QA toggles', () => {
  assert.match(js, /root\.dataset\.theme/);
  assert.match(js, /shell\.style\.width/);
  assert.match(js, /680px/);
  assert.doesNotMatch(js, /localStorage|sessionStorage|Date\(|Math\.random|setTimeout|setInterval/);
  assert.doesNotMatch(js, /import\s|export\s/);
});
