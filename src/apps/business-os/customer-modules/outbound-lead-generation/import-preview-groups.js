export function normalizeWzDivision(value) {
  if (value === null || value === undefined) return '';
  const s = String(value).trim();
  if (!s) return '';
  const m = s.match(/^\d{2}/);
  if (!m) return '';
  return String(Number(m[0]));
}

export function buildWzMapping(sheet) {
  const result = new Map();
  if (!Array.isArray(sheet) || sheet.length === 0) return result;

  const header = Array.isArray(sheet[0]) ? sheet[0] : [];
  const normHeader = (h) =>
    String(h == null ? '' : h).replace(/^\uFEFF/, '').trim().toLowerCase();
  let codeIdx = -1;
  let labelIdx = -1;
  for (let i = 0; i < header.length; i++) {
    const h = normHeader(header[i]);
    if (codeIdx === -1 && h === 'code') codeIdx = i;
    else if (labelIdx === -1 && h === 'listenname thesen') labelIdx = i;
  }
  if (codeIdx === -1 || labelIdx === -1) return result;

  const clean = (v) =>
    String(v == null ? '' : v).replace(/^\uFEFF/, '').trim();

  for (let r = 1; r < sheet.length; r++) {
    const row = sheet[r];
    if (!Array.isArray(row)) continue;

    let hasContent = false;
    for (let c = 0; c < row.length; c++) {
      if (clean(row[c]) !== '') { hasContent = true; break; }
    }
    if (!hasContent) continue;

    const rawCode = row[codeIdx];
    let numeric;
    if (typeof rawCode === 'number') {
      if (!Number.isInteger(rawCode)) continue;
      if (!/^\d{1,2}$/.test(String(rawCode))) continue;
      numeric = rawCode;
    } else if (typeof rawCode === 'string') {
      const t = clean(rawCode);
      if (!/^\d{1,2}$/.test(t)) continue;
      numeric = Number(t);
    } else {
      continue;
    }
    const codeStr = String(numeric);

    const label = clean(row[labelIdx]);
    if (!label) continue;

    result.set(codeStr, label);
  }

  return result;
}
