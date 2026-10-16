// Simulierte Fähigkeit ctx.cap.dataframe.window für Tests (Node und Browser, keine Node-Imports).
// Semantik = Kernform (caps/doc_extract.rs Window, Strang CAPSIG):
//   window({table, skip?, limit ≤ 1000, filter? (nur leer), sort? (nur leer)})
//     → {rows, row_count, content_hash, schema_hash}; unbekannte table → throw {code:'not_found'};
//   nicht-leere filter/sort, unbekannte Felder, limit 0/>1000 → throw {code:'invalid'} (wie der Kern).

function hash(text) {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, '0');
}

function compare(a, b) {
  if (a === b) return 0;
  if (a === undefined || a === null) return -1;
  if (b === undefined || b === null) return 1;
  if (typeof a === 'number' && typeof b === 'number') return a < b ? -1 : 1;
  return String(a) < String(b) ? -1 : String(a) > String(b) ? 1 : 0;
}

export function createDataframeSim(tables = {}) {
  const calls = [];
  const failures = [];
  const sim = {
    calls,
    tables,
    failNext(error) { failures.push(error); },
    async window(request) {
      calls.push(JSON.parse(JSON.stringify(request)));
      if (failures.length) throw failures.shift();
      const allowed = ['table', 'skip', 'offset', 'limit', 'filter', 'sort'];
      // eslint-disable-next-line no-throw-literal
      if (Object.keys(request).some((k) => !allowed.includes(k))) throw { code: 'invalid', message: '$.args' };
      const emptyFilter = request.filter == null || Object.keys(request.filter).length === 0;
      const emptySort = request.sort == null || (Array.isArray(request.sort) && request.sort.length === 0);
      // eslint-disable-next-line no-throw-literal
      if (!emptyFilter || !emptySort || !(request.limit > 0 && request.limit <= 1000)) throw { code: 'invalid', message: '$.window' };
      const source = tables[request.table];
      // eslint-disable-next-line no-throw-literal
      if (!source) throw { code: 'not_found', message: `dataframe table ${request.table} not found` };
      const rows = [...source];
      const offset = request.skip ?? request.offset ?? 0;
      const window = rows.slice(offset, offset + request.limit);
      return {
        rows: JSON.parse(JSON.stringify(window)),
        row_count: rows.length,
        content_hash: hash(JSON.stringify(rows)),
        schema_hash: hash(JSON.stringify(Object.keys(source[0] || {}))),
      };
    },
  };
  return sim;
}

export function measuredRows(count) {
  return Array.from({ length: count }, (_, index) => ({
    row_id: `MLP-${String(index + 1).padStart(4, '0')}`,
    rpm: 1000 + index * 10,
    note: index % 7 === 0 ? 'peak load' : 'nominal',
  }));
}
