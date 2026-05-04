import { queueCompetitiveScrape, type ScrapeTriggerKind } from "@/lib/competitive-analysis-runtime";
import { NextResponse } from "next/server";

type ScrapeRequest = {
  criterion?: string;
  mode?: "rescrape_now" | "next_standard_scrape";
  triggerKind?: ScrapeTriggerKind;
};

export async function POST(request: Request) {
  const body = await request.json() as ScrapeRequest;
  const result = await queueCompetitiveScrape({
    criterion: body.criterion,
    mode: body.mode ?? "rescrape_now",
    triggerKind: body.triggerKind ?? "manual"
  });

  return NextResponse.json(result);
}
