import {
  V1_5_QUERY_FETCH_CAPABILITY,
  remoteSupportsQueryFetch,
} from '../dist/ctox-rxdb-js.mjs';

// A V1.5-aware server with capability advertised AND flag on → must lit.
assert(
  remoteSupportsQueryFetch({
    capabilities: [V1_5_QUERY_FETCH_CAPABILITY],
    v1_5: { queryDemandLoadingEnabled: true },
  }),
  'capability + flag on must enable demand loading',
);

// V1.5 capable but flag off → must NOT lit. This is the runtime kill switch.
assert(
  !remoteSupportsQueryFetch({
    capabilities: [V1_5_QUERY_FETCH_CAPABILITY],
    v1_5: { queryDemandLoadingEnabled: false },
  }),
  'flag off must mask the capability',
);

// V1.5 capable, flag missing entirely → default-on (backward compat with
// future servers that omit the field). This must NOT break.
assert(
  remoteSupportsQueryFetch({
    capabilities: [V1_5_QUERY_FETCH_CAPABILITY],
  }),
  'missing flag defaults to on (capability is authoritative)',
);

// No capability at all → off regardless of flag.
assert(
  !remoteSupportsQueryFetch({
    capabilities: [],
    v1_5: { queryDemandLoadingEnabled: true },
  }),
  'no capability means no demand loading even if flag is on',
);

console.log('ctox-rxdb-js feature flag handshake smoke OK');

function assert(c, m) { if (!c) throw new Error(m); }
