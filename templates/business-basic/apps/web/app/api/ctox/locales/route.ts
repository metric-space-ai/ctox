import { NextResponse } from "next/server";
import { defaultLocale, localeRegistry } from "@ctox-business/ui";

export async function GET() {
  return NextResponse.json({
    ok: true,
    defaultLocale,
    locales: localeRegistry
  });
}
