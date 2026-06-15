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
  const url = new URL(rawUrl);
  if (["about:", "data:"].includes(url.protocol)) return true;
  if (!["https:", "http:", "file:"].includes(url.protocol)) return false;
  if (url.protocol === "file:") return true;
  return allowedOrigins.has(url.origin);
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

function normalizePathname(pathname) {
  const normalized = String(pathname || "/").replace(/\/{2,}/g, "/");
  if (normalized.length > 1 && normalized.endsWith("/")) {
    return normalized.slice(0, -1);
  }
  return normalized || "/";
}

module.exports = {
  scrubCtoxConfigFromUrl,
  scrubCtoxConfigFromWebContents,
  isAllowedBusinessOsNavigation,
  isForbiddenBusinessOsHttpDataRequest,
};
