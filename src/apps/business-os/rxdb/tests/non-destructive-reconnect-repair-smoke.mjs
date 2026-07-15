import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const app = readFileSync(new URL('../../app.js', import.meta.url), 'utf8');
const match = app.match(/async function repairRecoveringDataPlane\(\) \{([\s\S]*?)\n\}/);

assert.ok(match, 'repairRecoveringDataPlane must exist');
assert.match(match[1], /state\.sync\.restartCollections\(collections\)/);
assert.doesNotMatch(match[1], /repairBusinessDataPlane|resetBusinessDb/);

console.log('Non-destructive reconnect repair smoke OK');
