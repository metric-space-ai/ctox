export type FiscalPeriod = {
  closedAt?: string;
  endDate: string;
  id: string;
  startDate: string;
  status: "closed" | "open";
};

export function assertPeriodOpen(periods: FiscalPeriod[], postingDate: string) {
  const date = postingDate.slice(0, 10);
  const period = periods.find((item) => item.startDate <= date && item.endDate >= date);
  if (!period) throw new Error("fiscal_period_missing");
  if (period.status === "closed") throw new Error("fiscal_period_closed");
  return period;
}

export function closeFiscalPeriod(period: FiscalPeriod, closedAt = new Date().toISOString()): FiscalPeriod {
  if (period.status === "closed") return period;
  return { ...period, closedAt, status: "closed" };
}
