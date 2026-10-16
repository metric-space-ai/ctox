// Origin: CTOX
// License: Apache-2.0
// Trigger-Handler (§16.20): on iot_attributes insert|update. Regelauswertung und Widget-Wächter der iot-App –
// vormals im Core: conditions.rs:702-1130 evaluate_event/emit_matches und widget_runtime.rs:181-360 tick_widget(s).
// command = {type:'trigger', trigger_id, change:{collection, op, doc}}. Fehler eines Wächters setzen nur dessen Widget
// auf needs_attention und brechen die Auswertung nie ab (wie im Core).
import { isObject } from './_base.js';
import {
  C, WATCHER_LOOKBACK_MS, createAlarm, evaluateRuleset, evaluateWatcher, widgetRow, writeRow,
} from './_iotcore.js';
import { fetchRange } from './_timeseries.js';

// ref: widget_runtime.rs:137-179 build_seed_prompt (wörtlich; nur der Skill-Name heißt jetzt iot/operations)
function seedPrompt(actionPrompt, signalRef, assetId, reasons, points, nowMs) {
  const prompt = (actionPrompt ?? '').trim() || 'Prüfe das ausgelöste Signal und handle angemessen.';
  const lastPoint = points[points.length - 1];
  const last = lastPoint ? `${lastPoint[1]} (vor ${Math.floor((nowMs - lastPoint[0]) / 1000)}s)` : '—';
  const tail = points.slice(-8).map(([, v]) => `${v}`).join(', ');
  return `${prompt}\n\n— Auslöser —\nSignal: ${signalRef}\nAsset: ${assetId}\nGrund: ${reasons.join('; ')}\n`
    + `Letztwert: ${last}\nLetzte Werte: ${tail}\n\nHandle gemäß Skill \`iot/operations\`. Die Überwachung läuft weiter.`;
}

// Regeln: je aktivierte Regel des Realms eine Auswertung gegen das Ereignis-Asset (Logik in _iotcore.evaluateRuleset).
async function evaluateRulesets(ctx, asset, event, nowMs, report) {
  const rulesets = await ctx.read(C.rulesets, { selector: { realm: asset.realm, enabled: true } });
  for (const ruleset of rulesets) await evaluateRuleset(ctx, ruleset, asset, event, nowMs, report);
}

async function tickWidgets(ctx, asset, event, nowMs, report) {
  const signalRef = `${asset.id}::${event.attribute_name}`;
  const widgets = await ctx.read(C.widgets, { selector: { signal_ref: signalRef, realm: asset.realm } });
  let points = null;
  for (const widget of widgets) {
    const code = widget.trigger_code ?? '';
    if (code.trim() === '' || widget.trigger_status === 'paused') continue;
    if (points === null) {
      const res = await fetchRange(ctx, asset.id, event.attribute_name, nowMs - WATCHER_LOOKBACK_MS, nowMs);
      points = res.rows.filter((r) => typeof r.value === 'number' || typeof r.value === 'boolean')
        .map((r) => [r.timestamp_ms, typeof r.value === 'boolean' ? (r.value ? 1 : 0) : r.value]);
    }
    const result = evaluateWatcher(code, points, nowMs, widget.trigger_state);
    const status = result.error ? 'needs_attention' : result.fired.length ? 'fired' : 'armed';
    await writeRow(ctx, C.widgets, widget.id, widgetRow({ ...widget, trigger_status: status, trigger_state: result.state }), widget);
    if (!result.fired.length) continue;
    const alarm = await createAlarm(ctx, {
      realm: widget.realm, title: `IoT-Auftrag ausgelöst: ${signalRef}`, content: result.fired.join('; '), severity: 'Medium',
      source: widget.realm.trim() === '' ? 'GlobalRuleset' : 'RealmRuleset', sourceId: widget.id, assetIds: [asset.id],
    });
    // §16.30: Skill aus dem eigenen Paket; Ergebnis als Rückruf an den Alarm (ctox.iot.task.complete/.failed).
    await ctx.cap.agent.task.enqueue({
      title: `IoT-Auftrag ausgelöst: ${signalRef}`, instruction: seedPrompt(widget.action_prompt, signalRef, asset.id, result.fired, points, nowMs),
      skill: 'iot/operations', thread_key: `iot-widget:${widget.id}`, payload: { alarm_id: alarm.id, widget_id: widget.id, signal_ref: signalRef },
      idempotency_key: `iot-alarm:${alarm.id}`, on_complete: 'ctox.iot.task.complete', on_failed: 'ctox.iot.task.failed',
    });
    report.widgets.push(widget.id);
  }
}

export default async function handle(command, ctx) {
  const doc = command?.change?.doc;
  const report = { rulesets: [], otherwise: [], widgets: [], alarms: [] };
  if (command?.change?.collection !== C.attributes || !isObject(doc) || typeof doc.asset_id !== 'string') return report;
  const assetRow = await ctx.get(C.assets, doc.asset_id);
  if (!assetRow || !isObject(assetRow.data)) return report;
  const asset = assetRow.data;
  const event = { attribute_name: doc.attribute_name, value: doc.data?.value ?? null, old_value: doc.prev_value ?? null };
  const nowMs = ctx.now();
  await evaluateRulesets(ctx, asset, event, nowMs, report);
  await tickWidgets(ctx, asset, event, nowMs, report);
  return report;
}
