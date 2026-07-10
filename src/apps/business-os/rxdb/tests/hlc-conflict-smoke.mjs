import {
  compareHybridLogicalClocks,
  formatHybridLogicalClock,
  nextHybridLogicalClock,
  parseHybridLogicalClock,
} from '../dist/ctox-rxdb-js.mjs';

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

const first = nextHybridLogicalClock(null, { nowMs: 1_000, nodeId: 'tab-a' });
const second = nextHybridLogicalClock(first, { nowMs: 900, nodeId: 'tab-a' });
assert(compareHybridLogicalClocks(second, first) > 0,
  'logical component advances when the wall clock moves backwards');
assert(parseHybridLogicalClock(second)?.logical === 1,
  'backward wall-clock write increments the logical counter');

const deviceA = formatHybridLogicalClock({ physicalMs: 2_000, logical: 0, nodeId: 'tab-a' });
const deviceB = formatHybridLogicalClock({ physicalMs: 2_000, logical: 0, nodeId: 'tab-b' });
assert(compareHybridLogicalClocks(deviceA, deviceB) < 0,
  'node id deterministically resolves otherwise-equal cross-device clocks');
assert(compareHybridLogicalClocks(deviceB, deviceA) > 0,
  'HLC ordering is antisymmetric');
assert(compareHybridLogicalClocks('', deviceA) < 0,
  'mixed-version state with a valid HLC outranks missing clock metadata');

console.log('ctox-rxdb HLC conflict smoke OK');
