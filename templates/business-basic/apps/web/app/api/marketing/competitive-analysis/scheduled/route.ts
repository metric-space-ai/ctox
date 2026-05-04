import { queueCompetitiveScrape } from "@/lib/competitive-analysis-runtime";
import { NextResponse } from "next/server";

export async function GET() {
  const result = await queueCompetitiveScrape({
    mode: "rescrape_now",
    scheduledFor: new Date().toISOString(),
    triggerKind: "scheduled"
  });

  return NextResponse.json(result);
}
