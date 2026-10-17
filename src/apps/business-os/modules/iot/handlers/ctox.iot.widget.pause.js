// Origin: CTOX
// License: Apache-2.0
// ctox.iot.widget.pause – ref: src/core/iot/commands.rs:2485-2497 (paused optional, Vorgabe true), :1526-1570 widget_set_pause.
// Fortsetzen prüft den Wächter neu, damit ein kaputter Wächter nicht still „armed“ zurückkommt.
import { C, loadWidgetInRealm, outcome, requireField, sessionRealm, statusForCode, widgetRow, writeRow } from './_iotcore.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const id = requireField(command, 'widget_id', 'string');
  const paused = typeof command.payload.paused === 'boolean' ? command.payload.paused : true;
  const existing = await loadWidgetInRealm(ctx, id, realm);
  const status = paused ? 'paused' : statusForCode(existing.trigger_code);
  const projection = await writeRow(ctx, C.widgets, id, widgetRow({ ...existing, trigger_status: status }), existing);
  return outcome({ widget: { id, trigger_status: status } }, [projection]);
}
