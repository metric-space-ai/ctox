import {
  V1_5_QUERY_FETCH_CAPABILITY,
  V1_5_STATUS_FIELDS,
  createV1_5StatusState,
  snapshotV1_5Status,
} from '../dist/ctox-rxdb-js.mjs';

assert(V1_5_QUERY_FETCH_CAPABILITY === 'ctox-rxdb-query-fetch-v1', 'capability constant changed unexpectedly');
assert(Object.isFrozen(V1_5_STATUS_FIELDS), 'status fields must be frozen');
assert(V1_5_STATUS_FIELDS.includes('queryDemandLoadingActive'), 'missing queryDemandLoadingActive field');
assert(V1_5_STATUS_FIELDS.includes('peerCapabilityQueryFetchV1'), 'missing peerCapabilityQueryFetchV1 field');

const state = createV1_5StatusState();
assert(state.rxdbProtocolVersion === '1', 'baseline must report protocol version 1');
assert(state.queryDemandLoadingEnabled === false, 'demand-loading must be off in baseline');
assert(state.queryDemandLoadingActive === false, 'demand-loading must be inactive in baseline');
assert(state.peerCapabilityQueryFetchV1 === false, 'query-fetch capability must be unnegotiated in baseline');

const snapshot = snapshotV1_5Status(state);
for (const field of V1_5_STATUS_FIELDS) {
  assert(field in snapshot, `snapshot missing field ${field}`);
}

const snapshotOfNothing = snapshotV1_5Status(undefined);
for (const field of V1_5_STATUS_FIELDS) {
  assert(snapshotOfNothing[field] === null, `unknown state should null field ${field}`);
}

console.log('ctox-rxdb-js v1.5 status smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
