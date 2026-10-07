import assert from 'node:assert/strict';
import {
  createDemandLoadingTransport, createQueryDemandLoader, createSidecarWithMemoryBackend,
} from '../dist/ctox-rxdb-js.mjs';
import { CLIENT_QUERY_STREAM_LIMIT } from '../src/demand-loading-transport.mjs';
import { CTOX_QUERY_RPC } from '../src/protocol-contract.generated.mjs';

const flush = () => new Promise(resolve => setImmediate(resolve));
async function until(predicate) {
  const deadline = Date.now() + 2000;
  while (!predicate()) {
    assert(Date.now() < deadline, 'query fixture did not reach its dispatch barrier');
    await flush();
  }
}
function native() {
  const requests = [];
  const cancellations = [];
  const transport = createDemandLoadingTransport({ getPeerId: () => 'native' });
  transport.attach({
    connections: new Map([['native', { channel: { readyState: 'open' }, peer: { connectionState: 'connected' } }]]),
    async request(peerId, method, [envelope]) {
      if (method === CTOX_QUERY_RPC.fetch) requests.push(envelope);
      if (method === CTOX_QUERY_RPC.cancel) cancellations.push(envelope.requestId);
      return { ack: true };
    },
  });
  const complete = (request, documents = []) => transport.requestHandlers[CTOX_QUERY_RPC.chunk]({
    params: [{ requestId: request.requestId, sequence: 0, documents, complete: true, authoritativeRevision: 'native-current' }],
  });
  return { transport, requests, cancellations, complete };
}
function storage() {
  const documents = new Map();
  return {
    primaryPath: 'id', databaseName: 'startup-admission',
    async bulkWrite(rows) { for (const row of rows) { const doc = row.document || row; documents.set(doc.id, { ...doc }); } },
    async findDocumentsById(ids) { return Object.fromEntries(ids.filter(id => documents.has(id)).map(id => [id, documents.get(id)])); },
    async queryDocuments() { return [...documents.values()]; },
  };
}
function reader(remote, authority, name = 'leads') {
  let invoked = 0;
  const loader = createQueryDemandLoader({
    storageCollection: storage(), collectionName: name, schemaVersion: 1,
    sidecar: createSidecarWithMemoryBackend({ databaseName: 'startup-' + Math.random() }),
    clock: () => 1, // Separate loaders must still issue distinct consumer IDs.
    queryGeneration: () => authority.generation,
    readPermissionDigest: () => authority.permission,
    requestQueryFetch: (envelope, options) => {
      invoked += 1;
      return remote.transport.requestQueryFetch(envelope, options);
    },
    requestCancel: request => remote.transport.requestQueryCancel(request),
  });
  return { loader, invocations: () => invoked };
}
const strict = token => ({ selector: {}, limit: 1, requireRevision: token });

// Three strict hydration tokens share only the live wire operation. Cancelling
// its first consumer twice must leave the other consumers and native stream live.
{
  const remote = native();
  const authority = { generation: 'connection-1', permission: 'owner-1' };
  const first = reader(remote, authority);
  const second = reader(remote, authority);
  const controller = new AbortController();
  let logs = 0;
  const debug = console.debug;
  console.debug = () => { logs += 1; };
  try {
    const abandoned = first.loader.resolveQuery(strict('first'), { signal: controller.signal }).catch(error => error);
    const surviving = first.loader.resolveQuery(strict('second'));
    const otherLoader = second.loader.resolveQuery(strict('third'));
    await until(() => first.invocations() + second.invocations() === 3);
    await until(() => remote.requests.length === 1);
    assert.equal(remote.transport.diagnostics().queryFetchCoalescedRequests, 2);
    const source = remote.requests[0];
    controller.abort();
    assert.equal((await abandoned).code, 'QUERY_CANCELLED');
    await remote.transport.requestQueryCancel({ requestId: source.requestId });
    assert.deepEqual(remote.cancellations, [], 'an abandoned consumer cannot cancel surviving strict readers');
    await remote.complete(source, [{ id: 'lead', name: 'native lead' }]);
    assert.equal((await surviving)[0].name, 'native lead');
    assert.equal((await otherLoader)[0].name, 'native lead');
    assert.equal(logs, 0, 'V1.5 production logging must be opt-in');
    await flush();
    const fresh = first.loader.resolveQuery(strict('later'));
    await until(() => remote.requests.length === 2);
    await remote.complete(remote.requests[1], [{ id: 'lead', name: 'later native lead' }]);
    assert.equal((await fresh)[0].name, 'later native lead', 'completed strict results cannot satisfy a new token');
  } finally { console.debug = debug; }
  await flush();
}

// Neither a new permission digest nor a replaced connection may join a previous
// authority's wire request. Each loader retains its own final publication fence.
{
  const remote = native();
  const oldAuthority = { generation: 'old', permission: 'owner-1' };
  const currentAuthority = { generation: 'old', permission: 'owner-2' };
  const old = reader(remote, oldAuthority, 'business_commands');
  const current = reader(remote, currentAuthority, 'business_commands');
  const pendingOld = old.loader.resolveQuery(strict('old')).catch(error => error);
  const pendingCurrent = current.loader.resolveQuery(strict('new'));
  await until(() => remote.requests.length === 2);
  oldAuthority.generation = 'retired';
  await remote.complete(remote.requests[0], [{ id: 'command', status: 'old' }]);
  await remote.complete(remote.requests[1], [{ id: 'command', status: 'current' }]);
  assert.equal((await pendingOld).code, 'QUERY_CANCELLED');
  assert.equal((await pendingCurrent)[0].status, 'current');
  assert.equal(remote.transport.diagnostics().queryFetchCoalescedRequests, 0);
  await flush();
}

// Cancelling the last consumer sends one native cancel; repeated aborts do
// not act on another caller. Peer loss rejects every surviving shared reader.
{
  const remote = native();
  const a = remote.transport.requestQueryFetch({ requestId: 'last-a', collectionName: 'leads', queryFingerprint: 'last' }, { authorityKey: 'owner' }).catch(error => error);
  const b = remote.transport.requestQueryFetch({ requestId: 'last-b', collectionName: 'leads', queryFingerprint: 'last' }, { authorityKey: 'owner' }).catch(error => error);
  await until(() => remote.requests.length === 1);
  await remote.transport.requestQueryCancel({ requestId: 'last-a' });
  await remote.transport.requestQueryCancel({ requestId: 'last-b' });
  assert.equal((await a).code, 'QUERY_CANCELLED');
  assert.equal((await b).code, 'QUERY_CANCELLED');
  assert.deepEqual(remote.cancellations, ['last-a']);
  await flush();
  const c = remote.transport.requestQueryFetch({ requestId: 'lost-a', collectionName: 'leads', queryFingerprint: 'lost' }, { authorityKey: 'owner' }).catch(error => error);
  const d = remote.transport.requestQueryFetch({ requestId: 'lost-b', collectionName: 'leads', queryFingerprint: 'lost' }, { authorityKey: 'owner' }).catch(error => error);
  await until(() => remote.requests.length === 2);
  remote.transport.abortPeerRequests('native', 'peer-closed');
  assert.equal((await c).code, 'QUERY_CANCELLED');
  assert.equal((await d).code, 'QUERY_CANCELLED');
  await flush();
}

// Aborted coalesced jobs waiting for an authorized peer cannot dispatch when
// that peer later becomes ready. A later generation is a fresh wire read.
{
  let peerId = null;
  const requests = [];
  const transport = createDemandLoadingTransport({ getPeerId: () => peerId });
  transport.attach({ connections: new Map(), async request(...args) { requests.push(args); return { ack: true }; } });
  const a = transport.requestQueryFetch({ requestId: 'waiting-a', collectionName: 'leads' }, { authorityKey: 'generation-1' }).catch(error => error);
  const b = transport.requestQueryFetch({ requestId: 'waiting-b', collectionName: 'leads' }, { authorityKey: 'generation-1' }).catch(error => error);
  await transport.requestQueryCancel({ requestId: 'waiting-a' });
  await transport.requestQueryCancel({ requestId: 'waiting-b' });
  assert.equal((await a).code, 'QUERY_CANCELLED');
  assert.equal((await b).code, 'QUERY_CANCELLED');
  peerId = 'native';
  await flush();
  assert.deepEqual(requests, []);
}

// Different connection generations with the same permission cannot coalesce.
{
  const remote = native();
  const old = reader(remote, { generation: 'before-reconnect', permission: 'owner' });
  const current = reader(remote, { generation: 'after-reconnect', permission: 'owner' });
  const a = old.loader.resolveQuery(strict('before'));
  const b = current.loader.resolveQuery(strict('after'));
  await until(() => remote.requests.length === 2);
  assert.equal(remote.transport.diagnostics().queryFetchCoalescedRequests, 0);
  await remote.complete(remote.requests[0], [{ id: 'lead' }]);
  await remote.complete(remote.requests[1], [{ id: 'lead' }]);
  await Promise.all([a, b]);
  await flush();
}

// Admission, shared across collection transports, uses the generated native
// stream bound; excess requests wait and each release admits exactly one.
{
  assert.equal(CLIENT_QUERY_STREAM_LIMIT, CTOX_QUERY_RPC.maxInFlightStreams);
  const one = native();
  const two = native();
  const promises = Array.from({ length: CLIENT_QUERY_STREAM_LIMIT + 2 }, (_, index) => {
    const remote = index % 2 ? one : two;
    return remote.transport.requestQueryFetch({
      requestId: 'bounded-' + index, collectionName: 'leads', queryFingerprint: 'window-' + index,
      query: { selector: {}, limit: 1 }, window: { offset: 0, limit: 1 },
    });
  });
  await until(() => one.requests.length + two.requests.length === CLIENT_QUERY_STREAM_LIMIT);
  assert.equal(one.transport.diagnostics().activeQueryStreams, CTOX_QUERY_RPC.maxInFlightStreams);
  assert.equal(one.transport.diagnostics().queuedQueryRequests, 2);
  await one.complete(one.requests[0]);
  await until(() => one.requests.length + two.requests.length === CLIENT_QUERY_STREAM_LIMIT + 1);
  assert.equal(one.transport.diagnostics().activeQueryStreams, CTOX_QUERY_RPC.maxInFlightStreams);
  for (const remote of [one, two]) for (const request of [...remote.requests]) await remote.complete(request);
  await until(() => one.requests.length + two.requests.length === CLIENT_QUERY_STREAM_LIMIT + 2);
  for (const remote of [one, two]) for (const request of remote.requests) await remote.complete(request);
  await Promise.all(promises);
  await flush();
  assert.equal(one.transport.diagnostics().activeQueryStreams, 0);
}
console.log('query startup admission/coalescing smoke OK (component fixture; not installed tenant latency)');
