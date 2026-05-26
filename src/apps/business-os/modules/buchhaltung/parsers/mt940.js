/**
 * parsers/mt940.js
 *
 * SWIFT MT940 Text Account Statement Parser.
 * Line-by-line parser for parsing SWIFT MT940 files containing :61: and :86: segment tags.
 */

/**
 * Parses SWIFT MT940 text file content into structured transaction records.
 *
 * @param {string} textContent - The raw text payload of the MT940 file
 * @returns {Array<Object>} Parsed transaction lines
 */
export function parseMT940(textContent) {
  const lines = textContent.split(/\r?\n/);
  const parsedLines = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].trim();

    if (line.startsWith(':61:')) {
      // Format of :61: is typically: :61:YYMMDD[MMDD]C/D[CurrencyCode]AmountTransactionTypeReference
      // Example: :61:2605220522CR119,00NTRFMS129817

      // Parse date: YYMMDD
      const year = '20' + line.substring(4, 6);
      const month = line.substring(6, 8);
      const day = line.substring(8, 10);
      const dateStr = `${year}-${month}-${day}`;

      // Credit or Debit indicator (typically C/D or RC/RD)
      // We look at position 10 or 11
      const isCredit = line.substring(10, 12).includes('C');

      // Extract amount string (digits with optional decimal comma or dot)
      // Skip the indicator characters
      const indicatorOffset = line.substring(10, 12).includes('R') ? 12 : 11;
      const remainder = line.substring(indicatorOffset);

      // Match the digits and comma/dot before transaction code letters
      const amountMatch = remainder.match(/^([0-9]+[,.]?[0-9]*)/);
      const amountStr = amountMatch ? amountMatch[1] : '0';

      // Convert to Cents integer
      const floatAmount = parseFloat(amountStr.replace(',', '.'));
      const amountCents = Math.round(floatAmount * 100);
      const cents = isCredit ? amountCents : -amountCents;

      // 2. Parse narrative from next line if it starts with :86:
      let narration = 'Banktransaktion';
      let counterpartyName = 'Kunde / Lieferant';

      if (lines[i + 1] && lines[i + 1].trim().startsWith(':86:')) {
        const narrationLine = lines[i + 1].trim().substring(4);
        narration = narrationLine;

        // Simple heuristic to split name if present in MT940 format
        if (narrationLine.includes('?20')) {
          // MT940 tags name with ?20
          const parts = narrationLine.split('?20');
          if (parts[1]) counterpartyName = parts[1].split('?')[0].trim();
        } else if (narrationLine.includes('Name:')) {
          const parts = narrationLine.split('Name:');
          if (parts[1]) counterpartyName = parts[1].split(';')[0].trim();
        }
      }

      parsedLines.push({
        value_date: dateStr,
        narration: narration,
        amount: cents,
        counterparty_name: counterpartyName,
        counterparty_iban: 'DE1002000000000000000'
      });
    }
  }

  return parsedLines;
}
