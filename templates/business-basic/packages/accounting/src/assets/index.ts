import { moneyFromMajor, moneyToMajor, type CurrencyCode } from "../money";
import { LedgerPosting, type JournalDraft } from "../posting/service";

export type FixedAsset = {
  accumulatedDepreciationAccountId: string;
  acquisitionAccountId: string;
  acquisitionCost: number;
  acquisitionDate: string;
  assetAccountId: string;
  currency: CurrencyCode;
  depreciationExpenseAccountId: string;
  id: string;
  name: string;
  receiptId?: string;
  salvageValue: number;
  usefulLifeMonths: number;
};

export type DepreciationScheduleLine = {
  accumulatedDepreciation: number;
  amount: number;
  bookValue: number;
  fiscalYear: number;
  journalEntryId?: string;
  postingDate: string;
};

export function buildStraightLineDepreciationSchedule(asset: FixedAsset): DepreciationScheduleLine[] {
  if (asset.usefulLifeMonths <= 0) throw new Error("asset_useful_life_required");
  const depreciableBase = round(Math.max(0, asset.acquisitionCost - asset.salvageValue));
  const monthly = round(depreciableBase / asset.usefulLifeMonths);
  const lines: DepreciationScheduleLine[] = [];
  let accumulated = 0;

  for (let month = 1; month <= asset.usefulLifeMonths; month += 1) {
    const date = addMonths(asset.acquisitionDate, month);
    const isLast = month === asset.usefulLifeMonths;
    const amount = isLast ? round(depreciableBase - accumulated) : monthly;
    accumulated = round(accumulated + amount);
    lines.push({
      accumulatedDepreciation: accumulated,
      amount,
      bookValue: round(asset.acquisitionCost - accumulated),
      fiscalYear: Number(date.slice(0, 4)),
      postingDate: date
    });
  }

  return lines;
}

export function buildAssetAcquisitionJournalDraft(input: {
  asset: FixedAsset;
  companyId: string;
  inputVatAccountId?: string;
  inputVatAmount?: number;
  payableAccountId: string;
}): JournalDraft {
  const posting = new LedgerPosting(input.companyId, "asset", input.asset.id, input.asset.acquisitionDate, input.asset.currency)
    .debit(input.asset.assetAccountId, moneyFromMajor(input.asset.acquisitionCost, input.asset.currency));

  if (input.inputVatAmount && input.inputVatAmount > 0) {
    if (!input.inputVatAccountId) throw new Error("asset_input_vat_account_required");
    posting.debit(input.inputVatAccountId, moneyFromMajor(input.inputVatAmount, input.asset.currency), undefined, { taxCode: "DE_19_INPUT" });
  }

  return posting
    .credit(input.payableAccountId, moneyFromMajor(input.asset.acquisitionCost + (input.inputVatAmount ?? 0), input.asset.currency))
    .toJournalDraft("manual", `Asset acquisition ${input.asset.name}`);
}

export function buildAssetDepreciationJournalDraft(input: {
  asset: FixedAsset;
  companyId: string;
  line: DepreciationScheduleLine;
}): JournalDraft {
  return new LedgerPosting(input.companyId, "asset", input.asset.id, input.line.postingDate, input.asset.currency)
    .debit(input.asset.depreciationExpenseAccountId, moneyFromMajor(input.line.amount, input.asset.currency))
    .credit(input.asset.accumulatedDepreciationAccountId, moneyFromMajor(input.line.amount, input.asset.currency))
    .toJournalDraft("depreciation", `Depreciation ${input.asset.name}`);
}

export function buildAssetDisposalJournalDraft(input: {
  accumulatedDepreciation: number;
  disposalDate: string;
  gainAccountId: string;
  lossAccountId: string;
  proceeds: number;
  proceedsAccountId: string;
  asset: FixedAsset;
  companyId: string;
}): JournalDraft {
  const accumulatedDepreciation = round(Math.max(0, input.accumulatedDepreciation));
  const proceeds = round(Math.max(0, input.proceeds));
  const bookValue = round(input.asset.acquisitionCost - accumulatedDepreciation);
  const gain = round(Math.max(0, proceeds - bookValue));
  const loss = round(Math.max(0, bookValue - proceeds));
  const posting = new LedgerPosting(input.companyId, "asset", `${input.asset.id}-disposal`, input.disposalDate, input.asset.currency)
    .credit(input.asset.assetAccountId, moneyFromMajor(input.asset.acquisitionCost, input.asset.currency));

  if (accumulatedDepreciation > 0) {
    posting.debit(input.asset.accumulatedDepreciationAccountId, moneyFromMajor(accumulatedDepreciation, input.asset.currency));
  }
  if (proceeds > 0) {
    posting.debit(input.proceedsAccountId, moneyFromMajor(proceeds, input.asset.currency));
  }

  if (loss > 0) {
    posting.debit(input.lossAccountId, moneyFromMajor(loss, input.asset.currency));
  }
  if (gain > 0) {
    posting.credit(input.gainAccountId, moneyFromMajor(gain, input.asset.currency));
  }

  return posting.toJournalDraft("manual", `Asset disposal ${input.asset.name}`);
}

export function netBookValueFromEntries(input: {
  accumulatedDepreciationAccountId: string;
  assetAccountId: string;
  entries: JournalDraft[];
}) {
  const lines = input.entries.flatMap((entry) => entry.lines);
  const acquisition = lines
    .filter((line) => line.accountId === input.assetAccountId)
    .reduce((sum, line) => sum + moneyToMajor(line.debit) - moneyToMajor(line.credit), 0);
  const accumulated = lines
    .filter((line) => line.accountId === input.accumulatedDepreciationAccountId)
    .reduce((sum, line) => sum + moneyToMajor(line.credit) - moneyToMajor(line.debit), 0);
  return round(acquisition - accumulated);
}

function addMonths(dateValue: string, months: number) {
  const date = new Date(`${dateValue}T00:00:00.000Z`);
  date.setUTCMonth(date.getUTCMonth() + months);
  return date.toISOString().slice(0, 10);
}

function round(value: number) {
  return Math.round((value + Number.EPSILON) * 100) / 100;
}
