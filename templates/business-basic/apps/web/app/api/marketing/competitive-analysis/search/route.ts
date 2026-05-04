import { searchCompetitorCompanies } from "@/lib/competitive-analysis-runtime";
import { NextResponse } from "next/server";

type SearchRequest = {
  query?: string;
};

export async function POST(request: Request) {
  const body = await request.json() as SearchRequest;
  const result = await searchCompetitorCompanies(body.query ?? "");

  if (!result.ok) {
    return NextResponse.json(result, { status: 400 });
  }

  return NextResponse.json(result);
}
