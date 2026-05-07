import {
  createAccountingAuditEvent,
  createAccountingCommand,
  createAccountingProposal,
  createBusinessOutboxEvent
} from "@ctox-business/accounting";
import { decideAccountingProposal, saveAccountingWorkflowSnapshot, type AccountingJournalDraft } from "@ctox-business/db/accounting";
import { NextResponse } from "next/server";
import { getBusinessBundle } from "@/lib/business-seed";
import { getDatabaseBackedBusinessBundle } from "@/lib/business-db-bundle";

const companyId = "business-basic-company";
const bankAccountId = "acc-bank";

export const dynamic = "force-dynamic";

type ManualPostingRequest = {
  accountId?: string;
  recordId?: string;
};

export async function POST(request: Request) {
  if (!process.env.DATABASE_URL) {
    return NextResponse.json({ error: "DATABASE_URL not configured", persisted: false }, { status: 400 });
  }

  const body = await request.json().catch(() => ({})) as ManualPostingRequest;
  const recordId = body.recordId;
  const accountId = body.accountId;
  if (!recordId || !accountId) {
    return NextResponse.json({ error: "recordId_and_accountId_required", persisted: false }, { status: 400 });
  }

  const data = await getDatabaseBackedBusinessBundle(await getBusinessBundle());
  const transaction = data.bankTransactions.find((item) => item.id === recordId);
  const account = data.accounts.find((item) => item.id === accountId);

  if (!transaction) {
    return NextResponse.json({ error: "bank_transaction_not_found", persisted: false }, { status: 404 });
  }
  if (!account || !account.isPosting || account.id === bankAccountId) {
    return NextResponse.json({ error: "posting_account_not_allowed", persisted: false }, { status: 400 });
  }
  if (transaction.status === "Matched") {
    return NextResponse.json({ error: "bank_transaction_already_matched", persisted: false }, { status: 409 });
  }

  const amountMinor = Math.round(Math.abs(transaction.amount) * 100);
  if (amountMinor <= 0) {
    return NextResponse.json({ error: "bank_transaction_amount_must_not_be_zero", persisted: false }, { status: 400 });
  }

  const command = createAccountingCommand({
    companyId,
    payload: {
      accountId,
      amount: transaction.amount,
      bankTransactionId: transaction.id,
      matchType: "manual"
    },
    refId: transaction.id,
    refType: "bank_transaction",
    requestedBy: "business-user",
    type: "AcceptBankMatch"
  });
  const journalDraft = manualJournalDraft({
    accountId,
    amountMinor,
    bookingDate: transaction.bookingDate,
    currency: transaction.currency,
    purpose: transaction.purpose,
    recordId: transaction.id,
    transactionAmount: transaction.amount
  });
  const paymentProjection = {
    allocations: [],
    amountMinor,
    bankAccountExternalId: bankAccountId,
    bankStatementLineExternalId: transaction.id,
    companyId,
    currency: transaction.currency,
    externalId: `pay-${transaction.id}`,
    kind: transaction.amount >= 0 ? "incoming" : "outgoing",
    paymentDate: transaction.bookingDate,
    postedJournalEntryExternalId: journalExternalId(journalDraft)
  };
  const proposal = createAccountingProposal({
    companyId,
    confidence: 1,
    createdByAgent: "manual-reconciliation",
    evidence: {
      accountId,
      accountName: account.name,
      counterparty: transaction.counterparty,
      purpose: transaction.purpose
    },
    kind: "bank_match",
    proposedCommand: command,
    refId: transaction.id,
    refType: "bank_transaction",
    status: "open"
  });
  const audit = createAccountingAuditEvent({
    action: "bank_match.manual_post",
    actorId: "business-user",
    actorType: "user",
    after: { accountId, command, journalDraft, paymentProjection },
    companyId,
    refId: transaction.id,
    refType: "bank_transaction"
  });
  const outbox = createBusinessOutboxEvent({
    companyId,
    id: `outbox-business.bank_match.manual_post-${transaction.id}`,
    payload: { accountId, command, journalDraft, paymentProjection, proposalId: proposal.id },
    topic: "business.bank_match.manual_post"
  });

  await saveAccountingWorkflowSnapshot({ audit, journalDraft, outbox, payment: paymentProjection, proposal });
  const acceptedProposal = await decideAccountingProposal({
    actorId: "business-user",
    externalId: proposal.id,
    resultingJournalEntryId: journalExternalId(journalDraft),
    status: "accepted"
  });

  return NextResponse.json({
    persisted: true,
    proposal: acceptedProposal,
    recordId: transaction.id,
    resultingJournalEntryId: journalExternalId(journalDraft)
  });
}

function manualJournalDraft(input: {
  accountId: string;
  amountMinor: number;
  bookingDate: string;
  currency: string;
  purpose: string;
  recordId: string;
  transactionAmount: number;
}): AccountingJournalDraft {
  const debit = (minor: number) => ({ minor });
  const credit = (minor: number) => ({ minor });
  const zero = { minor: 0 };
  const lines = input.transactionAmount >= 0
    ? [
      { accountId: bankAccountId, credit: zero, debit: debit(input.amountMinor) },
      { accountId: input.accountId, credit: credit(input.amountMinor), debit: zero }
    ]
    : [
      { accountId: input.accountId, credit: zero, debit: debit(input.amountMinor) },
      { accountId: bankAccountId, credit: credit(input.amountMinor), debit: zero }
    ];

  return {
    companyId,
    lines,
    narration: `Manual bank posting ${input.recordId}: ${input.purpose}`,
    postingDate: input.bookingDate,
    refId: input.recordId,
    refType: "bank_transaction",
    type: "payment"
  };
}

function journalExternalId(journal: AccountingJournalDraft) {
  return `je-${journal.type}-${journal.refType}-${journal.refId}`;
}
