import { NextResponse } from "next/server";
import {
  executeWarehouseCheckoutEvent,
  type WarehouseCheckoutEvent,
  type WarehouseCheckoutEventType
} from "@/lib/warehouse-runtime";

export async function POST(request: Request) {
  const body = await request.json().catch(() => ({})) as Partial<WarehouseCheckoutEvent>;
  const eventType = parseCheckoutEventType(body.eventType);
  if (!eventType || typeof body.checkoutSessionId !== "string" || typeof body.eventId !== "string") {
    return NextResponse.json({
      ok: false,
      error: "invalid_checkout_event"
    }, { status: 400 });
  }

  try {
    const result = await executeWarehouseCheckoutEvent({
      checkoutSessionId: body.checkoutSessionId,
      eventId: body.eventId,
      eventType,
      lines: body.lines,
      orderId: body.orderId,
      paymentIntentId: body.paymentIntentId,
      provider: body.provider ?? "stripe"
    });
    return NextResponse.json({
      ok: true,
      resource: "warehouse",
      ...result
    });
  } catch (error) {
    return NextResponse.json({
      ok: false,
      error: error instanceof Error ? error.message : String(error),
      resource: "warehouse"
    }, { status: 400 });
  }
}

function parseCheckoutEventType(value: unknown): WarehouseCheckoutEventType | null {
  if (
    value === "checkout.created" ||
    value === "checkout.expired" ||
    value === "payment.failed" ||
    value === "payment.succeeded" ||
    value === "fulfillment.shipped"
  ) return value;
  return null;
}
