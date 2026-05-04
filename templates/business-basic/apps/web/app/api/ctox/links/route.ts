import { NextResponse } from "next/server";
import { businessDeepLink } from "@ctox-business/ui";
import { inferDeepLinkPanel } from "../../../../lib/deep-link-panels";

export async function GET(request: Request) {
  const url = new URL(request.url);
  const link = businessDeepLink({
    baseUrl: url.origin,
    module: url.searchParams.get("module") ?? "",
    submodule: url.searchParams.get("submodule") ?? undefined,
    recordId: url.searchParams.get("recordId") ?? undefined,
    panel: url.searchParams.get("panel") ?? inferDeepLinkPanel(url.searchParams.get("module") ?? "", url.searchParams.get("submodule") ?? ""),
    drawer: parseDrawer(url.searchParams.get("drawer")),
    locale: url.searchParams.get("locale") ?? undefined,
    theme: url.searchParams.get("theme") ?? undefined
  });

  if (!link) {
    return NextResponse.json({ ok: false, error: "Unknown module or submodule." }, { status: 404 });
  }

  return NextResponse.json({ ok: true, link });
}

function parseDrawer(value: string | null) {
  if (value === "left-bottom" || value === "bottom" || value === "right") return value;
  return "right";
}
