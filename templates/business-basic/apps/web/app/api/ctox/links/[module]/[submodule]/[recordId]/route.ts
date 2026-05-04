import { NextResponse } from "next/server";
import { businessDeepLink } from "@ctox-business/ui";
import { inferDeepLinkPanel } from "../../../../../../../lib/deep-link-panels";

export async function GET(
  request: Request,
  { params }: { params: Promise<{ module: string; submodule: string; recordId: string }> }
) {
  const resolved = await params;
  const url = new URL(request.url);
  const link = businessDeepLink({
    baseUrl: url.origin,
    module: resolved.module,
    submodule: resolved.submodule,
    recordId: resolved.recordId,
    panel: url.searchParams.get("panel") ?? inferDeepLinkPanel(resolved.module, resolved.submodule),
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
