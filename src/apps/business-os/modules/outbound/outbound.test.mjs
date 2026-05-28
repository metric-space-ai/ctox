import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});

const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __outboundTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

test('campaign scope recovers existing outbound rows for the only visible campaign', () => {
  const scoped = hooks.campaignScopedRows({
    campaigns: [{ id: 'outbound_default_campaign', name: 'Outbound Firmenqualifizierung' }],
    sources: [{ id: 'src-1', campaign_id: 'legacy-campaign', title: 'Legacy import' }],
    companies: [{ id: 'co-1', campaign_id: 'legacy-campaign', name: 'Acme GmbH' }],
    pipeline: [{ id: 'pipe-1', campaign_id: 'legacy-campaign', company_id: 'co-1', company_name: 'Acme GmbH' }],
  }, 'outbound_default_campaign');

  assert.equal(scoped.recovered, true);
  assert.deepEqual(scoped.companies.map((item) => item.id), ['co-1']);
  assert.deepEqual(scoped.pipeline.map((item) => item.id), ['pipe-1']);
});

test('campaign scope does not mix unrelated rows when direct campaign data exists', () => {
  const scoped = hooks.campaignScopedRows({
    campaigns: [
      { id: 'camp-a', name: 'A' },
      { id: 'camp-b', name: 'B' },
    ],
    sources: [],
    companies: [
      { id: 'co-a', campaign_id: 'camp-a', name: 'A GmbH' },
      { id: 'co-b', campaign_id: 'camp-b', name: 'B GmbH' },
    ],
    pipeline: [
      { id: 'pipe-a', campaign_id: 'camp-a', company_id: 'co-a', company_name: 'A GmbH' },
      { id: 'pipe-b', campaign_id: 'camp-b', company_id: 'co-b', company_name: 'B GmbH' },
    ],
  }, 'camp-a');

  assert.deepEqual(scoped.companies.map((item) => item.id), ['co-a']);
  assert.deepEqual(scoped.pipeline.map((item) => item.id), ['pipe-a']);
});

test('outbound import validation requires source-specific input', () => {
  assert.equal(hooks.validateOutboundImportPayload({ title: '', source_type: 'text', source: { text: 'Acme' } }).valid, false);
  assert.equal(hooks.validateOutboundImportPayload({ title: 'Import', source_type: 'text', source: { text: '' } }).valid, false);
  assert.equal(hooks.validateOutboundImportPayload({ title: 'Import', source_type: 'url', source: { url: 'not-a-url' } }).valid, false);
  assert.equal(hooks.validateOutboundImportPayload({ title: 'Import', source_type: 'excel', source: { files: [] } }).valid, false);
  assert.equal(hooks.validateOutboundImportPayload({ title: 'Import', source_type: 'excel', source: { files: [{ name: 'companies.csv' }] } }).valid, true);
});
