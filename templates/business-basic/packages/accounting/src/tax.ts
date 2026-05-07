export type TaxRate = {
  accountId?: string;
  code: "DE_0" | "DE_19" | "DE_7" | "DE_KU" | "DE_RC" | (string & {});
  rate: number;
  type: "input" | "kleinunternehmer" | "output" | "reverse_charge";
};

export const germanTaxRates: TaxRate[] = [
  { accountId: "acc-vat-output", code: "DE_19", rate: 19, type: "output" },
  { accountId: "acc-vat-output", code: "DE_7", rate: 7, type: "output" },
  { code: "DE_0", rate: 0, type: "output" },
  { code: "DE_RC", rate: 0, type: "reverse_charge" },
  { code: "DE_KU", rate: 0, type: "kleinunternehmer" }
];

export function resolveGermanTaxRate(input: { kleinunternehmer?: boolean; reverseCharge?: boolean; taxRate: number }) {
  if (input.kleinunternehmer) return germanTaxRates.find((rate) => rate.code === "DE_KU")!;
  if (input.reverseCharge) return germanTaxRates.find((rate) => rate.code === "DE_RC")!;
  return germanTaxRates.find((rate) => rate.rate === input.taxRate && rate.type === "output") ?? germanTaxRates[0]!;
}
