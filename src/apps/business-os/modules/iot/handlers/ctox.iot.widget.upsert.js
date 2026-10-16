// Origin: CTOX
// License: Apache-2.0
// ctox.iot.widget.upsert – ref: src/core/iot/commands.rs:1401-1460 widget_upsert (Selbstreparatur-Tor: ungültiger
// Wächter → needs_attention, lauffähiger → armed), :1360-1399 project_widget
import { C, ensureRealm, generateId, outcome, parsePayload, sessionRealm, statusForCode, widgetRow, writeRow } from './_iotcore.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const req = parsePayload(command, {
    id: 'string?', dashboard_id: 'string', realm: 'string', signal_ref: 'string', cond_text: 'string?', action_prompt: 'string?',
    trigger_code: 'string?', render_code: 'string?', x: 'i64?', y: 'i64?', w: 'i64?', h: 'i64?', sort_index: 'i64?',
  });
  const id = req.id ?? generateId(ctx);
  const existing = await ctx.get(C.widgets, id);
  if (req.id) ensureRealm(existing, realm, `widget not found: ${id}`);
  // ON CONFLICT überschreibt alle Definitionsfelder; trigger_status/trigger_state bleiben (commands.rs:1426-1438).
  let triggerStatus = existing?.trigger_status ?? null;
  if (req.trigger_code !== null && req.trigger_code.trim() !== '') triggerStatus = statusForCode(req.trigger_code);
  const widget = { ...req, realm, trigger_status: triggerStatus, trigger_state: existing?.trigger_state ?? null };
  const projection = await writeRow(ctx, C.widgets, id, widgetRow(widget), existing);
  return outcome({ widget: { id, dashboard_id: req.dashboard_id, signal_ref: req.signal_ref } }, [projection]);
}
