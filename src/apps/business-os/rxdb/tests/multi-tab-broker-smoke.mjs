import { createMemoryBroker } from '../dist/ctox-rxdb-js.mjs';

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

console.log('ctox-rxdb-js multi-tab broker smoke OK', { brokerKind: auto.kind });

function assert(c, m) { if (!c) throw new Error(m); }
