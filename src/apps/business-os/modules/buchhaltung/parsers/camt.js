/**
 * parsers/camt.js
 *
 * SEPA camt.053 XML Account Statement Parser.
 * Zustandsloser XML-Parser to convert standard ISO 20022 SEPA statement entries into Fibu bank lines.
 */

/**
 * Parses a SEPA camt.053 XML string and returns an array of transaction records.
 * Amounts are converted into Cent-based integers (+ for deposits, - for payments).
 *
 * @param {string} xmlText - The raw XML content of the camt.053 file
 * @param {Object} [domParserInstance] - Optional DOMParser instance (useful for Node.js environments)
 * @returns {Array<Object>} Parsed transaction lines
 */
export function parseCamt053(xmlText, domParserInstance) {
  const parser = domParserInstance || new DOMParser();
  const xmlDoc = parser.parseFromString(xmlText, "text/xml");
  const entries = xmlDoc.getElementsByTagName('Ntry');

  const parsedLines = [];

  for (let i = 0; i < entries.length; i++) {
    const entry = entries[i];

    // 1. Value Date / Buchungsdatum
    const valDate = entry.getElementsByTagName('BookgDt')[0]?.getElementsByTagName('Dt')[0]?.textContent ||
                    entry.getElementsByTagName('ValDt')[0]?.getElementsByTagName('Dt')[0]?.textContent ||
                    new Date().toISOString().split('T')[0];

    // 2. Amount and Indicator
    const amountStr = entry.getElementsByTagName('Amt')[0]?.textContent || '0';
    const indicator = entry.getElementsByTagName('CdtDbtInd')[0]?.textContent || 'CRDT'; // CRDT = Credit (+), DBIT = Debit (-)

    const amountCents = Math.round(parseFloat(amountStr) * 100);
    const cents = indicator === 'CRDT' ? amountCents : -amountCents;

    // 3. Narration / Verwendungszweck (from Unstructured elements)
    const narration = entry.getElementsByTagName('Ustrd')[0]?.textContent || 'Banküberweisung';

    // 4. Counterparty Name (from Debtr or Cdtr)
    const counterparty = entry.getElementsByTagName('Dbtr')[0]?.getElementsByTagName('Nm')[0]?.textContent ||
                         entry.getElementsByTagName('Cdtr')[0]?.getElementsByTagName('Nm')[0]?.textContent ||
                         'Kunde / Lieferant';

    // 5. Counterparty IBAN
    const dbtrIban = entry.getElementsByTagName('DbtrAcct')[0]?.getElementsByTagName('Id')[0]?.getElementsByTagName('IBAN')[0]?.textContent;
    const cdtrIban = entry.getElementsByTagName('CdtrAcct')[0]?.getElementsByTagName('Id')[0]?.getElementsByTagName('IBAN')[0]?.textContent;
    const iban = dbtrIban || cdtrIban || 'DE1002000000000000000';

    parsedLines.push({
      value_date: valDate,
      narration: narration,
      amount: cents,
      counterparty_name: counterparty,
      counterparty_iban: iban
    });
  }

  return parsedLines;
}
