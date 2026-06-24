"use strict";

const SENSITIVE_SEARCH_PARAMS = new Set([
  "ctox_config",
  "room_password",
  "signaling_room_password",
  "token",
  "launch_token",
]);

const ALLOWED_BUSINESS_OS_CONTROL_PATHS = new Set([
  "/api/business-os/status",
  "/api/business-os/sync/config",
  "/api/business-os/ctox/subscription-auth/start",
  "/api/business-os/ctox/subscription-auth/callback",
]);

// Request kinds that carry Business-OS data rather than shell assets. These are
// the only kinds the default-deny boundary constrains, so document/script/style/
// image/font loads for the shell itself are never blocked.
const BUSINESS_OS_DATA_RESOURCE_TYPES = new Set(["xhr", "fetch", "websocket", "webSocket"]);

// Schemes the desktop is willing to hand to the OS via shell.openExternal. Never
// pass file:/data:/custom schemes to openExternal from a remotely-loaded view.
const SAFE_EXTERNAL_PROTOCOLS = new Set(["http:", "https:", "mailto:"]);

function scrubCtoxConfigFromUrl(rawUrl) {
  const url = new URL(rawUrl);
  let changed = false;
  for (const key of SENSITIVE_SEARCH_PARAMS) {
    if (url.searchParams.has(key)) {
      url.searchParams.delete(key);
      changed = true;
    }
  }
  return changed ? url.toString() : rawUrl;
}

async function scrubCtoxConfigFromWebContents(webContents) {
  const currentUrl = webContents.getURL();
  const scrubbed = scrubCtoxConfigFromUrl(currentUrl);
  if (scrubbed === currentUrl) return false;
  await webContents.executeJavaScript(
    `history.replaceState(history.state, document.title, ${JSON.stringify(scrubbed)});`,
    true,
  );
  return true;
}

function isAllowedBusinessOsNavigation(rawUrl, allowedOrigins) {
  if (rawUrl === "about:blank") return true;
  let url;
  try {
    url = new URL(rawUrl);
  } catch (_error) {
    return false;
  }
  // A remotely-loaded instance view may only navigate the top frame to an
  // allowlisted http(s) origin. data:, file: and custom schemes are denied so
  // they cannot be used to render attacker-controlled HTML or read local files
  // in the privileged renderer; everything else is deflected to the OS browser.
  if (!["https:", "http:"].includes(url.protocol)) return false;
  return allowedOrigins.has(url.origin);
}

function isSafeExternalUrl(rawUrl) {
  try {
    return SAFE_EXTERNAL_PROTOCOLS.has(new URL(rawUrl).protocol);
  } catch (_error) {
    return false;
  }
}

function isForbiddenBusinessOsHttpDataRequest(rawUrl) {
  let url;
  try {
    url = new URL(rawUrl);
  } catch (_error) {
    return true;
  }
  if (!["http:", "https:"].includes(url.protocol)) return false;
  const path = normalizePathname(url.pathname);
  if (path.startsWith("/api/business-os/") || path === "/api/business-os") {
    return !ALLOWED_BUSINESS_OS_CONTROL_PATHS.has(path);
  }
  if (path.startsWith("/rxdb/dist/")) return false;
  if (path.startsWith("/rxdb/")) return true;
  if (path === "/commands" || path.startsWith("/commands/")) return true;
  return false;
}

// Default-deny boundary: any data-shaped request (xhr/fetch/websocket) to the
// instance's own launch host that is not an explicit control-plane or static
// asset path is blocked. This closes the gap where a NEW or differently-named
// HTTP data route (e.g. /files, /sync, /business_commands) would otherwise
// silently bridge Business-OS data off the RxDB/WebRTC plane.
function isForbiddenBusinessOsDataResourceRequest(rawUrl, resourceType, launchOrigin) {
  if (!BUSINESS_OS_DATA_RESOURCE_TYPES.has(String(resourceType || ""))) return false;
  let url;
  try {
    url = new URL(rawUrl);
  } catch (_error) {
    return true;
  }
  if (!["http:", "https:", "ws:", "wss:"].includes(url.protocol)) return false;
  // Only constrain the shell talking to its own host (https xhr or wss alike);
  // cross-host requests are governed by the denylist above and the shell's CSP.
  const launchHost = hostOf(launchOrigin);
  if (launchHost && url.host !== launchHost) return false;
  const path = normalizePathname(url.pathname);
  if (ALLOWED_BUSINESS_OS_CONTROL_PATHS.has(path)) return false;
  if (path.startsWith("/rxdb/dist/")) return false;
  return true;
}

function hostOf(origin) {
  try {
    return new URL(origin).host;
  } catch (_error) {
    return "";
  }
}

function normalizePathname(pathname) {
  // Lowercase so a case-variant data path (e.g. /API/Business-OS/Records on a
  // case-insensitive server) cannot slip past the classifier.
  const normalized = String(pathname || "/").replace(/\/{2,}/g, "/").toLowerCase();
  if (normalized.length > 1 && normalized.endsWith("/")) {
    return normalized.slice(0, -1);
  }
  return normalized || "/";
}

module.exports = {
  scrubCtoxConfigFromUrl,
  scrubCtoxConfigFromWebContents,
  isAllowedBusinessOsNavigation,
  isSafeExternalUrl,
  isForbiddenBusinessOsHttpDataRequest,
  isForbiddenBusinessOsDataResourceRequest,
};
