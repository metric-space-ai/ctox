export type BusinessOutboxEvent = {
  companyId: string;
  createdAt: string;
  id: string;
  payload: Record<string, unknown>;
  status: "delivered" | "failed" | "pending";
  topic: string;
};

export function createBusinessOutboxEvent(input: Omit<BusinessOutboxEvent, "createdAt" | "id" | "status"> & {
  createdAt?: string;
  id?: string;
  status?: BusinessOutboxEvent["status"];
}): BusinessOutboxEvent {
  return {
    ...input,
    createdAt: input.createdAt ?? new Date().toISOString(),
    id: input.id ?? `outbox-${input.topic}-${cryptoSafeId()}`,
    status: input.status ?? "pending"
  };
}

function cryptoSafeId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) return crypto.randomUUID();
  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
