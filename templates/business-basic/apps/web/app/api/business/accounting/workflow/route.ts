import { listAccountingProposals, listBusinessOutboxEvents } from "@ctox-business/db/accounting";
import { NextResponse } from "next/server";

export async function GET() {
  if (!process.env.DATABASE_URL) {
    return NextResponse.json({
      outbox: [],
      persistence: "disabled",
      proposals: [],
      reason: "DATABASE_URL not configured"
    });
  }

  try {
    const [proposals, outbox] = await Promise.all([
      listAccountingProposals(),
      listBusinessOutboxEvents()
    ]);

    return NextResponse.json({
      outbox,
      persistence: "enabled",
      proposals
    });
  } catch (error) {
    return NextResponse.json({
      error: error instanceof Error ? error.message : String(error),
      outbox: [],
      persistence: "error",
      proposals: []
    }, { status: 500 });
  }
}
