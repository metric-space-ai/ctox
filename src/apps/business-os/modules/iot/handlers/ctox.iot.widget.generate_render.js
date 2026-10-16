// Origin: CTOX
// License: Apache-2.0
// ctox.iot.widget.generate_render – ref: src/core/iot/commands.rs:2478-2484 (Pflichtfeld), :1702-1720 generate_render.
// Codegen über `model`; render_code ist opaker Code, gesandboxt beim Rendern.
import { generate, renderPrompt } from './_codegen.js';
import { C, loadWidgetInRealm, outcome, requireField, sessionRealm, widgetRow, writeRow } from './_iotcore.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const id = requireField(command, 'widget_id', 'string');
  const widget = await loadWidgetInRealm(ctx, id, realm);
  const code = await generate(ctx, { kind: 'iot_render_code', widgetId: id, signalRef: widget.signal_ref, prompt: renderPrompt(id, widget) });
  const projection = await writeRow(ctx, C.widgets, id, widgetRow({ ...widget, render_code: code }), widget);
  return outcome({ widget_id: id, kind: 'iot_render_code' }, [projection]);
}
