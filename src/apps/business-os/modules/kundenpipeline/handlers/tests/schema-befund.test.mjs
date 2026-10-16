import test from 'node:test';
import handle from '../kundenpipeline.triage.write.js';
import { cmd, rejects } from './_ctx.mjs';
import { setup, vorgang } from './_fixtures.mjs';

// Befund B1: Rust-Ingest legt Vorgaenge mit triage_json/run_json = null an
// (decision_hub.rs :143-144). collections.schema.json erlaubt dort nur `object`.
// Unter v4-Schemapruefung scheitert dann JEDER Rueckschrieb eines solchen Vorgangs.
test('Befund B1: null in run_json macht den Vorgang unter v4 unschreibbar', async () => {
  const t = await setup({ seed: { kundenpipeline_vorgaenge: [vorgang({ triage_json: null, run_json: null })] } });
  await rejects(handle(cmd('kundenpipeline.triage.write', { vorgang_id: 'kpl-v-1', triage: {} }), t.ctx), 'invalid', /run_json/);
});
