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

console.log('browser module pure contract smoke OK');
