import { NextResponse } from "next/server";
import { getMarketingBundle } from "../../../lib/marketing-seed";

export async function GET() {
  return NextResponse.json({
    ok: true,
    data: await getMarketingBundle()
  });
}
