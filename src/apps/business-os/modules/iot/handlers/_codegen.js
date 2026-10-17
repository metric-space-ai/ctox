// Origin: CTOX
// License: Apache-2.0
// Widget-Codegen über das Modell-Gateway der App-Laufzeit `ctx.model(req)` (vormals Agenten-Queue im Core).
// CAP-3b #844: model.run ist KEIN OS-Primitiv unter ctx.cap; das Modell kommt über ctx.model (#830-Bindung,
// Grant `model`). Fehlt die Bindung, wirft ctx.model typisiert `unavailable` (Zone-Meldung).
// ref: src/core/iot/commands.rs:1608-1771 (WidgetCodegenCtx, Prompts). Prompts sprechen jetzt den JSON-Wächter der App
// (siehe _iotcore.js „Wächter“) statt Rhai; das Zurückschreiben übernimmt der Handler selbst.
import { fail, isObject } from './_base.js';

/** Text aus einer Responses-förmigen Antwort ziehen. */
export function responseText(response) {
  if (typeof response === 'string') return response;
  if (!isObject(response)) return '';
  if (typeof response.output_text === 'string') return response.output_text;
  if (typeof response.text === 'string') return response.text;
  const parts = [];
  for (const item of Array.isArray(response.output) ? response.output : []) {
    for (const c of Array.isArray(item?.content) ? item.content : []) if (typeof c?.text === 'string') parts.push(c.text);
  }
  return parts.join('');
}

/** Code-Zäune entfernen. */
export const stripFence = (text) => text.trim().replace(/^```[a-zA-Z]*\s*\n?/, '').replace(/\n?```\s*$/, '').trim();

export async function generate(ctx, { kind, widgetId, signalRef, prompt }) {
  const response = await ctx.model({
    input: prompt,
    metadata: { kind, widget_id: widgetId, signal_ref: signalRef, skill: 'iot/operations' },
  });
  const code = stripFence(responseText(response));
  if (code === '') fail('unavailable', `model returned no code for widget ${widgetId}`);
  return code;
}

/** ref: commands.rs:1722-1748 build_trigger_codegen_prompt */
export function triggerPrompt(widgetId, w, cond) {
  const action = w.action_prompt ?? '(noch keine)';
  return 'Du bist CTOX und programmierst die Trigger-Logik (den Wächter) eines IoT-Automatisierungs-Widgets.\n\n'
    + `Widget-ID: ${widgetId}\n`
    + `Signal: ${w.signal_ref} (Form <asset_id>::<attribute_name>)\n`
    + `Bedingung (Wenn, Freitext): "${cond}"\n`
    + `Geplante Aktion (Dann): "${action}"\n\n`
    + 'Schreibe einen Wächter als JSON-Objekt {"when": <ausdruck>, "reason": "<Grund>", "for_ms"?: <ms>}. Er wird pro neuem '
    + 'Datenpunkt ausgewertet und meldet `reason`, sobald `when` gilt (bei for_ms erst, wenn es so lange ununterbrochen gilt).\n'
    + '- ausdruck: {"op":"gt|gte|lt|lte|eq|neq","left":<wert>,"right":<wert>} | {"op":"and|or","items":[…]} | {"op":"not","item":…}\n'
    + '- wert: Zahl | {"fn":"last|avg|min|max|count|rate|age_ms|has_data","window":"15m"} (rate pro Sekunde)\n'
    + 'Zeitfenster: ms/s/m/h/d. Kein Code, nur Daten.\n\n'
    + 'Gib nur das JSON-Objekt aus.';
}

/** ref: commands.rs:1750-1771 build_render_codegen_prompt */
export function renderPrompt(widgetId, w) {
  return 'Du bist CTOX und programmierst den Widget-Code (die Visualisierung) eines IoT-Automatisierungs-Widgets. '
    + 'Die Visualisierung ist dem Auftrag UNTERGEORDNET - schlicht, kein Grafana.\n\n'
    + `Widget-ID: ${widgetId}\n`
    + `Signal: ${w.signal_ref}\n`
    + `Bedingung (Wenn): "${w.cond_text ?? ''}"\n\n`
    + 'Schreibe den Rumpf einer JS-Funktion `render(host, api)`, die in das eigene Kachel-Element `host` rendert. '
    + 'Gesandboxte API (NUR diese): api.signal.last()/.window("15m")/.rate("15m"); api.draw.line/value/gauge/grid; api.fmt. '
    + 'KEIN Zugriff auf window/document/parent/fetch/eval/import. Halte es minimal (Wert + Sparkline genuegt meist).\n\n'
    + 'Gib nur den Code aus.';
}
