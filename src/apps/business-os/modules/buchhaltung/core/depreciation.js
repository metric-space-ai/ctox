/**
 * core/depreciation.js
 *
 * Asset Depreciation (AfA) Engine.
 * Handles precise monthly/yearly asset depreciation schedules with leap-year and calendar delta normalization.
 */

/**
 * Checks if a year is a leap year.
 *
 * @param {number} year
 * @returns {boolean}
 */
export function isLeapYear(year) {
  return (year % 4 === 0 && year % 100 !== 0) || (year % 400 === 0);
}

/**
 * Calculates count of leap days in years between startYear and endYear (exclusive).
 *
 * @param {number} startYear
 * @param {number} endYear
 * @returns {number}
 */
export function getLeapDaysBetween(startYear, endYear) {
  let leapDays = 0;
  for (let y = startYear; y < endYear; y++) {
    if (isLeapYear(y)) leapDays++;
  }
  return leapDays;
}

/**
 * Calculates a standard-year calibrated calendar delta in days between two dates.
 * Corrects leap days to prevent systematic drift, matching Tryton's normalized_delta.
 *
 * @param {Date} startDate
 * @param {Date} endDate
 * @returns {number} Calibrated elapsed days
 */
export function normalizedDelta(startDate, endDate) {
  if (startDate > endDate) {
    throw new Error("Startdatum darf nicht nach Enddatum liegen.");
  }

  const oneDayMs = 24 * 60 * 60 * 1000;
  const diffDays = Math.round((endDate - startDate) / oneDayMs);

  let correction = 0;
  const startYear = startDate.getFullYear();
  const endYear = endDate.getFullYear();
  const startMonth = startDate.getMonth() + 1; // JS months are 0-indexed
  const endMonth = endDate.getMonth() + 1;

  if (startYear === endYear) {
    if (isLeapYear(startYear) && startMonth <= 2 && endMonth > 2) {
      correction -= 1;
    }
  } else {
    if (isLeapYear(startYear) && startMonth <= 2) {
      correction -= 1;
    }
    if (isLeapYear(endYear) && endMonth > 2) {
      correction -= 1;
    }
    correction -= getLeapDaysBetween(startYear + 1, endYear);
  }

  return diffDays + correction;
}

/**
 * Generates a full monthly depreciation schedule for an asset.
 * Supports both linear and degressive monthly depreciation.
 * All financial values are Cent-based integers.
 *
 * @param {number} acquisitionValue - Purchase cost in Cents (e.g. 420000 for 4200.00 EUR)
 * @param {number} residualValue - Scrap value in Cents (usually 0 or 100 for 1.00 EUR reminder)
 * @param {string|Date} startDate - Start of depreciation (YYYY-MM-DD or Date object)
 * @param {number} durationMonths - Total lifespan in months (e.g., 36 months for 3 years)
 * @param {number} degressiveRate - Degressive depreciation percentage rate (0 for linear, e.g. 0.20 for 20%)
 * @returns {Array} - The list of periodic depreciation entries
 */
export function computeDepreciationSchedule(acquisitionValue, residualValue, startDate, durationMonths, degressiveRate = 0) {
  if (acquisitionValue < 0 || residualValue < 0) {
    throw new Error("Werte dürfen nicht negativ sein.");
  }

  const depreciatingValue = acquisitionValue - residualValue;
  if (depreciatingValue <= 0) return [];

  const schedule = [];
  let remainingDepreciation = depreciatingValue;
  let currentBookValue = acquisitionValue;

  const startD = new Date(startDate);
  let currentDate = new Date(startD);

  // Standard monthly linear rate
  const standardMonthlyLinearRate = Math.floor(depreciatingValue / durationMonths);

  for (let i = 0; i < durationMonths; i++) {
    let periodRate = 0;

    if (degressiveRate > 0) {
      // Calculate degressive: BookValue * rate / 12
      const yearlyRate = Math.round(currentBookValue * degressiveRate);
      const monthlyRate = Math.round(yearlyRate / 12);

      // Degressive should switch to linear when linear rate is higher
      const remainingMonths = durationMonths - i;
      const linearAlternative = Math.floor(remainingDepreciation / remainingMonths);

      periodRate = Math.max(monthlyRate, linearAlternative);
    } else {
      // Standard linear proration
      periodRate = standardMonthlyLinearRate;
    }

    // Safety check: do not exceed remaining depreciating value
    if (periodRate > remainingDepreciation) {
      periodRate = remainingDepreciation;
    }

    const periodDate = new Date(currentDate);
    currentBookValue -= periodRate;
    remainingDepreciation -= periodRate;

    schedule.push({
      period_no: i + 1,
      date: periodDate.toISOString().split('T')[0],
      depreciation: periodRate,
      book_value: currentBookValue
    });

    currentDate.setMonth(currentDate.getMonth() + 1);
  }

  return schedule;
}
