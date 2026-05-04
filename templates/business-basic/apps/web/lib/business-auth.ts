import type { RequestCookies } from "next/dist/compiled/@edge-runtime/cookies";
import type { ReadonlyRequestCookies } from "next/dist/server/web/spec-extension/adapters/request-cookies";
import { sessionCookieName } from "./company-settings";

export type BusinessAccess = {
  subject: string;
  source: "business" | "website";
  roles: string[];
};

type WebsiteSessionPayload = {
  sub?: string;
  email?: string;
  name?: string;
  roles?: string[];
  permissions?: string[];
  exp?: number;
};

const websiteAccessModes = new Set(["website", "hybrid"]);
const localAccessModes = new Set(["local", "hybrid"]);

export const websiteSessionCookieName = process.env.CTOX_WEBSITE_SESSION_COOKIE ?? "ctox_website_session";

export function businessAccessMode() {
  const mode = process.env.CTOX_BUSINESS_OS_ACCESS_MODE ?? "local";
  if (mode === "website" || mode === "hybrid" || mode === "local") return mode;
  return "local";
}

export async function resolveBusinessAccessFromCookies(cookies: RequestCookies | ReadonlyRequestCookies) {
  const mode = businessAccessMode();
  const localSession = cookies.get(sessionCookieName)?.value;
  if (localSession && localAccessModes.has(mode)) {
    return {
      subject: decodeURIComponent(localSession),
      source: "business",
      roles: ["business_os_admin"]
    } satisfies BusinessAccess;
  }

  if (!websiteAccessModes.has(mode)) return null;

  const websiteSession = cookies.get(websiteSessionCookieName)?.value;
  const payload = await verifyWebsiteSessionCookie(websiteSession);
  if (!payload || !hasBusinessOsRole(payload)) return null;

  return {
    subject: payload.sub ?? payload.email ?? payload.name ?? "website-user",
    source: "website",
    roles: payload.roles ?? []
  } satisfies BusinessAccess;
}

export function isWebsiteBusinessAccessEnabled() {
  return websiteAccessModes.has(businessAccessMode());
}

export function websiteBusinessRole() {
  return process.env.CTOX_BUSINESS_OS_ROLE ?? "business_os_user";
}

export function websiteBusinessAdminRole() {
  return process.env.CTOX_BUSINESS_OS_ADMIN_ROLE ?? "business_os_admin";
}

async function verifyWebsiteSessionCookie(value?: string) {
  if (!value) return null;
  const secret = process.env.CTOX_WEBSITE_AUTH_SECRET;
  if (!secret) return null;

  const [payloadPart, signaturePart] = value.split(".");
  if (!payloadPart || !signaturePart) return null;

  const expectedSignature = await signPayload(payloadPart, secret);
  if (!timingSafeEqual(signaturePart, expectedSignature)) return null;

  const payload = parseJson(base64UrlDecode(payloadPart));
  if (!isWebsiteSessionPayload(payload)) return null;
  if (payload.exp && payload.exp < Math.floor(Date.now() / 1000)) return null;

  return payload;
}

function hasBusinessOsRole(payload: WebsiteSessionPayload) {
  const roles = new Set(payload.roles ?? []);
  const permissions = new Set(payload.permissions ?? []);
  return (
    roles.has(websiteBusinessRole())
    || roles.has(websiteBusinessAdminRole())
    || permissions.has("business_os:access")
    || permissions.has("business_os:admin")
  );
}

async function signPayload(payloadPart: string, secret: string) {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { hash: "SHA-256", name: "HMAC" },
    false,
    ["sign"]
  );
  const signature = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(payloadPart));
  return base64UrlEncode(new Uint8Array(signature));
}

function timingSafeEqual(left: string, right: string) {
  const leftBytes = new TextEncoder().encode(left);
  const rightBytes = new TextEncoder().encode(right);
  if (leftBytes.length !== rightBytes.length) return false;

  let diff = 0;
  leftBytes.forEach((byte, index) => {
    diff |= byte ^ rightBytes[index]!;
  });
  return diff === 0;
}

function base64UrlDecode(value: string) {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, "=");
  return atob(padded);
}

function base64UrlEncode(bytes: Uint8Array) {
  let binary = "";
  bytes.forEach((byte) => {
    binary += String.fromCharCode(byte);
  });
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function parseJson(value: string) {
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return null;
  }
}

function isWebsiteSessionPayload(value: unknown): value is WebsiteSessionPayload {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const payload = value as WebsiteSessionPayload;
  return (
    (!payload.roles || Array.isArray(payload.roles))
    && (!payload.permissions || Array.isArray(payload.permissions))
  );
}
