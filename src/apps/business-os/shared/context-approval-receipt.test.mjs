import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('context approval requests return after the native local receipt', async () => {
  const source = await readFile(new URL('../app.js', import.meta.url), 'utf8');
  const start = source.indexOf("command_type: 'threads.ctox_approval.request'");
  assert.notEqual(start, -1, 'approval command branch must exist');
  // The branch closes the menu through `hideSubmittedMenu`, which is declared
  // above it and delegates to `hideGlobalCtoxContextMenu`. Searching for that
  // call *after* the branch measured where the helper happens to be written and
  // went red when the call was factored out.
  const end = source.indexOf('hideSubmittedMenu();', start);
  assert.notEqual(end, -1, 'approval command branch must close the context menu');
  assert.match(
    source,
    /const hideSubmittedMenu = \(\) => \{[\s\S]*?hideGlobalCtoxContextMenu\(\);/,
    'hideSubmittedMenu must close the global context menu',
  );
  const branch = source.slice(start, end);
  assert.match(
    branch,
    /\}, \{ until: 'local' \}\);/,
    'approval dispatch must not wait for the full historical command pull',
  );
});
