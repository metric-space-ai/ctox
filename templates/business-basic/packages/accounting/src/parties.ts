export type PartyKind = "customer" | "employee" | "vendor";

export type Party = {
  defaultPayableAccountId?: string;
  defaultReceivableAccountId?: string;
  id: string;
  kind: PartyKind;
  name: string;
  taxId?: string;
  vatId?: string;
};

export function createParty(input: Party) {
  if (!input.id) throw new Error("party_id_required");
  if (!input.name) throw new Error("party_name_required");
  if (input.kind === "customer" && !input.defaultReceivableAccountId) throw new Error("customer_receivable_account_required");
  if (input.kind === "vendor" && !input.defaultPayableAccountId) throw new Error("vendor_payable_account_required");
  return input;
}
