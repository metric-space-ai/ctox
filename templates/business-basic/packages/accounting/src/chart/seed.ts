import { skr03Chart } from "./skr03";
import { skr04Chart } from "./skr04";
import type { ChartAccount, ChartOfAccounts } from "./types";

export function selectChart(id: ChartOfAccounts["id"] = "skr03") {
  return id === "skr04" ? skr04Chart : skr03Chart;
}

export function seedChartAccounts(input: { chart?: ChartOfAccounts["id"]; companyId: string }) {
  const chart = selectChart(input.chart);
  return chart.accounts.map((account) => ({
    ...account,
    companyId: input.companyId,
    externalId: account.id
  }));
}

export function findAccountByCode(accounts: ChartAccount[], code: string) {
  return accounts.find((account) => account.code === code);
}
