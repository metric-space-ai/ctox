// Origin: CTOX
// License: Apache-2.0
//
// Shared browser business logic for the browser app v4 handlers.
// ref: src/core/business_os/rxdb_peer_browser.rs (read-only port).
//
// Split (Architektur §16.13): the embedded browser engine (navigate, tabs,
// clipboard, dialog/permission answering, credential fill, frame pump) is the
// native FAEHIGKEIT `ctx.cap.browser.*`. The FACHLOGIK ported here is the
// controller-lease bookkeeping on the `browser_sessions` collection and the
// lease gate that every engine op must pass first.

export function fail(code, message) {
  throw { code, message };
}

// ref: rxdb_peer_browser.rs — fixed 120 s controller lease window.
export const CONTROLLER_LEASE_MS = 120_000;

export function payloadString(payload, key) {
  const value = payload?.[key];
  if (typeof value !== 'string') return undefined;
  const trimmed = value.trim();
  return trimmed === '' ? undefined : value;
}

// ref: normalize_browser_runtime_url — trim; a blank url is treated as absent.
export function normalizeUrl(value) {
  return typeof value === 'string' ? value.trim() : '';
}

/** session_id / tab_id resolution from the payload. */
export function sessionId(payload) {
  const id = payloadString(payload, 'session_id') ?? payloadString(payload, 'session');
  if (!id) fail('invalid', 'browser command requires session_id');
  return id;
}
export function tabId(payload) {
  return payloadString(payload, 'tab_id') ?? payloadString(payload, 'tab') ?? '';
}

/**
 * Enforce the controller lease for an engine op (every command except
 * session.start and the controller.* arms). ref: rxdb_peer_browser.rs:1316-1339.
 */
export function assertLeaseHolder(session, payload, nowMs) {
  if (!session || typeof session !== 'object') return; // no session yet → start path
  const owner = typeof session.controller_user_id === 'string' ? session.controller_user_id : '';
  const lease = typeof session.controller_lease_id === 'string' ? session.controller_lease_id : '';
  const expires = typeof session.controller_lease_expires_at_ms === 'number' ? session.controller_lease_expires_at_ms : 0;
  const requested = payloadString(payload, 'lease_id') ?? payloadString(payload, 'controller_lease_id');
  const ownerId = typeof session.owner_user_id === 'string' ? session.owner_user_id : owner;
  if (!(owner === ownerId && requested === lease && expires > nowMs)) {
    fail('forbidden', 'browser controller lease is missing or expired');
  }
}

/**
 * Translate a lifecycle command into the engine runtime op + params.
 * ref: rxdb_peer_browser.rs:1652-1717 (the op/op_params match).
 */
export function engineOp(commandType, payload, targetUrl, tab) {
  switch (commandType) {
    case 'browser.navigate':
    case 'browser.session.start':
    case 'browser.reset':
      return ['navigate', { url: targetUrl, timeoutMs: 30000 }];
    case 'browser.reload':
      return ['reload', { timeoutMs: 30000 }];
    case 'browser.back':
      return ['back', { timeoutMs: 30000 }];
    case 'browser.forward':
      return ['forward', { timeoutMs: 30000 }];
    case 'browser.tab.open':
      return ['tab_open', { tabId: tab, url: targetUrl, timeoutMs: 30000 }];
    case 'browser.tab.activate':
      return ['tab_activate', { tabId: tab }];
    case 'browser.tab.close':
      return ['tab_close', { tabId: tab }];
    case 'browser.dialog.respond':
      return ['dialog_respond', {
        accept: payload?.accept === true,
        value: payload?.value ?? null,
      }];
    case 'browser.permission.respond':
      return ['permission_respond', { accept: payload?.accept === true }];
    case 'browser.clipboard.copy':
      return ['clipboard_copy', {}];
    default:
      return ['nav_state', {}];
  }
}
