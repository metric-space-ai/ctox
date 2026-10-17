// Sellify CRM lookup over the replicated Sellify projections (foreign
// collections sellify_companies / sellify_people / sellify_campaigns).
//
// Faithful port of store_outbound_commands.rs (origin/main 4794af03d):
//   :1483 decode_sellify_lookup_record      → toRecord
//   :1493 sellify_lookup_field_rows         → fieldRows
//   :1547 outbound_sellify_campaign_name_groups → campaignNameGroups
//   :1626 sellify_lookup_spec               → lookupSpec
//   :1673 outbound_sellify_lookup           → sellifyLookup
//   :1732 outbound_sellify_lookup_in_snapshot → lookupInSnapshot
//   :1889 outbound_sellify_batch_lookup     → batchLookup
//   :2048 sellify_project_batch_record      → projectBatchRecord
//   store.rs rxdb_string_field_lookup_sql / rxdb_string_field_group_sql (SQL → ctx.read + JS)
//
// Known, documented deviations (V4-PORT.md §Sellify): Rust also finds rows
// whose RxDB row is deleted (exact/ids/contains SQL has no `deleted = 0`);
// ctx.read/ctx.get return no tombstones. The contains prefilter is a Mango
// $regex, so numeric field values (Rust: CAST AS TEXT) are not matched.
import { asU64, fail, has, isObject, requiredString, utf8Len } from './olg.js';

const SPEC = {
  company: ['sellify_companies', ['contact_id', 'name', 'company_name', 'website', 'website_url', 'email', 'phone']],
  campaign: ['sellify_campaigns', ['name', 'title', 'contact_id', 'company_id', 'person_id', 'selection_id']],
  person: ['sellify_people', ['person_id', 'contact_id', 'email', 'phone', 'name', 'display_name', 'first_name', 'last_name']],
};

export function lookupSpec(entity) {
  const spec = SPEC[entity];
  if (!spec) fail('invalid', 'entity must be company, person or campaign');
  return spec;
}

const selectorText = (selector, key) => (isObject(selector) && typeof selector[key] === 'string' ? selector[key].trim() : '');

/** SQLite `CAST(COALESCE(json_extract(data,'$.updated_at_ms'),0) AS INTEGER)`. */
export function sqlCastInteger(value) {
  if (typeof value === 'number') return Number.isFinite(value) ? Math.trunc(value) : 0;
  if (value === true) return 1;
  if (typeof value === 'string') {
    const match = /^\s*([+-]?\d+)/.exec(value);
    return match ? Number(match[1]) : 0;
  }
  return 0;
}

/** SQLite `LIKE '%needle%'` with default (ASCII-only) case folding. */
export function likeContains(haystack, needle) {
  const fold = (s) => s.replace(/[A-Z]/g, (c) => c.toLowerCase());
  return fold(haystack).includes(fold(needle));
}

/** SQLite text affinity of json_extract(…) for CAST AS TEXT / LIKE. */
function sqlText(value) {
  if (typeof value === 'string') return value;
  if (typeof value === 'number') return String(value);
  if (value === true) return '1';
  if (value === false) return '0';
  return null;
}

const escapeRegex = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** Rust `value.parse::<i64>()` on the trimmed selector value. */
function parseI64(value) {
  if (!/^[+-]?\d+$/.test(value)) return null;
  const big = BigInt(value);
  if (big > 9223372036854775807n || big < -9223372036854775808n) return null;
  return Number(big);
}

function orderRows(docs) {
  return docs.sort((a, b) => {
    const d = sqlCastInteger(b.updated_at_ms) - sqlCastInteger(a.updated_at_ms);
    if (d) return d;
    return a.id < b.id ? 1 : a.id > b.id ? -1 : 0;
  });
}

/** ref: :1483 — record = row data with `id` defaulted to the row id. */
function toRecord(id, doc) {
  const record = JSON.parse(JSON.stringify(doc));
  if (isObject(record) && !has(record, 'id')) record.id = id;
  return record;
}

/** ref: :1493 sellify_lookup_field_rows */
export async function fieldRows(ctx, collection, field, value, contains, limit) {
  if (limit === 0 || (contains && utf8Len(value.trim()) < 2)) return [];
  let docs;
  if (contains) {
    const needle = value.trim();
    docs = (await ctx.read(collection, { selector: { [field]: { $regex: escapeRegex(needle), $options: 'i' } } }))
      .filter((doc) => {
        const text = sqlText(doc?.[field]);
        return text != null && likeContains(text, needle);
      });
  } else {
    const numeric = parseI64(value);
    docs = await ctx.read(collection, { selector: { [field]: { $in: numeric == null ? [value] : [value, numeric] } } });
  }
  return orderRows(docs).slice(0, limit).map((doc) => [doc.id, toRecord(doc.id, doc)]);
}

/** ref: :1547 outbound_sellify_campaign_name_groups (+ store.rs rxdb_string_field_group_sql) */
export async function campaignNameGroups(ctx, collection, allowed, payload) {
  const MAX_GROUPS = 200;
  const counts = new Map();
  let scanned = 0;
  for (const key of ['fuzzy_selectors', 'selectors']) {
    const selectors = Array.isArray(payload[key]) ? payload[key] : [];
    for (const selector of selectors) {
      const field = selectorText(selector, 'field');
      const needle = selectorText(selector, 'value');
      if (!allowed.includes(field)) fail('invalid', `unsupported campaign lookup field \`${field}\``);
      if (!needle) continue;
      if (utf8Len(needle) < 2) continue;
      const docs = (await ctx.read(collection, { selector: { [field]: { $regex: escapeRegex(needle), $options: 'i' } } }))
        .filter((doc) => {
          const text = sqlText(doc?.[field]);
          const deleted = doc?.is_deleted;
          return text != null && likeContains(text, needle) && !(deleted === 1 || deleted === true || deleted === 'true');
        });
      const groups = new Map();
      for (const doc of docs) {
        const grouped = doc[field];
        if (typeof grouped !== 'string') fail('invalid', `campaign group value of \`${field}\` is not text`);
        groups.set(grouped, (groups.get(grouped) || 0) + 1);
      }
      for (const count of groups.values()) scanned += count;
      for (const [name, count] of groups) {
        const trimmed = name.trim();
        if (!trimmed) continue;
        counts.set(trimmed, Math.max(counts.get(trimmed) || 0, count));
      }
    }
  }
  const all = [...counts.entries()].sort((a, b) => (b[1] - a[1]) || (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
  return {
    ok: true,
    entity: 'campaign',
    groups: all.slice(0, MAX_GROUPS).map(([name, count]) => ({ name, count })),
    scanned,
    truncated: all.length > MAX_GROUPS,
    records: [],
  };
}

/** ref: :1732 outbound_sellify_lookup_in_snapshot */
export async function lookupInSnapshot(ctx, collection, entity, allowed, payload, limit, completeProbe) {
  const groupByName = entity === 'campaign' && payload.group_by === 'name';
  if (groupByName) return campaignNameGroups(ctx, collection, allowed, payload);
  const records = [];
  const seen = new Set();
  if (Array.isArray(payload.ids)) {
    for (const raw of payload.ids) {
      if (typeof raw !== 'string') continue;
      const id = raw.trim();
      if (!id) continue;
      if (records.length >= limit) break;
      const doc = await ctx.get(collection, id);
      if (doc) {
        const record = toRecord(id, doc);
        if (!seen.has(id)) { seen.add(id); records.push(record); }
      }
    }
  }
  for (const [key, contains] of [['selectors', false], ['fuzzy_selectors', true]]) {
    if (!Array.isArray(payload[key])) continue;
    for (const selector of payload[key]) {
      if (records.length >= limit) break;
      const field = selectorText(selector, 'field');
      const value = selectorText(selector, 'value');
      if (!allowed.includes(field)) fail('invalid', `unsupported ${entity} lookup field \`${field}\``);
      if (!value) continue;
      const rows = await fieldRows(ctx, collection, field, value, contains,
        completeProbe ? limit : Math.max(0, limit - records.length));
      for (const [id, record] of rows) {
        if (!seen.has(id)) { seen.add(id); records.push(record); }
      }
    }
  }
  const fields = Array.isArray(payload.fields) ? payload.fields.filter((f) => typeof f === 'string') : [];
  if (fields.length) {
    for (const record of records) {
      if (!isObject(record)) continue;
      for (const key of Object.keys(record)) {
        if (key !== 'id' && !fields.includes(key)) delete record[key];
      }
    }
  }
  return { ok: true, entity, records };
}

/** Rust opens a required read snapshot first: a missing projection is an error, never "empty". */
async function requireReadable(ctx, collection) {
  await ctx.read(collection, { selector: {}, limit: 1 });
}

/** ref: :1673 outbound_sellify_lookup */
export async function sellifyLookup(ctx, payload) {
  if (has(payload, 'batch')) return batchLookup(ctx, payload);
  const entity = requiredString(payload, ['entity']);
  const [collection, allowed] = lookupSpec(entity);
  const groupByName = entity === 'campaign' && payload.group_by === 'name';
  await requireReadable(ctx, collection);
  const limitCap = groupByName ? 50_000 : entity === 'campaign' ? 2000 : 100;
  const limit = Math.min(Math.max(asU64(payload.limit) ?? 25, 1), limitCap);
  return lookupInSnapshot(ctx, collection, entity, allowed, payload, limit, false);
}

/** ref: :2048 sellify_project_batch_record */
export function projectBatchRecord(record, fields) {
  const result = {};
  for (const key of ['id', '_deleted', 'is_deleted']) {
    if (has(record, key)) result[key] = record[key];
  }
  for (const field of fields) {
    if (typeof field !== 'string') fail('invalid', 'Sellify field must be a string');
    const parts = field.split('.');
    let source = record;
    let present = true;
    for (const part of parts) {
      if (has(source, part)) source = source[part];
      else { present = false; break; }
    }
    if (!present) continue;
    let target = result;
    for (const part of parts.slice(0, -1)) {
      if (!has(target, part)) target[part] = {};
      if (!isObject(target[part])) fail('invalid', 'overlapping Sellify output field paths');
      target = target[part];
    }
    target[parts[parts.length - 1]] = JSON.parse(JSON.stringify(source));
  }
  return result;
}

/** ref: :1889 outbound_sellify_batch_lookup */
export async function batchLookup(ctx, payload) {
  const requests = payload.batch;
  if (!Array.isArray(requests)) fail('invalid', 'Sellify batch must be an array');
  if (!(requests.length >= 1 && requests.length <= 50)) fail('invalid', 'Sellify batch must contain 1 to 50 requests');
  const keys = new Set();
  const entities = new Set();
  let totalSelectors = 0;
  let fuzzySelectors = 0;
  // Validate every request before opening any collection or returning data.
  for (const request of requests) {
    const key = requiredString(request, ['key']);
    if (!(utf8Len(key) <= 160 && !keys.has(key))) fail('invalid', 'invalid or duplicate Sellify batch key');
    keys.add(key);
    const entity = requiredString(request, ['entity']);
    if (!(entity === 'company' || entity === 'person')) fail('invalid', 'Sellify batch entity must be company or person');
    if (has(request, 'batch') || has(request, 'group_by') || has(request, 'ids')) {
      fail('invalid', 'nested batches, groups and internal record IDs are not supported in a Sellify batch');
    }
    const [, allowed] = lookupSpec(entity);
    let criteria = 0;
    for (const selectorKey of ['selectors', 'fuzzy_selectors']) {
      if (!has(request, selectorKey)) continue;
      const selectors = request[selectorKey];
      if (!Array.isArray(selectors)) fail('invalid', 'Sellify selectors must be an array');
      if (selectors.length > 16) fail('invalid', 'too many Sellify batch selectors');
      for (const selector of selectors) {
        const field = requiredString(selector, ['field']);
        const value = requiredString(selector, ['value']);
        if (!(allowed.includes(field) && utf8Len(value) <= 512)) fail('invalid', 'unsupported Sellify batch selector');
        if (selectorKey === 'fuzzy_selectors' && utf8Len(value) < 2) fail('invalid', 'Sellify fuzzy batch selector is too short to execute');
        entities.add(entity);
        criteria += 1;
        totalSelectors += 1;
        if (selectorKey === 'fuzzy_selectors') fuzzySelectors += 1;
        if (!(totalSelectors <= 200 && fuzzySelectors <= 8)) {
          fail('invalid', 'Sellify batch exceeds the 200-selector/8-fuzzy-probe budget; split the batch');
        }
      }
    }
    if (criteria <= 0) fail('invalid', 'Sellify batch request requires a nonempty selector');
    const fields = request.fields;
    if (!Array.isArray(fields)) fail('invalid', 'Sellify batch requires explicit output fields');
    if (!(fields.length >= 1 && fields.length <= 32)) fail('invalid', 'Sellify batch needs 1 to 32 output fields');
    for (const field of fields) {
      if (typeof field !== 'string') fail('invalid', 'Sellify output field must be a string');
      const ok = utf8Len(field) <= 160 && field.split('.').every((part) => part !== '' && /^[A-Za-z0-9_]+$/.test(part));
      if (!ok) fail('invalid', 'invalid Sellify output field path');
    }
    if (has(request, 'limit')) {
      const limit = asU64(request.limit);
      if (!(limit != null && limit >= 1 && limit <= 50)) fail('invalid', 'Sellify batch limit must be 1 to 50');
    }
  }
  for (const entity of [...entities].sort()) await requireReadable(ctx, lookupSpec(entity)[0]);
  const results = [];
  for (const request of requests) {
    const entity = requiredString(request, ['entity']);
    const key = requiredString(request, ['key']);
    const [collection, allowed] = lookupSpec(entity);
    const limit = asU64(request.limit) ?? 25;
    // Read one extra record: a capped/ambiguous probe must never mean empty or complete.
    const query = { ...request };
    delete query.fields;
    const response = await lookupInSnapshot(ctx, collection, entity, allowed, query, limit + 1, true);
    const records = response.records;
    const projected = records.slice(0, limit).map((record) => projectBatchRecord(record, request.fields));
    results.push({ key, entity, records: projected, complete: records.length <= limit });
  }
  const response = { ok: true, schema: 'ctox.outbound.sellify_lookup_batch.v1', results };
  if (utf8Len(JSON.stringify(response)) > 1_048_576) {
    fail('quota', 'Sellify batch exceeds the 1 MiB response budget; reduce the batch or output fields');
  }
  return response;
}
