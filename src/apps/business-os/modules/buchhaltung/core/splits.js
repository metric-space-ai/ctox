/**
 * Utility for split bookings (Aufteilungsbuchungen) in CTOX Fibu.
 * Manages Cent-based split math to guarantee 100% precision.
 */

/**
 * Calculates the remaining amount to be allocated in a split transaction.
 * All amounts are handled in Cent (integers) to prevent floating-point precision issues.
 *
 * @param {number} totalAmount - The total transaction amount in Cent.
 * @param {Array<{amount: number}>} splits - Array of split items already allocated.
 * @returns {number} The remaining amount in Cent.
 */
export function calculateRemainingSplit(totalAmount, splits = []) {
  const absTotal = Math.abs(totalAmount);
  const allocatedSum = splits.reduce((sum, item) => sum + Math.abs(item.amount || 0), 0);
  return absTotal - allocatedSum;
}

/**
 * Validates if the sum of splits matches the total transaction amount exactly.
 *
 * @param {number} totalAmount - Total amount in Cent.
 * @param {Array<{amount: number}>} splits - Array of split items.
 * @returns {boolean} True if perfectly balanced.
 */
export function validateSplitBalanced(totalAmount, splits = []) {
  if (splits.length === 0) return false;
  return calculateRemainingSplit(totalAmount, splits) === 0;
}

/**
 * Compiles a list of journal lines for a split transaction.
 *
 * @param {string} entryId - The parent journal entry ID.
 * @param {number} bankAmount - The bank transaction amount (negative for credit/debit bank flow).
 * @param {string} bankAccount - The bank asset account (e.g. '1200' SKR03).
 * @param {Array<{amount: number, accountCode: string, narration: string}>} splits - Split allocations.
 * @returns {Array<object>} Formatted journal entry lines.
 */
export function compileSplitJournalLines(entryId, bankAmount, bankAccount, splits = []) {
  if (!validateSplitBalanced(bankAmount, splits)) {
    throw new Error('Cannot compile splits: The split sum must exactly equal the total bank amount.');
  }

  const lines = [];
  const isReceipt = bankAmount > 0; // Positive bank transaction = receipt/income

  // 1. Bank Line
  lines.push({
    journal_entry_id: entryId,
    account_code: bankAccount,
    debit: isReceipt ? Math.abs(bankAmount) : 0,
    credit: !isReceipt ? Math.abs(bankAmount) : 0,
    narration: 'Banktransaktion (Split-Ausgleich)'
  });

  // 2. Offsetting Split Lines
  splits.forEach((split) => {
    const amt = Math.abs(split.amount);
    // If it's a bank debit (bankAmount < 0, we paid something), the offsetting lines are debited (Soll)
    // If it's a bank credit (bankAmount > 0, we received money), the offsetting lines are credited (Haben)
    lines.push({
      journal_entry_id: entryId,
      account_code: split.accountCode,
      debit: !isReceipt ? amt : 0,
      credit: isReceipt ? amt : 0,
      narration: split.narration || 'Split-Buchungszeile'
    });
  });

  return lines;
}
