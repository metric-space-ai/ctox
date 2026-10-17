// Origin: CTOX
// License: Apache-2.0
// ctox.iot.dashboard.upsert – ref: src/core/iot/commands.rs:1262-1302 dashboard_upsert, :1242-1260 project_dashboard
import { C, dashboardRow, ensureRealm, generateId, outcome, parsePayload, sessionRealm, writeRow } from './_iotcore.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const req = parsePayload(command, {
    id: 'string?', realm: 'string', name: 'string', scope: 'string?', scope_ref: 'string?', view_mode: 'string?', sort_index: 'i64?',
  });
  const id = req.id ?? generateId(ctx);
  const existing = await ctx.get(C.dashboards, id);
  if (req.id) ensureRealm(existing, realm, `dashboard not found: ${id}`);
  // ON CONFLICT überschreibt alle Felder, fehlende Optionen werden leer (commands.rs:1287-1295).
  const projection = await writeRow(ctx, C.dashboards, id, dashboardRow({ ...req, realm }), existing);
  return outcome({ dashboard: { id, realm, name: req.name } }, [projection]);
}
