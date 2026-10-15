import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { runHandler } from './index.mjs';

test('ctx.asset reads only hash-verified UTF-8 text from this app version', async () => {
  const code = 'export default async (command,ctx) => ctx.asset(command.payload.path);';
  const prompt = 'Use the active project instructions.';
  const bytes = Buffer.from([0xff]);
  const oversized = Buffer.alloc(1024 * 1024 + 1, 'a');
  const entries = [
    ['handlers/run.js', Buffer.from(code)], ['prompts/run.md', Buffer.from(prompt)],
    ['prompts/binary.md', bytes], ['prompts/large.md', oversized],
  ];
  const files = Object.fromEntries(entries.map(([name,value]) => [name, createHash('sha256').update(value).digest('hex')]));
  const artifacts = new Map(entries.map(([name,value]) => [files[name], value]));
  const manifest = { app_id:'asset-app', app_ver:'1', collections:{}, handlers:{run:'handlers/run.js'}, files };
  const read = asset => runHandler(manifest, 'handlers/run.js', {path:asset}, {fixtures:{artifacts}});
  assert.equal(await read('prompts/run.md'), prompt);
  await assert.rejects(read('../prompts/run.md'), {code:'invalid'});
  await assert.rejects(read('/prompts/run.md'), {code:'invalid'});
  await assert.rejects(read('other-app/prompts/run.md'), {code:'not_found'});
  await assert.rejects(read('prompts/binary.md'), {code:'invalid'});
  await assert.rejects(read('prompts/large.md'), {code:'quota'});
  artifacts.set(files['prompts/run.md'], Buffer.from('tampered'));
  await assert.rejects(read('prompts/run.md'), {code:'conflict'});
});
