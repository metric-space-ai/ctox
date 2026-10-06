export function normalizeWzDivision(value) {
  if (value === null || value === undefined) return '';
  const s = String(value).trim();
  if (!s) return '';
  const m = s.match(/^\d{2}/);
  if (!m) return '';
  return String(Number(m[0]));
}
