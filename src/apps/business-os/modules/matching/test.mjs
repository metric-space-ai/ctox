import assert from 'node:assert/strict';
import { afterEach, test } from 'node:test';

import {
  getContactsCollection,
  getMatchingCollectionDiagnostics,
  setBusinessOsDatabaseContext
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

function businessOsContext({ requirements = [], objects = [], matches = [] } = {}, permissions = {}) {
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
    }
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

test('reports shell-facade collection diagnostics', async () => {
  setBusinessOsDatabaseContext(businessOsContext());
  const diagnostics = await getMatchingCollectionDiagnostics({ probePull: true });

  assert.equal(diagnostics.databaseName, 'business-os-shell-facade');
  assert.equal(diagnostics.collections.length, 3);
  assert.deepEqual(
    diagnostics.collections.map(item => [item.collection, item.localCount, item.pull.count]),
    [
      ['matching_requirements', 0, 0],
      ['matching_objects', 0, 0],
      ['matching_results', 0, 0]
    ]
  );
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
