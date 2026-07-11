import { CtoxWebRtcNativePeer } from '../src/webrtc-native.mjs';

const originalSetTimeout = globalThis.setTimeout;
const originalClearTimeout = globalThis.clearTimeout;
let removal = null;
const errors = [];
const fakePeer = {
  transportStats: {
    backpressureWaitCount: 0,
    backpressureStallCount: 0,
    rejectedFrames: 0,
    lastBufferedAmount: 0,
  },
  recordTransportStatus(update) { Object.assign(this.transportStats, update); },
  events: { emit(type, error) { if (type === 'error') errors.push(error); } },
  removeConnection(...args) { removal = args; },
};
const channel = {
  bufferedAmount: 16 * 1024 * 1024,
  bufferedAmountLowThreshold: 0,
  addEventListener() {},
  removeEventListener() {},
};

try {
  globalThis.setTimeout = (callback) => {
    queueMicrotask(callback);
    return 1;
  };
  globalThis.clearTimeout = () => {};
  let caught = null;
  try {
    await CtoxWebRtcNativePeer.prototype.waitForSendBuffer.call(
      fakePeer,
      channel,
      { remotePeerId: 'native-peer-1' },
    );
  } catch (error) {
    caught = error;
  }
  assert(caught?.code === 'ctox_webrtc_send_buffer_stalled', 'capacity timeout must be typed');
  assert(caught?.retryable === true, 'capacity timeout remains retryable at the room circuit');
  assert(removal?.[0] === 'native-peer-1', 'stalled peer must be removed immediately');
  assert(removal?.[1] === 'send-buffer-stalled', 'peer close must retain the terminal stall reason');
  assert(removal?.[2] === caught, 'all pending requests must receive the typed stall error');
  assert(removal?.[3]?.reconnect === false, 'low-level transport must not reconnect around the room circuit');
  assert(errors[0] === caught, 'typed stall must enter diagnostics');
  assert(fakePeer.transportStats.backpressureStallCount === 1, 'stall counter must advance');
  assert(fakePeer.transportStats.rejectedFrames === 1, 'rejected frame counter must advance');
  console.log('ctox-rxdb terminal backpressure smoke OK');
} finally {
  globalThis.setTimeout = originalSetTimeout;
  globalThis.clearTimeout = originalClearTimeout;
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
