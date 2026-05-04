import { getCtoxResource } from "@/lib/ctox-seed";
import { NextResponse } from "next/server";

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ resource: string }> }
) {
  const { resource } = await params;
  const data = await getCtoxResource(resource);

  if (!data) {
    return NextResponse.json({ ok: false, error: "unknown_ctox_resource" }, { status: 404 });
  }

  return NextResponse.json({ ok: true, data });
}
