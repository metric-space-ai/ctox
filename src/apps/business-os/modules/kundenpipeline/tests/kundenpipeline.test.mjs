import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import { collections } from '../schema.js';
import { collections as coreCollections } from '../../ctox/schema.js';
import {
  __kundenpipelineTestHooks as hooks,
  decisionCommand,
} from '../index.js';

test('Decision Hub manifest is an isolated shell-v2 module', async () => {
  const manifest = JSON.parse(await readFile(new URL('../module.json', import.meta.url), 'utf8'));
  assert.equal(manifest.id, 'kundenpipeline');
  assert.equal(manifest.entry, 'modules/kundenpipeline/index.html');
  assert.equal(manifest.layout.shell_contract, 'v2');
  assert.equal(manifest.layout.icon_asset, 'modules/kundenpipeline/assets/icon/decision-hub-256.png');
  assert.deepEqual(manifest.collections, [
    'kundenpipeline_vorgaenge',
    'kundenpipeline_entscheidungen',
    'kundenpipeline_projekte',
    'business_commands',
  ]);
});
test('Decision Hub raster icon hash matches checked-in provenance', async () => {
  const icon = await readFile(new URL('../assets/icon/decision-hub-256.png', import.meta.url));
  const provenance = JSON.parse(await readFile(new URL('../assets/icon/provenance.json', import.meta.url), 'utf8'));
  const derivative = provenance.derivatives.find((item) => item.path === 'decision-hub-256.png');
  assert.ok(derivative);
  assert.equal(createHash('sha256').update(icon).digest('hex'), derivative.sha256);
});

test('module context stays on the shell data and command boundaries', async () => {
  const source = await readFile(new URL('../index.js', import.meta.url), 'utf8');
  assert.match(source, /state\.ctx\?\.db\?\.collection/);
  assert.match(source, /state\.ctx\?\.commandBus\?\.dispatch/);
  assert.match(source, /kundenpipeline\.decision\.resolve/);
  assert.match(source, /kundenpipeline\.decision\.answer/);
  assert.doesNotMatch(source, /\/api\/business-os/);
});

test('Decision Hub declares the native projection collections', () => {
  assert.deepEqual(Object.keys(collections), [
    'kundenpipeline_vorgaenge',
    'kundenpipeline_entscheidungen',
    'kundenpipeline_projekte',
    'business_commands',
  ]);
  assert.equal(collections.kundenpipeline_entscheidungen.primaryKey, 'id');
  assert.ok(collections.kundenpipeline_entscheidungen.indexes.includes('status'));
  assert.ok(collections.kundenpipeline_vorgaenge.indexes.includes('kunde_id'));
  assert.equal(collections.business_commands.version, 2);
  assert.deepEqual(collections.business_commands, coreCollections.business_commands);
});

test('decision cards filter open items and retain agent options', () => {
  const decisions = [
    { id: 'open', titel: 'Projektwahl', typ: 'agent_escalation', status: 'offen', created_at_ms: 1, aktionen_json: [{ wert: 'yes', label: 'Ja' }] },
    { id: 'done', title: 'Erledigt', status: 'entschieden', created_at_ms: 2 },
    { id: 'deleted', title: 'Nicht sichtbar', status: 'offen', is_deleted: true, created_at_ms: 3 },
  ];
  const open = hooks.filterDecisions(decisions, { filter: 'open' });
  assert.deepEqual(open.map((item) => item.id), ['open']);
  assert.deepEqual(hooks.normalizeDecision(decisions[0]).options, [{ wert: 'yes', label: 'Ja' }]);
  assert.deepEqual(hooks.filterDecisions(decisions, { filter: 'all', search: 'erledigt' }).map((item) => item.id), ['done']);
});

test('agent escalation resolves through the native decision command', () => {
  const command = decisionCommand({
    id: 'decision-1',
    typ: 'agent_escalation',
    status: 'offen',
  }, 'option-a');
  assert.equal(command.module, 'kundenpipeline');
  assert.equal(command.command_type, 'kundenpipeline.decision.resolve');
  assert.equal(command.record_id, 'decision-1');
  assert.deepEqual(command.payload, {
    entscheidung_id: 'decision-1',
    option_id: 'option-a',
    comment: '',
    kanal: 'desktop',
  });
});

test('legacy customer decisions use the native answer command', () => {
  const command = decisionCommand({
    id: 'decision-2',
    vorgang_id: 'case-2',
    typ: 'zuordnung',
    status: 'offen',
  }, 'annehmen');
  assert.equal(command.command_type, 'kundenpipeline.decision.answer');
  assert.deepEqual(command.payload, {
    entscheidung_id: 'decision-2',
    vorgang_id: 'case-2',
    wert: 'annehmen',
    kanal: 'desktop',
  });
});
