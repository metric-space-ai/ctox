import type { ChartOfAccounts } from "./types";

export const skr03Chart: ChartOfAccounts = {
  id: "skr03",
  label: "SKR03 Basis",
  accounts: [
    { id: "acc-bank", code: "1200", name: "Bank", rootType: "asset", accountType: "bank", currency: "EUR" },
    { id: "acc-ar", code: "1400", name: "Forderungen aus Lieferungen und Leistungen", rootType: "asset", accountType: "receivable", currency: "EUR" },
    { id: "acc-vat-input", code: "1576", name: "Abziehbare Vorsteuer 19%", rootType: "asset", accountType: "tax", currency: "EUR" },
    { id: "acc-ap", code: "1600", name: "Verbindlichkeiten aus Lieferungen und Leistungen", rootType: "liability", accountType: "payable", currency: "EUR" },
    { id: "acc-vat-output", code: "1776", name: "Umsatzsteuer 19%", rootType: "liability", accountType: "tax", currency: "EUR" },
    { id: "acc-equity", code: "0800", name: "Gezeichnetes Kapital", rootType: "equity", accountType: "temporary", currency: "EUR" },
    { id: "acc-contractor", code: "3125", name: "Fremdleistungen", rootType: "expense", accountType: "expense", currency: "EUR" },
    { id: "acc-software", code: "4920", name: "Software und Cloud", rootType: "expense", accountType: "expense", currency: "EUR" },
    { id: "acc-fees", code: "4970", name: "Nebenkosten des Geldverkehrs", rootType: "expense", accountType: "expense", currency: "EUR" },
    { id: "acc-revenue-implementation", code: "8337", name: "Implementation services", rootType: "income", accountType: "income", currency: "EUR" },
    { id: "acc-revenue-research", code: "8338", name: "Research services", rootType: "income", accountType: "income", currency: "EUR" },
    { id: "acc-revenue-saas", code: "8400", name: "SaaS subscriptions", rootType: "income", accountType: "income", currency: "EUR" },
    { id: "acc-revenue-support", code: "8401", name: "Support subscriptions", rootType: "income", accountType: "income", currency: "EUR" }
  ]
};
