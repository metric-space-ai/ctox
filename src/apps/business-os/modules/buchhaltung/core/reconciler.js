/**
 * core/reconciler.js
 *
 * Bank Statement Reconciler Matching Engine.
 * Implements weighted scoring heuristics to match bank transaction lines with open receipts.
 */

/**
 * Calculates a match confidence score (from 0 to 100) between a bank statement line and a receipt.
 *
 * Heuristics weights:
 * - Exact absolute amount match: 50 points
 * - Invoice reference number found in bank line narration: 40 points
 * - Party/Supplier name found in bank line narration (case-insensitive): 10 points
 *
 * @param {Object} bankLine - Bank statement transaction line
 * @param {Object} receipt - OCR receipt / incoming invoice
 * @returns {number} Score from 0 to 100
 */
export function calculateMatchScore(bankLine, receipt) {
  if (!bankLine || !receipt) return 0;

  let score = 0;

  // 1. Exact amount match (weight: 50)
  // bankLine.amount can be negative (outgoing bank transfer) or positive (incoming transfer)
  const transactionAbsCents = Math.abs(bankLine.amount || 0);
  const receiptGrossCents = receipt.gross_amount || 0;

  if (transactionAbsCents === receiptGrossCents && transactionAbsCents > 0) {
    score += 50;
  }

  // 2. Invoice number match in purpose / narration text (weight: 40)
  const narration = (bankLine.narration || '').toLowerCase();
  const invoiceNo = (receipt.invoice_number || '').toLowerCase().trim();

  if (invoiceNo && narration.includes(invoiceNo)) {
    score += 40;
  }

  // 3. Counterparty / Supplier name match (weight: 10)
  const supplier = (receipt.supplier_name || '').toLowerCase().trim();
  const counterparty = (bankLine.counterparty_name || '').toLowerCase().trim();

  if (supplier && (narration.includes(supplier) || counterparty.includes(supplier))) {
    score += 10;
  }

  return score;
}
