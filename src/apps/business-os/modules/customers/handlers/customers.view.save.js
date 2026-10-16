// customers.view.save – ref: store_customer_commands.rs:866-920 (customers_view_save) + :1545 (save_view_children) @ 4794af03d
import {
  idFromCommand, objectPayload, requiredStr, validateAllowed, putDefaultString, putDefaultBool, putDefaultI64,
  putDefaultObject, putRecord, writeActivity, str, stableDigest, jsonDisplay, isObject, clone,
} from './_lib.js';

async function saveViewChildren(tx, viewId, value, collection, now) {
  if (!Array.isArray(value)) return [];
  const saved = [];
  for (const [index, item] of value.entries()) {
    const record = objectPayload(item);
    record.view_id = viewId;
    putDefaultI64(record, 'position', index);
    putDefaultBool(record, 'is_deleted', false);
    putDefaultI64(record, 'created_at_ms', now);
    // Rust: Digest über die Display-Form des Datensatzes VOR dem Setzen von id (serde_json, preserve_order).
    const id = str(record, ['id'])
      ?? `${collection === 'customer_view_filters' ? 'filter' : 'sort'}_${stableDigest(
        `${viewId}:${collection}:${index}:${jsonDisplay(record)}`,
      )}`;
    record.id = id;
    await putRecord(tx, collection, id, now, record);
    saved.push(record);
  }
  return saved;
}

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const payload = command.payload;
    const viewId = idFromCommand(command, ['view_id', 'id'], 'view');
    const view = objectPayload(isObject(payload) && Object.prototype.hasOwnProperty.call(payload, 'view') ? payload.view : payload);
    view.id = viewId;
    requiredStr(view, ['name']);
    const objectType = requiredStr(view, ['object_type']);
    validateAllowed('view object_type', objectType, ['account', 'contact', 'opportunity']);
    putDefaultString(view, 'view_type', 'table');
    putDefaultString(view, 'visibility', 'private');
    putDefaultBool(view, 'is_deleted', false);
    putDefaultI64(view, 'created_at_ms', now);
    putDefaultObject(view, 'payload');
    await putRecord(tx, 'customer_views', viewId, now, view);
    const filters = await saveViewChildren(tx, viewId, isObject(payload) ? payload.filters : undefined, 'customer_view_filters', now);
    const sorts = await saveViewChildren(tx, viewId, isObject(payload) ? payload.sorts : undefined, 'customer_view_sorts', now);
    const activity = await writeActivity(tx, command, now, {
      activity_type: 'view_saved',
      name: 'Ansicht gespeichert',
      linked_record_type: 'view',
      linked_record_id: viewId,
      linked_record_name: str(view, ['name']),
      properties: { view: clone(view), filters, sorts },
    });
    return { ok: true, view, activity };
  });
}
