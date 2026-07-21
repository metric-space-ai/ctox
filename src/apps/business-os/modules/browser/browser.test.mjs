import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { __browserTestHooks } from './index.js';

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
assert.deepEqual(__browserTestHooks.selectedViewport({ value: '390x844' }), { width: 390, height: 844 });
assert.equal(
  __browserTestHooks.browserSessionIdFromArgs({ session_id: 'browser_session_web_stack_auth_xing-com' }),
  'browser_session_web_stack_auth_xing-com',
);
assert.equal(__browserTestHooks.browserSessionIdFromArgs({ session_id: 'not-a-browser-session' }), '');
assert.equal(__browserTestHooks.browserCommandRequiresController('browser.navigate', { id: 'browser_session_test' }), true);
assert.equal(__browserTestHooks.browserCommandRequiresController('browser.controller.acquire', { id: 'browser_session_test' }), false);
assert.equal(__browserTestHooks.browserCommandRequiresController('browser.session.start', null), false);
assert.equal(__browserTestHooks.browserSurfaceIsFocused({ host: { closest: () => null } }), false);
assert.equal(__browserTestHooks.browserSurfaceIsFocused({
  host: { closest: () => ({ classList: { contains: (name) => name === 'is-focused' } }) },
}), true);
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
const focusedCtx = {
  session: { user: { id: 'user-1' } },
  host: { closest: () => ({ classList: { contains: (name) => name === 'is-focused' } }) },
};
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
    secret_name: 'DNB_HOOVERS_BROWSER_LOGIN',
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
const desktopWrapperJs = await readFile(new URL('../../desktop-apps/browser/app.js', import.meta.url), 'utf8');
const syncJs = await readFile(new URL('../../shared/sync.js', import.meta.url), 'utf8');
const source = `${css}\n${html}`;
const forbiddenSurfacePattern = new RegExp(['ctox-pane--gla' + 'ss', 'Prem' + 'ium', 'gla' + 'ss'].join('|'), 'i');

assert.doesNotMatch(source, forbiddenSurfacePattern);
assert.doesNotMatch(source, /border-(?:left|right)\s*:\s*(?:[2-9]|[0-9]{2,})px/);
assert.doesNotMatch(source, /border-radius:\s*(?:10|12|14|16|18|20|24)px/);
assert.doesNotMatch(source, /box-shadow:\s*(?:0|inset|rgba|color-mix)/);
assert.match(css, /@container business-app-window \(max-width: 640px\)/);
assert.match(css, /\.browser-session-list[\s\S]*overflow-x: auto/);
assert.match(html, /data-browser-start/);
assert.match(html, /data-browser-private/);
assert.match(html, /data-browser-viewport/);
assert.match(html, /data-browser-new-tab/);
assert.match(html, /data-browser-go/);
assert.match(html, /data-browser-upload/);
assert.match(html, /data-browser-controller-acquire/);
assert.match(html, /data-browser-controller-release/);
assert.match(html, /data-browser-clipboard-copy/);
assert.match(html, /data-browser-clipboard-paste/);
assert.match(html, /data-browser-downloads/);
assert.match(js, /dispatch\(command, \{ until: 'accepted' \}\)/);
assert.match(js, /state\.latestSession = requestedSessionPending\s*\? null/);
assert.match(js, /\[refs\.go, refs\.stop,/);
assert.match(js, /templateUrl\.search = moduleUrl\.search/);
assert.match(js, /templateUrl\.searchParams\.set\('fragment', STYLE_BUILD\)/);
assert.match(js, /fetch\(templateUrl, \{ cache: 'no-store' \}\)/);
assert.match(desktopWrapperJs, /browserModuleUrl\.search = new URL\(import\.meta\.url\)\.search/);
assert.doesNotMatch(desktopWrapperJs, /modules\/browser\/index\.js\?v=/);
assert.match(js, /lease_id: state\.controllerLeaseId/);
assert.match(js, /if \(requiresController\) payload\.lease_id = state\.controllerLeaseId/);
assert.match(js, /session\.controller_lease_id === state\.controllerLeaseId/);
assert.match(syncJs, /isReadOnlyProjectionCollection[\s\S]{0,500}browser_sessions/);
assert.doesNotMatch(js, /upsertDoc\(browserCollection\(ctx, 'browser_sessions'\)/);
assert.doesNotMatch(html, /data-browser-(?:seed|clear|reset)/);
assert.match(js, /addEventListener\?\.\('focus', handleFocusRefresh\)/);
assert.doesNotMatch(
  js.match(/async function startBrowserRuntimeSync[\s\S]*?\n\}/)?.[0] || '',
  /catch\s*\([^)]*\)\s*\{[\s\S]*console\.warn/,
  'browser sync startup errors must reach the visible command error state',
);

// --- IA: two-pane sessions selector + remote canvas (2026-07-21) ---

// LEFT column carries the full SHELL-wired canonical grammar (data-pg-*).
assert.match(html, /ctox-workspace--two-pane/);
assert.match(html, /class="ctox-pane browser-sessions"/);
assert.match(html, /data-pg-search/);
assert.match(html, /data-pg-view="cards"/);
assert.match(html, /data-pg-view="list"/);
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
assert.match(css, /\.browser-sessions\s*\{[^}]*grid-template-rows:\s*auto auto minmax\(0, 1fr\) auto/);

// Grammar re-renders reactively on the shell event; no chrome wiring here.
assert.match(js, /addEventListener\('ctox-pane-grammar-change', onLeftGrammarChange\)/);
assert.match(js, /__ctoxPaneGrammar/);

// In-place selection flip: selecting a session marks rows in place and must
// NOT rebuild the list (a rebuild resets the well scroll to the top).
assert.match(js, /refs\.sessions\?\.addEventListener\('click'[\s\S]*?markActiveSession\(refs, sessionId\)/);
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
assert.match(js, /importBrowserSessions\(ctx, state, refs\)/);
assert.match(js, /exportBrowserSessions\(state, refs\)/);

const hooks = __browserTestHooks;
const sampleSessions = [
  { id: 'browser_session_a', runtime_status: 'active', profile_mode: 'persistent', current_url: 'https://acme.example/app', updated_at_ms: 3 },
  { id: 'browser_session_b', runtime_status: 'starting', profile_mode: 'private', title: 'Login', updated_at_ms: 2 },
  { id: 'browser_session_c', runtime_status: 'stopped', profile_mode: 'persistent', current_url: 'https://old.example', updated_at_ms: 1 },
];

// Band counts: zeros included; the band ignores its own selection.
assert.deepEqual(hooks.browserSessionViewCounts(sampleSessions, { band: 'active' }), { all: 3, active: 2, closed: 1 });
assert.deepEqual(hooks.browserSessionViewCounts([], {}), { all: 0, active: 0, closed: 0 });
assert.equal(hooks.browserSessionBand(sampleSessions[0]), 'active');
assert.equal(hooks.browserSessionBand(sampleSessions[2]), 'closed');

// Filtering by band / search / profile.
assert.equal(hooks.filterSessionsForView(sampleSessions, { band: 'closed' }).length, 1);
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
