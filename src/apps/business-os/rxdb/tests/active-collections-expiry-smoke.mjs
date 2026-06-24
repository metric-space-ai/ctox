import { createActiveCollectionRegistry } from '../dist/ctox-rxdb-js.mjs';

const registry = createActiveCollectionRegistry({ recentExecMs: 150 });
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

await delay(240);
const latest = events.at(-1) || [];
assert(!latest.includes('desktop_files'), 'expired exec read must publish inactive collection set');

console.log('ctox-rxdb active collections expiry smoke OK');

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
