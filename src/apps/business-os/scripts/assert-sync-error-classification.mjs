import { __ctoxSyncTestHooks } from '../shared/sync.js';

const {
  classifySignalingControlPlaneError,
  classifyControlledReplicationCancellation,
  classifyPeerLifecycleEvent,
  classifyReplicationIoError,
  extractReplicationErrorDetails,
} = __ctoxSyncTestHooks;

const signalingCases = [
  {
    name: 'signaling room instance mismatch envelope',
    error: {
      type: 'ctoxError',
      scope: 'control-plane',
      code: 'instance_mismatch',
      reason: 'Browser joined the wrong signaling instance room.',
    },
  },
  {
    name: 'app-local fork normalized control-plane error',
    error: {
      name: 'CtoxSignalingControlPlaneError',
      code: 'instance_mismatch',
      phase: 'signaling-control-plane',
      severity: 'error',
      retryable: false,
      message: 'Browser joined the wrong signaling instance room.',
    },
  },
];

for (const item of signalingCases) {
  const classified = classifySignalingControlPlaneError(item.error);
  if (!classified) {
    throw new Error(`${item.name}: not classified`);
  }
  if (
    classified.name !== 'CtoxSignalingControlPlaneError'
    || classified.code !== 'instance_mismatch'
    || classified.phase !== 'signaling-control-plane'
    || classified.retryable !== false
  ) {
    throw new Error(`${item.name}: wrong classification ${JSON.stringify(classified)}`);
  }
}

const cases = [
  {
    name: 'rust masterChangesSince envelope',
    collection: 'desktop_files',
    error: {
      name: 'RxError (RC_PULL)',
      code: 'RC_PULL',
      parameters: {
        type: 'ctoxError',
        scope: 'replication',
        rxdb: true,
        code: 'RC_PULL',
        phase: 'replication-pull',
        direction: 'pull',
        checkpoint: { sequence: 12 },
        batchSize: 20,
        errors: [
          {
            rxdb: true,
            code: 'TEST_PULL',
            name: 'RxError (TEST_PULL)',
            message: 'pull failed',
            parameters: { attempt: 1 },
          },
        ],
      },
    },
    expected: {
      code: 'ctox_replication_pull_failed',
      direction: 'pull',
      upstreamCode: 'RC_PULL',
      batchSize: 20,
      rowCount: null,
      retryable: true,
    },
  },
  {
    name: 'rust masterWrite envelope with explicit rowCount',
    collection: 'desktop_files',
    error: {
      name: 'RxError (RC_PUSH)',
      code: 'RC_PUSH',
      parameters: {
        type: 'ctoxError',
        scope: 'replication',
        rxdb: true,
        code: 'RC_PUSH',
        phase: 'replication-push',
        direction: 'push',
        rowCount: 3,
        errors: [
          {
            rxdb: true,
            code: 'TEST_PUSH',
            name: 'RxError (TEST_PUSH)',
            message: 'push failed',
            parameters: { attempt: 1 },
          },
        ],
      },
    },
    expected: {
      code: 'ctox_replication_push_failed',
      direction: 'push',
      upstreamCode: 'RC_PUSH',
      batchSize: null,
      rowCount: 3,
      retryable: true,
    },
  },
  {
    name: 'rust invalid push response envelope',
    collection: 'desktop_files',
    error: {
      name: 'RxError (RC_PUSH_NO_AR)',
      code: 'RC_PUSH_NO_AR',
      parameters: {
        type: 'ctoxError',
        scope: 'replication',
        rxdb: true,
        code: 'RC_PUSH_NO_AR',
        phase: 'replication-push',
        direction: 'push',
        pushRows: [
          { newDocumentState: { id: 'a' } },
          { newDocumentState: { id: 'b' } },
        ],
      },
    },
    expected: {
      code: 'ctox_replication_push_contract_invalid',
      direction: 'push',
      upstreamCode: 'RC_PUSH_NO_AR',
      batchSize: null,
      rowCount: 2,
      retryable: false,
    },
  },
];

const failures = [];

for (const item of cases) {
  const details = extractReplicationErrorDetails(item.error);
  const classified = classifyReplicationIoError(item.collection, item.error);
  if (!classified) {
    failures.push(`${item.name}: not classified`);
    continue;
  }
  for (const [key, expected] of Object.entries(item.expected)) {
    if (classified[key] !== expected) {
      failures.push(`${item.name}: ${key} expected ${JSON.stringify(expected)} got ${JSON.stringify(classified[key])}`);
    }
  }
  if (classified.name !== 'CtoxReplicationIoError') {
    failures.push(`${item.name}: wrong error name ${classified.name}`);
  }
  if (classified.collection !== item.collection) {
    failures.push(`${item.name}: collection was not preserved`);
  }
  if (details.direction !== item.expected.direction) {
    failures.push(`${item.name}: extracted direction mismatch`);
  }
}

const lifecycleCases = [
  {
    name: 'native peer connection lost',
    error: { code: 'ERR_CONNECTION_FAILURE', message: 'peer connection closed' },
    expected: 'peer_connection_lost',
  },
  {
    name: 'native data channel close during replacement',
    error: { message: 'ctox_data_channel_error: Data channel is closed' },
    expected: 'peer_data_channel_closed',
  },
  {
    name: 'browser peer connection limit',
    error: { message: 'Cannot create so many PeerConnections' },
    expected: 'peer_connection_limit',
  },
  {
    name: 'controlled replication cancellation during restart',
    error: new Error('WebRTC replication cancelled'),
    expected: 'replication_cancelled',
  },
];

for (const item of lifecycleCases) {
  const classified = classifyPeerLifecycleEvent(item.error);
  if (!classified) {
    failures.push(`${item.name}: lifecycle event was not classified`);
    continue;
  }
  if (classified.name !== 'CtoxWebRtcPeerLifecycleEvent') {
    failures.push(`${item.name}: wrong lifecycle name ${classified.name}`);
  }
  if (classified.code !== item.expected) {
    failures.push(`${item.name}: expected lifecycle code ${item.expected} got ${classified.code}`);
  }
  if (classified.phase !== 'peer-reconnect') {
    failures.push(`${item.name}: wrong lifecycle phase ${classified.phase}`);
  }
  if (classified.severity !== 'recoverable' || classified.retryable !== true || classified.lifecycle !== true) {
    failures.push(`${item.name}: lifecycle event is not recoverable/retryable`);
  }
}

const controlledCancellation = classifyControlledReplicationCancellation(new Error('WebRTC replication cancelled'));
if (!controlledCancellation || controlledCancellation.code !== 'replication_cancelled') {
  failures.push('controlled replication cancellation: direct classifier did not return replication_cancelled');
}

if (failures.length) {
  console.error(`Sync error classification guard failed:\n${failures.map((failure) => `- ${failure}`).join('\n')}`);
  process.exit(1);
}

console.log('Sync error classification guard OK');
