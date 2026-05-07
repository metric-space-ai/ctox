export type NumberSeriesState = {
  fiscalYear: number;
  key: "invoice" | "receipt" | "credit_note" | "journal" | "dunning" | (string & {});
  nextValue: number;
  prefix: string;
};

export type NumberSeriesAllocation = {
  number: string;
  state: NumberSeriesState;
};

export function allocateNumber(state: NumberSeriesState, width = 4): NumberSeriesAllocation {
  if (!Number.isInteger(state.nextValue) || state.nextValue < 1) throw new Error("number_series_next_value_invalid");
  const number = `${state.prefix}${String(state.nextValue).padStart(width, "0")}`;
  return {
    number,
    state: {
      ...state,
      nextValue: state.nextValue + 1
    }
  };
}

export function fiscalYearFromDate(date: string | Date) {
  const value = typeof date === "string" ? new Date(`${date.slice(0, 10)}T00:00:00.000Z`) : date;
  if (Number.isNaN(value.getTime())) throw new Error("fiscal_date_invalid");
  return value.getUTCFullYear();
}

export function createSeriesState(input: {
  date: string | Date;
  key: NumberSeriesState["key"];
  nextValue?: number;
  prefix?: string;
}): NumberSeriesState {
  const fiscalYear = fiscalYearFromDate(input.date);
  return {
    fiscalYear,
    key: input.key,
    nextValue: input.nextValue ?? 1,
    prefix: input.prefix ?? defaultPrefix(input.key, fiscalYear)
  };
}

function defaultPrefix(key: NumberSeriesState["key"], fiscalYear: number) {
  if (key === "invoice") return `RE-${fiscalYear}-`;
  if (key === "receipt") return `EB-${fiscalYear}-`;
  if (key === "credit_note") return `GS-${fiscalYear}-`;
  if (key === "journal") return `B-${fiscalYear}-`;
  if (key === "dunning") return `MA-${fiscalYear}-`;
  return `${String(key).toUpperCase()}-${fiscalYear}-`;
}
