export type TaxRate = {
  accountId?: string;
  code: "DE_0" | "DE_19" | "DE_7" | "DE_KU" | "DE_RC" | (string & {});
  rate: number;
  type: "input" | "kleinunternehmer" | "output" | "reverse_charge";
};

export type GermanTaxChart = "skr03" | "skr04";

export const germanTaxRates: TaxRate[] = [
  { accountId: "acc-vat-output", code: "DE_19", rate: 19, type: "output" },
  { accountId: "acc-vat-output-7", code: "DE_7", rate: 7, type: "output" },
  { accountId: "acc-vat-input", code: "DE_19_INPUT", rate: 19, type: "input" },
  { accountId: "acc-vat-input-7", code: "DE_7_INPUT", rate: 7, type: "input" },
  { code: "DE_0", rate: 0, type: "output" },
  { code: "DE_RC", rate: 0, type: "reverse_charge" },
  { code: "DE_KU", rate: 0, type: "kleinunternehmer" }
];

export function resolveGermanTaxRate(input: { kleinunternehmer?: boolean; reverseCharge?: boolean; taxRate: number }) {
  if (input.kleinunternehmer) return germanTaxRates.find((rate) => rate.code === "DE_KU")!;
  if (input.reverseCharge) return germanTaxRates.find((rate) => rate.code === "DE_RC")!;
  return germanTaxRates.find((rate) => rate.rate === input.taxRate && rate.type === "output") ?? germanTaxRates[0]!;
}

export function germanInputVatAccountId(taxCode?: string) {
  if (taxCode === "DE_7_INPUT") return "acc-vat-input-7";
  return "acc-vat-input";
}

export function germanTaxRatesForChart(chart: GermanTaxChart = "skr03") {
  const accountCodeById = chart === "skr04" ? skr04TaxAccountCodes : skr03TaxAccountCodes;
  return germanTaxRates.map((rate) => ({
    ...rate,
    accountCode: rate.accountId ? accountCodeById[rate.accountId] : undefined,
    chart
  }));
}

const skr03TaxAccountCodes: Record<string, string> = {
  "acc-vat-input": "1576",
  "acc-vat-input-7": "1571",
  "acc-vat-output": "1776",
  "acc-vat-output-7": "1771"
};

const skr04TaxAccountCodes: Record<string, string> = {
  "acc-vat-input": "1406",
  "acc-vat-input-7": "1401",
  "acc-vat-output": "3806",
  "acc-vat-output-7": "3801"
};
