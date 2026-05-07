import { buildReverseJournalDraft, createAccountingAuditEvent, createBusinessOutboxEvent, moneyFromMajor } from "@ctox-business/accounting";
import { NextResponse } from "next/server";
import { getDatabaseBackedBusinessBundle } from "@/lib/business-db-bundle";
import { getBusinessBundle } from "@/lib/business-seed";

const companyId = "business-basic-company";

export async function POST(request: Request) {
  const body = await parseJsonBody(request);
  const journalEntryId = typeof body?.journalEntryId === "string" ? body.journalEntryId : null;
  const reason = typeof body?.reason === "string" && body.reason.trim() ? body.reason.trim() : "GoBD correction requested.";
  const postingDate = typeof body?.postingDate === "string" ? body.postingDate : new Date().toISOString().slice(0, 10);

  if (!journalEntryId) {
    return NextResponse.json({ error: "journal_entry_id_required", persisted: false }, { status: 400 });
  }

  const data = await getDatabaseBackedBusinessBundle(await getBusinessBundle());
  const original = data.journalEntries.find((entry) => entry.id === journalEntryId || entry.number === journalEntryId);
  if (!original) {
    return NextResponse.json({ error: "journal_entry_not_found", persisted: false }, { status: 404 });
  }
  if (original.status !== "Posted") {
    return NextResponse.json({ error: "journal_entry_not_posted", persisted: false }, { status: 409 });
  }

  const reverseDraft = buildReverseJournalDraft({
    companyId,
    currency: "EUR",
    lines: original.lines.map((line) => ({
      accountId: line.accountId,
      credit: moneyFromMajor(line.credit, "EUR"),
      debit: moneyFromMajor(line.debit, "EUR"),
      partyId: line.partyId,
      taxCode: line.taxCode
    })),
    narration: typeof original.narration === "string" ? original.narration : original.narration.en,
    postingDate: original.postingDate,
    refId: original.refId,
    refType: original.refType,
    type: original.type
  }, {
    narration: `${reason} Reverses ${original.number}.`,
    postingDate,
    refId: `${original.id}-storno`
  });

  const audit = createAccountingAuditEvent({
    action: "journal.reverse.prepare",
    actorId: "business-runtime",
    actorType: "system",
    after: {
      originalJournalEntryId: original.id,
      reason,
      reverseDraft
    },
    companyId,
    refId: original.id,
    refType: "journal_entry"
  });
  const outbox = createBusinessOutboxEvent({
    companyId,
    id: `outbox-business.journal.reverse-${original.id}`,
    payload: {
      originalJournalEntryId: original.id,
      reason,
      reverseDraft
    },
    topic: "business.journal.reverse"
  });

  return NextResponse.json({
    audit,
    original: {
      id: original.id,
      number: original.number,
      refId: original.refId,
      refType: original.refType
    },
    outbox,
    persisted: false,
    reason: "Prepared only. Persist through the accounting workflow acceptance path before posting.",
    reverseDraft
  });
}

async function parseJsonBody(request: Request) {
  try {
    return await request.json() as { journalEntryId?: unknown; postingDate?: unknown; reason?: unknown };
  } catch {
    return null;
  }
}
