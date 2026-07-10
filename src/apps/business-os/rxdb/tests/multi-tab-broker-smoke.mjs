import { createMemoryBroker } from '../dist/ctox-rxdb-js.mjs';

class FakeBroadcastChannel {
  static rooms = new Map();

  constructor(name) {
    this.name = name;
    this.closed = false;
    this.onmessage = null;
    const room = FakeBroadcastChannel.rooms.get(name) || new Set();
    room.add(this);
    FakeBroadcastChannel.rooms.set(name, room);
  }

  postMessage(data) {
    if (this.closed) throw new Error('Channel is closed');
    for (const peer of FakeBroadcastChannel.rooms.get(this.name) || []) {
      if (peer === this || peer.closed) continue;
      peer.onmessage?.({ data });
    }
  }

  close() {
    if (this.closed) return;
    this.closed = true;
    const room = FakeBroadcastChannel.rooms.get(this.name);
    room?.delete(this);
    if (!room?.size) FakeBroadcastChannel.rooms.delete(this.name);
  }
}

const broker = createMemoryBroker({ databaseName: 'x' });
assert(await broker.claim('w1') === true, 'first claim succeeds');
assert(await broker.claim('w1') === false, 'duplicate claim denied');
await broker.release('w1');
assert(await broker.claim('w1') === true, 'after release, re-claim succeeds');
assert(broker.kind === 'memory', 'in Node we fall back to memory broker');
broker.close();

// BroadcastChannel may not exist in this Node — the factory must still work
// and degenerate to the memory broker rather than throwing.
const { createBroadcastChannelBroker } = await import('../dist/ctox-rxdb-js.mjs');
const auto = createBroadcastChannelBroker({ databaseName: 'auto' });
assert(['broadcast-channel', 'memory'].includes(auto.kind), `auto broker kind ${auto.kind}`);
assert(await auto.claim('w1') === true, 'auto broker accepts a first claim');
auto.close();
assert(auto.closed === true, 'closed broker exposes terminal state');
assert(await auto.claim('after-close') === false, 'closed broker rejects new claims without throwing');

// Restart regression: closing the old collection replication state must
// release its claim before the BroadcastChannel disappears. Otherwise the
// replacement state waits for the full 30 s TTL and module mounts fail with
// "Timed out waiting for multi-tab query owner".
const NativeBroadcastChannel = globalThis.BroadcastChannel;
globalThis.BroadcastChannel = FakeBroadcastChannel;
try {
  const owner = createBroadcastChannelBroker({ databaseName: 'restart', tabId: 'a' });
  const replacement = createBroadcastChannelBroker({ databaseName: 'restart', tabId: 'b' });
  assert(await owner.claim('shared-window') === true, 'old state owns the query window');
  assert(await replacement.claim('shared-window') === false, 'replacement observes the live owner');
  owner.close();
  assert(await replacement.claim('shared-window') === true, 'replacement claims immediately after owner close');
  replacement.close();

  const closing = createBroadcastChannelBroker({ databaseName: 'closing', tabId: 'c' });
  const pendingClaim = closing.claim('during-close');
  closing.close();
  assert(await pendingClaim === false, 'claim racing close settles without posting to a closed channel');
  await closing.release('during-close');
} finally {
  if (NativeBroadcastChannel === undefined) delete globalThis.BroadcastChannel;
  else globalThis.BroadcastChannel = NativeBroadcastChannel;
}

console.log('ctox-rxdb-js multi-tab broker smoke OK', { brokerKind: auto.kind });

function assert(c, m) { if (!c) throw new Error(m); }
