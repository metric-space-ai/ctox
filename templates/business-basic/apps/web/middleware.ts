import { NextResponse, type NextRequest } from "next/server";
import { resolveBusinessAccessFromCookies } from "./lib/business-auth";

export async function middleware(request: NextRequest) {
  if (await resolveBusinessAccessFromCookies(request.cookies)) return NextResponse.next();

  if (request.nextUrl.pathname.startsWith("/api/")) {
    return NextResponse.json({ ok: false, error: "unauthorized" }, { status: 401 });
  }

  const loginUrl = request.nextUrl.clone();
  loginUrl.pathname = "/";
  loginUrl.searchParams.set("next", `${request.nextUrl.pathname}${request.nextUrl.search}`);
  return NextResponse.redirect(loginUrl);
}

export const config = {
  matcher: [
    "/app/:path*",
    "/api/business/:path*",
    "/api/ctox/:path*",
    "/api/marketing/:path*",
    "/api/operations/:path*",
    "/api/sales/:path*",
    "/api/settings/:path*"
  ]
};
