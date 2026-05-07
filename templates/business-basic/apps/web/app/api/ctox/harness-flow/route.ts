import { getCtoxHarnessFlow } from "@/lib/ctox-core-bridge";
import { NextResponse } from "next/server";

export async function GET() {
  return NextResponse.json(await getCtoxHarnessFlow());
}
