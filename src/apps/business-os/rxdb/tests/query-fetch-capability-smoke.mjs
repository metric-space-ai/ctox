import {
  V1_5_QUERY_FETCH_CAPABILITY,
  remoteSupportsQueryFetch,
} from '../dist/ctox-rxdb-js.mjs';

assert(V1_5_QUERY_FETCH_CAPABILITY === 'ctox-rxdb-query-fetch-v1', 'capability constant changed');

assert(remoteSupportsQueryFetch({
  capabilities: [
    'ctox-rxdb-native-v1',
    'ctox-file-chunks-v1',
    V1_5_QUERY_FETCH_CAPABILITY,
  ],
}), 'capability detection should pass for V1.5 native');

assert(!remoteSupportsQueryFetch({
  capabilities: ['ctox-rxdb-native-v1', 'ctox-file-chunks-v1'],
}), 'capability detection should refuse a V1-only peer');

assert(!remoteSupportsQueryFetch(null), 'null protocol must yield false');
assert(!remoteSupportsQueryFetch({}), 'protocol without capabilities must yield false');
assert(!remoteSupportsQueryFetch({ capabilities: 'string-not-array' }), 'non-array capabilities must yield false');

console.log('ctox-rxdb-js query-fetch capability smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
