import { NextResponse } from "next/server";
import { businessModules } from "@ctox-business/ui";

export async function GET() {
  return NextResponse.json({
    ok: true,
    modules: businessModules
  });
}

