/**
 * CTOX Business OS - HyperFormula ESM Port
 * parser/Address.js — Cell and range coordinate converters and parsers.
 *
 * ref: hyperformula/src/parser/Address.ts:1-200
 */

// Helper: Convert column index to name (e.g. 0 -> A, 27 -> AB)
// ref: hyperformula/src/parser/addressRepresentationConverters.ts:15-35
export function colIndexToName(index) {
  let name = "";
  let temp = index;
  while (temp >= 0) {
    name = String.fromCharCode((temp % 26) + 65) + name;
    temp = Math.floor(temp / 26) - 1;
  }
  return name;
}

// Helper: Convert column name to index (e.g. A -> 0, AB -> 27)
// ref: hyperformula/src/parser/addressRepresentationConverters.ts:40-60
export function colNameToIndex(name) {
  let index = 0;
  const cleanName = name.replace(/\$/g, '').toUpperCase();
  for (let i = 0; i < cleanName.length; i++) {
    index = index * 26 + (cleanName.charCodeAt(i) - 64);
  }
  return index - 1;
}

// Parse a single cell address string (e.g. "Sheet1!$B$3", "A1")
// ref: hyperformula/src/parser/CellAddress.ts:25-85
export function parseCellAddress(str, currentSheetId = 0) {
  const match = str.match(/^(?:(?:'([^']+)'|([A-Za-z0-9_]+))!)?(\$?)([A-Z]+)(\$?)([0-9]+)$/i);
  if (!match) return null;

  const sheetName = match[1] || match[2] || currentSheetId;
  const absCol = match[3] === '$';
  const colName = match[4];
  const absRow = match[5] === '$';
  const rowNum = parseInt(match[6], 10);

  return {
    sheet: sheetName,
    col: colNameToIndex(colName),
    row: rowNum - 1,
    absCol,
    absRow
  };
}

// Convert address object to string
// ref: hyperformula/src/parser/CellAddress.ts:90-130
export function cellAddressToString(addr) {
  const sheetPart = addr.sheet !== undefined && addr.sheet !== 0 ? `'${addr.sheet}'!` : "";
  const colPart = (addr.absCol ? "$" : "") + colIndexToName(addr.col);
  const rowPart = (addr.absRow ? "$" : "") + (addr.row + 1);
  return `${sheetPart}${colPart}${rowPart}`;
}
