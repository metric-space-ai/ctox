import assert from 'node:assert/strict';
import { once } from 'node:events';
import { PassThrough } from 'node:stream';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const { forwardNativeLogLines } = require('../../../../core/rxdb/tools/native_log_lines.js');

const sample = { command_id: 'größer_🧪', authentication_ms: 11.9, identity_stamping_ms: 8.3 };
const diagnostic = `command_intake_sample=${JSON.stringify(sample)}`;
const records = ['CTOX Business OS listening', diagnostic, 'last line without newline'];
const bytes = Buffer.from(records.join('\r\n'));
const chunkings = [
  [bytes],
  [...bytes].map((byte) => Buffer.from([byte])),
  [bytes.subarray(0, 10), bytes.subarray(10)],
];

for (const chunks of chunkings) {
  const input = new PassThrough();
  const output = [];
  const observed = [];
  const reader = forwardNativeLogLines(input, {
    write: (line) => { output.push(line); return true; },
  }, '[ctox:err] ', (line) => observed.push(line));
  const closed = once(reader, 'close');
  for (const chunk of chunks) input.write(chunk);
  input.end();
  await closed;
  assert.deepEqual(observed, records, 'records must survive arbitrary byte boundaries and final EOF');
  assert.deepEqual(output, records.map((line) => `[ctox:err] ${line}\n`));
  const captured = output[1].slice('[ctox:err] command_intake_sample='.length);
  assert.deepEqual(JSON.parse(captured), sample, 'measurement keys, IDs and values must remain unchanged');
  assert.equal(observed.filter((line) => line.includes('CTOX Business OS listening')).length, 1);
}

console.log('native log line framing smoke OK: split JSON, UTF-8, listening marker, CRLF and final EOF');
