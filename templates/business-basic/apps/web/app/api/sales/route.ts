import { NextResponse } from "next/server";
import { getSalesBundle } from "@/lib/sales-seed";

export async function GET() {
  return NextResponse.json(await getSalesBundle());
}
