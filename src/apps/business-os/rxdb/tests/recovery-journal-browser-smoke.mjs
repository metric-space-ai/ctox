import http from 'node:http';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '../../node_modules/playwright/index.mjs';

const testDir = dirname(fileURLToPath(import.meta.url));
const bundle = readFileSync(resolve(testDir, '../dist/ctox-rxdb-js.mjs'));
const server = http.createServer((request, response) => {
  if (request.url === '/bundle.mjs') {
    response.writeHead(200, { 'content-type': 'text/javascript' });
    response.end(bundle);
    return;
  }
  response.writeHead(200, { 'content-type': 'text/html' });
  response.end('<!doctype html><title>recovery journal smoke</title>');
});
await new Promise((resolveReady) => server.listen(0, '127.0.0.1', resolveReady));
const { port } = server.address();
const systemChrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const browser = await chromium.launch({
  headless: true,
  ...(existsSync(systemChrome) ? { executablePath: systemChrome } : {}),
});

try {
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${port}/`);
  const result = await page.evaluate(async () => {
    const { openCtoxIndexedDbStorage, openRecoveryJournal } = await import('/bundle.mjs');
    const databaseName = `recovery-journal-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const journal = await openRecoveryJournal({ databaseName, instanceId: 'instance-a' });
    const replayed = [];
    journal.registerCollection('tickets', {
      schemaHash: 'schema-a',
      applyBatch: async (batch) => {
        replayed.push(structuredClone(batch.rows));
        const success = {};
        for (const doc of batch.rows) success[doc.id] = doc;
        return { success };
      },
    });
    const doc = { id: 'ticket-1', title: 'offline', _meta: { ctoxHlc: 'abc:0:tab-a' } };
    const batchId = await journal.appendBatch({
      collection: 'tickets',
      schemaHash: 'schema-a',
      operation: 'upsert',
      rows: [doc],
    });
    const pendingBeforeReplay = await journal.getStatus();
    const replay = await journal.replayRegisteredCollections();
    const batchAfterReplay = (await journal.listBatches('pending')).find((entry) => entry.batchId === batchId);
    await journal.markMasterAcknowledged('tickets', { 'ticket-1': doc });
    const pendingAfterAck = await journal.getStatus();

    const command = {
      id: 'cmd-1',
      command_id: 'cmd-1',
      command_type: 'example.record.update',
      module: 'example',
      payload: { person_id: 36833, department: '' },
      status: 'pending',
      _meta: { ctoxHlc: 'command-local:0:tab-a' },
    };
    await journal.appendBatch({
      collection: 'business_commands',
      schemaHash: 'commands-a',
      operation: 'upsert',
      rows: [command],
    });
    const commandBatch = (await journal.listBatches('pending'))
      .find((entry) => entry.documentIds?.includes('cmd-1'));
    await journal.commitBatch(commandBatch.batchId, { 'cmd-1': command });
    await journal.markMasterAcknowledged('business_commands', {
      'cmd-1': {
        ...command,
        status: 'completed',
        payload: { ...command.payload, inbound_channel: 'example' },
        result: { ok: true },
        _meta: { ctoxHlc: 'command-native:0:native' },
      },
    });
    const pendingAfterCommandAck = await journal.getStatus();

    await journal.appendBatch({
      collection: 'tickets',
      schemaHash: 'schema-a',
      operation: 'upsert',
      rows: [{ id: 'ticket-2', title: 'export me' }],
    });
    const exported = await journal.export('correct horse battery staple');
    const artifactText = await exported.blob.text();
    const preview = await journal.previewImport(artifactText, 'correct horse battery staple');
    const other = await openRecoveryJournal({ databaseName: `${databaseName}-other`, instanceId: 'instance-b' });
    let mismatchCode = '';
    try {
      await other.previewImport(artifactText, 'correct horse battery staple');
    } catch (error) {
      mismatchCode = error?.code || '';
    }
    other.close();

    const storageName = `${databaseName}-storage`;
    const storage = await openCtoxIndexedDbStorage({ databaseName: storageName });
    const tickets = storage.collection('tickets', {
      schema: {
        version: 0,
        type: 'object',
        primaryKey: 'id',
        properties: {
          id: { type: 'string', maxLength: 128 },
          title: { type: 'string' },
        },
        required: ['id'],
      },
    });
    await tickets.bulkUpsert([{ id: 'ticket-storage-1', title: 'journal before primary' }]);
    const storageBatch = (await storage.recoveryJournal.listBatches('pending'))[0];
    const storedDocument = await tickets.findOne('ticket-storage-1');
    const structuredConflict = await storage.recoveryJournal.recordConflict({
      collection: 'tickets',
      base: { id: 'ticket-conflict-1', title: 'base' },
      local: { id: 'ticket-conflict-1', title: 'local wins' },
      master: { id: 'ticket-conflict-1', title: 'native' },
    });
    await storage.recoveryJournal.resolveConflict(structuredConflict.conflictId, 'keep_local');
    const localResolutionBatch = (await storage.recoveryJournal.listBatches('pending'))
      .find((batch) => batch.documentIds?.includes('ticket-conflict-1'));
    const deleteConflict = await storage.recoveryJournal.recordConflict({
      collection: 'tickets',
      conflictType: 'delete_vs_update',
      local: { id: 'ticket-deleted-1', title: 'recover me' },
      master: { id: 'ticket-deleted-1', _deleted: true },
    });
    let tombstoneKeepLocalCode = '';
    try {
      await storage.recoveryJournal.resolveConflict(deleteConflict.conflictId, 'keep_local');
    } catch (error) {
      tombstoneKeepLocalCode = error?.code || '';
    }
    await storage.recoveryJournal.resolveConflict(deleteConflict.conflictId, 'restore_as_copy');
    const unresolvedAfterResolution = await storage.recoveryJournal.listConflicts();
    const recoveredCopyBatch = (await storage.recoveryJournal.listBatches('pending'))
      .find((batch) => batch.documentIds?.some((id) => id.startsWith('ticket-deleted-1-recovered-')));
    tickets.close();
    storage.close();

    const restartStorageName = `${databaseName}-restart-storage`;
    const commandSchema = {
      version: 0,
      type: 'object',
      primaryKey: 'id',
      properties: {
        id: { type: 'string', maxLength: 128 },
        command_id: { type: 'string' },
        command_type: { type: 'string' },
        module: { type: 'string' },
        payload: { type: 'object', additionalProperties: true },
        status: { type: 'string' },
      },
      required: ['id'],
    };
    const restartStorage = await openCtoxIndexedDbStorage({ databaseName: restartStorageName });
    const commands = restartStorage.collection('business_commands', { schema: commandSchema });
    const restartCommand = {
      id: 'cmd-restart', command_id: 'cmd-restart', command_type: 'example.record.update',
      module: 'example', payload: { person_id: 36833 }, status: 'pending',
    };
    await commands.bulkUpsert([restartCommand]);
    await commands._bulkUpsertOnce([{ ...restartCommand, status: 'completed' }], {
      replicationOrigin: { role: 'native', peerId: 'native-a' },
    });
    commands.close();
    restartStorage.close();
    const reopenedStorage = await openCtoxIndexedDbStorage({ databaseName: restartStorageName });
    const reopenedCommands = reopenedStorage.collection('business_commands', { schema: commandSchema });
    await reopenedCommands.initializeRecovery();
    const pendingAfterRestartReconciliation = await reopenedStorage.recoveryJournal.getStatus();
    reopenedCommands.close();
    reopenedStorage.close();
    journal.close();
    indexedDB.deleteDatabase(`${databaseName}__recovery_v2`);
    indexedDB.deleteDatabase(`${databaseName}-other__recovery_v2`);
    indexedDB.deleteDatabase(storageName);
    indexedDB.deleteDatabase(`${storageName}__recovery_v2`);
    indexedDB.deleteDatabase(restartStorageName);
    indexedDB.deleteDatabase(`${restartStorageName}__recovery_v2`);
    return {
      pendingBeforeReplay,
      pendingAfterAck,
      pendingAfterCommandAck,
      replay,
      replayed,
      primaryCommittedAtMs: batchAfterReplay?.primaryCommittedAtMs || 0,
      preview,
      mismatchCode,
      plaintextLeaked: artifactText.includes('export me'),
      storageBatchSequence: storageBatch?.sequence || 0,
      journalHlc: storageBatch?.rows?.[0]?._meta?.ctoxHlc || '',
      primaryHlc: storedDocument?._meta?.ctoxHlc || '',
      localResolutionJournaled: Boolean(localResolutionBatch),
      localResolutionHlc: localResolutionBatch?.rows?.[0]?._meta?.ctoxHlc || '',
      tombstoneKeepLocalCode,
      recoveredCopyJournaled: Boolean(recoveredCopyBatch),
      unresolvedAfterResolution: unresolvedAfterResolution.length,
      pendingAfterRestartReconciliation,
    };
  });

  assert(result.pendingBeforeReplay.pendingWrites === 1, 'journal commit must precede primary replay');
  assert(result.replay[0]?.status === 'replayed', 'pending batch must replay on startup');
  assert(result.replayed[0]?.[0]?.id === 'ticket-1', 'replay must preserve the complete local document');
  assert(result.primaryCommittedAtMs > 0, 'successful primary replay must be recorded durably');
  assert(result.pendingAfterAck.pendingWrites === 0, 'native acknowledgement must clear pending status');
  assert(result.pendingAfterCommandAck.pendingWrites === 0, 'a completed native command must acknowledge the submitted command payload');
  assert(result.preview.pendingWrites === 1, 'encrypted export preview must report pending writes');
  assert(result.mismatchCode === 'recovery_instance_mismatch', 'instance remapping must be rejected');
  assert(result.plaintextLeaked === false, 'portable recovery artifact must be encrypted');
  assert(result.storageBatchSequence === 1, 'journal and sequence allocation must commit atomically');
  assert(Boolean(result.journalHlc), 'local storage must assign an HLC before journaling');
  assert(result.primaryHlc === result.journalHlc, 'journal and primary must persist the identical HLC');
  assert(result.localResolutionJournaled, 'keep-local conflict resolution must be journaled as a new local write');
  assert(Boolean(result.localResolutionHlc), 'resolved local writes must receive a fresh HLC');
  assert(result.tombstoneKeepLocalCode === 'structured_conflict_requires_resolution', 'native tombstones must not be overwritten in place');
  assert(result.recoveredCopyJournaled, 'delete-vs-update recovery copy must be journaled');
  assert(result.unresolvedAfterResolution === 0, 'resolved conflicts must leave the pending conflict set');
  assert(result.pendingAfterRestartReconciliation.pendingWrites === 0, 'startup must reconcile journal entries against persisted native command state');
  console.log('ctox-rxdb recovery journal browser smoke OK');
} finally {
  await browser.close();
  await new Promise((resolveClose) => server.close(resolveClose));
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
