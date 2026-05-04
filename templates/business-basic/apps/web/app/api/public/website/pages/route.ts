import { getMarketingResource, text, type SupportedLocale, type WebsitePage } from "@/lib/marketing-seed";
import { NextResponse } from "next/server";

export async function GET(request: Request) {
  const locale = new URL(request.url).searchParams.get("locale") === "en" ? "en" : "de";
  const pages = await getMarketingResource("website") as WebsitePage[] | null;

  return NextResponse.json({
    ok: true,
    data: (pages ?? [])
      .filter((page) => page.status === "published")
      .map((page) => publicPage(page, locale))
  });
}

function publicPage(page: WebsitePage, locale: SupportedLocale) {
  return {
    id: page.id,
    title: page.title,
    path: page.path,
    updated: page.updated,
    intent: text(page.intent, locale),
    nextAction: text(page.nextAction, locale)
  };
}
