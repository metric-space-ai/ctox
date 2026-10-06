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

export function selectImportGroups(rows, sheet, selectedGroups) {
  const wzMapping = buildWzMapping(sheet);
  const list = Array.isArray(rows) ? rows : [];
  const incoming = Array.isArray(selectedGroups)
    ? selectedGroups.filter((x) => typeof x === 'string')
    : [];
  const selectedIds = new Set(incoming);

  const groups = new Map();

  for (let i = 0; i < list.length; i++) {
    const row = list[i];
    if (row == null) continue;
    const raw = row.raw;
    const wzValue = raw ? raw['branche(wz)'] : undefined;
    const division = normalizeWzDivision(wzValue);

    let id;
    let label;
    if (division === '') {
      id = 'missing:0';
      label = '(ohne WZ-Code)';
    } else {
      const mapped = wzMapping.get(division);
      if (mapped) {
        id = 'list:' + encodeURIComponent(mapped);
        label = mapped;
      } else {
        id = 'div:' + division;
        label = 'WZ-Abteilung ' + division;
      }
    }

    let g = groups.get(id);
    const isSelected = selectedIds.has(id);
    if (!g) {
      g = { id, label, count: 0, selected: isSelected, rows: [] };
      groups.set(id, g);
    } else if (isSelected) {
      g.selected = true;
    }
    g.count++;
    g.rows.push(row);
  }

  const groupArr = Array.from(groups.values());
  const collator = new Intl.Collator('de', { sensitivity: 'base' });
  groupArr.sort((a, b) => {
    if (b.count !== a.count) return b.count - a.count;
    return collator.compare(a.label, b.label);
  });

  const groupsOut = groupArr.map((g) => ({
    id: g.id,
    label: g.label,
    count: g.count,
    selected: g.selected,
  }));

  const selectedRows = [];
  for (let i = 0; i < groupArr.length; i++) {
    const g = groupArr[i];
    if (g.selected) {
      for (let j = 0; j < g.rows.length; j++) {
        selectedRows.push(g.rows[j]);
      }
    }
  }

  const selectedCount = selectedRows.length;
  const groupLimit = 5000;
  const canProceed = selectedCount > 0 && selectedCount <= groupLimit;
  let message = '';
  if (selectedCount > groupLimit) {
    message = 'Mehr als ' + groupLimit + ' Zeilen ausgewählt — Auswahl bitte reduzieren.';
  }

  return {
    groups: groupsOut,
    selectedRows,
    selectedCount,
    canProceed,
    message,
    groupLimit,
  };
}
