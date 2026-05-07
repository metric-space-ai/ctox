import { NextResponse } from "next/server";
import { findDuplicateBankLines, parseBankCsv, parseCamt053, parseMt940 } from "@ctox-business/accounting/bank-import";

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

    return NextResponse.json({
      duplicateCount: duplicateResults.filter((item) => item.duplicateOf).length,
      duplicates: duplicateResults,
      statement
    });
  } catch (error) {
    return NextResponse.json({
      error: error instanceof Error ? error.message : String(error)
    }, { status: 400 });
  }
}
