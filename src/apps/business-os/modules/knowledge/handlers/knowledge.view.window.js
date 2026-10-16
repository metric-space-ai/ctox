// Trigger-Handler knowledge.view.window (Shell v4, Strang K2, Entscheidung E11).
//
// knowledge_table_rows ist die Projektion eines Sichtfensters (Klasse window, Zeilenfilter = eigener Nutzer).
// Die App schreibt ihr Fenster als Dokument in knowledge_views {id, table, filter, sort, skip, limit} über api.put.
// Die App-Laufzeit ruft diesen Handler aus dem Änderungsstrom (§16.20 `on: {collection:'knowledge_views',
// op:'insert'|'update'}`) mit command = {type:'trigger', trigger_id, change:{collection, op, doc, seq?}}.
// Der Handler liest das Fenster über das Primitiv ctx.cap.dataframe.window (Kernform {table, skip, limit};
// Filter/Sortierung im Handler, siehe readWindow) (Parquet bleibt im Quellspeicher,
// Vorlage rxdb_peer_knowledge_rows.rs) und ersetzt die Zeilen dieses Views in einer Schreibtransaktion.
//
// Vertrag §16.16: export default async function handle(command, ctx); Fehler als throw {code, message}.
// Kein Dateisystem, kein Netz, keine Node-Globals.

export const VIEWS = 'knowledge_views';
export const ROWS = 'knowledge_table_rows';
export const TABLES = 'knowledge_tables';
export const MAX_LIMIT = 500;
export const MAX_SORT_KEYS = 4;
export const MAX_FILTER_TEXT = 200;
const ID_MAX = 160;

function fail(code, message) {
  // eslint-disable-next-line no-throw-literal
  throw { code, message };
}

const isObject = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);

// Fenster prüfen und kanonisch machen. Ungültige Views schreiben nichts (lokaler Stand bleibt sichtbar).
export function normalizeView(doc) {
  if (!isObject(doc)) fail('invalid', 'knowledge_views: Dokument fehlt');
  const id = typeof doc.id === 'string' ? doc.id : '';
  if (!id || id.length > ID_MAX) fail('invalid', `knowledge_views.id muss 1..${ID_MAX} Zeichen haben`);
  const table = typeof doc.table === 'string' ? doc.table.trim() : '';
  if (!table) fail('invalid', 'knowledge_views.table fehlt');
  const skip = doc.skip ?? 0;
  if (!Number.isInteger(skip) || skip < 0) fail('invalid', 'knowledge_views.skip muss eine ganze Zahl ≥ 0 sein');
  const limit = doc.limit;
  if (!Number.isInteger(limit) || limit < 1 || limit > MAX_LIMIT) {
    fail('invalid', `knowledge_views.limit muss 1..${MAX_LIMIT} sein`);
  }
  const filter = {};
  if (doc.filter != null) {
    if (!isObject(doc.filter)) fail('invalid', 'knowledge_views.filter muss ein Objekt sein');
    if (doc.filter.text != null) {
      if (typeof doc.filter.text !== 'string') fail('invalid', 'knowledge_views.filter.text muss Text sein');
      const text = doc.filter.text.trim();
      if (text.length > MAX_FILTER_TEXT) fail('invalid', `knowledge_views.filter.text länger als ${MAX_FILTER_TEXT}`);
      if (text) filter.text = text;
    }
  }
  const sort = [];
  if (doc.sort != null) {
    if (!Array.isArray(doc.sort) || doc.sort.length > MAX_SORT_KEYS) {
      fail('invalid', `knowledge_views.sort muss eine Liste mit höchstens ${MAX_SORT_KEYS} Einträgen sein`);
    }
    for (const entry of doc.sort) {
      const keys = isObject(entry) ? Object.keys(entry) : [];
      if (keys.length !== 1 || !keys[0] || !['asc', 'desc'].includes(entry[keys[0]])) {
        fail('invalid', 'knowledge_views.sort: Einträge haben die Form {<spalte>: "asc"|"desc"}');
      }
      sort.push({ [keys[0]]: entry[keys[0]] });
    }
  }
  const actorId = typeof doc.actor_id === 'string' ? doc.actor_id : '';
  return { id, table, filter, sort, skip, limit, actor_id: actorId };
}

export function windowKey(view) {
  return JSON.stringify([view.table, view.filter, view.sort, view.skip, view.limit]);
}

export function rowId(viewId, rowIndex) {
  return `${viewId}#${rowIndex}`;
}

const bareTableId = (value) => {
  const text = String(value || '').trim();
  return text.startsWith('table:') ? text.slice('table:'.length) : text;
};

// Katalogeintrag der Tabelle (knowledge_tables, app-eigen). Fehlt er, gibt es kein Fenster.
async function findCatalog(ctx, table) {
  const bare = bareTableId(table);
  const rows = await ctx.read(TABLES, {
    selector: { $or: [{ id: `table:${bare}` }, { id: bare }, { logical_table_id: bare }, { table_id: bare }] },
    limit: 1,
  });
  return rows[0] || null;
}

// Tabellen-id für das Primitiv dataframe.window({table}): der Katalog nennt sie (dataframe_ref), sonst die
// Knowledge-Tabellen-id ohne 'table:'-Präfix (Kern: knowledge_table_row_window(root, table_id, …)).
export function dataframeRef(catalog, table) {
  const ref = catalog?.dataframe_ref ?? catalog?.payload?.dataframe_ref;
  if (typeof ref === 'string' && ref) return ref;
  return bareTableId(table);
}

// Kernform (Strang CAPSIG): dataframe.window({table, skip?, limit ≤ 1000}) nimmt nur leere filter/sort. Ohne
// Filter/Sortierung liest der Handler genau sein Fenster. Mit Filter (Volltext, case-insensitiv über alle Werte)
// oder Sortierung (mehrstufig, stabil, null/undefined zuerst) lädt er die Tabelle seitenweise, filtert und
// sortiert selbst und schneidet dann das Fenster; row_count = Zeilen nach Filter (wie bisher).
export const CORE_PAGE = 1000;
export const MAX_SCAN_ROWS = 100_000;

function compareValues(a, b) {
  if (a === b) return 0;
  if (a === undefined || a === null) return -1;
  if (b === undefined || b === null) return 1;
  if (typeof a === 'number' && typeof b === 'number') return a < b ? -1 : 1;
  return String(a) < String(b) ? -1 : String(a) > String(b) ? 1 : 0;
}

export async function readWindow(ctx, table, view) {
  const text = String(view.filter?.text || '').toLowerCase();
  if (!text && view.sort.length === 0) {
    return ctx.cap.dataframe.window({ table, skip: view.skip, limit: view.limit });
  }
  const all = [];
  let first = null;
  for (let skip = 0; ; skip += CORE_PAGE) {
    const page = await ctx.cap.dataframe.window({ table, skip, limit: CORE_PAGE });
    if (first === null) first = page;
    const rows = Array.isArray(page?.rows) ? page.rows : [];
    all.push(...rows);
    const total = Number.isInteger(page?.row_count) ? page.row_count : all.length;
    if (rows.length < CORE_PAGE || all.length >= total) break;
    if (all.length >= MAX_SCAN_ROWS) fail('quota', `knowledge view ${view.id}: Filter/Sortierung über mehr als ${MAX_SCAN_ROWS} Zeilen`);
  }
  let rows = text
    ? all.filter((row) => Object.values(isObject(row) ? row : { value: row }).some((v) => String(v).toLowerCase().includes(text)))
    : all;
  if (view.sort.length) {
    rows = rows.map((row, index) => ({ row, index })).sort((left, right) => {
      for (const entry of view.sort) {
        const [column, dir] = Object.entries(entry)[0];
        const order = compareValues(left.row?.[column], right.row?.[column]);
        if (order) return dir === 'desc' ? -order : order;
      }
      return left.index - right.index;
    }).map(({ row }) => row);
  }
  return {
    rows: rows.slice(view.skip, view.skip + view.limit),
    row_count: rows.length,
    // Tabelleninhalt + Fensterschlüssel (window_key, separat geprüft) bestimmen das Fenster eindeutig.
    content_hash: typeof first?.content_hash === 'string' ? first.content_hash : '',
    schema_hash: typeof first?.schema_hash === 'string' ? first.schema_hash : '',
  };
}

async function replaceRows(ctx, view, rows) {
  return ctx.transaction(async (tx) => {
    const existing = await tx.read(ROWS, { selector: { view_id: view.id } });
    const keep = new Set(rows.map((row) => row.id));
    let removed = 0;
    for (const doc of existing) {
      if (keep.has(doc.id)) continue;
      await tx.remove(ROWS, doc.id);
      removed += 1;
    }
    for (const row of rows) await tx.put(ROWS, row);
    return removed;
  });
}

export default async function handle(command, ctx) {
  if (command?.type !== 'trigger') fail('invalid', 'knowledge.view.window läuft nur als Trigger');
  const change = command.change || {};
  if (change.collection !== VIEWS) return { status: 'skipped', reason: 'collection' };
  if (change.op !== 'insert' && change.op !== 'update') return { status: 'skipped', reason: 'op' };

  // Cursor je Trigger (§16.20): bereits verarbeitete Änderungen (erneute Zustellung) überspringen.
  const seq = Number.isInteger(change.seq) ? change.seq : null;
  const cursor = ctx.cursor ? await ctx.cursor.get() : null;
  if (seq !== null && Number.isInteger(cursor?.seq) && seq <= cursor.seq) return { status: 'skipped', reason: 'seen' };

  const view = normalizeView(change.doc);
  // Nur der aktuelle Stand des Views zählt: eine neuere Änderung hat ihren eigenen Trigger-Lauf.
  const current = await ctx.get(VIEWS, view.id);
  if (!current) return { status: 'skipped', reason: 'deleted' };
  if (windowKey(normalizeView(current)) !== windowKey(view)) return { status: 'skipped', reason: 'superseded' };

  const key = windowKey(view);
  const actorId = view.actor_id || (typeof current.actor_id === 'string' ? current.actor_id : '');
  const catalog = await findCatalog(ctx, view.table);
  let result = null;
  if (catalog) {
    try {
      result = await readWindow(ctx, dataframeRef(catalog, view.table), view);
    } catch (error) {
      if (error?.code !== 'not_found') throw error; // übrige Fehler: wiederholbar, lokaler Stand bleibt
    }
  }

  if (!result) {
    // Tabelle unbekannt oder archiviert (Rust: ROWS_TABLE_NOT_FOUND): Fenster leeren.
    const removed = await replaceRows(ctx, view, []);
    if (seq !== null && ctx.cursor) await ctx.cursor.set({ seq });
    return { status: 'not_found', view_id: view.id, rows: 0, removed };
  }

  const windowRows = Array.isArray(result.rows) ? result.rows.slice(0, view.limit) : [];
  const rowCount = Number.isInteger(result.row_count) && result.row_count >= 0 ? result.row_count : windowRows.length;
  const contentHash = typeof result.content_hash === 'string' ? result.content_hash : '';
  const schemaHash = typeof result.schema_hash === 'string' ? result.schema_hash : '';

  const existing = await ctx.read(ROWS, { selector: { view_id: view.id } });
  const unchanged = existing.length === windowRows.length && contentHash !== ''
    && existing.every((doc) => doc.window_key === key && doc.content_hash === contentHash);
  if (unchanged) {
    if (seq !== null && ctx.cursor) await ctx.cursor.set({ seq });
    return { status: 'unchanged', view_id: view.id, rows: existing.length, row_count: rowCount };
  }

  const now = ctx.now();
  const docs = windowRows.map((values, index) => {
    const rowIndex = view.skip + index;
    return {
      id: rowId(view.id, rowIndex),
      view_id: view.id,
      table_id: bareTableId(view.table),
      actor_id: actorId,
      row_index: rowIndex,
      values: isObject(values) ? values : { value: values },
      row_count: rowCount,
      window_key: key,
      content_hash: contentHash,
      schema_hash: schemaHash,
      updated_at_ms: now,
    };
  });
  const removed = await replaceRows(ctx, view, docs);
  if (seq !== null && ctx.cursor) await ctx.cursor.set({ seq });
  return { status: 'projected', view_id: view.id, rows: docs.length, removed, row_count: rowCount };
}
