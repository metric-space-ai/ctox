import { createHmac, timingSafeEqual } from "crypto";
import { NextResponse } from "next/server";
import {
  executeWarehouseCheckoutEvent,
  type WarehouseCheckoutEvent,
  type WarehouseCheckoutEventType,
  type WarehouseCheckoutLine
} from "@/lib/warehouse-runtime";

export const runtime = "nodejs";

type StripeMetadata = Record<string, string | undefined>;

type StripeObject = {
  client_reference_id?: string | null;
  id?: string;
  metadata?: StripeMetadata | null;
  object?: string;
  payment_intent?: string | { id?: string } | null;
  payment_status?: string | null;
};

type StripeEvent = {
  data?: { object?: StripeObject };
  id?: string;
  type?: string;
};

type NormalizedStripeEvent =
  | { event: WarehouseCheckoutEvent }
  | { ignored: true; reason: string };

export async function POST(request: Request) {
  const rawBody = await request.text();
  const signature = request.headers.get("stripe-signature");
  const endpointSecret = process.env.STRIPE_WEBHOOK_SECRET;

  if (endpointSecret && !verifyStripeSignature(rawBody, signature, endpointSecret)) {
    return NextResponse.json({ ok: false, error: "invalid_stripe_signature" }, { status: 400 });
  }

  if (!endpointSecret && process.env.NODE_ENV === "production") {
    return NextResponse.json({ ok: false, error: "stripe_webhook_secret_required" }, { status: 500 });
  }

  const stripeEvent = parseStripeEvent(rawBody);
  if (!stripeEvent) {
    return NextResponse.json({ ok: false, error: "invalid_stripe_event" }, { status: 400 });
  }

  const normalized = normalizeStripeEvent(stripeEvent);
  if ("ignored" in normalized) {
    return NextResponse.json({ ok: true, ignored: true, reason: normalized.reason });
  }

  try {
    const result = await executeWarehouseCheckoutEvent(normalized.event);
    return NextResponse.json({
      ok: true,
      provider: "stripe",
      resource: "warehouse",
      stripeEventId: stripeEvent.id,
      stripeEventType: stripeEvent.type,
      ...result
    });
  } catch (error) {
    return NextResponse.json({
      ok: false,
      error: error instanceof Error ? error.message : String(error),
      provider: "stripe",
      resource: "warehouse"
    }, { status: 400 });
  }
}

function parseStripeEvent(rawBody: string): StripeEvent | null {
  try {
    const parsed = JSON.parse(rawBody) as StripeEvent;
    if (!parsed || typeof parsed.id !== "string" || typeof parsed.type !== "string") return null;
    return parsed;
  } catch {
    return null;
  }
}

function normalizeStripeEvent(stripeEvent: StripeEvent): NormalizedStripeEvent {
  const object = stripeEvent.data?.object;
  if (!object) return { ignored: true, reason: "missing_data_object" };

  const metadata = object.metadata ?? {};
  const checkoutSessionId = resolveCheckoutSessionId(stripeEvent, object, metadata);
  if (!checkoutSessionId) return { ignored: true, reason: "missing_checkout_session_id" };

  const eventType = mapStripeEventType(stripeEvent.type, object);
  if (!eventType) return { ignored: true, reason: `unsupported_stripe_event:${stripeEvent.type}` };

  return {
    event: {
      checkoutSessionId,
      eventId: stripeEvent.id ?? `${stripeEvent.type}:${checkoutSessionId}`,
      eventType,
      lines: parseWarehouseLines(metadata),
      orderId: metadata.order_id ?? metadata.orderId ?? object.client_reference_id ?? undefined,
      paymentIntentId: resolvePaymentIntentId(object, metadata),
      provider: "stripe"
    }
  };
}

function mapStripeEventType(stripeType: string | undefined, object: StripeObject): WarehouseCheckoutEventType | null {
  if (stripeType === "checkout.session.expired") return "checkout.expired";
  if (stripeType === "checkout.session.async_payment_failed") return "payment.failed";
  if (stripeType === "checkout.session.async_payment_succeeded") return "payment.succeeded";
  if (stripeType === "payment_intent.payment_failed" || stripeType === "payment_intent.canceled") return "payment.failed";
  if (stripeType === "payment_intent.succeeded") return "payment.succeeded";
  if (stripeType === "checkout.session.completed") {
    if (object.payment_status && object.payment_status !== "paid" && object.payment_status !== "no_payment_required") return null;
    return "payment.succeeded";
  }
  return null;
}

function resolveCheckoutSessionId(stripeEvent: StripeEvent, object: StripeObject, metadata: StripeMetadata) {
  if (metadata.checkout_session_id) return metadata.checkout_session_id;
  if (metadata.checkoutSessionId) return metadata.checkoutSessionId;
  if (object.object === "checkout.session" && object.id) return object.id;
  if (stripeEvent.type?.startsWith("checkout.session.") && object.id) return object.id;
  return null;
}

function resolvePaymentIntentId(object: StripeObject, metadata: StripeMetadata) {
  if (metadata.payment_intent_id) return metadata.payment_intent_id;
  if (metadata.paymentIntentId) return metadata.paymentIntentId;
  if (typeof object.payment_intent === "string") return object.payment_intent;
  return object.payment_intent?.id ?? undefined;
}

function parseWarehouseLines(metadata: StripeMetadata): WarehouseCheckoutLine[] | undefined {
  const encodedLines = metadata.warehouse_lines ?? metadata.warehouseLines;
  if (encodedLines) {
    try {
      const parsed = JSON.parse(encodedLines) as unknown;
      if (Array.isArray(parsed)) {
        const lines = parsed.map(parseWarehouseLine).filter((line): line is WarehouseCheckoutLine => Boolean(line));
        if (lines.length) return lines;
      }
    } catch {
      return undefined;
    }
  }

  const inventoryItemId = metadata.warehouse_item_id ?? metadata.inventoryItemId;
  const quantity = Number(metadata.warehouse_quantity ?? metadata.quantity);
  if (!inventoryItemId || !Number.isFinite(quantity) || quantity <= 0) return undefined;
  return [{
    inventoryItemId,
    inventoryOwnerPartyId: metadata.inventory_owner_party_id ?? metadata.inventoryOwnerPartyId,
    locationId: metadata.warehouse_location_id ?? metadata.locationId,
    lotId: metadata.lot_id ?? metadata.lotId ?? null,
    quantity,
    serialId: metadata.serial_id ?? metadata.serialId ?? null,
    sourceLineId: metadata.source_line_id ?? metadata.sourceLineId
  }];
}

function parseWarehouseLine(value: unknown): WarehouseCheckoutLine | null {
  if (!value || typeof value !== "object") return null;
  const line = value as Partial<WarehouseCheckoutLine>;
  const quantity = Number(line.quantity);
  if (typeof line.inventoryItemId !== "string" || !Number.isFinite(quantity) || quantity <= 0) return null;
  return {
    inventoryItemId: line.inventoryItemId,
    inventoryOwnerPartyId: typeof line.inventoryOwnerPartyId === "string" ? line.inventoryOwnerPartyId : undefined,
    locationId: typeof line.locationId === "string" ? line.locationId : undefined,
    lotId: typeof line.lotId === "string" ? line.lotId : null,
    quantity,
    serialId: typeof line.serialId === "string" ? line.serialId : null,
    sourceLineId: typeof line.sourceLineId === "string" ? line.sourceLineId : undefined
  };
}

function verifyStripeSignature(rawBody: string, signatureHeader: string | null, endpointSecret: string) {
  if (!signatureHeader) return false;
  const timestamp = signatureHeader
    .split(",")
    .find((part) => part.startsWith("t="))
    ?.slice(2);
  const signatures = signatureHeader
    .split(",")
    .filter((part) => part.startsWith("v1="))
    .map((part) => part.slice(3));
  if (!timestamp || !signatures.length) return false;

  const signedPayload = `${timestamp}.${rawBody}`;
  const expected = createHmac("sha256", endpointSecret).update(signedPayload).digest("hex");
  return signatures.some((signature) => safeEqualHex(signature, expected));
}

function safeEqualHex(actual: string, expected: string) {
  const actualBuffer = Buffer.from(actual, "hex");
  const expectedBuffer = Buffer.from(expected, "hex");
  if (actualBuffer.length !== expectedBuffer.length) return false;
  return timingSafeEqual(actualBuffer, expectedBuffer);
}
