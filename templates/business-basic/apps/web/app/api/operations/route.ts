import { NextResponse } from "next/server";
import { getOperationsBundle } from "@/lib/operations-store";

export async function GET() {
  return NextResponse.json(await getOperationsBundle());
}
