type WebsiteIdentity = {
  sub: string;
  roles: string[];
  permissions: string[];
};

type WebsiteSessionPayload = WebsiteIdentity & {
  exp: number;
};

export async function createWebsiteSessionCookie(identity: WebsiteIdentity) {
  const payload: WebsiteSessionPayload = {
    ...identity,
    exp: Math.floor(Date.now() / 1000) + 60 * 60 * 12
  };
  const payloadPart = base64UrlEncode(new TextEncoder().encode(JSON.stringify(payload)));
  const signature = await signPayload(payloadPart, authSecret());
  return `${payloadPart}.${signature}`;
}

export function parseWebsiteSession(value?: string) {
  if (!value) return null;
  const [payloadPart] = value.split(".");
  if (!payloadPart) return null;

  try {
    const payload = JSON.parse(base64UrlDecode(payloadPart)) as WebsiteSessionPayload;
    return {
      sub: payload.sub,
      roles: Array.isArray(payload.roles) ? payload.roles : [],
      permissions: Array.isArray(payload.permissions) ? payload.permissions : []
    };
  } catch {
    return null;
  }
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

function authSecret() {
  const secret = process.env.WEBSITE_AUTH_SECRET;
  if (!secret) throw new Error("WEBSITE_AUTH_SECRET is required.");
  return secret;
}

function base64UrlEncode(bytes: Uint8Array) {
  let binary = "";
  bytes.forEach((byte) => {
    binary += String.fromCharCode(byte);
  });
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function base64UrlDecode(value: string) {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, "=");
  return atob(padded);
}
