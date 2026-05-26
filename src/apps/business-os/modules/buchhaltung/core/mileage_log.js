/**
 * Core engine for German mileage log (Fahrtenbuch) and flat-rate reimbursement (§ 9 EStG).
 * Handles the 0.30 € per kilometer rate and annual private share splits.
 */

export const STANDARD_MILEAGE_RATE = 30; // 0.30 € in Cent per kilometer

/**
 * Calculates travel reimbursement based on business kilometers.
 *
 * @param {number} km - Distance in kilometers.
 * @param {number} rate - Rate in Cent (defaults to 30 Cent).
 * @returns {number} Reimbursement amount in Cent.
 */
export function calculateMileageReimbursement(km, rate = STANDARD_MILEAGE_RATE) {
  if (!km || km < 0) return 0;
  return Math.round(km * rate);
}

/**
 * Computes business and private shares from a list of logbook trips.
 * Useful for tax advisors to calculate the private share ratio of a company car.
 *
 * @param {Array<{km: number, purpose: 'business'|'private'|'commute'}>} trips - List of trips.
 * @returns {object} Aggregated stats and percentage ratios.
 */
export function calculateAnnualUsageShares(trips = []) {
  let totalKm = 0;
  let businessKm = 0;
  let privateKm = 0;
  let commuteKm = 0;

  trips.forEach(trip => {
    const dist = parseFloat(trip.km || 0);
    totalKm += dist;
    if (trip.purpose === 'business') {
      businessKm += dist;
    } else if (trip.purpose === 'private') {
      privateKm += dist;
    } else if (trip.purpose === 'commute') {
      commuteKm += dist;
    }
  });

  const businessShare = totalKm > 0 ? (businessKm / totalKm) * 100 : 0;
  const privateShare = totalKm > 0 ? (privateKm / totalKm) * 100 : 0;
  const commuteShare = totalKm > 0 ? (commuteKm / totalKm) * 100 : 0;

  return {
    totalKm: parseFloat(totalKm.toFixed(2)),
    businessKm: parseFloat(businessKm.toFixed(2)),
    privateKm: parseFloat(privateKm.toFixed(2)),
    commuteKm: parseFloat(commuteKm.toFixed(2)),
    ratios: {
      business: parseFloat(businessShare.toFixed(2)),
      private: parseFloat(privateShare.toFixed(2)),
      commute: parseFloat(commuteShare.toFixed(2))
    }
  };
}

/**
 * Compiles a journal entry for mileage reimbursement.
 *
 * @param {string} entryId - Journal entry ID.
 * @param {number} km - Distance in km.
 * @param {string} expenseAccount - Expense account (e.g. '4673' Fahrtkosten privat PKW, SKR03).
 * @param {string} creditAccount - Contra account (e.g. '1890' Privateinlage, SKR03).
 * @returns {Array<object>} Journal lines.
 */
export function compileMileageJournalLines(entryId, km, expenseAccount = '4673', creditAccount = '1890') {
  const amt = calculateMileageReimbursement(km);

  return [
    {
      journal_entry_id: entryId,
      account_code: expenseAccount,
      debit: amt,
      credit: 0,
      narration: `Kilometerpauschale geschäftliche Fahrt (${km} km)`
    },
    {
      journal_entry_id: entryId,
      account_code: creditAccount,
      debit: 0,
      credit: amt,
      narration: `Kilometerpauschale Ausgleich`
    }
  ];
}
