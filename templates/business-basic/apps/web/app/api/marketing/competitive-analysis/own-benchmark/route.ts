import { buildOwnBenchmark } from "@/lib/competitive-analysis-runtime";
import { NextResponse } from "next/server";

export async function GET() {
  return NextResponse.json({ ok: true, benchmark: buildOwnBenchmark() });
}
