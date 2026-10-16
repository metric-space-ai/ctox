import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { registerHooks } from 'node:module';
// v4: index.js importiert 'ctox:shared/…' (loest sonst nur der Service Worker auf, PORTING §4 Falle 2).
registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('ctox:shared/')) {
      return next(new URL(`../../shared/${specifier.slice('ctox:shared/'.length)}`, import.meta.url).href, context);
    }
    return next(specifier, context);
  },
});
const { __browserTestHooks } = await import('./index.js');
import { __ctoxSyncTestHooks } from '../../shared/sync.js';

assert.deepEqual(
  __browserTestHooks.SCRAPING_ADAPTER_COLLECTIONS,
  ['outbound_research_adapters', 'outbound_lead_generation_adapters'],
  'the Browser scraping rail must include both core and managed tenant adapters',
);

assert.equal(__browserTestHooks.normalizeUrl('example.com'), 'https://example.com');
assert.equal(__browserTestHooks.normalizeUrl('http://localhost:3000/path'), 'http://localhost:3000/path');
assert.equal(__browserTestHooks.normalizeUrl(''), 'https://example.com');
assert.equal(__browserTestHooks.formatBytes(512), '512 B');
assert.equal(__browserTestHooks.formatBytes(1536), '1.5 KB');
assert.equal(__browserTestHooks.titleCase('browser_frames'), 'Browser frames');
assert.equal(
  __browserTestHooks.userSessionPrefix({ user: { id: 'Michael.Welsch@example.com' } }),
  'browser_session_michael-welsch-example-com',
);
assert.deepEqual(
  __browserTestHooks.browserActorIds({
    user: {
      id: 'user-1',
      email: 'michael@example.com',
      login: 'michael',
    },
  }),
  ['user-1', 'michael@example.com', 'michael'],
);
assert.deepEqual(__browserTestHooks.selectedViewport({ value: '390x844' }), { width: 390, height: 844 });
assert.equal(
  __browserTestHooks.browserSessionIdFromArgs({ session_id: 'browser_session_web_stack_auth_xing-com' }),
  'browser_session_web_stack_auth_xing-com',
);
assert.equal(__browserTestHooks.browserSessionIdFromArgs({ session_id: 'not-a-browser-session' }), '');
assert.equal(__browserTestHooks.browserSessionNeedsStart(null), true);
assert.equal(__browserTestHooks.browserSessionNeedsStart({ id: 'browser_session_a', runtime_status: 'disconnected' }), true);
assert.equal(__browserTestHooks.browserSessionNeedsStart({ id: 'browser_session_a', runtime_status: 'error' }), true);
assert.equal(__browserTestHooks.browserSessionNeedsStart({ id: 'browser_session_a', runtime_status: 'starting' }), false);
assert.equal(__browserTestHooks.browserSessionNeedsStart({ id: 'browser_session_a', runtime_status: 'active' }), false);
assert.equal(__browserTestHooks.browserSessionIsLive({ id: 'browser_session_a', runtime_status: 'active' }), true);
assert.equal(__browserTestHooks.browserSessionIsLive({ id: 'browser_session_a', runtime_status: 'starting' }), false);
assert.equal(
  __browserTestHooks.browserSessionError({
    last_error: 'browser session limit for this user reached: 3/3 live sessions',
    error: 'shortened fallback',
  }),
  'browser session limit for this user reached: 3/3 live sessions',
  'session truth uses the native last_error without replacing it',
);
assert.equal(
  __browserTestHooks.browserUiState({ runtime_status: 'disconnected' }),
  'offline',
  'a disconnected row is not a live browser process',
);
assert.equal(
  __browserTestHooks.browserSessionBand({ runtime_status: 'synthetic' }),
  'closed',
  'a synthetic row without Chromium is not active',
);
assert.equal(
  __browserTestHooks.browserStatusLabel({ runtime_status: 'blocked' }),
  'Aktion erforderlich',
  'an explainable runtime limit is shown as actionable instead of a generic error',
);
assert.equal(
  __browserTestHooks.frameEmptyText({
    latestSession: { runtime_status: 'error', last_error: 'Chromium executable is not available' },
    latestCommand: null,
  }),
  'Chromium executable is not available',
  'the empty browser surface shows the exact native runtime reason',
);
assert.equal(__browserTestHooks.browserStartErrorIsRetryable({ code: 'sync_unavailable' }), true);
assert.equal(__browserTestHooks.browserStartErrorIsRetryable({ code: 'auth_required' }), false);
assert.equal(__browserTestHooks.browserCommandRequiresController('browser.navigate', { id: 'browser_session_test' }), true);
assert.equal(__browserTestHooks.browserCommandRequiresController('browser.controller.acquire', { id: 'browser_session_test' }), false);
assert.equal(__browserTestHooks.browserCommandRequiresController('browser.session.start', null), false);
// v4: kein Shell-DOM-Vorfahr (.shell-window.is-focused); die App führt den Fensterfokus selbst (V4-PORT OFFEN-3).
assert.equal(__browserTestHooks.browserSurfaceIsFocused({ surfaceFocused: false }), false);
assert.equal(__browserTestHooks.browserSurfaceIsFocused({ surfaceFocused: true }), true);
assert.equal(
  __browserTestHooks.shouldRenewControllerLease({
    id: 'browser_session_test',
    controller_user_id: 'user-1',
    controller_lease_id: 'lease-1',
    controller_lease_expires_at_ms: 1_060_000,
  }, 'user-1', 1_000_000, { controllerLeaseId: 'lease-1' }),
  true,
);
assert.equal(
  __browserTestHooks.shouldRenewControllerLease({
    id: 'browser_session_test',
    controller_user_id: 'user-1',
    controller_lease_id: 'lease-1',
    controller_lease_expires_at_ms: 1_000_000,
  }, 'user-1', 1_000_000, { controllerLeaseId: 'lease-1' }),
  false,
  'an expired lease must not produce an endless rejected renew loop',
);
assert.equal(
  __browserTestHooks.shouldRenewControllerLease({
    id: 'browser_session_test',
    controller_user_id: 'user-2',
    controller_lease_id: 'lease-1',
    controller_lease_expires_at_ms: 1_060_000,
  }, 'user-1', 1_000_000, { controllerLeaseId: 'lease-1' }),
  false,
);
assert.equal(
  __browserTestHooks.shouldRenewControllerLease({
    id: 'browser_session_test',
    controller_user_id: 'user-1',
    controller_lease_id: 'lease-1',
    controller_lease_expires_at_ms: 1_090_000,
  }, 'user-1', 1_000_000, { controllerLeaseId: 'lease-1' }),
  false,
  'a healthy lease should not be renewed early',
);
for (const blockedState of [
  { documentVisible: false },
  { documentFocused: false },
  { surfaceFocused: false },
  { renewInFlight: true },
]) {
  assert.equal(
    __browserTestHooks.shouldRenewControllerLease({
      id: 'browser_session_test',
      controller_user_id: 'user-1',
      controller_lease_id: 'lease-1',
      controller_lease_expires_at_ms: 1_060_000,
    }, 'user-1', 1_000_000, { ...blockedState, controllerLeaseId: 'lease-1' }),
    false,
    `lease renewal must stop for passive surface state ${JSON.stringify(blockedState)}`,
  );
}
assert.equal(
  __browserTestHooks.shouldRenewControllerLease({
    id: 'browser_session_test',
    controller_user_id: 'user-1',
    controller_lease_id: '',
    controller_lease_expires_at_ms: 1_060_000,
  }, 'user-1', 1_000_000, { controllerLeaseId: '' }),
  false,
  'a renewal without the current lease id cannot be authoritative',
);
assert.equal(
  __browserTestHooks.shouldRenewControllerLease({
    id: 'browser_session_test',
    controller_user_id: 'user-1',
    controller_lease_id: 'lease-remote',
    controller_lease_expires_at_ms: 1_060_000,
  }, 'user-1', 1_000_000, { controllerLeaseId: 'lease-local' }),
  false,
  'a replicated lease owned by another surface must stay passive',
);
const focusedCtx = { actor: { id: 'user-1' }, surfaceFocused: true };
const activeSession = {
  id: 'browser_session_test',
  controller_user_id: 'user-1',
  controller_lease_id: 'lease-local',
  controller_lease_expires_at_ms: 1_060_000,
};
assert.equal(
  __browserTestHooks.browserSurfaceCanControl(focusedCtx, {
    latestSession: activeSession,
    controllerLeaseId: 'lease-local',
  }, 1_000_000),
  true,
);
assert.equal(
  __browserTestHooks.browserSurfaceCanControl({
    // v4-Akteur (api.actor.id) mit der weiterhin lesbaren v3-Form {user:{id,email}}.
    actor: { id: 'user-1', user: { id: 'user-1', email: 'michael@example.com' } },
    surfaceFocused: true,
  }, {
    latestSession: {
      ...activeSession,
      controller_user_id: 'michael@example.com',
    },
    controllerLeaseId: 'lease-local',
  }, 1_000_000),
  true,
  'the capability actor email must control a session even when the shell user also has an opaque id',
);
assert.equal(
  __browserTestHooks.browserSurfaceCanControl(focusedCtx, {
    latestSession: activeSession,
    controllerLeaseId: 'lease-other',
  }, 1_000_000),
  false,
  'another tab with the same user must not inherit the active surface lease',
);
assert.deepEqual(
  __browserTestHooks.browserAuthRequestFromArgs({
    session_id: 'browser_session_web_stack_auth_dnbhoovers_com_cmd_123',
    tab_id: 'browser_tab_browser_session_web_stack_auth_dnbhoovers_com_cmd_123',
    source_id: 'dnbhoovers.com',
    target_url: 'https://app.dnbhoovers.com/',
    purpose: 'web_stack_auth',
    allowed_domains: ['dnbhoovers.com', 'app.dnbhoovers.com'],
    capture_script: 'dnbhoovers.company_capture.v1',
    required_secret_name: 'DNB_HOOVERS_BROWSER_LOGIN',
  }),
  {
    session_id: 'browser_session_web_stack_auth_dnbhoovers_com_cmd_123',
    tab_id: 'browser_tab_browser_session_web_stack_auth_dnbhoovers_com_cmd_123',
    url: 'https://app.dnbhoovers.com/',
    target_url: 'https://app.dnbhoovers.com/',
    source_id: 'dnbhoovers.com',
    purpose: 'web_stack_auth',
    allowed_domains: ['dnbhoovers.com', 'app.dnbhoovers.com'],
    capture_script: 'dnbhoovers.company_capture.v1',
    verify_selector: '',
    secret_name: 'DNB_HOOVERS_BROWSER_LOGIN',
    auth_assist_command_id: '',
    auth_assist_task_id: '',
    requesting_task_id: '',
    instruction: '',
    auth_assist_status: 'pending',
    profile_mode: 'persistent',
    secret_value_in_rxdb: false,
  },
);
assert.equal(
  __browserTestHooks.browserAuthRequestFromArgs({
    session_id: 'browser_session_default',
    target_url: 'https://example.com',
    purpose: 'general',
  }),
  null,
);

const css = await readFile(new URL('./index.css', import.meta.url), 'utf8');
const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
const js = await readFile(new URL('./index.js', import.meta.url), 'utf8');
const moduleDefinition = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));
const moduleRegistry = JSON.parse(await readFile(new URL('../registry.json', import.meta.url), 'utf8'));
const desktopWrapperJs = await readFile(new URL('../../desktop-apps/browser/app.js', import.meta.url), 'utf8').catch(() => '');
const syncJs = await readFile(new URL('../../shared/sync.js', import.meta.url), 'utf8');
const source = `${css}\n${html}`;
const forbiddenSurfacePattern = new RegExp(['ctox-pane--gla' + 'ss', 'Prem' + 'ium', 'gla' + 'ss'].join('|'), 'i');

assert.doesNotMatch(source, forbiddenSurfacePattern);
assert.doesNotMatch(source, /border-(?:left|right)\s*:\s*(?:[2-9]|[0-9]{2,})px/);
assert.doesNotMatch(source, /border-radius:\s*(?:10|12|14|16|18|20|24)px/);
assert.doesNotMatch(source, /box-shadow:\s*(?:0|inset|rgba|color-mix)/);
assert.equal(moduleDefinition.layout.shell_contract, 'v2');
assert.equal(moduleDefinition.layout.shell_geometry_contract, 'browser-v2-reference-1');
assert.equal(moduleDefinition.layout.shell_header_rows, 2);
assert.equal(moduleDefinition.layout.shell_icon_rows, 2);
assert.equal(
  moduleDefinition.layout.icon_asset,
  'shared/assets/workjet-icons/operator-selection-v1/browser.jpg',
  'Shell V2 must receive the approved large Browser raster icon',
);
assert.equal(moduleDefinition.version, '0.3.0');
const registeredBrowser = moduleRegistry.modules.find((module) => module.id === 'browser');
assert.ok(registeredBrowser, 'Browser must remain present in the immutable module registry');
assert.equal(registeredBrowser.layout.shell_geometry_contract, moduleDefinition.layout.shell_geometry_contract);
assert.equal(registeredBrowser.layout.shell_header_rows, moduleDefinition.layout.shell_header_rows);
assert.equal(registeredBrowser.layout.shell_icon_rows, moduleDefinition.layout.shell_icon_rows);
assert.equal(registeredBrowser.layout.icon_asset, moduleDefinition.layout.icon_asset);
assert.equal(registeredBrowser.version, moduleDefinition.version);
// 640 -> 767: der Mobile-Umbau (mobile usability phase 2) hat den
// Schmal-Breakpoint bewusst verschoben; der Waechter prueft die Absicht
// (es gibt eine Schmal-Variante), nicht die alte Zahl.
assert.match(css, /@container business-app-window \(max-width: 768px\)/);
assert.match(css, /\.browser-session-list[\s\S]*overflow-x: auto/);
assert.match(html, /data-browser-start/);
assert.equal(
  (html.match(/data-shell-v2-header-row="1"/g) || []).length,
  2,
  'Browser uses one canonical Shell-v2 title row for the session pane and one for the canvas pane',
);
assert.match(html, /class="ctox-filterbar" data-shell-v2-header-row="2"/);
assert.match(html, /class="ctox-pane-header ctox-pane-band browser-canvas-head" data-shell-v2-header-row="1"/);
assert.match(html, /class="ctox-pane-title" data-browser-status-title/);
assert.match(html, /class="ctox-view-switch browser-canvas-switch" data-shell-v2-header-row="2"/);
assert.match(html, /class="ctox-pane-tabs browser-view-switch"/);
assert.doesNotMatch(
  html.match(/<header class="ctox-pane-header ctox-pane-band" data-shell-v2-header-row="1">[\s\S]*?<\/header>/)?.[0] || '',
  /ctox-filterbar/,
  'the session filter row must remain a Shell-v2 sibling instead of being nested into the title header',
);
assert.match(html, /data-browser-private/);
assert.match(html, /data-browser-viewport/);
assert.match(html, /data-browser-new-tab/);
assert.match(html, /data-browser-go/);
assert.doesNotMatch(html, />Los<\/button>/, 'the address action must stay a compact icon control');
// Off-Canvas-Drawer am 31.08.2026 durch die Shell-Haltepunkte 1024/768 ersetzt.
assert.match(css, /grid-template-columns:\s*minmax\(120px, 1fr\) 30px 34px/);


assert.match(html, /data-browser-upload/);
assert.match(html, /data-browser-automation-overlay/);
assert.match(html, /data-browser-automation-code/);
assert.match(html, /data-browser-controller-acquire/);
assert.match(html, /data-browser-controller-release/);
assert.match(html, /data-browser-clipboard-copy/);
assert.match(html, /data-browser-clipboard-paste/);
assert.match(html, /data-browser-downloads/);
// v4 (App-API 4.0): ein Befehlsweg – api.command, nie warten; Status über h.status in die Zustandszone.
const dispatchBrowserCommandSource = js.match(/async function dispatchBrowserCommand[\s\S]*?\n\}/)?.[0] || '';
const submitCommandSource = js.match(/function submitCommand\(env[\s\S]*?\n\}/)?.[0] || '';
assert.match(dispatchBrowserCommandSource, /submitCommand\(env, commandType,/, 'every browser command goes through submitCommand');
assert.match(submitCommandSource, /env\.api\.command\(commandType, payload, payload\?\.record_id \? \{ record_id: payload\.record_id \} : undefined\)/, 'submitCommand uses api.command with record_id option');
assert.match(submitCommandSource, /handle\.status\.subscribe[\s\S]*?'failed'[\s\S]*?env\.status\.report[\s\S]*?'done'[\s\S]*?env\.status\.clear/,
  'failed goes to the status zone, done clears it');
assert.doesNotMatch(submitCommandSource, /await|\.result\b|until:/, 'a browser command must never be awaited (R5)');
assert.equal((js.match(/env\.api\.command\(/g) || []).length, 1, 'exactly one command path');
assert.doesNotMatch(js.replace(/^\s*\/\/.*$/gm, ''), /requestNative|ctox\.browser\.live\.v1|commandBus|startCommandSync|restartCollection|refreshBrowserProjections|startBrowserRuntimeSync/,
  'the v3 direct channel and sync coupling are gone in 4.0');
assert.match(js, /browserSessionError\(session\)/, 'the Browser surface renders the persisted runtime reason');
assert.doesNotMatch(
  js,
  /vorübergehend nicht erreichbar|wird neu aufgebaut/i,
  'the Browser surface must not promise a rebuild that does not exist',
);
assert.match(js, /\[refs\.go, refs\.stop,/);
// v4: Markup relativ zum Modul, ohne Cache-Buster (R10).
assert.match(js, /fetch\(new URL\('\.\/index\.html', import\.meta\.url\)\)/);
assert.doesNotMatch(js, /STYLE_BUILD|searchParams\.get\('v'\)|[?]v=|cache: 'no-store'/);
assert.match(js, /function renderAutomationOverlay[\s\S]*?capture_script[\s\S]*?Playwright-Scraper/);
assert.match(css, /\.browser-automation-overlay[\s\S]*?position:\s*absolute/);
assert.match(
  js,
  /refs\.address\?\.addEventListener\('keydown',[\s\S]*?event\.key !== 'Enter'[\s\S]*?submitAddress\(\)/,
  'Enter in the compact address bar must navigate without relying on implicit form submission',
);
// v4: Eingabe als Dokument der eigenen Collection (api.put), keine eigene Warteschlange, kein Abfragetakt;
// das Live-Bild kommt nur als browser_frames-Projektion (V4-PORT OFFEN-1).
assert.match(
  js.match(/async function writeInputEvent[\s\S]*?\n\}/)?.[0] || '',
  /env\.api\.put\('browser_input_events', event\)/,
  'pointer and keyboard events are written through api.put',
);
assert.doesNotMatch(js, /setInterval\(|const flushInput|const pump = async/, 'no input queue and no frame polling loop (R12)');
assert.match(
  js.match(/async function requestBrowserControllerLease[\s\S]*?\n\}/)?.[0] || '',
  /dispatchBrowserCommand\(env, state, commandType, \{ lease_id: leaseId \}\)/,
  'controller acquire, renew and release use the browser.controller.* command (handler keeps the lease)',
);
if (desktopWrapperJs) {
  assert.match(desktopWrapperJs, /browserModuleUrl\.search = new URL\(import\.meta\.url\)\.search/);
  assert.doesNotMatch(desktopWrapperJs, /modules\/browser\/index\.js[?]v=/);
}
assert.match(js, /lease_id: state\.controllerLeaseId/);
assert.match(js, /if \(requiresController\) payload\.lease_id = state\.controllerLeaseId/);
assert.match(js, /session\.controller_lease_id === state\.controllerLeaseId/);
assert.match(
  js.match(/async function fillWebStackCredential[\s\S]*?\n\}/)?.[0] || '',
  /lease_id: state\.controllerLeaseId/,
  'credential fill must be bound to the active browser controller lease',
);
assert.doesNotMatch(
  js.match(/async function fillWebStackCredential[\s\S]*?\n\}/)?.[0] || '',
  /credential_value|secret_value\s*:/,
  'credential fill must never place secret values on the RxDB command bus',
);
assert.match(syncJs, /isReadOnlyProjectionCollection[\s\S]{0,500}browser_sessions/);
assert.equal(__ctoxSyncTestHooks.isDemandOnlyPullCollection('browser_sessions'), true);
assert.equal(__ctoxSyncTestHooks.isDemandOnlyPullCollection('browser_tabs'), true);
assert.equal(__ctoxSyncTestHooks.isDemandOnlyPullCollection('unknown-browser-collection'), false);
assert.doesNotMatch(js, /api\.(?:put|patch)\('browser_sessions'/, 'browser_sessions is a core-owned projection');
assert.match(js, /\{ owner_user_id: \{ \$in: actorIds \} \},\s*\{ 'payload\.purpose': 'web_stack_auth' \}/);
assert.match(js, /if \(!browserSessionIdFromArgs\(args\)\) return;/);
assert.doesNotMatch(html, /data-browser-(?:seed|clear|reset)/);
assert.match(js, /addEventListener\?\.\('focus', handleFocusRefresh\)/);

// --- IA: two-pane sessions selector + remote canvas (2026-07-21) ---

// LEFT column carries the full SHELL-wired canonical grammar (data-pg-*).
assert.match(html, /ctox-workspace--two-pane/);
assert.match(html, /class="ctox-pane browser-sessions"/);
assert.match(html, /data-pg-search/);
assert.match(html, /data-pg-view="cards"/);
// Betreiber-Direktive 31.08.2026: EIN Umschalt-Knopf statt Knopfpaar.
assert.equal((html.match(/data-pg-view=/g) || []).length, 1);
assert.match(html, /data-pg-tray-toggle/);
assert.match(html, /data-pg-reset/);
assert.match(html, /data-pg-footer/);
assert.match(html, /ctox-pane-body ctox-well/);
// >= 2 real counted views (zeros included) — never a single-tab band.
const sessionBands = html.match(/data-pg-band="[^"]+"/g) || [];
assert.ok(sessionBands.length >= 2, 'sessions band needs >= 2 real views');
for (const key of ['all', 'active', 'closed']) {
  assert.match(html, new RegExp(`data-pg-band="${key}"`));
  assert.match(html, new RegExp(`data-pg-count="${key}"`));
}
// Standing header actions: Neu (create), Import, Export as collected icons.
assert.match(html, /class="ctox-pane-icon" data-browser-start/);
assert.match(html, /data-action="import"/);
assert.match(html, /data-action="export"/);
// No manual refresh button on reactive data.
assert.doesNotMatch(html, /data-browser-refresh/);
// MAIN keeps the remote canvas + chrome bar (unique work surface).
assert.match(html, /class="ctox-pane browser-canvas"/);
assert.match(html, /data-browser-canvas/);
assert.match(html, /data-browser-frame-shell/);

// Explicit pane grid rows + grid-column pins (primary column keeps priority).
assert.match(css, /\.browser-sessions\s*\{[^}]*grid-column:\s*1/);
assert.match(css, /\.browser-canvas\s*\{[^}]*grid-column:\s*3/);
assert.match(css, /\.browser-sessions\s*\{[^}]*grid-template-rows:\s*auto auto auto auto minmax\(0, 1fr\) auto/);

// Grammar re-renders reactively on the shell event; no chrome wiring here.
assert.match(js, /addEventListener\('ctox-pane-grammar-change', onLeftGrammarChange\)/);
assert.match(js, /__ctoxPaneGrammar/);

// In-place selection flip: selecting a session marks rows in place and must
// NOT rebuild the list (a rebuild resets the well scroll to the top).
assert.match(js, /refs\.sessions\?\.addEventListener\('click'[\s\S]*?markActiveSession\(refs, sessionId\)/);
assert.match(
  js,
  /state\.selectedSessionId = sessionId;[\s\S]*?state\.latestSession = selectedSession;[\s\S]*?state\.latestFrame = null;/,
  'selecting a session must synchronously bind the work surface before async refreshes',
);
assert.match(
  js,
  /if \(refreshInFlight\) \{[\s\S]*?refreshQueued = true;[\s\S]*?refreshInFlight = loadAndRender\(\)/,
  'reactive collection updates must coalesce instead of exhausting query streams',
);
// v4: alle Lesezugriffe sind lokale Live-Abfragen; das Bild nur aus der browser_frames-Projektion der
// gewählten Sitzung, solange die Echtzeit-Spur des Kerns fehlt (V4-PORT OFFEN-1).
assert.doesNotMatch(js, /directLiveEnabled|latestDirectFrame|directOptimisticSession|startDirectBrowserLive/);
assert.match(js, /readCollection\(env, 'browser_frames', \{[\s\S]*?selector: \{ session_id: frameSessionId \}/);
assert.equal(
  (js.match(/api\.query\(collectionName, \{\}\)\.subscribe/g) || []).length,
  1,
  'one live subscription loop over BROWSER_LIVE_COLLECTIONS',
);
assert.match(js, /function markActiveSession[\s\S]*?classList\.toggle\('is-selected'/);
assert.doesNotMatch(
  js.match(/refs\.sessions\?\.addEventListener\('click'[\s\S]*?\}\);/)?.[0] || '',
  /innerHTML/,
  'selecting a session must not rebuild the list',
);
// renderSessions only rebuilds the well when the data signature changes.
assert.match(js, /refs\.sessions\.dataset\.sig !== signature[\s\S]*?refs\.sessions\.innerHTML =/);

// Import/export handlers wired to the header icons.
assert.match(js, /\[data-action="import"\]/);
assert.match(js, /\[data-action="export"\]/);
assert.match(js, /importBrowserSessions\(env, state, refs\)/);
assert.match(js, /exportBrowserSessions\(state, refs\)/);

const hooks = __browserTestHooks;
const sampleSessions = [
  { id: 'browser_session_a', runtime_status: 'active', profile_mode: 'persistent', current_url: 'https://acme.example/app', updated_at_ms: 3 },
  { id: 'browser_session_b', runtime_status: 'starting', profile_mode: 'private', title: 'Login', updated_at_ms: 2 },
  { id: 'browser_session_c', runtime_status: 'stopped', profile_mode: 'persistent', current_url: 'https://old.example', updated_at_ms: 1 },
];

assert.deepEqual(
  hooks.mergeRequestedSession(sampleSessions, {
    id: 'browser_session_requested',
    runtime_status: 'active',
    updated_at_ms: 4,
  }).map((session) => session.id),
  ['browser_session_requested', 'browser_session_a', 'browser_session_b', 'browser_session_c'],
);
assert.equal(
  hooks.mergeRequestedSession(sampleSessions, {
    ...sampleSessions[1],
    runtime_status: 'active',
    updated_at_ms: 5,
  }).filter((session) => session.id === 'browser_session_b').length,
  1,
  'the targeted requested session must replace its stale list entry',
);
assert.deepEqual(
  hooks.mergeRequestedDocument(
    [{ id: 'browser_tab_old', updated_at_ms: 1 }],
    { id: 'browser_tab_requested', updated_at_ms: 2 },
  ).map((document) => document.id),
  ['browser_tab_requested', 'browser_tab_old'],
);
assert.match(
  js.match(/async function fillWebStackCredential[\s\S]*?\n\}/)?.[0] || '',
  /submitCommand\(env, 'browser\.credential\.fill'/,
  'credential fill is sent as a command; its result arrives through the session projection (never awaited in 4.0)',
);

// Band counts: zeros included; the band ignores its own selection.
assert.deepEqual(
  hooks.browserSessionViewCounts(sampleSessions, { band: 'active' }),
  { all: 3, active: 1, closed: 2 },
  'only a process-confirmed active session belongs in the active count',
);
assert.deepEqual(hooks.browserSessionViewCounts([], {}), { all: 0, active: 0, closed: 0 });
assert.equal(hooks.browserSessionBand(sampleSessions[0]), 'active');
assert.equal(hooks.browserSessionBand(sampleSessions[2]), 'closed');

// Filtering by band / search / profile.
assert.equal(hooks.filterSessionsForView(sampleSessions, { band: 'closed' }).length, 2);
assert.equal(hooks.filterSessionsForView(sampleSessions, { search: 'acme' }).length, 1);
assert.equal(hooks.filterSessionsForView(sampleSessions, { filters: { profile: 'private' } }).length, 1);
assert.equal(hooks.filterSessionsForView(sampleSessions, { filters: { profile: 'all' } }).length, 3);

// Signature is selection-independent (stable data => stable signature => no
// rebuild on select) and changes only when the rendered data changes.
const filtered = hooks.filterSessionsForView(sampleSessions, {});
const sigA = hooks.sessionListSignature(filtered, 'cards', {});
assert.equal(sigA, hooks.sessionListSignature(filtered, 'cards', {}), 'unchanged data => identical signature');
const mutated = filtered.map((session, index) => index === 0 ? { ...session, runtime_status: 'stopped' } : session);
assert.notEqual(hooks.sessionListSignature(mutated, 'cards', {}), sigA, 'status change => rebuild');
assert.notEqual(hooks.sessionListSignature(filtered, 'list', {}), sigA, 'view change => rebuild');

// Auto-reveal: the work surface shows only when a session is selected.
assert.equal(hooks.browserWorkbenchVisible(true), true);
assert.equal(hooks.browserWorkbenchVisible(true, true), false);
assert.equal(hooks.browserWorkbenchVisible(false), false);
assert.match(js, /is-session-active/);

// Export/import round-trips honestly (read-only overlay, never persisted).
const exported = hooks.buildBrowserSessionsExport(sampleSessions, 1234);
assert.equal(exported.kind, 'browser_sessions');
assert.equal(exported.exported_at_ms, 1234);
assert.equal(exported.sessions.length, 3);
const reimported = hooks.parseBrowserSessionsImport(exported);
assert.equal(reimported.length, 3);
assert.equal(reimported[0].__imported, true);
assert.equal(reimported[0].id, 'browser_session_a');
assert.deepEqual(hooks.parseBrowserSessionsImport({ sessions: [{ foo: 'bar' }] }), []);
// Imported entries never override a real owned session in the render list.
assert.equal(
  hooks.sessionRenderList({ visibleSessions: [sampleSessions[0]], importedSessions: [{ id: 'browser_session_a', __imported: true }, { id: 'browser_session_z', __imported: true }] }).length,
  2,
);

// Shard meta is a single muted selector line.
assert.match(hooks.browserSessionShardMeta(sampleSessions[0], 2), /Persönlich · .+ · 2 Tabs/);

console.log('browser module pure contract smoke OK');

// --- Abgelaufene Steuerungs-Pacht: neu HOLEN statt erneuern (13.08.2026) ---
// Auf der Kundeninstanz gemessen: beide Sitzungen "active", beide Pachten
// abgelaufen, NULL Eingabe-Ereignisse in zehn Minuten aktiver Bedienung.
// Die Erneuerung schliesst den abgelaufenen Zustand bewusst aus (Endlosschleife),
// aber es gab keinen Weg zurueck. Der fehlt nicht mehr.

// Abgelaufen -> zurueckholen.
assert.equal(
  __browserTestHooks.shouldReacquireControllerLease({
    id: 'browser_session_test',
    // Der Kommentar oben haelt fest: beide Sitzungen waren "active". Das
    // Fixture trug den Status nur nie. Seit dem 15.08.2026 ist er tragend —
    // eine tote Sitzung braucht einen Start, keine frische Pacht.
    status: 'active',
    controller_user_id: 'user-1',
    controller_lease_id: 'lease-1',
    controller_lease_expires_at_ms: 1_000_000,
  }, 'user-1', 1_000_000, {}),
  true,
  'eine abgelaufene Pacht muss zurueckgeholt werden',
);

// Noch gueltig -> nichts tun, dafuer ist die Erneuerung da.
assert.equal(
  __browserTestHooks.shouldReacquireControllerLease({
    id: 'browser_session_test',
    controller_user_id: 'user-1',
    controller_lease_expires_at_ms: 1_060_000,
  }, 'user-1', 1_000_000, {}),
  false,
  'eine gueltige Pacht wird nicht neu geholt',
);

// Fremd gesteuert -> nicht an sich reissen.
assert.equal(
  __browserTestHooks.shouldReacquireControllerLease({
    id: 'browser_session_test',
    controller_user_id: 'user-2',
    controller_lease_expires_at_ms: 1_000_000,
  }, 'user-1', 1_000_000, {}),
  false,
  'eine fremd gesteuerte Sitzung darf nicht uebernommen werden',
);

// Kein zweiter Versuch waehrend einer laeuft, und keine Endlosschleife nach
// einem Fehlschlag — genau der Schutz, den die Erneuerung schon hat.
assert.equal(
  __browserTestHooks.shouldReacquireControllerLease({
    id: 'browser_session_test', controller_user_id: 'user-1',
    controller_lease_expires_at_ms: 1_000_000,
  }, 'user-1', 1_000_000, { reacquireInFlight: true }),
  false,
  'kein zweiter Versuch waehrend einer laeuft',
);
assert.equal(
  __browserTestHooks.shouldReacquireControllerLease({
    id: 'browser_session_test', controller_user_id: 'user-1',
    controller_lease_expires_at_ms: 1_000_000,
  }, 'user-1', 1_000_000, { lastReacquireAtMs: 995_000 }),
  false,
  'nach einem Fehlschlag nicht sofort wieder — keine Endlosschleife',
);

// Der Sperrgrund muss im Klartext benannt werden, nicht geschwiegen.
const fokussiert = { surfaceFocused: true, actor: { id: 'user-1' } };
assert.match(
  __browserTestHooks.eingabeSperrgrund(fokussiert, {
    latestSession: {
      id: 's1', controller_user_id: 'user-1', controller_lease_id: 'lease-1',
      controller_lease_expires_at_ms: 900_000,
    },
    controllerLeaseId: 'lease-1',
  }, 1_000_000),
  /abgelaufen/i,
  'eine abgelaufene Steuerung muss als solche gemeldet werden',
);
assert.equal(
  __browserTestHooks.eingabeSperrgrund(fokussiert, {
    latestSession: {
      id: 's1', controller_user_id: 'user-1', controller_lease_id: 'lease-1',
      controller_lease_expires_at_ms: 1_060_000,
    },
    controllerLeaseId: 'lease-1',
  }, 1_000_000),
  '',
  'bei gueltiger Steuerung gibt es keinen Sperrgrund',
);

// --- Eingabe-Nutzlast: Felder, die browser_runtime_input_event erwartet ---
assert.match(js, /submitBrowserNav\(env, state, refs, 'navigate'/);
assert.match(js, /submitBrowserNav\(env, state, refs, 'back'/);
assert.match(js, /submitBrowserNav\(env, state, refs, 'forward'/);
assert.match(js, /submitBrowserNav\(env, state, refs, 'reload'/);
assert.match(js, /function installInputHandlers[\s\S]*pointerdown[\s\S]*pointerup[\s\S]*pointermove[\s\S]*wheel[\s\S]*keydown[\s\S]*keyup/);
assert.doesNotMatch(
  js.match(/function installInputHandlers[\s\S]*?\n\}/)?.[0] || '',
  /addEventListener\('click'/,
  'Textauswahl braucht die Zeigerfolge, kein zusammengefasstes click',
);

function createInputCollection() {
  const docs = [];
  return {
    docs,
    findOne(id) {
      return { exec: async () => docs.find((doc) => doc.id === id) || null };
    },
    async upsert(doc) {
      docs.push(doc);
      return doc;
    },
  };
}

// v4-Montageumgebung (env): api.put/api.command wie App-API 4.0, Zustandszone, Fensterfokus app-lokal.
function createInputCtx(collection) {
  const dispatched = [];
  const zone = [];
  return {
    actor: { id: 'user-1', display_name: 'Test' },
    surfaceFocused: true,
    dispatched,
    zone,
    commandWatches: new Set(),
    status: { report: (item) => zone.push(item), clear: () => {} },
    api: {
      put(name, doc) {
        if (name === 'browser_input_events') collection.docs.push(doc);
        return { local: Promise.resolve(), result: Promise.resolve({ ref: doc.id }) };
      },
      command(type, payload) {
        const id = `cmd-${dispatched.length + 1}`;
        dispatched.push({ id, type, payload });
        return { id, local: Promise.resolve(), status: { subscribe: () => () => {} } };
      },
    },
  };
}

function createInputState() {
  return {
    latestSession: {
      id: 'browser_session_test',
      owner_user_id: 'user-1',
      controller_user_id: 'user-1',
      controller_lease_id: 'lease-1',
      controller_lease_expires_at_ms: Date.now() + 60_000,
      last_frame_seq: 9,
    },
    latestTab: { id: 'browser_tab_test' },
    latestFrame: { session_id: 'browser_session_test', tab_id: 'browser_tab_test', seq: 9 },
    controllerLeaseId: 'lease-1',
    lastInputSeq: 0,
    lastPointerMoveAt: 0,
    lastPointerClick: null,
    pointerIsDown: false,
  };
}

function createCanvasStub(listeners = {}) {
  return {
    width: 1280,
    height: 720,
    focus() {},
    setPointerCapture() {},
    getBoundingClientRect() {
      return { left: 0, top: 0, width: 1280, height: 720 };
    },
    addEventListener(type, handler) {
      listeners[type] = handler;
    },
    emit(type, event) {
      return listeners[type]?.(event);
    },
  };
}

function pointerEvent(overrides = {}) {
  return {
    clientX: 120,
    clientY: 80,
    detail: 0,
    button: 0,
    buttons: 0,
    pointerId: 1,
    pointerType: 'mouse',
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    preventDefault() {},
    ...overrides,
  };
}

// A 16:9 remote viewport inside a taller responsive canvas is letterboxed.
// Pointer coordinates must be relative to the painted pixels, not to the CSS
// box including its top and bottom margins.
{
  const canvas = createCanvasStub();
  canvas.getBoundingClientRect = () => ({ left: 10, top: 20, width: 800, height: 600 });
  assert.deepEqual(
    hooks.canvasPoint(canvas, { clientX: 410, clientY: 95 }),
    { x: 640, y: 0 },
  );
  assert.deepEqual(
    hooks.canvasPoint(canvas, { clientX: 410, clientY: 320 }),
    { x: 640, y: 360 },
  );
}

// v4: directResponseBelongsToSurface/applyDirectNavigationState gehörten zum Direktkanal (Frame-Pump mit
// Navigations-Epoche); der Kanal entfällt in 4.0 (V4-PORT OFFEN-1). Navigation kommt aus dem Befehlsergebnis.
{
  const state = {
    latestSession: { id: 'browser_session_new', current_url: 'https://example.com/', title: 'Example Domain' },
    latestTab: { title: 'Example Domain' },
  };
  hooks.applyLatestNavigationResult(state, [
    { type: 'browser.navigate', status: 'completed', payload: { session_id: 'browser_session_old' }, result: { url: 'https://old.example/' }, updated_at_ms: 9 },
    { type: 'browser.navigate', status: 'completed', payload: { session_id: 'browser_session_new' }, result: { url: 'https://httpbingo.org/forms/post', title: 'Form' }, updated_at_ms: 5 },
  ]);
  assert.equal(state.latestSession.current_url, 'https://httpbingo.org/forms/post');
  assert.equal(state.latestSession.title, 'Form');
  assert.equal(state.latestTab.url, 'https://httpbingo.org/forms/post');
}

{
  const collection = createInputCollection();
  const ctx = createInputCtx(collection);
  const state = createInputState();
  const canvas = createCanvasStub();
  await hooks.writePointerInput(ctx, { canvas }, state, 'mouseDown', pointerEvent({ detail: 3, buttons: 1 }));
  const payload = hooks.browserInputPayload(collection.docs[0]);
  assert.equal(payload.type, 'mouseDown');
  assert.equal(payload.detail, 3);
  assert.equal(payload.clickCount, 3);
  assert.equal(payload.button, 'left');
  assert.equal(payload.buttons, 1);
  assert.equal(payload.x, 120);
  assert.equal(payload.y, 80);
  assert.deepEqual(payload.modifiers, []);
}

{
  const collection = createInputCollection();
  const ctx = createInputCtx(collection);
  const state = createInputState();
  const canvas = createCanvasStub();
  await hooks.writePointerInput(ctx, { canvas }, state, 'mouseDown', pointerEvent({ detail: 0, buttons: 1 }));
  await hooks.writePointerInput(ctx, { canvas }, state, 'mouseUp', pointerEvent({ detail: 0, buttons: 0 }));
  await hooks.writePointerInput(ctx, { canvas }, state, 'mouseDown', pointerEvent({ detail: 0, buttons: 1 }));
  await hooks.writePointerInput(ctx, { canvas }, state, 'mouseUp', pointerEvent({ detail: 0, buttons: 0 }));
  await hooks.writePointerInput(ctx, { canvas }, state, 'mouseDown', pointerEvent({ detail: 0, buttons: 1 }));
  assert.equal(hooks.browserInputPayload(collection.docs[0]).clickCount, 1);
  assert.equal(hooks.browserInputPayload(collection.docs[2]).clickCount, 2);
  assert.equal(hooks.browserInputPayload(collection.docs[4]).clickCount, 3);
}

{
  const collection = createInputCollection();
  const ctx = createInputCtx(collection);
  const state = createInputState();
  await hooks.writeKeyboardInput(ctx, state, 'keyDown', {
    key: 'a',
    code: 'KeyA',
    altKey: false,
    ctrlKey: false,
    metaKey: true,
    shiftKey: true,
    repeat: false,
    location: 0,
  });
  const payload = hooks.browserInputPayload(collection.docs[0]);
  assert.equal(payload.type, 'keyDown');
  assert.equal(payload.key, 'a');
  assert.equal(payload.code, 'KeyA');
  assert.equal(payload.text, '');
  assert.deepEqual(payload.modifiers, ['Meta', 'Shift']);
}

{
  const collection = createInputCollection();
  const ctx = createInputCtx(collection);
  const state = createInputState();
  await hooks.writeKeyboardInput(ctx, state, 'keyDown', {
    key: 'a',
    code: 'KeyA',
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
  });
  await hooks.writeKeyboardInput(ctx, state, 'keyDown', {
    key: 'Enter',
    code: 'Enter',
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
  });
  assert.equal(hooks.browserInputPayload(collection.docs[0]).text, 'a');
  assert.equal(hooks.browserInputPayload(collection.docs[0]).key, 'a');
  assert.equal(hooks.browserInputPayload(collection.docs[0]).code, 'KeyA');
  assert.equal(hooks.browserInputPayload(collection.docs[1]).text, '');
  assert.equal(hooks.browserInputPayload(collection.docs[1]).key, 'Enter');
  assert.equal(hooks.browserInputPayload(collection.docs[1]).code, 'Enter');
}

{
  const collection = createInputCollection();
  const ctx = createInputCtx(collection);
  const state = createInputState();
  const canvas = createCanvasStub();
  await hooks.writePointerInput(ctx, { canvas }, state, 'wheel', pointerEvent({
    deltaX: 12,
    deltaY: -48,
    buttons: 0,
    button: -1,
  }));
  const payload = hooks.browserInputPayload(collection.docs[0]);
  assert.equal(payload.type, 'wheel');
  assert.equal(payload.dx, 12);
  assert.equal(payload.dy, -48);
  assert.equal(payload.clickCount, 0);
}

{
  const collection = createInputCollection();
  const ctx = createInputCtx(collection);
  const state = createInputState();
  const listeners = {};
  const canvas = createCanvasStub(listeners);
  hooks.installInputHandlers(ctx, { canvas }, state, () => {});
  canvas.emit('pointerdown', pointerEvent({ buttons: 1, clientX: 40, clientY: 40 }));
  canvas.emit('pointermove', pointerEvent({ buttons: 1, clientX: 80, clientY: 90 }));
  canvas.emit('pointermove', pointerEvent({ buttons: 1, clientX: 140, clientY: 160 }));
  canvas.emit('pointerup', pointerEvent({ buttons: 0, clientX: 140, clientY: 160 }));
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(collection.docs.map((doc) => doc.type), ['mouseDown', 'mouseMove', 'mouseMove', 'mouseUp']);
  assert.equal(collection.docs[1].x, 80);
  assert.equal(collection.docs[2].y, 160);
  assert.ok(!collection.docs.some((doc) => doc.type === 'click'));
}

{
  const ctx = createInputCtx(createInputCollection());
  const state = createInputState();
  await hooks.submitBrowserNav(ctx, state, { address: { value: 'example.com/next' } }, 'navigate');
  await hooks.submitBrowserNav(ctx, state, {}, 'back');
  await hooks.submitBrowserNav(ctx, state, {}, 'forward');
  await hooks.submitBrowserNav(ctx, state, {}, 'reload');
  assert.deepEqual(
    ctx.dispatched.map((command) => command.type),
    ['browser.navigate', 'browser.back', 'browser.forward', 'browser.reload'],
  );
  assert.equal(ctx.dispatched[0].payload.url, 'https://example.com/next');
  for (const command of ctx.dispatched) {
    assert.equal(command.payload.client_context.module_id, 'browser');
    assert.equal(command.payload.session_id, 'browser_session_test');
    assert.equal(command.payload.lease_id, 'lease-1');
  }
}

// --- Startmerkliste darf nicht dauerhaft merken (13.08.2026) ---
// Gemessen: 34 Sitzungen, 0 aktiv, kein Chrome-Prozess, keine Protokollzeile in
// 20 Minuten — weder Start-Knopf noch Plus-Symbol bewirkten etwas. Ursache war
// requestedSessionStarts: der Eintrag wurde nur im FEHLERfall entfernt, nicht
// wenn eine erfolgreich gestartete Sitzung spaeter wegbrach.
{
  const quelle = await readFile(new URL('./index.js', import.meta.url), 'utf8');
  const i = quelle.indexOf('function ensureRequestedBrowserSession');
  assert.ok(i > 0, 'ensureRequestedBrowserSession existiert');
  const block = quelle.slice(i, i + 2200);
  const zustandGelesen = block.indexOf('browserSessionNeedsStart');
  const merklisteGeprueft = block.indexOf('requestedSessionStarts.has');
  assert.ok(zustandGelesen > 0 && merklisteGeprueft > 0);
  assert.ok(
    zustandGelesen < merklisteGeprueft,
    'der Sitzungszustand muss VOR der Merkliste geprueft werden — sonst blockiert '
    + 'ein alter Eintrag den Neustart einer weggebrochenen Sitzung dauerhaft',
  );
  assert.match(
    block, /requestedSessionStarts\.delete/,
    'eine laufende Sitzung muss aus der Merkliste entfernt werden',
  );
  assert.doesNotMatch(
    block.slice(0, zustandGelesen), /if \(state\.requestedSessionStarts\.has/,
    'die Merkliste darf nicht mehr die erste Bedingung sein',
  );
}

// --- Sitzungszustand: wer steuert, wo muss jemand eingreifen (13.08.2026) ---
// Vorher stand in jeder Zeile nur "Persoenlich · Bereit · 1 Tab". Bei 34
// Sitzungen sah niemand, ob eine Sitzung dem Nutzer gehoert, von der Recherche
// gefahren wird, oder auf einen menschlichen Klick wartet.
{
  const ich = { id: 'user-1' };
  const jetzt = 1_000_000;
  const z = (s, ctx = { actor: ich }) =>
    __browserTestHooks.browserSitzungZustand(s, ctx, jetzt).text;

  // Eingriff schlaegt alles: eine laufende Sitzung, die auf einen Menschen wartet.
  assert.equal(z({ id: 's', runtime_status: 'active', title: 'Just a moment...' }),
    'Eingriff nötig', 'eine Bot-Pruefung braucht den Nutzer');
  assert.equal(z({ id: 's', runtime_status: 'active', error: 'Anmeldung erforderlich' }),
    'Eingriff nötig', 'eine Anmeldung braucht den Nutzer');

  // Eigene Steuerung mit gueltiger Pacht.
  assert.equal(z({ id: 's', runtime_status: 'active', controller_user_id: 'user-1',
    controller_lease_expires_at_ms: jetzt + 60_000 }), 'Sie steuern');

  // Abgelaufene Pacht ist KEINE eigene Steuerung mehr.
  assert.equal(z({ id: 's', runtime_status: 'active', controller_user_id: 'user-1',
    controller_lease_expires_at_ms: jetzt - 1 }), 'Automatik',
    'eine abgelaufene Pacht darf nicht als eigene Steuerung gelten');

  // Fremd gesteuert = Automatik. Diese Sitzungen nicht versehentlich uebernehmen.
  assert.equal(z({ id: 's', runtime_status: 'active', controller_user_id: 'recherche',
    controller_lease_expires_at_ms: jetzt + 60_000 }), 'Automatik');

  // Nicht laufend = ruhend, unabhaengig von allem anderen.
  assert.equal(z({ id: 's', runtime_status: 'disconnected', controller_user_id: 'user-1',
    controller_lease_expires_at_ms: jetzt + 60_000 }), 'Ruhend');

  // Eine ruhende Sitzung mit Anmelde-Titel ist KEIN Eingriff — da laeuft nichts,
  // worauf jemand warten koennte.
  assert.equal(z({ id: 's', runtime_status: 'ended', title: 'Login' }), 'Ruhend');
}

// --- Tote Sitzung mit gueltiger Pacht (gemessen 15.08.2026, Kundeninstanz) ---
// Zustand, der die Adressleiste zwei Tage lang lahmlegte:
//   status = disconnected, controller_user = der Nutzer, Pacht gueltig.
// Die Entscheidung hing allein an der Pacht, also wurde immer navigiert —
// an eine Sitzung, die es nicht mehr gab. Der Start-Zweig war unerreichbar.
{
  // BEIDE Statusfelder, wie das echte Dokument sie traegt. Die erste Fassung
  // dieses Fixtures kannte nur `status` — die Gegenprobe fiel dadurch im
  // Browserlauf am 15.08.2026 durch, weil `runtime_status` in
  // browserSessionIsLive Vorrang hat und weiter auf 'disconnected' stand.
  // Ein Fixture, das nur ein Feld kennt, kann aus dem falschen Grund gruen sein.
  const toteSitzungMitPacht = {
    id: 'browser_session_michael-welsch-metric-space-ai_1786625483977',
    status: 'disconnected',
    runtime_status: 'disconnected',
    controller_user_id: 'michael.welsch@metric-space.ai',
    controller_lease_id: '809bfc72-d13c-4a33-9b4d-7d92186e1b93',
    controller_lease_expires_at_ms: 2_000_000_000_000,
  };
  const lebendeSitzung = { ...toteSitzungMitPacht, status: 'active', runtime_status: 'active' };

  // runtime_status hat Vorrang: 'active' allein im Statusfeld genuegt NICHT.
  assert.equal(
    __browserTestHooks.browserAddressAction({ ...toteSitzungMitPacht, status: 'active' }, true),
    'start',
    'runtime_status=disconnected schlaegt status=active — sonst navigiert man in eine Leiche',
  );

  assert.equal(
    __browserTestHooks.browserAddressAction(toteSitzungMitPacht, true),
    'start',
    'tote Sitzung muss in den Start-Zweig fuehren, auch mit gueltiger Pacht',
  );
  assert.equal(
    __browserTestHooks.browserAddressAction(lebendeSitzung, true),
    'navigate',
    'lebende Sitzung mit Steuerung navigiert',
  );
  assert.equal(
    __browserTestHooks.browserAddressAction(lebendeSitzung, false),
    'start',
    'ohne Steuerung wird eine neue Sitzung gestartet',
  );
  assert.equal(__browserTestHooks.browserAddressAction(null, true), 'start');

  // Die Wiederbeschaffung darf eine tote Sitzung nicht warmhalten: 259
  // erfolgreiche acquire in 48 h hielten genau den Fehlerzustand fest.
  const abgelaufen = { ...toteSitzungMitPacht, controller_lease_expires_at_ms: 1 };
  assert.equal(
    __browserTestHooks.shouldReacquireControllerLease(
      abgelaufen, ['michael.welsch@metric-space.ai'], 1_000_000,
    ),
    false,
    'tote Sitzung braucht einen Start, keine neue Pacht',
  );
  assert.equal(
    __browserTestHooks.shouldReacquireControllerLease(
      { ...lebendeSitzung, controller_lease_expires_at_ms: 1 },
      ['michael.welsch@metric-space.ai'], 1_000_000,
    ),
    true,
    'lebende Sitzung mit abgelaufener Pacht wird weiterhin zurueckgeholt',
  );
}

// --- Mehrfachstart: zwei Klicks duerfen nicht zwei Browser starten ---
// Am 16.08.2026 auf der Kundeninstanz gemessen: sieben Startbefehle in zwei
// Minuten, jeder mit eigener Sitzungskennung, 21 Chrome-Prozesse, 34 Eintraege
// in der Sitzungsliste. Mitverursacht durch die Reparatur vom 15.08., seit der
// "Los" bei einer toten Sitzung richtigerweise in den Start-Zweig fuehrt.
{
  const h = __browserTestHooks;
  assert.equal(h.startSperrgrund({}, 1_000_000), '', 'ohne laufenden Start ist Starten erlaubt');
  assert.match(
    h.startSperrgrund({ startPendingSince: 1_000_000 }, 1_002_000),
    /bereits gestartet/,
    'waehrend ein Start laeuft wird kein zweiter ausgeloest',
  );
  // Die Sperre muss ablaufen — sonst sperrt ein haengender Start den Nutzer aus.
  assert.equal(
    h.startSperrgrund({ startPendingSince: 1_000_000 }, 1_008_001),
    '',
    'nach Ablauf der Sperre darf wieder gestartet werden',
  );
  assert.match(
    h.startSperrgrund({ requestedSessionId: 's1', latestSession: { id: 's1', runtime_status: 'active' } }, 1_000_000),
    /bereits aktiv/,
  );
  // Eine TOTE angeforderte Sitzung bleibt startbar — sonst wieder eine Falle ohne Ausgang.
  assert.equal(
    h.startSperrgrund({ requestedSessionId: 's1', latestSession: { id: 's1', runtime_status: 'disconnected' } }, 1_000_000),
    '',
    'tote angeforderte Sitzung bleibt startbar',
  );
}

// Jede Beschriftung muss in BEIDEN Sprachen existieren. Ohne diesen Waechter
// faellt genau der Fall durch, der beim Skript-Umschalter passiert ist: die
// Funktion war fertig und getestet, die Knoepfe trugen aber nur den deutschen
// HTML-Fallback -- im englischen Produkt haette dort deutscher Text gestanden,
// und ein rein funktionaler Test haette das grün gemeldet.
{
  const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
  const de = JSON.parse(await readFile(new URL('./locales/de.json', import.meta.url), 'utf8'));
  const en = JSON.parse(await readFile(new URL('./locales/en.json', import.meta.url), 'utf8'));
  const htmlKeys = [...html.matchAll(/data-t="([^"]+)"/g)].map((treffer) => treffer[1]);
  assert.ok(htmlKeys.length > 0, 'index.html traegt data-t-Beschriftungen');
  for (const schluessel of htmlKeys) {
    assert.ok(schluessel in de, `data-t="${schluessel}" fehlt in locales/de.json`);
    assert.ok(schluessel in en, `data-t="${schluessel}" fehlt in locales/en.json`);
  }
  assert.deepEqual(
    Object.keys(de).sort(),
    Object.keys(en).sort(),
    'de.json und en.json muessen dieselben Schluessel tragen',
  );
  // Die im Code nachgeschlagenen Schluessel zaehlen genauso, auch wenn sie nie
  // im HTML stehen.
  // v4: scriptNoSession/scriptLoading/scriptFailed/scriptNeedsControl waren Zustandstexte der Skriptansicht und
  // sind entfallen (Zustandszone browser.script.unavailable, V4-PORT OFFEN-2).
  for (const schluessel of ['viewLive', 'viewScript']) {
    assert.ok(schluessel in de, `${schluessel} fehlt in locales/de.json`);
    assert.ok(schluessel in en, `${schluessel} fehlt in locales/en.json`);
  }
}

// Und die andere Haelfte derselben Luecke: der Waechter oben prueft nur, was
// bereits ein data-t TRAEGT. Zwoelf von 24 Knoepfen trugen keines -- darunter
// "Steuerung uebernehmen" und die Zwischenablage --, waren also gar nicht
// uebersetzbar und haetten im englischen Produkt deutschen Text gezeigt.
// Ein Knopf mit sichtbarem Wort-Text braucht ein data-t; reine Symbolknoepfe
// (Pfeil, Hamburger) brauchen stattdessen ein aria-label.
{
  const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
  const knoepfe = [...html.matchAll(/<button\b([^>]*)>([^<]*)<\/button>/g)];
  assert.ok(knoepfe.length > 5, 'index.html enthaelt Knoepfe');
  const ohneBeschriftung = [];
  const symbolOhneLabel = [];
  for (const [, attribute, beschriftung] of knoepfe) {
    const text = beschriftung.trim();
    if (!text) continue;
    // Wort-Text = enthaelt mindestens einen Buchstaben.
    const istWort = /\p{Letter}/u.test(text);
    if (istWort && !attribute.includes('data-t=')) ohneBeschriftung.push(text);
    if (!istWort && !attribute.includes('aria-label')) symbolOhneLabel.push(text);
  }
  assert.deepEqual(
    ohneBeschriftung, [],
    `Knoepfe mit Wort-Text ohne data-t (im englischen Produkt deutsch): ${ohneBeschriftung.join(', ')}`,
  );
  assert.deepEqual(
    symbolOhneLabel, [],
    `Symbolknoepfe ohne aria-label (fuer Screenreader stumm): ${symbolOhneLabel.join(', ')}`,
  );
}
