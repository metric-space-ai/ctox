// Origin: CTOX
// License: Apache-2.0
// ctox.iot.widget.delete – ref: src/core/iot/commands.rs:2443-2449 (Pflichtfeld), :1462-1485 widget_delete
import { C, ensureRealm, outcome, removeRow, requireField, sessionRealm } from './_iotcore.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const id = requireField(command, 'widget_id', 'string');
  ensureRealm(await ctx.get(C.widgets, id), realm, `widget not found: ${id}`);
  const projection = await removeRow(ctx, C.widgets, id) ?? { collection: C.widgets, id };
  return outcome({ deleted: id }, [projection]);
}
