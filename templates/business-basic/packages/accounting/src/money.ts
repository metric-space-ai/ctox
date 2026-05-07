export type CurrencyCode = "EUR" | "USD" | (string & {});

export type Money = {
  currency: CurrencyCode;
  minor: number;
  scale: number;
};

export function moneyFromMajor(amount: number, currency: CurrencyCode = "EUR", scale = 2): Money {
  if (!Number.isFinite(amount)) throw new Error("money_amount_must_be_finite");
  return {
    currency,
    minor: Math.round((amount + Number.EPSILON) * 10 ** scale),
    scale
  };
}

export function moneyFromMinor(minor: number, currency: CurrencyCode = "EUR", scale = 2): Money {
  if (!Number.isInteger(minor)) throw new Error("money_minor_must_be_integer");
  return { currency, minor, scale };
}

export function zeroMoney(currency: CurrencyCode = "EUR", scale = 2): Money {
  return { currency, minor: 0, scale };
}

export function moneyToMajor(amount: Money) {
  return amount.minor / 10 ** amount.scale;
}

export function addMoney(left: Money, right: Money): Money {
  assertSameMoneyUnit(left, right);
  return { ...left, minor: left.minor + right.minor };
}

export function subtractMoney(left: Money, right: Money): Money {
  assertSameMoneyUnit(left, right);
  return { ...left, minor: left.minor - right.minor };
}

export function isZeroMoney(amount: Money) {
  return amount.minor === 0;
}

export function assertSameMoneyUnit(left: Money, right: Money) {
  if (left.currency !== right.currency || left.scale !== right.scale) {
    throw new Error("money_currency_or_scale_mismatch");
  }
}

export function formatMoney(amount: Money, locale = "de-DE") {
  return new Intl.NumberFormat(locale, {
    currency: amount.currency,
    maximumFractionDigits: amount.scale,
    minimumFractionDigits: amount.scale,
    style: "currency"
  }).format(moneyToMajor(amount));
}
