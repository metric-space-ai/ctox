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

export async function extractImportRows(payload, helpers) {
  const emptyMeta = () => ({
    skippedOutsideTable: 0,
    sheets: {},
    hasWorkbookMeta: false,
  });

  if (!payload || !payload.source) {
    return { rows: [], meta: emptyMeta() };
  }

  const source = payload.source;
  const files = Array.isArray(source.files) ? source.files : [];

  if (payload.source_type === 'text') {
    if (!helpers || typeof helpers.extractCompanyRowsFromText !== 'function') {
      throw new Error('Missing required helper: extractCompanyRowsFromText');
    }
    const text = typeof source.text === 'string' ? source.text : '';
    const rows = helpers.extractCompanyRowsFromText(text);
    if (!Array.isArray(rows)) {
      throw new Error('extractCompanyRowsFromText did not return an Array');
    }
    const out = [];
    for (let i = 0; i < rows.length; i++) {
      out.push(rows[i]);
    }
    return { rows: out, meta: emptyMeta() };
  }

  if (files.length === 0) {
    return { rows: [], meta: emptyMeta() };
  }

  const ext = (name) => {
    if (typeof name !== 'string') return '';
    const i = name.lastIndexOf('.');
    return i >= 0 ? name.slice(i + 1).toLowerCase() : '';
  };

  const rowsOut = [];
  let skippedOutsideTable = 0;
  let hasWorkbookMeta = false;
  const sheets = {};

  for (let i = 0; i < files.length; i++) {
    const file = files[i];
    if (!file) continue;
    const e = ext(file && file.name);
    if (e === 'xlsx') {
      if (!helpers || typeof helpers.extractCompanyRowsFromWorkbookFile !== 'function') {
        throw new Error('Missing required helper: extractCompanyRowsFromWorkbookFile');
      }
      let result = helpers.extractCompanyRowsFromWorkbookFile(file, {
        withMeta: true,
        includeSheets: ['WZ-Code'],
      });
      // Some callers may have already invoked; tolerate a Promise.
      if (result && typeof result.then === 'function') {
        result = await result;
      }
      if (Array.isArray(result)) {
        for (let j = 0; j < result.length; j++) {
          rowsOut.push(result[j]);
        }
        continue;
      }
      if (!result || typeof result !== 'object' || !Array.isArray(result.rows)) {
        throw new Error('Malformed XLSX result: expected Array or { rows: Array }');
      }
      hasWorkbookMeta = true;
      const fileRows = result.rows;
      const metaSkipped = Number(result.meta && result.meta.skippedOutsideTable);
      if (Number.isFinite(metaSkipped) && metaSkipped >= 0) {
        skippedOutsideTable += metaSkipped;
      }
      const fileSheets = (result.meta && result.meta.sheets) || {};
      const wzSheet = Array.isArray(fileSheets['WZ-Code']) ? fileSheets['WZ-Code'] : null;
      if (wzSheet) {
        const incoming = buildWzMapping(wzSheet);
        for (const [code, label] of incoming.entries()) {
          if (sheets[code] !== undefined && sheets[code] !== label) {
            throw new Error(
              'Konflikt im WZ-Code-Mapping: Code ' + code +
              ' hat unterschiedliche Listenamen ("' + sheets[code] +
              '" vs. "' + label + '"). Bitte WZ-Code-Tabelle vereinheitlichen.'
            );
          }
          sheets[code] = label;
        }
      }
      for (let j = 0; j < fileRows.length; j++) {
        rowsOut.push(fileRows[j]);
      }
      continue;
    }
    if (e === 'csv' || e === 'tsv' || e === 'txt') {
      if (!helpers) {
        throw new Error('Missing helpers');
      }
      if (typeof helpers.parseDelimitedText !== 'function') {
        throw new Error('Missing required helper: parseDelimitedText');
      }
      if (typeof helpers.importDateiText !== 'function') {
        throw new Error('Missing required helper: importDateiText');
      }
      if (typeof helpers.normalizeCompanyRow !== 'function') {
        throw new Error('Missing required helper: normalizeCompanyRow');
      }
      let text;
      try {
        text = helpers.importDateiText(file);
      } catch (err) {
        throw new Error('importDateiText failed: ' + (err && err.message ? err.message : String(err)));
      }
      if (text && typeof text.then === 'function') {
        text = await text;
      }
      const delimRows = helpers.parseDelimitedText(typeof text === 'string' ? text : '');
      if (!Array.isArray(delimRows)) {
        throw new Error('parseDelimitedText did not return an Array');
      }
      for (let j = 0; j < delimRows.length; j++) {
        rowsOut.push(helpers.normalizeCompanyRow(delimRows[j], j));
      }
    }
  }

  const wzMatrix = [['Code', 'Listenname THESEN']];
  const codes = Object.keys(sheets);
  for (let i = 0; i < codes.length; i++) {
    const code = codes[i];
    wzMatrix.push([code, sheets[code]]);
  }

  return {
    rows: rowsOut,
    meta: {
      skippedOutsideTable,
      sheets: { 'WZ-Code': wzMatrix },
      hasWorkbookMeta,
    },
  };
}