import { NextResponse } from "next/server";
import { getBusinessBundle } from "@/lib/business-seed";
import { getDatabaseBackedBusinessBundle } from "@/lib/business-db-bundle";

export async function GET() {
  return NextResponse.json(await getDatabaseBackedBusinessBundle(await getBusinessBundle()));
}
