import { NextResponse, type NextRequest } from "next/server";
import { createWebsiteSessionCookie } from "../../../../lib/website-session";

export async function POST(request: NextRequest) {
  const form = await request.formData().catch(() => null);
  const user = String(form?.get("user") ?? "");
  const password = String(form?.get("password") ?? "");
  const next = String(form?.get("next") ?? "/");
  const identity = resolveIdentity(user, password);

  if (!identity) {
    return NextResponse.json({ ok: false, error: "invalid_login" }, { status: 401 });
  }

  const response = NextResponse.redirect(safeRedirect(request, next), 302);
  response.cookies.set({
    httpOnly: true,
    maxAge: 60 * 60 * 12,
    name: "ctox_website_session",
    path: "/",
    sameSite: "lax",
    secure: request.nextUrl.protocol === "https:",
    value: await createWebsiteSessionCookie(identity)
  });
  return response;
}

function resolveIdentity(user: string, password: string) {
  const customerUser = process.env.WEBSITE_USER ?? "customer";
  const customerPassword = process.env.WEBSITE_PASSWORD ?? "customer";
  const teamUser = process.env.WEBSITE_TEAM_USER ?? "team";
  const teamPassword = process.env.WEBSITE_TEAM_PASSWORD ?? "business-os";

  if (user === teamUser && password === teamPassword) {
    return {
      sub: user,
      roles: splitList(process.env.WEBSITE_TEAM_ROLES ?? "business_os_user"),
      permissions: splitList(process.env.WEBSITE_TEAM_PERMISSIONS ?? "business_os:access")
    };
  }

  if (user === customerUser && password === customerPassword) {
    return {
      sub: user,
      roles: splitList(process.env.WEBSITE_USER_ROLES ?? "customer"),
      permissions: splitList(process.env.WEBSITE_USER_PERMISSIONS ?? "")
    };
  }

  return null;
}

function splitList(value: string) {
  return value.split(",").map((item) => item.trim()).filter(Boolean);
}

function safeRedirect(request: NextRequest, next: string) {
  if (!next || next.startsWith("http://") || next.startsWith("https://") || next.startsWith("//")) {
    return new URL("/", request.url);
  }
  return new URL(next.startsWith("/") ? next : `/${next}`, request.url);
}
