import type { ChartOfAccounts } from "./types";

export const skr04Chart: ChartOfAccounts = {
  id: "skr04",
  label: "SKR04 Basis",
  accounts: [
    { id: "acc-bank", code: "1800", name: "Bank", rootType: "asset", accountType: "bank", currency: "EUR" },
    { id: "acc-ar", code: "1200", name: "Forderungen aus Lieferungen und Leistungen", rootType: "asset", accountType: "receivable", currency: "EUR" },
    { id: "acc-vat-input", code: "1406", name: "Abziehbare Vorsteuer 19%", rootType: "asset", accountType: "tax", currency: "EUR" },
    { id: "acc-ap", code: "3300", name: "Verbindlichkeiten aus Lieferungen und Leistungen", rootType: "liability", accountType: "payable", currency: "EUR" },
    { id: "acc-vat-output", code: "3806", name: "Umsatzsteuer 19%", rootType: "liability", accountType: "tax", currency: "EUR" },
    { id: "acc-equity", code: "2900", name: "Gezeichnetes Kapital", rootType: "equity", accountType: "temporary", currency: "EUR" },
    { id: "acc-contractor", code: "5900", name: "Fremdleistungen", rootType: "expense", accountType: "expense", currency: "EUR" },
    { id: "acc-software", code: "6815", name: "Software und Cloud", rootType: "expense", accountType: "expense", currency: "EUR" },
    { id: "acc-fees", code: "6855", name: "Nebenkosten des Geldverkehrs", rootType: "expense", accountType: "expense", currency: "EUR" },
    { id: "acc-revenue-implementation", code: "4337", name: "Implementation services", rootType: "income", accountType: "income", currency: "EUR" },
    { id: "acc-revenue-research", code: "4338", name: "Research services", rootType: "income", accountType: "income", currency: "EUR" },
    { id: "acc-revenue-saas", code: "4400", name: "SaaS subscriptions", rootType: "income", accountType: "income", currency: "EUR" },
    { id: "acc-revenue-support", code: "4401", name: "Support subscriptions", rootType: "income", accountType: "income", currency: "EUR" }
  ]
};
