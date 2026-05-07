import type { BankStatement } from "./types";

export function parseCamt053(input: string, sourceFilename?: string): BankStatement {
  const entryBlocks = blocks(input, "Ntry");
  const lines = entryBlocks.map((block, index) => {
    const amountText = text(block, "Amt") || "0";
    const creditDebit = text(block, "CdtDbtInd");
    const currency = attr(block, "Amt", "Ccy") || "EUR";
    const bookingDateBlock = blocks(block, "BookgDt")[0] ?? "";
    const valueDateBlock = blocks(block, "ValDt")[0] ?? "";
    const bookingDate = text(bookingDateBlock, "Dt") || text(bookingDateBlock, "DtTm").slice(0, 10) || "";
    const valueDate = text(valueDateBlock, "Dt") || text(valueDateBlock, "DtTm").slice(0, 10) || undefined;
    return {
      amount: (creditDebit === "DBIT" ? -1 : 1) * Number.parseFloat(amountText),
      bookingDate,
      currency,
      endToEndRef: text(block, "EndToEndId") || undefined,
      lineNo: index + 1,
      purpose: text(block, "Ustrd") || undefined,
      remitterIban: text(block, "IBAN") || undefined,
      remitterName: text(block, "Nm") || undefined,
      valueDate
    };
  });

  return {
    currency: lines[0]?.currency ?? "EUR",
    format: "camt053",
    lines,
    sourceFilename,
    startDate: lines[0]?.bookingDate,
    endDate: lines.at(-1)?.bookingDate
  };
}

function blocks(input: string, tag: string) {
  const tagPattern = localTag(tag);
  return [...input.matchAll(new RegExp(`<${tagPattern}\\b[\\s\\S]*?<\\/${tagPattern}>`, "gi"))].map((match) => match[0]);
}

function text(block: string, tag: string) {
  const tagPattern = localTag(tag);
  const match = new RegExp(`<${tagPattern}\\b[^>]*>([\\s\\S]*?)<\\/${tagPattern}>`, "i").exec(block);
  return match?.[1]?.replace(/<[^>]+>/g, "").trim() ?? "";
}

function attr(block: string, tag: string, name: string) {
  const tagPattern = localTag(tag);
  const match = new RegExp(`<${tagPattern}\\b[^>]*\\s${name}="([^"]+)"`, "i").exec(block);
  return match?.[1] ?? "";
}

function localTag(tag: string) {
  return `(?:[A-Za-z_][\\w.-]*:)?${tag}`;
}
