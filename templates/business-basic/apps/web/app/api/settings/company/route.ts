import { NextResponse, type NextRequest } from "next/server";
import { resolveBusinessAccessFromCookies } from "@/lib/business-auth";
import { companyNameCookieName, normalizeCompanyName } from "../../../../lib/company-settings";

export async function GET(request: NextRequest) {
  const companyName = request.nextUrl.searchParams.get("company") ?? request.nextUrl.searchParams.get("name");
  return updateCompanyName(request, companyName, request.nextUrl.searchParams.get("next") ?? "/app/ctox/settings");
}

export async function POST(request: NextRequest) {
  const contentType = request.headers.get("content-type") ?? "";
  let companyName = "";
  let next = "/app/ctox/settings";

  if (contentType.includes("application/json")) {
    const body = await request.json().catch(() => ({})) as Record<string, unknown>;
    companyName = String(body.companyName ?? body.company ?? body.name ?? "");
    next = String(body.next ?? body.redirect ?? next);
  } else {
    const form = await request.formData().catch(() => null);
    companyName = String(form?.get("companyName") ?? form?.get("company") ?? form?.get("name") ?? "");
    next = String(form?.get("next") ?? form?.get("redirect") ?? next);
  }

  return updateCompanyName(request, companyName, next);
}

async function updateCompanyName(request: NextRequest, companyName: string | null, next: string) {
  if (!await resolveBusinessAccessFromCookies(request.cookies)) {
    return NextResponse.json({ ok: false, error: "unauthorized" }, { status: 401 });
  }

  const response = NextResponse.redirect(safeRedirect(request, next));
  response.cookies.set({
    httpOnly: true,
    maxAge: 60 * 60 * 24 * 365,
    name: companyNameCookieName,
    path: "/",
    sameSite: "lax",
    secure: request.nextUrl.protocol === "https:",
    value: normalizeCompanyName(companyName)
  });
  return response;
}

function safeRedirect(request: NextRequest, next: string) {
  if (!next || next.startsWith("http://") || next.startsWith("https://") || next.startsWith("//")) {
    return new URL("/app/ctox/settings", request.url);
  }
  return new URL(next.startsWith("/") ? next : `/${next}`, request.url);
}
