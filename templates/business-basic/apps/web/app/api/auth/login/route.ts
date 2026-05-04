import { NextResponse, type NextRequest } from "next/server";
import { sessionCookieName } from "../../../../lib/company-settings";
const defaultUser = "admin";
const defaultPassword = "ctox-business";

export async function GET(request: NextRequest) {
  const search = request.nextUrl.searchParams;
  const user = search.get("user") ?? search.get("username") ?? search.get("u") ?? "";
  const password = search.get("password") ?? search.get("pass") ?? search.get("p") ?? "";
  return loginResponse(request, user, password, search.get("next") ?? search.get("redirect") ?? "/app");
}

export async function POST(request: NextRequest) {
  const contentType = request.headers.get("content-type") ?? "";
  let user = "";
  let password = "";
  let next = "/app";

  if (contentType.includes("application/json")) {
    const body = await request.json().catch(() => ({})) as Record<string, unknown>;
    user = String(body.user ?? body.username ?? "");
    password = String(body.password ?? body.pass ?? "");
    next = String(body.next ?? body.redirect ?? "/app");
  } else {
    const form = await request.formData().catch(() => null);
    user = String(form?.get("user") ?? form?.get("username") ?? "");
    password = String(form?.get("password") ?? form?.get("pass") ?? "");
    next = String(form?.get("next") ?? form?.get("redirect") ?? "/app");
  }

  return loginResponse(request, user, password, next);
}

function loginResponse(request: NextRequest, user: string, password: string, next: string) {
  if (!isValidLogin(user, password)) {
    return NextResponse.json({ ok: false, error: "invalid_login" }, { status: 401 });
  }

  const target = safeRedirect(request, next);
  const response = NextResponse.redirect(target, 302);
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
