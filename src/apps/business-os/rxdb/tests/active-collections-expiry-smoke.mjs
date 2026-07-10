import { createActiveCollectionRegistry } from '../dist/ctox-rxdb-js.mjs';

// Keep a wide margin over the 100 ms notification debounce so a busy CI host
// cannot let the read expire before the first debounced notification runs.
const registry = createActiveCollectionRegistry({ recentExecMs: 1_000 });
const events = [];
registry.onChange((list) => {
  events.push([...list]);
});

registry.markRead('desktop_files');
await delay(130);

assert(
  events.some((list) => list.includes('desktop_files')),
  'recent exec read must publish an active collection',
);

await delay(1_100);
const latest = events.at(-1) || [];
assert(!latest.includes('desktop_files'), 'expired exec read must publish inactive collection set');

console.log('ctox-rxdb active collections expiry smoke OK');

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
