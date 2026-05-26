/**
 * exporters/datev.js
 *
 * DATEV EXTF CSV-Stapel-Exporter.
 * Generates official DATEV EXTF compliant CSV formatting (version 7.0/8.0 specification) for import into tax advisor systems.
 */

/**
 * Generates the full DATEV EXTF CSV file content as a string.
 *
 * @param {Array} ledgerDF - Denormalized posted ledger entries
 * @param {string} startDate - Start date of selection (YYYY-MM-DD)
 * @param {string} endDate - End date of selection (YYYY-MM-DD)
 * @param {string} [creatorName="Michael Welsch"] - Name of the accounting operator
 * @returns {string} Fully formatted CSV payload
 */
export function generateDatevCsvString(ledgerDF, startDate, endDate, creatorName = "Michael Welsch") {
  const startCompact = startDate.replace(/-/g, '');
  const endCompact = endDate.replace(/-/g, '');

  // DATEV EXTF v7.0 Header Line (declaring record type, version, software, creator, date ranges)
  const headerLine = `EXTF;700;21;Buchungsstapel;7;${startCompact};${endCompact};CTOX Fibu;${creatorName};1;1;0;;;;;`;

  // CSV Column Headers
  const columnHeaders = "Umsatz;Soll/Haben-Kennzeichen;WKZ;Kurs;Basisumsatz;Gegenkonto;Belegfeld 1;Belegfeld 2;Datum;Konto;Buchungstext;Belegfeld 2;";

  const filtered = ledgerDF.filter(l => l.posting_date >= startDate && l.posting_date <= endDate);

  if (filtered.length === 0) {
    return "";
  }

  const rows = [];
  filtered.forEach(line => {
    const isSoll = line.debit > 0;
    const amount = isSoll ? line.debit : line.credit;

    // DATEV expects decimals formatted with a comma (e.g. 119,00)
    const amountFormatted = (amount / 100).toFixed(2).replace('.', ',');

    // DATEV expects date in format DDMM (e.g. 2205 for May 22nd)
    const rawDate = line.posting_date.replace(/-/g, ''); // YYYYMMDD
    const dateDatev = rawDate.substring(6, 8) + rawDate.substring(4, 6);

    // Clean account code to get numerical code only
    const accountCode = line.account_id.includes('_') ? line.account_id.split('_')[1] : line.account_id;

    // Clean narration to prevent CSV breaks
    const cleanNarration = (line.narration || '').replace(/["';\n\r]/g, '').substring(0, 60);

    // Simple double-entry counterpart matching
    // In DATEV, each single split is written against an offset/counter account
    // We default to standard offsetting ledger accounts (Receivables/Payables or Bank)
    const counterpartCode = isSoll ? '1600' : '1200'; // Default offset accounts in Cents split

    const row = `"${amountFormatted}";"${isSoll ? 'S' : 'H'}";"EUR";"";"";"${counterpartCode}";"${line.number || ''}";"";"${dateDatev}";"${accountCode}";"${cleanNarration}";"";`;
    rows.push(row);
  });

  return [headerLine, columnHeaders, ...rows].join("\n");
}
