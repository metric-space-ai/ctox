/**
 * reports/elster.js
 *
 * ELSTER Umsatzsteuer-Voranmeldung (UStVA) Mapper.
 * Maps sales/turnover taxes and input taxes (Vorsteuer) to official German tax fields.
 */

/**
 * Calculates ELSTER UStVA fields (81, 86, 66) and remaining tax payable (Zahllast).
 * All financial amounts are returned in Cent-based integers.
 *
 * @param {Array} accounts - Accounts list with computed salden
 * @param {string} skrName - Active chart of accounts template name (SKR03 or SKR04)
 * @returns {Object} ELSTER fields and calculated Zahllast
 */
export function calculateElsterUstva(accounts, skrName) {
  // Account codes vary by SKR (SKR03 vs SKR04)
  const isSKR03 = skrName === 'SKR03';

  // Tax Accounts codes mapping
  const ust19Code = isSKR03 ? '1776' : '3806'; // Umsatzsteuer 19%
  const ust7Code  = isSKR03 ? '1771' : '3801'; // Umsatzsteuer 7%

  const vorsteuer19Code = isSKR03 ? '1576' : '1406'; // Vorsteuer 19%
  const vorsteuer7Code  = isSKR03 ? '1571' : '1401'; // Vorsteuer 7%

  // Retrieve accounts
  const ust19 = accounts.find(a => a.code === ust19Code);
  const ust7  = accounts.find(a => a.code === ust7Code);

  const vor19 = accounts.find(a => a.code === vorsteuer19Code);
  const vor7  = accounts.find(a => a.code === vorsteuer7Code);

  // 1. Feld 81: Steuerpflichtige Umsätze zum Regelsatz (19%)
  // Tax is the Credit balance of the 19% VAT account
  const tax81 = ust19 ? (ust19.credit_saldo - ust19.debit_saldo) : 0;
  // Base amount is mathematically: Tax / 0.19
  const base81 = Math.round(tax81 / 0.19);

  // 2. Feld 86: Steuerpflichtige Umsätze zum ermäßigten Satz (7%)
  const tax86 = ust7 ? (ust7.credit_saldo - ust7.debit_saldo) : 0;
  const base86 = Math.round(tax86 / 0.07);

  // 3. Feld 66: Abziehbare Vorsteuerbeträge (19% & 7%)
  // Vorsteuer is Debit-heavy (expense/asset-like), so balance is Debit - Credit
  const vor19Val = vor19 ? (vor19.debit_saldo - vor19.credit_saldo) : 0;
  const vor7Val  = vor7 ? (vor7.debit_saldo - vor7.credit_saldo) : 0;
  const feld66 = vor19Val + vor7Val;

  // 4. Zahllast (remaining tax payable) = Total VAT - Total Input Tax
  const totalVAT = tax81 + tax86;
  const zahllast = totalVAT - feld66;

  return {
    feld81: {
      base: base81,
      tax: tax81
    },
    feld86: {
      base: base86,
      tax: tax86
    },
    feld66: feld66,
    zahllast: zahllast
  };
}
