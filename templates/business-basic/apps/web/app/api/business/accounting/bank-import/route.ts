import { createHash } from "node:crypto";
import { NextResponse } from "next/server";
import {
  createAccountingAuditEvent,
  createAccountingCommand,
  createBusinessOutboxEvent,
  findDuplicateBankLines,
  parseBankCsv,
  parseCamt053,
  parseMt940
} from "@ctox-business/accounting";
import { saveAccountingWorkflowSnapshot } from "@ctox-business/db/accounting";
import { getBusinessBundle } from "@/lib/business-seed";
import { getDatabaseBackedBusinessBundle } from "@/lib/business-db-bundle";
import { prepareBankMatchForAccounting } from "@/lib/business-accounting";

type BankImportRequest = {
  content?: string;
  format?: "camt053" | "csv" | "mt940";
  sourceFilename?: string;
};

const sampleCsv = [
  "booking_date;value_date;amount;currency;remitter_name;remitter_iban;purpose;end_to_end_ref",
  "2026-05-07;2026-05-07;297,50;EUR;Kunstmen GmbH;DE123;RE-2026-001;E2E-1",
  "2026-05-08;2026-05-08;-39,22;EUR;Stripe Payments Europe;IE123;Processing fees April;E2E-2"
].join("\n");

const companyId = "business-basic-company";

export async function POST(request: Request) {
  const body = await request.json().catch(() => ({})) as BankImportRequest;
  const format = body.format ?? "csv";
  const content = body.content ?? sampleCsv;

  try {
    const statement = format === "mt940"
      ? parseMt940(content, body.sourceFilename)
      : format === "camt053"
        ? parseCamt053(content, body.sourceFilename)
        : parseBankCsv(content, { sourceFilename: body.sourceFilename });
    const duplicateResults = findDuplicateBankLines(statement.lines);
    const sourceSha256 = sha256(content);
    const sourceFilename = body.sourceFilename ?? statement.sourceFilename ?? `bank-import.${format}`;
    const statementExternalId = `bank-statement-${format}-${sourceSha256.slice(0, 16)}`;
    const bankStatement = {
      accountExternalId: "acc-bank",
      closingBalanceMinor: toMinor(statement.closingBalance ?? statement.lines.reduce((sum, line) => sum + line.amount, 0)),
      companyId,
      currency: statement.currency,
      endDate: statement.endDate ?? statement.lines.at(-1)?.bookingDate ?? null,
      externalId: statementExternalId,
      format,
      importedBy: "business-runtime",
      lines: duplicateResults.map((result) => ({
        amountMinor: toMinor(result.line.amount),
        bookingDate: result.line.bookingDate,
        currency: result.line.currency,
        duplicateOfLineExternalId: result.duplicateOf ? bankLineExternalId(statementExternalId, result.duplicateOf.lineNo) : null,
        endToEndRef: result.line.endToEndRef ?? null,
        externalId: bankLineExternalId(statementExternalId, result.line.lineNo),
        lineNo: result.line.lineNo,
        matchStatus: result.duplicateOf ? "ignored" : inferredMatchStatus(result.line.purpose),
        matchedJournalEntryExternalId: null,
        purpose: result.line.purpose ?? null,
        remitterIban: result.line.remitterIban ?? null,
        remitterName: result.line.remitterName ?? null,
        valueDate: result.line.valueDate ?? null
      })),
      openingBalanceMinor: toMinor(statement.openingBalance ?? 0),
      sourceFilename,
      sourceSha256,
      startDate: statement.startDate ?? statement.lines[0]?.bookingDate ?? null
    };
    const command = createAccountingCommand({
      companyId,
      payload: {
        duplicateCount: duplicateResults.filter((item) => item.duplicateOf).length,
        format,
        lineCount: statement.lines.length,
        sourceFilename,
        sourceSha256,
        statementId: statementExternalId
      },
      refId: statementExternalId,
      refType: "bank_statement",
      requestedBy: "business-runtime",
      type: "ImportBankStatement"
    });
    const audit = createAccountingAuditEvent({
      action: "bank_statement.prepare_import",
      actorId: "business-runtime",
      actorType: "system",
      after: { command, statement: { externalId: statementExternalId, lineCount: statement.lines.length } },
      companyId,
      refId: statementExternalId,
      refType: "bank_statement"
    });
    const outbox = createBusinessOutboxEvent({
      companyId,
      id: `outbox-business.bank_statement.prepare_import-${statementExternalId}`,
      payload: { command, statement: bankStatement },
      topic: "business.bank_statement.prepare_import"
    });
    const matchSnapshot = await buildMatchedLineSnapshot(statementExternalId, statement.lines);

    if (process.env.DATABASE_URL) {
      await saveAccountingWorkflowSnapshot({ audit, bankStatement, outbox });
      if (matchSnapshot) {
        await saveAccountingWorkflowSnapshot({
          audit: matchSnapshot.audit,
          journalDraft: matchSnapshot.journalDraft,
          outbox: matchSnapshot.outbox,
          payment: matchSnapshot.paymentProjection,
          proposal: matchSnapshot.proposal
        });
      }
    }

    return NextResponse.json({
      duplicateCount: duplicateResults.filter((item) => item.duplicateOf).length,
      duplicates: duplicateResults,
      persisted: Boolean(process.env.DATABASE_URL),
      workflow: {
        audit,
        matchedProposal: matchSnapshot?.proposal,
        outbox,
        statementExternalId
      },
      statement
    });
  } catch (error) {
    return NextResponse.json({
      error: error instanceof Error ? error.message : String(error)
    }, { status: 400 });
  }
}

async function buildMatchedLineSnapshot(statementExternalId: string, lines: BankStatementLines) {
  const data = await getDatabaseBackedBusinessBundle(await getBusinessBundle());
  const line = lines.find((item) => data.invoices.some((invoice) => item.purpose?.includes(invoice.number)));
  if (!line) return null;

  const invoice = data.invoices.find((item) => line.purpose?.includes(item.number));
  return prepareBankMatchForAccounting({
    transaction: {
      amount: line.amount,
      bookingDate: line.bookingDate,
      confidence: invoice ? 0.92 : 0.45,
      counterparty: line.remitterName ?? "Unknown",
      currency: line.currency === "USD" ? "USD" : "EUR",
      id: bankLineExternalId(statementExternalId, line.lineNo),
      matchedRecordId: invoice?.id,
      matchType: invoice ? "invoice" : "manual",
      purpose: line.purpose ?? "",
      status: invoice ? "Suggested" : "Unmatched",
      valueDate: line.valueDate ?? line.bookingDate
    }
  });
}

type BankStatementLines = ReturnType<typeof parseBankCsv>["lines"];

function bankLineExternalId(statementExternalId: string, lineNo: number) {
  return `${statementExternalId}-line-${lineNo}`;
}

function inferredMatchStatus(purpose?: string) {
  return purpose && /(?:RE|INV|RG)-?\d{4}/i.test(purpose) ? "suggested" : "unmatched";
}

function sha256(value: string) {
  return createHash("sha256").update(value).digest("hex");
}

function toMinor(amount: number) {
  return Math.round((amount + Number.EPSILON) * 100);
}
