// Strang OUT3: Registry-Lesesicht über die OLG-Collections (foreign read) statt Core-Fähigkeit scrape.registry.read.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.research_source.registry_read.js';

const NOW = Date.UTC(2026, 9, 15, 12);
const data = () => ({
  outbound_lead_generation_scrape_targets: {
    northdata: { id: 'northdata', target_key: 'northdata', target_id: 't1', display_name: 'North Data', status: 'active', target_kind: 'prospect-research', start_url: 'https://northdata.de', latest_script_revision_no: 4, updated_at: '2026-10-15T10:00:00.000Z' },
    dnb: { id: 'dnb', target_key: 'dnb', target_id: 't2', status: 'active', updated_at: '2026-10-15T11:00:00.000Z' },
  },
  outbound_lead_generation_scrape_runs: {
    r1: { id: 'r1', run_id: 'r1', target_id: 't1', status: 'succeeded', started_at: '2026-10-14T10:00:00Z', result: { detail: 'x'.repeat(400) } },
    r2: { id: 'r2', run_id: 'r2', target_id: 't1', status: 'failed', started_at: '2026-10-15T10:00:00Z', result: { failure_mode: 'portal_drift' } },
  },
  outbound_lead_generation_scrape_account_states: {
    dnb: { id: 'dnb', target_id: 't2', reason: 'login rejected', failed_probes: 2, next_probe_at_ms: NOW + 1000, probe_lease_owner: 'cmd', probe_lease_until_ms: NOW, updated_at_ms: 5 },
  },
  outbound_lead_generation_scrape_scripts: {
    'dnb:r1': { id: 'dnb:r1', target_key: 'dnb', revision_no: 1, script_ref: 'ref-1', script_sha256: 'aa', language: 'javascript' },
    'dnb:r2': { id: 'dnb:r2', target_key: 'dnb', revision_no: 2, script_ref: 'ref-2', script_sha256: 'bb', language: 'javascript' },
  },
});

test('projects OLG registry targets (updated_at DESC) with runs and account state, filters keys; no core capability', async () => {
  // Strang OLG725: keine files-Attrappe mehr – outbound darf OLG-Dateien nicht lesen (Kern §16.47 Nr. 4).
  const t = createTestCtx({ now: NOW, data: data() });
  const result = await handle(cmd('outbound.research_source.registry_read', { target_keys: ['northdata'] }), t.ctx);
  assert.equal(result.schema, 'ctox.outbound.research_source_registry.v1');
  assert.equal(result.targets.length, 1);
  assert.equal(result.targets[0].last_run.run_id, 'r2');
  assert.equal(result.targets[0].last_run.failure_mode, 'portal_drift');
  assert.equal(result.targets[0].last_successful_run.run_id, 'r1');
  assert.equal(result.targets[0].last_successful_run.detail.length, 300, 'Texte ≤ 300 Zeichen (Rust :1357)');
  assert.equal(result.script, null);
  assert.equal(t.calls.caps.length, 0);
  assert.equal(t.calls.commands.length, 0, 'ohne include_script_target kein Befehl an OLG');
  const all = await handle(cmd('outbound.research_source.registry_read', { include_script_target: 'dnb' }), t.ctx);
  assert.deepEqual(all.targets.map((x) => x.target_key), ['dnb', 'northdata']);
  assert.equal(all.targets[0].account_state.status, 'provider_account_inactive');
  assert.equal(all.targets[0].account_state.probe_in_progress, false, 'Lease bis genau jetzt ist abgelaufen (INV-SCR4)');
  assert.deepEqual(t.calls.caps, [], 'kein files.get (und keine andere Fähigkeit) auf OLG-Dateien');
  assert.equal(t.calls.commands.length, 1);
  const [sent] = t.calls.commands;
  assert.equal(sent.type, 'outbound.scrape.script.read');
  assert.deepEqual(sent.payload, { target_key: 'dnb' });
  assert.equal(sent.options.module, 'outbound-lead-generation');
  assert.equal(sent.options.on_complete, 'outbound.research_source.script_read.done');
  assert.equal(sent.options.on_failed, 'outbound.research_source.script_read.done');
  assert.match(sent.options.id, /:script$/);
  assert.deepEqual(all.script, { target_key: 'dnb', status: 'requested', command_id: sent.options.id });
  assert.equal('text' in all.script, false, 'der Skripttext kommt nur über den Rückruf');
});

test('include_script_target: OLG lehnt ab → typisierter Fehler unavailable', async () => {
  const t = createTestCtx({ data: data(), commandResult: ({ id }) => ({ id, status: 'rejected', reason: 'forbidden/commands_in' }) });
  const error = await rejects(handle(cmd('outbound.research_source.registry_read', { include_script_target: 'dnb' }), t.ctx));
  assert.equal(error.code, 'unavailable');
  assert.match(error.message, /outbound\.scrape\.script\.read: forbidden\/commands_in/);
});

test('include_script_target of an unknown target is not_found (Rust :1420)', async () => {
  const t = createTestCtx({ data: data() });
  const error = await rejects(handle(cmd('outbound.research_source.registry_read', { include_script_target: 'nope' }), t.ctx));
  assert.equal(error.code, 'not_found');
  assert.equal(error.message, 'scrape target nope could not be read: target not found');
  assert.equal(t.calls.commands.length, 0, 'kein Befehl für ein unbekanntes Ziel');
});
