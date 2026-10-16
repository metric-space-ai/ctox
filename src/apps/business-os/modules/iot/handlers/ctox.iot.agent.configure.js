// Origin: CTOX
// License: Apache-2.0
// ctox.iot.agent.configure – ref: src/core/iot/commands.rs:1773-1842 agent_configure, :1900-1964 project_agent(_status).
// Konfiguration und Status liegen in den iot-Collections; den Transport (MQTT/HTTP/WebSocket) betreibt der Core als
// generisches Primitiv net.ingest – die App meldet ihm nur den Endpunkt an (kein IoT-Fachmodell im Core).
import { fail } from './_base.js';
import { C, agentRow, agentStatusRow, dataObject, ensureRealm, generateId, outcome, parsePayload, sessionRealm, writeRow } from './_iotcore.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const req = parsePayload(command, { id: 'string?', realm: 'string', name: 'string', kind: 'string', enabled: 'bool=true', data: 'any' });
  const data = dataObject(req.data);
  const id = req.id ?? generateId(ctx);
  const existing = await ctx.get(C.agents, id);
  if (req.id) ensureRealm(existing, realm, `agent not found: ${id}`);
  // Webhook-Bindungen (kind 'webhook', ctox.iot.webhook.register) tragen Token-Digest und Idempotenzfenster.
  if (existing?.kind === 'webhook') fail('invalid', `iot agent ${id} is a webhook binding`);
  const projections = [await writeRow(ctx, C.agents, id, agentRow({ realm, name: req.name, kind: req.kind, enabled: req.enabled, data }), existing)];
  // Neuer Agent startet als „unconfigured“; bei bestehendem Status bleibt link_state (ON CONFLICT nur realm).
  const status = await ctx.get(C.agentStatus, id);
  projections.push(await writeRow(ctx, C.agentStatus, id, agentStatusRow({
    realm, agent_id: id, link_state: status?.link_state ?? 'unconfigured', last_event_ms: status?.last_event_ms, error: status?.error,
  }), status));
  await ctx.cap.net.ingest.subscribe({ endpoint: id, protocol: req.kind, enabled: req.enabled, config: data });
  return outcome({ agent: { id, realm, name: req.name, kind: req.kind, enabled: req.enabled } }, projections);
}
