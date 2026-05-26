/**
 * Core engine for advanced German tax advisor rules ("Steuerberater-Tricks"):
 * 1. 70/30 Entertainment Expenses split (Bewirtungskosten)
 * 2. 35 € Gift threshold checking (Geschenke an Geschäftsfreunde)
 * 3. Internet/Phone private share adjustment (Telefon-Privatanteil)
 */

/**
 * Calculates the exact split for a restaurant bill.
 * Under German tax law, 70% of business-related meal expenses are tax-deductible,
 * 30% are non-deductible, but 100% of the input VAT (Vorsteuer) is fully deductible.
 * All inputs and outputs are in Cent.
 *
 * @param {number} grossAmount - Total receipt amount in Cent.
 * @param {number} vatRatePercent - Standard VAT rate, e.g. 19.
 * @returns {object} Split breakdown in Cent.
 */
export function calculateEntertainmentSplit(grossAmount, vatRatePercent = 19) {
  // Gross to Net calculation
  const factor = 1 + (vatRatePercent / 100);
  const netAmount = Math.round(grossAmount / factor);
  const vatAmount = grossAmount - netAmount;

  // 70% deductible, 30% non-deductible
  const deductibleNet = Math.round(netAmount * 0.70);
  const nonDeductibleNet = netAmount - deductibleNet; // Avoid rounding leak

  return {
    grossAmount,
    netAmount,
    vatAmount,
    deductibleNet,
    nonDeductibleNet
  };
}

/**
 * Recommends the correct account code for a business gift based on the 35.00 € limit (§ 4 Abs. 5 Satz 1 Nr. 1 EStG).
 * Net amount threshold is exactly 35.00 € (3500 Cent) per person/year.
 *
 * @param {number} netAmount - Gift net cost in Cent.
 * @param {string} skrType - 'SKR03' | 'SKR04'
 * @returns {string} Suggested account code.
 */
export function recommendGiftAccount(netAmount, skrType = 'SKR03') {
  const isSufficient = netAmount <= 3500; // <= 35.00 € net

  if (skrType === 'SKR04') {
    return isSufficient ? '6610' : '6611'; // 6610 (Abzugsfähig), 6611 (Nicht abzugsfähig)
  }
  return isSufficient ? '4630' : '4635'; // 4630 (Abzugsfähig), 4635 (Nicht abzugsfähig)
}

/**
 * Calculates the private share adjustment for phone/internet expenses.
 *
 * @param {number} totalNetExpense - Total telephone net expense in Cent.
 * @param {number} privateSharePercent - e.g. 20 (for 20% private usage).
 * @returns {number} Private share amount in Cent to be credited back to expenses.
 */
export function calculatePrivatePhoneShare(totalNetExpense, privateSharePercent = 20) {
  if (!totalNetExpense || totalNetExpense < 0) return 0;
  return Math.round(totalNetExpense * (privateSharePercent / 100));
}

/**
 * Compiles a journal entry for a 70/30 entertainment split.
 *
 * @param {string} entryId - Journal entry ID.
 * @param {number} grossAmount - Total amount in Cent.
 * @param {number} vatRatePercent - VAT rate, e.g. 19.
 * @param {string} skrType - 'SKR03' | 'SKR04'
 * @param {string} paymentAccount - The credit account, e.g. '1600' (Kreditor) or '1890' (Privateinlage).
 * @returns {Array<object>} Journal lines.
 */
export function compileEntertainmentJournalLines(entryId, grossAmount, vatRatePercent = 19, skrType = 'SKR03', paymentAccount = '1600') {
  const splits = calculateEntertainmentSplit(grossAmount, vatRatePercent);

  const accDeductible = skrType === 'SKR04' ? '6640' : '4650'; // Bewirtungskosten abzugsfähig
  const accNonDeductible = skrType === 'SKR04' ? '6644' : '4654'; // Bewirtungskosten nicht abzugsfähig

  // Vorsteuer account
  let accVorsteuer = '1576'; // Vorsteuer 19% SKR03 default
  if (vatRatePercent === 7) {
    accVorsteuer = skrType === 'SKR04' ? '1401' : '1571';
  } else {
    accVorsteuer = skrType === 'SKR04' ? '1406' : '1576';
  }

  return [
    {
      journal_entry_id: entryId,
      account_code: accDeductible,
      debit: splits.deductibleNet,
      credit: 0,
      narration: `Bewirtungskosten abzugsfähig (70% von ${splits.netAmount / 100} € net)`
    },
    {
      journal_entry_id: entryId,
      account_code: accNonDeductible,
      debit: splits.nonDeductibleNet,
      credit: 0,
      narration: `Bewirtungskosten nicht abzugsfähig (30% von ${splits.netAmount / 100} € net)`
    },
    {
      journal_entry_id: entryId,
      account_code: accVorsteuer,
      debit: splits.vatAmount,
      credit: 0,
      narration: `Vorsteuer aus Bewirtung (${vatRatePercent}%)`
    },
    {
      journal_entry_id: entryId,
      account_code: paymentAccount,
      debit: 0,
      credit: grossAmount,
      narration: 'Bewirtungsbeleg Ausgleich'
    }
  ];
}
