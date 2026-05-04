import { addManualCompany, emitManualCompanyAdded, queueCompetitiveScrape } from "@/lib/competitive-analysis-runtime";
import { NextResponse } from "next/server";

type WatchlistRequest = {
  name?: string;
  url?: string;
  scrapeMode?: "rescrape_now" | "next_standard_scrape";
};

export async function POST(request: Request) {
  const body = await request.json() as WatchlistRequest;
  const result = addManualCompany({ name: body.name, url: body.url });

  if (!result.ok) {
    return NextResponse.json(result, { status: 400 });
  }

  if (!result.company) {
    return NextResponse.json({ ok: false, error: "company_missing" }, { status: 500 });
  }

  const core = await emitManualCompanyAdded(result.company);
  const scrape = await queueCompetitiveScrape({
    triggerKind: "watchlist_added",
    mode: body.scrapeMode ?? "next_standard_scrape"
  });

  return NextResponse.json({ ...result, core, scrape });
}
