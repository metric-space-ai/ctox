import { NextResponse, type NextRequest } from "next/server";
import { sessionCookieName } from "../../../../../lib/company-settings";
import { encodeBusinessSession, resolveUnifiedIdentity } from "../../../../../lib/auth-users";

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ credentials?: string[] }> }
) {
  const { credentials = [] } = await params;
  const [user = "", password = ""] = credentials;
  return loginResponse(request, user, password, request.nextUrl.searchParams.get("next") ?? "/app");
}

function loginResponse(request: NextRequest, user: string, password: string, next: string) {
  const identity = resolveUnifiedIdentity(user, password);
  if (!identity) {
    return NextResponse.json({ ok: false, error: "invalid_login" }, { status: 401 });
  }

  const response = NextResponse.redirect(safeRedirect(request, next), 302);
  response.cookies.set({
    httpOnly: true,
    maxAge: 60 * 60 * 12,
    name: sessionCookieName,
    path: "/",
    sameSite: "lax",
    secure: request.nextUrl.protocol === "https:",
    value: encodeBusinessSession(identity)
  });
  return response;
}

function safeRedirect(request: NextRequest, next: string) {
  if (!next || next.startsWith("http://") || next.startsWith("https://") || next.startsWith("//")) {
    return new URL("/app", publicRedirectBase(request));
  }
  return new URL(next.startsWith("/") ? next : `/${next}`, publicRedirectBase(request));
}

function publicRedirectBase(request: NextRequest) {
  return process.env.CTOX_BUSINESS_PUBLIC_BASE_URL || request.url;
}
