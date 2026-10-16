// Origin: CTOX
// License: Apache-2.0
//
// Engine-op runner + controller-lease runner shared by the browser handlers.
// ref: rxdb_peer_browser.rs — the op dispatch (:1652) and controller block (:1222).
import {
  CONTROLLER_LEASE_MS, fail, payloadString, normalizeUrl, sessionId, tabId,
  assertLeaseHolder, engineOp,
} from './browser.js';

const SESSIONS = 'browser_sessions';
const TABS = 'browser_tabs';
const DEFAULT_URL = 'https://example.com';

/** Resolve the navigation target: explicit url, else the tab/session's last url. */
async function resolveTargetUrl(ctx, session, tab, payload) {
  const explicit = normalizeUrl(payload?.url);
  if (explicit) return explicit;
  const existingTab = tab ? await ctx.get(TABS, tab) : null;
  const previous = (existingTab && typeof existingTab.url === 'string' && existingTab.url)
    || (session && typeof session.current_url === 'string' && session.current_url)
    || DEFAULT_URL;
  return previous;
}

/**
 * Run a regular engine op (navigate/tab/reload/dialog/permission/clipboard).
 * The lease gate runs first for everything except session.start.
 */
export async function runEngine(command, ctx) {
  const payload = command.payload ?? {};
  const id = sessionId(payload);
  const tab = tabId(payload);
  const session = await ctx.get(SESSIONS, id);
  if (command.type !== 'browser.session.start') {
    assertLeaseHolder(session, payload, ctx.now());
  }
  const targetUrl = await resolveTargetUrl(ctx, session, tab, payload);
  const [op, params] = engineOp(command.type, payload, targetUrl, tab);
  return ctx.cap.browser.op({ session_id: id, tab_id: tab || null, op, params });
}

/**
 * Controller lease acquire/renew/release — pure record maintenance on
 * browser_sessions. ref: rxdb_peer_browser.rs:1222-1309.
 */
export async function runController(command, ctx) {
  const payload = command.payload ?? {};
  const id = sessionId(payload);
  const session = await ctx.get(SESSIONS, id);
  if (!session || typeof session !== 'object') fail('not_found', 'browser session not found');
  const leaseId = payloadString(payload, 'lease_id') ?? payloadString(payload, 'controller_lease_id');
  if (!leaseId) fail('invalid', 'browser controller lease id is required');
  const owner = typeof session.owner_user_id === 'string' && session.owner_user_id
    ? session.owner_user_id : (command.actor?.id ?? '');
  const currentController = typeof session.controller_user_id === 'string' ? session.controller_user_id : '';
  const currentLease = typeof session.controller_lease_id === 'string' ? session.controller_lease_id : '';

  if (command.type !== 'browser.controller.acquire') {
    if (!(currentController === owner && currentLease === leaseId)) {
      fail('conflict', 'browser controller lease does not match');
    }
  }

  const now = ctx.now();
  let controllerUserId;
  let controllerLeaseId;
  let controllerLeaseExpiresAtMs;
  if (command.type === 'browser.controller.release') {
    controllerUserId = '';
    controllerLeaseId = '';
    controllerLeaseExpiresAtMs = 0;
  } else {
    controllerUserId = owner;
    controllerLeaseId = leaseId;
    controllerLeaseExpiresAtMs = now + CONTROLLER_LEASE_MS;
  }

  await ctx.patch(SESSIONS, id, {
    controller_user_id: controllerUserId,
    controller_lease_id: controllerLeaseId,
    controller_lease_expires_at_ms: controllerLeaseExpiresAtMs,
    updated_at_ms: now,
  });

  return {
    ok: true,
    session_id: id,
    controller_user_id: controllerUserId,
    controller_lease_id: controllerLeaseId,
    controller_lease_expires_at_ms: controllerLeaseExpiresAtMs,
  };
}
