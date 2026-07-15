import { createMultiTabSyncCoordinator } from '../src/multi-tab-sync-coordinator.mjs';

const room = `room-${process.pid}-${Date.now()}`;
const first = createMultiTabSyncCoordinator({ databaseName: 'multi-tab-test', room, tabId: 'tab-a' });
const second = createMultiTabSyncCoordinator({ databaseName: 'multi-tab-test', room, tabId: 'tab-b' });
await first.start();
await second.start();
await delay(80);

assert(first.isLeader(), 'deterministic lower tab id must retain the fallback lease');
assert(!second.isLeader(), 'only one tab may own the sync line');
let dirty = null;
let pushFinished = false;
const unsubscribe = first.onDirty(async (message) => {
  dirty = message;
  await delay(10);
  pushFinished = true;
});
second.notifyDirty('tickets', ['ticket-1']);
await delay(30);
assert(dirty?.collection === 'tickets', 'follower dirty collection must reach the leader');
assert(dirty?.ids?.[0] === 'ticket-1', 'follower dirty ids must reach the leader');
pushFinished = false;
const acknowledgement = await second.notifyDirtyAndWait('business_commands', ['command-1']);
assert(acknowledgement?.ok === true, 'follower must receive a positive leader push acknowledgement');
assert(dirty?.collection === 'business_commands', 'acknowledged dirty collection must reach the leader');
assert(pushFinished, 'leader acknowledgement must wait until async push listeners finish');

unsubscribe();
await second.close();
await first.close();
assert(first.isClosed() && second.isClosed(), 'coordinator close must release reusable registry entries');

console.log('ctox-rxdb multi-tab leader smoke OK');

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
