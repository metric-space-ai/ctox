import { NextResponse, type NextRequest } from "next/server";
import { resolveBusinessAccessFromCookies } from "@/lib/business-auth";
import { bootstrapDemoTenant, normalizeBootstrapInput } from "@/lib/bootstrap-demo";
import { companyNameCookieName } from "@/lib/company-settings";

export async function GET(request: NextRequest) {
  if (!await resolveBusinessAccessFromCookies(request.cookies)) {
    return NextResponse.json({ ok: false, error: "unauthorized" }, { status: 401 });
  }

  const input = normalizeBootstrapInput({
    companyName: request.nextUrl.searchParams.get("companyName") ?? request.nextUrl.searchParams.get("company"),
    locale: request.nextUrl.searchParams.get("locale"),
    mission: request.nextUrl.searchParams.get("mission"),
    mode: "guided",
    vision: request.nextUrl.searchParams.get("vision")
  });

  return NextResponse.json({
    ok: true,
    mode: input.mode,
    companyName: input.companyName,
    slug: input.slug,
    wrote: false,
    plan: [
      { module: "settings", resource: "organization", count: 1, action: "upsert tenant settings" },
      { module: "marketing", resource: "website/campaigns/research", count: 7, action: "generate placeholders from mission and vision" },
      { module: "sales", resource: "campaigns/leads/offers/customers", count: 11, action: "generate placeholders from mission and vision" },
      { module: "operations", resource: "projects/work/knowledge", count: 12, action: "generate operating placeholders" },
      { module: "business", resource: "products/invoices/reports", count: 7, action: "generate commercial placeholders" },
      { module: "ctox", resource: "bugs", count: 1, action: "generate setup follow-up example" }
    ]
  });
}

export async function POST(request: NextRequest) {
  if (!await resolveBusinessAccessFromCookies(request.cookies)) {
    return NextResponse.json({ ok: false, error: "unauthorized" }, { status: 401 });
  }

  const contentType = request.headers.get("content-type") ?? "";
  const isJson = contentType.includes("application/json");
  const body = isJson
    ? await request.json().catch(() => ({})) as Record<string, unknown>
    : formToObject(await request.formData().catch(() => null));

  try {
    const result = await bootstrapDemoTenant({
      companyName: String(body.companyName ?? body.company ?? ""),
      locale: String(body.locale ?? ""),
      mission: String(body.mission ?? ""),
      mode: String(body.mode ?? "demo"),
      vision: String(body.vision ?? "")
    });
    const next = String(body.next ?? "/app/ctox/settings");
    const response = isJson ? NextResponse.json(result) : NextResponse.redirect(safeRedirect(request, next, result));
    response.cookies.set({
      httpOnly: true,
      maxAge: 60 * 60 * 24 * 365,
      name: companyNameCookieName,
      path: "/",
      sameSite: "lax",
      secure: request.nextUrl.protocol === "https:",
      value: result.companyName
    });
    return response;
  } catch (error) {
    return NextResponse.json({
      ok: false,
      error: error instanceof Error ? error.message : "bootstrap_failed"
    }, { status: 400 });
  }
}

function formToObject(form: FormData | null): Record<string, unknown> {
  if (!form) return {};
  return Object.fromEntries(form.entries());
}

function safeRedirect(request: NextRequest, next: string, result: { mode: string; wrote: boolean }) {
  const fallback = "/app/ctox/settings";
  const path = !next || next.startsWith("http://") || next.startsWith("https://") || next.startsWith("//")
    ? fallback
    : next.startsWith("/") ? next : `/${next}`;
  const url = new URL(path, request.url);
  url.searchParams.set("bootstrap", result.wrote ? "done" : "plan");
  url.searchParams.set("mode", result.mode);
  return url;
}
