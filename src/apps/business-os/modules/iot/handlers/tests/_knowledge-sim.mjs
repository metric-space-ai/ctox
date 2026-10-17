// In-Memory-Abbild des Kernprimitivs knowledge.query({argv:['data', …]}) für die Zeitreihen-Tests der iot-App.
// Verhalten nach src/core/knowledge/{data.rs,ops.rs,parquet_io.rs} (v4/cap-2 @ 83d979cf4): create (Fehler bei
// vorhandener Tabelle), describe, append (Tabelle muss existieren), select mit --where (<=,>=,!=,~,=,<,>; Wert
// als i64/f64/bool/String geparst wie parse_value), --order-by col[:desc], --offset, --limit.
// Unbekannte Tabelle → {ok:false, error:'knowledge data table not found …'} wie dispatch_capturing.

const flag = (argv, name) => { const i = argv.indexOf(name); return i >= 0 ? argv[i + 1] : undefined; };
const wheres = (argv) => argv.flatMap((a, i) => (a === '--where' ? [argv[i + 1]] : []));

function parseValue(v) {
  if (/^[+-]?\d+$/.test(v)) return Number(v);
  if (v !== '' && !Number.isNaN(Number(v))) return Number(v);
  if (v === 'true' || v === 'True') return true;
  if (v === 'false' || v === 'False') return false;
  return v;
}

function predicate(clause) {
  for (const op of ['<=', '>=', '!=', '~', '=', '<', '>']) {
    const idx = clause.indexOf(op);
    if (idx < 0) continue;
    const col = clause.slice(0, idx).trim();
    const raw = clause.slice(idx + op.length).trim();
    const lit = op === '~' ? raw : parseValue(raw);
    return (row) => {
      const v = row[col];
      if (op !== '~' && typeof v !== typeof lit) {
        if (op === '=' || op === '!=') return op === '!=';
        throw new Error(`cannot compare ${typeof v} with ${typeof lit}`);
      }
      switch (op) {
        case '=': return v === lit;
        case '!=': return v !== lit;
        case '<': return v < lit;
        case '<=': return v <= lit;
        case '>': return v > lit;
        case '>=': return v >= lit;
        default: return String(v).includes(lit);
      }
    };
  }
  throw new Error(`bad --where clause: ${clause}`);
}

export function knowledgeSim() {
  const tables = new Map();
  const calls = [];
  const query = ({ argv }) => {
    calls.push(argv);
    const [ns, verb, ...rest] = argv;
    if (ns !== 'data') return { ok: false, error: `unknown knowledge form: ${ns}` };
    const key = `${flag(rest, '--domain')}/${flag(rest, '--key')}`;
    const table = tables.get(key);
    if (verb === 'create') {
      if (table) return { ok: false, error: `knowledge data table already exists: ${key}` };
      tables.set(key, []);
      return { ok: true };
    }
    if (!table) return { ok: false, error: `knowledge data table not found: ${key}` };
    if (verb === 'describe') return { ok: true, row_count: table.length };
    if (verb === 'append') {
      const rows = JSON.parse(flag(rest, '--rows'));
      table.push(...rows.map((r) => ({ ...r })));
      return { ok: true, rows_added: rows.length, row_count: table.length };
    }
    if (verb === 'select') {
      const preds = wheres(rest).map(predicate);
      let rows = table.filter((r) => preds.every((p) => p(r)));
      const order = flag(rest, '--order-by');
      if (order) {
        const [col, dir] = order.split(':');
        rows = rows.slice().sort((a, b) => (a[col] < b[col] ? -1 : a[col] > b[col] ? 1 : 0) * (dir === 'desc' ? -1 : 1));
      }
      const offset = Number(flag(rest, '--offset') ?? 0);
      const limit = flag(rest, '--limit');
      rows = rows.slice(offset, limit === undefined ? undefined : offset + Number(limit));
      return { ok: true, rows: rows.map((r) => ({ ...r })), n_rows: rows.length };
    }
    return { ok: false, error: `unknown verb ${verb}` };
  };
  return { query, tables, calls };
}
