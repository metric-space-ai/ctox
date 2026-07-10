import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const functionStart = source.indexOf("function resetDataPlaneReady(reason = 'startup')");
const functionEnd = source.indexOf('\n}\n\nfunction resolveDataPlaneReady', functionStart);

assert.ok(functionStart >= 0 && functionEnd > functionStart, 'resetDataPlaneReady implementation is missing');
const implementation = source.slice(functionStart, functionEnd);
const reuseGuard = implementation.indexOf("state.dataPlaneReadyStatus === 'pending' && state.dataPlaneReady");
const replacement = implementation.indexOf('state.dataPlaneReady = new Promise');

assert.ok(reuseGuard >= 0, 'pending datastore readiness must reuse the existing promise');
assert.ok(reuseGuard < replacement, 'the pending-promise guard must run before a new promise is allocated');
assert.match(implementation, /return state\.dataPlaneReady;/, 'callers must receive the active readiness promise');

console.log('settings readiness race guard OK');
