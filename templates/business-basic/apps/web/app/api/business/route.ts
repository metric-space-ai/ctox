import { NextResponse } from "next/server";
import { getBusinessBundle } from "@/lib/business-seed";

export async function GET() {
  return NextResponse.json(await getBusinessBundle());
}
