// matching – Datenfassade und Quelltext-Wächter auf dem App-API-4.0-Vertrag (V4-PORT.md).
// Lauf: node --test src/apps/business-os/modules/matching/test.mjs
// Kern-Stub: CTOX_APP_API_STUB (Pin), sonst ~/.local/state/shell-v4/stub-pins/64acc2c.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { afterEach, test } from 'node:test';

import { getContactsCollection, setMatchingDataApi, disposeMatchingData } from './ui/businessOsDataSource.js';
import { realManifestRoutes } from '../../v4/tools/port/real-manifest.mjs';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
const { createStubKernel } = await import(pathToFileURL(join(STUB, 'ctox-app-api-stub.mjs')).href);

const ROUTES = realManifestRoutes('matching');
let n = 0;
async function kernelWith(docs = {}) {
  const kernel = createStubKernel({ storage: 'memory', latencyMs: 2, multiTab: false, instanceId: `matching-${process.pid}-${++n}` });
  await kernel.ready;
  // PIN-ALL (E48): echte Routen/Rechte/Collections aus v4/manifests/matching.manifest.json veröffentlichen, damit der Kern
  // Lesen und Schreiben (Schema) wie in Produktion prüft.
  await kernel.sim.push('business_apps', [{ id: 'matching', title: 'Matching' }]);
  for (const o of ROUTES._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
  await kernel.sim.publishAppVersion('matching', '1.0.2', { title: 'Matching', manifestExtra: ROUTES });
  for (const [collection, rows] of Object.entries(docs)) await kernel.sim.push(collection, rows);
  await kernel.sim.settle();
  return kernel;
}

afterEach(() => disposeMatchingData());

test('normalizes canonical matching requirement records for UI queries', async () => {
  const kernel = await kernelWith({
    matching_requirements: [{
      id: 'row-1',
      kind: 'requirement',
      title: 'Senior CRM Consultant',
      data: {
        source: { id: 'src-crm', name: 'CRM GmbH' },
        requirement: { id: 'req-1', title: 'Senior CRM Consultant' },
        requirementSource: { rawText: 'CRM migration project' }
      },
      status: 'active',
      updated_at_ms: 1
    }]
  });
  setMatchingDataApi(kernel.appApi('matching', '1.0.2'));

  const { database } = await getContactsCollection();
  const requirements = await database.requirements.all();
  const sources = await database.sources.all();

  assert.equal(requirements.length, 1);
  assert.equal(requirements[0].id, 'req-1');
  assert.equal(requirements[0].sourceId, 'src-crm');
  assert.equal(requirements[0].sourceName, 'CRM GmbH');
  assert.equal(sources.length, 0);
  await kernel.close();
});

test('reads both the handler envelope {data} and flat documents', async () => {
  const kernel = await kernelWith({
    matching_objects: [
      { id: 'obj-env', kind: 'object', title: 'Ada', status: 'active', data: { object: { id: 'obj-env', name: 'Ada Lovelace', skills: ['Math'] } }, updated_at_ms: 2 },
      { id: 'obj-flat', kind: 'object', title: 'Bob', status: 'active', data: { id: 'obj-flat', name: 'Bob Flat' }, updated_at_ms: 1 },
    ]
  });
  setMatchingDataApi(kernel.appApi('matching', '1.0.2'));
  const { database } = await getContactsCollection();
  const objects = await database.objects.all();
  assert.deepEqual(objects.map((o) => o.name), ['Ada Lovelace', 'Bob Flat']);
  assert.deepEqual(objects[0].skills, ['Math']);
  await kernel.close();
});

test('live changes reach onChange listeners with the changed id; no readiness API', async () => {
  const kernel = await kernelWith({});
  setMatchingDataApi(kernel.appApi('matching', '1.0.2'));
  const { database } = await getContactsCollection();
  const events = [];
  const off = database.objects.onChange((event) => events.push(event));
  await kernel.sim.push('matching_objects', [{ id: 'obj-live', kind: 'object', title: 'Live', status: 'active', data: { name: 'Live' }, updated_at_ms: 5 }]);
  await kernel.sim.settle();
  assert.ok(events.some((event) => event.documentData?.id === 'obj-live'));
  off();
  const source = await readFile(new URL('./ui/businessOsDataSource.js', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /collectionReadiness|subscribeCollectionReadiness|ctx\.db/);
  await kernel.close();
});

test('writes go through api.put as schema envelope and removals through api.remove', async () => {
  const kernel = await kernelWith({});
  const api = kernel.appApi('matching', '1.0.2');
  setMatchingDataApi(api);
  const { database } = await getContactsCollection();
  await database.requirements.save({ id: 'req-w', title: 'Neu', sourceId: 'src-1' });
  await kernel.sim.settle();
  const stored = api.get('matching_requirements', 'req-w').get();
  assert.equal(stored.kind, 'requirement');
  assert.equal(stored.title, 'Neu');
  assert.equal(stored.status, 'active');
  assert.equal(stored.data.sourceId, 'src-1');
  assert.equal(typeof stored.updated_at_ms, 'number');
  const doc = await database.requirements.one({ selector: { id: 'req-w' } });
  await doc.remove();
  await kernel.sim.settle();
  assert.equal(api.get('matching_requirements', 'req-w').get(), null);
  await kernel.close();
});

test('matching UI renders empty lists without state text and has no readiness subscriptions', async () => {
  const source = await readFile(new URL('./ui/index.js', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /subscribeMatchingCollectionReadiness|renderListOrState|syncFeedback|setInterval\(/);
});

test('matching transient dialogs and menus stay inside the module host', async () => {
  const source = await readFile(new URL('./ui/index.js', import.meta.url), 'utf8');
  assert.match(source, /function getScopedMatchingHost\(\)/);
  assert.match(source, /root\.appendChild\(menu\)/);
  // Anlegen „Neue Anforderung“ ist ein Blatt über api.shell.dialog (matchingPrompt), keine V2-Dialogschicht.
  assert.match(source, /async function openCreateRequirementForm[\s\S]{0,600}matchingPrompt\('Titel'/);
  assert.doesNotMatch(source, /business-dialog-layer/);
  assert.doesNotMatch(source, /document\.body\.append(?:Child)?\((?:menu|layer)\)/);
});

test('facade exposes only the three contract collections', async () => {
  const kernel = await kernelWith({});
  setMatchingDataApi(kernel.appApi('matching', '1.0.2'));
  const { database } = await getContactsCollection();
  // Fremde Collections gibt es in der Fassade nicht; Schreibrechte entscheidet der Core (forbidden).
  assert.equal(database.contacts, undefined);
  const source = await readFile(new URL('./ui/businessOsDataSource.js', import.meta.url), 'utf8');
  for (const name of ['matching_requirements', 'matching_objects', 'matching_results']) assert.match(source, new RegExp(name));
  await kernel.close();
});
