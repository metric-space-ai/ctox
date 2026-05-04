import { NextResponse, type NextRequest } from "next/server";
import { sessionCookieName } from "../../../../../lib/company-settings";

const defaultUser = "admin";
const defaultPassword = "ctox-business";

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ credentials?: string[] }> }
) {
  const { credentials = [] } = await params;
  const [user = "", password = ""] = credentials;
  return loginResponse(request, user, password, request.nextUrl.searchParams.get("next") ?? "/app");
}

function loginResponse(request: NextRequest, user: string, password: string, next: string) {
  if (!isValidLogin(user, password)) {
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
    value: encodeURIComponent(user)
  });
  return response;
}

function isValidLogin(user: string, password: string) {
  const expectedUser = process.env.CTOX_BUSINESS_USER ?? defaultUser;
  const expectedPassword = process.env.CTOX_BUSINESS_PASSWORD ?? defaultPassword;
  return user === expectedUser && password === expectedPassword;
}

function safeRedirect(request: NextRequest, next: string) {
  if (!next || next.startsWith("http://") || next.startsWith("https://") || next.startsWith("//")) {
    return new URL("/app", request.url);
  }
  return new URL(next.startsWith("/") ? next : `/${next}`, request.url);
}
