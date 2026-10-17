// Origin: CTOX
// License: Apache-2.0
// ctox.iot.widget.arrange – ref: src/core/iot/commands.rs:2450-2470 (Pflichtfelder x/y/w/h als i64), :1487-1524 widget_arrange
import { fail } from './_base.js';
import { C, ensureRealm, outcome, requireField, sessionRealm, widgetRow, writeRow } from './_iotcore.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const id = requireField(command, 'widget_id', 'string');
  const [x, y, w, h] = ['x', 'y', 'w', 'h'].map((k) => requireField(command, k, 'i64'));
  const existing = await ctx.get(C.widgets, id);
  ensureRealm(existing, realm, `widget not found: ${id}`);
  if (!existing) fail('not_found', `widget vanished: ${id}`); // UPDATE 0 Zeilen → project_widget scheitert
  const projection = await writeRow(ctx, C.widgets, id, widgetRow({ ...existing, x, y, w, h }), existing);
  return outcome({ widget: { id, x, y, w, h } }, [projection]);
}
