// Origin: CTOX
// License: Apache-2.0
// ctox.iot.widget.compile_trigger – ref: src/core/iot/commands.rs:2471-2477 (Pflichtfeld), :1676-1700 compile_trigger.
// Codegen über `model`; der erzeugte Wächter wird sofort geprüft (Selbstreparatur-Tor wie widget_upsert).
import { fail } from './_base.js';
import { generate, triggerPrompt } from './_codegen.js';
import { C, loadWidgetInRealm, outcome, requireField, sessionRealm, statusForCode, widgetRow, writeRow } from './_iotcore.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const id = requireField(command, 'widget_id', 'string');
  const widget = await loadWidgetInRealm(ctx, id, realm);
  const cond = widget.cond_text ?? '';
  if (cond.trim() === '') fail('invalid', `widget ${id} has no condition (Wenn) to compile`);
  const code = await generate(ctx, { kind: 'iot_trigger_code', widgetId: id, signalRef: widget.signal_ref, prompt: triggerPrompt(id, widget, cond) });
  const status = statusForCode(code);
  const projection = await writeRow(ctx, C.widgets, id, widgetRow({ ...widget, trigger_code: code, trigger_status: status }), widget);
  return outcome({ widget_id: id, kind: 'iot_trigger_code', trigger_status: status }, [projection]);
}
