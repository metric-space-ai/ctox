import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { afterEach, test } from 'node:test';

import { renderListOrState } from '../../shared/list-state.js';
import {
  getContactsCollection,
  getMatchingCollectionReadiness,
  setBusinessOsDatabaseContext,
  subscribeMatchingCollectionReadiness
} from './ui/businessOsDataSource.js';

function row(doc) {
  return { toJSON: () => structuredClone(doc) };
}

function collection(rows = []) {
  return {
    find() {
      return { exec: async () => rows.map(row) };
    },
    findOne(idOrQuery) {
      return {
        exec: async () => {
          const id = typeof idOrQuery === 'string' ? idOrQuery : idOrQuery?.selector?.id;
          const found = rows.find(item => item.id === id) || null;
          return found ? row(found) : null;
        }
      };
    },
    upsert: async (doc) => {
      rows.push(doc);
      return row(doc);
    }
  };
}

function businessOsContext({ requirements = [], objects = [], matches = [] } = {}, permissions = {}, sync = null) {
  const collections = {
    matching_requirements: collection(requirements),
    matching_objects: collection(objects),
    matching_results: collection(matches),
  };
  return {
    db: {
      collection: (name) => collections[name] || null
    },
    permissions: {
      canReadCollection: permissions.canReadCollection || (() => true),
      canWriteCollection: permissions.canWriteCollection || (() => true)
    },
    ...(sync ? { sync } : {})
  };
}

afterEach(() => {
  setBusinessOsDatabaseContext(null);
});

test('normalizes canonical matching requirement records for UI queries', async () => {
  setBusinessOsDatabaseContext(businessOsContext({
    requirements: [{
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
  }));

  const { database } = await getContactsCollection();
  const requirements = await database.requirements.find().exec();
  const sources = await database.sources.find().exec();

  assert.equal(requirements.length, 1);
  assert.equal(requirements[0].id, 'req-1');
  assert.equal(requirements[0].sourceId, 'src-crm');
  assert.equal(requirements[0].sourceName, 'CRM GmbH');
  assert.equal(sources.length, 0);
});

test('reads and subscribes through the canonical collection readiness facade', () => {
  const snapshots = {
    matching_requirements: Object.freeze({
      collection: 'matching_requirements',
      state: 'catching-up',
      ready: false,
      syncing: true,
      updatedAt: '2026-07-27T10:00:00.000Z'
    })
  };
  let unsubscribed = false;
  const sync = {
    collectionReadiness: (name) => snapshots[name],
    subscribeCollectionReadiness: (name, listener) => {
      listener(snapshots[name]);
      return () => { unsubscribed = true; };
    }
  };
  setBusinessOsDatabaseContext(businessOsContext({}, {}, sync));

  assert.equal(getMatchingCollectionReadiness('matching_requirements'), snapshots.matching_requirements);
  const emitted = [];
  const unsubscribe = subscribeMatchingCollectionReadiness('matching_requirements', (snapshot) => emitted.push(snapshot));
  assert.deepEqual(emitted, [snapshots.matching_requirements]);
  unsubscribe();
  assert.equal(unsubscribed, true);
});

test('renders unready replicated empties as syncing and ready empties as empty', () => {
  const options = {
    empty: 'No requirements available yet.',
    syncing: 'Requirements are syncing.'
  };
  const syncingHtml = renderListOrState([], { ready: false, syncing: true }, options);
  const emptyHtml = renderListOrState([], { ready: true, syncing: false }, options);

  assert.match(syncingHtml, /class="ctox-syncing"/);
  assert.match(syncingHtml, /Requirements are syncing\./);
  assert.match(emptyHtml, /class="ctox-empty"/);
  assert.match(emptyHtml, /No requirements available yet\./);
});

test('matching UI uses canonical readiness subscriptions and list states', async () => {
  const source = await readFile(new URL('./ui/index.js', import.meta.url), 'utf8');
  assert.match(source, /subscribeMatchingCollectionReadiness/);
  assert.match(source, /matchingCollectionReadiness\(collectionName\)/);
  assert.match(source, /renderListOrState/);
});

test('matching transient dialogs and menus stay inside the module host', async () => {
  const source = await readFile(new URL('./ui/index.js', import.meta.url), 'utf8');
  assert.match(source, /function getScopedMatchingHost\(\)/);
  assert.match(source, /syncFeedback\.setHostRoot\?\.\(matchingHost\)/);
  assert.doesNotMatch(source, /Matching wird synchronisiert|Sync fehlgeschlagen|Datenänderung übernommen|siehe Konsole/);
  assert.match(source, /__reloadRetryDelayMs = Math\.min\(60_000, delay \* 2\)/);
  assert.match(source, /root\.appendChild\(menu\)/);
  assert.match(source, /root\.append\(layer\)/);
  assert.doesNotMatch(source, /document\.body\.append(?:Child)?\((?:menu|layer)\)/);
});

test('denies writes when the Business OS permission facade denies collection writes', async () => {
  setBusinessOsDatabaseContext(businessOsContext({}, {
    canWriteCollection: () => false
  }));

  const { database } = await getContactsCollection();
  await assert.rejects(
    () => database.requirements.insert({ id: 'req-write-denied', title: 'Denied' }),
    /permission denied/
  );
});
