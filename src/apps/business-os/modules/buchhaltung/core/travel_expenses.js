/**
 * Core engine for German travel expense reimbursement (Verpflegungsmehraufwand - VMA)
 * in accordance with § 9 EStG. Handles meal deductions and multi-day travels.
 */

// Legal rates for Germany (Standard 2024/2025/2026)
export const RATES_DE = {
  FULL_DAY: 2800, // 28.00 € in Cent
  PARTIAL_DAY: 1400, // 14.00 € in Cent
  DEDUCTION_BREAKFAST: 560, // 20% of 28 € = 5.60 € in Cent
  DEDUCTION_LUNCH: 1120, // 40% of 28 € = 11.20 € in Cent
  DEDUCTION_DINNER: 1120 // 40% of 28 € = 11.20 € in Cent
};

/**
 * Calculates the allowance for a single day of travel.
 *
 * @param {string} type - 'single' | 'arrival' | 'departure' | 'full'
 * @param {number} hours - Number of hours away (only relevant for 'single' day trips)
 * @param {object} meals - { breakfast: boolean, lunch: boolean, dinner: boolean }
 * @returns {number} Allowance for that day in Cent (never less than 0).
 */
export function calculateDailyAllowance(type, hours = 0, meals = { breakfast: false, lunch: false, dinner: false }) {
  let baseRate = 0;

  if (type === 'single') {
    if (hours >= 8) {
      baseRate = RATES_DE.PARTIAL_DAY;
    } else {
      baseRate = 0;
    }
  } else if (type === 'arrival' || type === 'departure') {
    baseRate = RATES_DE.PARTIAL_DAY;
  } else if (type === 'full') {
    baseRate = RATES_DE.FULL_DAY;
  }

  if (baseRate === 0) return 0;

  // Apply meal deductions if provided by third parties / included in hotel bills
  let deductions = 0;
  if (meals.breakfast) deductions += RATES_DE.DEDUCTION_BREAKFAST;
  if (meals.lunch) deductions += RATES_DE.DEDUCTION_LUNCH;
  if (meals.dinner) deductions += RATES_DE.DEDUCTION_DINNER;

  const allowance = baseRate - deductions;
  return Math.max(0, allowance); // Net allowance cannot be negative
}

/**
 * Parses start and end timestamps and returns a list of days with recommended travel types.
 *
 * @param {string|Date} startDateTime - ISO or Date string (e.g. '2026-05-01T08:00:00')
 * @param {string|Date} endDateTime - ISO or Date string (e.g. '2026-05-03T18:00:00')
 * @returns {Array<object>} Array of day structures.
 */
export function generateTravelDays(startDateTime, endDateTime) {
  const start = new Date(startDateTime);
  const end = new Date(endDateTime);

  if (isNaN(start.getTime()) || isNaN(end.getTime()) || start > end) {
    throw new Error('Invalid start or end date/time for travel allowance calculation.');
  }

  // Normalize date boundaries
  const days = [];
  const curr = new Date(start.getFullYear(), start.getMonth(), start.getDate());
  const last = new Date(end.getFullYear(), end.getMonth(), end.getDate());

  while (curr <= last) {
    days.push(new Date(curr));
    curr.setDate(curr.getDate() + 1);
  }

  if (days.length === 1) {
    // Single day trip
    const diffHours = (end - start) / (1000 * 60 * 60);
    return [{
      date: days[0].toISOString().split('T')[0],
      type: 'single',
      hours: parseFloat(diffHours.toFixed(2)),
      breakfast: false,
      lunch: false,
      dinner: false
    }];
  }

  // Multi-day trip
  return days.map((dayDate, idx) => {
    let type = 'full';
    if (idx === 0) {
      type = 'arrival';
    } else if (idx === days.length - 1) {
      type = 'departure';
    }

    return {
      date: dayDate.toISOString().split('T')[0],
      type,
      hours: 24,
      breakfast: false,
      lunch: false,
      dinner: false
    };
  });
}

/**
 * Computes the total travel allowance for a complete trip.
 *
 * @param {Array<object>} travelDays - List of days returned from generateTravelDays/UI
 * @returns {object} { totalAllowance: number, breakdown: Array<object> }
 */
export function calculateTotalTravelAllowance(travelDays = []) {
  let totalAllowance = 0;
  const breakdown = travelDays.map(day => {
    const allowance = calculateDailyAllowance(day.type, day.hours || 0, {
      breakfast: day.breakfast || false,
      lunch: day.lunch || false,
      dinner: day.dinner || false
    });
    totalAllowance += allowance;
    return {
      date: day.date,
      type: day.type,
      allowance
    };
  });

  return {
    totalAllowance,
    breakdown
  };
}
