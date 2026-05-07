export type FiscalPeriod = {
  closedAt?: string;
  endDate: string;
  id: string;
  startDate: string;
  status: "closed" | "open";
};

export function assertPeriodOpen(periods: FiscalPeriod[], postingDate: string) {
  const date = postingDate.slice(0, 10);
  const matchingPeriods = periods.filter((item) => item.startDate <= date && item.endDate >= date);
  if (!matchingPeriods.length) throw new Error("fiscal_period_missing");
  if (matchingPeriods.some((item) => item.status === "closed")) throw new Error("fiscal_period_closed");
  return matchingPeriods
    .sort((left, right) => periodLengthDays(left) - periodLengthDays(right))[0]!;
}

export function closeFiscalPeriod(period: FiscalPeriod, closedAt = new Date().toISOString()): FiscalPeriod {
  if (period.status === "closed") return period;
  return { ...period, closedAt, status: "closed" };
}

function periodLengthDays(period: FiscalPeriod) {
  const start = new Date(`${period.startDate}T00:00:00.000Z`).getTime();
  const end = new Date(`${period.endDate}T00:00:00.000Z`).getTime();
  return Math.round((end - start) / 86_400_000);
}
