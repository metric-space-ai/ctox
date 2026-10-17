// Origin: CTOX
// License: Apache-2.0
// ctox.iot.dashboard.delete – ref: src/core/iot/commands.rs:2435-2441 (Pflichtfeld), :1304-1336 dashboard_delete
import { C, ensureRealm, outcome, removeRow, requireField, sessionRealm } from './_iotcore.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const id = requireField(command, 'dashboard_id', 'string');
  ensureRealm(await ctx.get(C.dashboards, id), realm, `dashboard not found: ${id}`);
  // Widgets des Dashboards zuerst (commands.rs:1324-1328); fehlendes Dashboard ist kein Fehler (Rust löscht 0 Zeilen).
  const projections = [];
  for (const widget of await ctx.read(C.widgets, { selector: { dashboard_id: id } })) {
    projections.push(await removeRow(ctx, C.widgets, widget.id));
  }
  projections.unshift(await removeRow(ctx, C.dashboards, id) ?? { collection: C.dashboards, id });
  return outcome({ deleted: id }, projections);
}
