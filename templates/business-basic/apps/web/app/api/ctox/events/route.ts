import { emitCtoxCoreEvent } from "@/lib/ctox-core-bridge";
import { NextResponse } from "next/server";

export async function GET() {
  return NextResponse.json({
    ok: true,
    data: [
      {
        id: "event-business-stack-ready",
        type: "business.stack.ready",
        module: "ctox",
        recordType: "stack",
        recordId: "business-basic",
        occurredAt: "2026-05-02T15:30:00.000Z",
        payload: {
          status: "ready",
          modules: ["sales", "marketing", "operations", "business", "ctox"]
        }
      }
    ]
  });
}

export async function POST(request: Request) {
  const event = await request.json();
  const result = await emitCtoxCoreEvent({
    type: event?.type ?? "business.event",
    module: event?.module ?? "ctox",
    recordType: event?.recordType ?? "event",
    recordId: event?.recordId ?? crypto.randomUUID(),
    payload: event?.payload ?? event ?? {}
  });

  return NextResponse.json({
    ok: true,
    accepted: true,
    eventType: event?.type ?? null,
    core: result
  });
}
