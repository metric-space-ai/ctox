import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('context approval requests return after the native local receipt', async () => {
  const source = await readFile(new URL('../app.js', import.meta.url), 'utf8');
  const start = source.indexOf("command_type: 'threads.ctox_approval.request'");
  const end = source.indexOf('hideGlobalCtoxContextMenu();', start);
  assert.notEqual(start, -1, 'approval command branch must exist');
  assert.notEqual(end, -1, 'approval command branch must close the context menu');
  const branch = source.slice(start, end);
  assert.match(
    branch,
    /\}, \{ until: 'local' \}\);/,
    'approval dispatch must not wait for the full historical command pull',
  );
});
