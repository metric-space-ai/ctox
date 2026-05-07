import { addMoney, isZeroMoney, moneyFromMajor, zeroMoney, type CurrencyCode, type Money } from "../money";

export type PostingLine = {
  accountId: string;
  credit: Money;
  debit: Money;
  partyId?: string;
  taxCode?: string;
  costCenterId?: string;
  projectId?: string;
};

export type JournalDraft = {
  companyId: string;
  currency: CurrencyCode;
  lines: PostingLine[];
  narration?: string;
  postingDate: string;
  refId: string;
  refType: string;
  type: "invoice" | "payment" | "receipt" | "manual" | "fx" | "depreciation" | "reverse";
};

export class LedgerPosting {
  private readonly lines: PostingLine[] = [];

  constructor(
    readonly companyId: string,
    readonly refType: string,
    readonly refId: string,
    readonly postingDate: string,
    readonly currency: CurrencyCode = "EUR"
  ) {}

  debit(accountId: string, amount: Money | number, partyId?: string, metadata: Omit<PostingLine, "accountId" | "credit" | "debit" | "partyId"> = {}) {
    const debit = typeof amount === "number" ? moneyFromMajor(amount, this.currency) : amount;
    this.lines.push({ ...metadata, accountId, credit: zeroMoney(debit.currency, debit.scale), debit, partyId });
    return this;
  }

  credit(accountId: string, amount: Money | number, partyId?: string, metadata: Omit<PostingLine, "accountId" | "credit" | "debit" | "partyId"> = {}) {
    const credit = typeof amount === "number" ? moneyFromMajor(amount, this.currency) : amount;
    this.lines.push({ ...metadata, accountId, credit, debit: zeroMoney(credit.currency, credit.scale), partyId });
    return this;
  }

  validate() {
    validatePostingLines(this.lines);
  }

  toJournalDraft(type: JournalDraft["type"], narration?: string): JournalDraft {
    this.validate();
    return {
      companyId: this.companyId,
      currency: this.currency,
      lines: this.lines.map((line) => ({ ...line })),
      narration,
      postingDate: this.postingDate,
      refId: this.refId,
      refType: this.refType,
      type
    };
  }
}

export function validatePostingLines(lines: PostingLine[]) {
  if (lines.length < 2) throw new Error("posting_requires_at_least_two_lines");

  const first = lines[0]?.debit ?? lines[0]?.credit;
  let debit = zeroMoney(first.currency, first.scale);
  let credit = zeroMoney(first.currency, first.scale);

  for (const line of lines) {
    const hasDebit = !isZeroMoney(line.debit);
    const hasCredit = !isZeroMoney(line.credit);
    if (hasDebit === hasCredit) throw new Error("posting_line_requires_exactly_one_side");
    debit = addMoney(debit, line.debit);
    credit = addMoney(credit, line.credit);
  }

  if (debit.minor !== credit.minor) {
    throw new Error("posting_debit_credit_mismatch");
  }
}
