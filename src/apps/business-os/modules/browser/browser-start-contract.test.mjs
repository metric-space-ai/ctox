import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const source = await readFile(new URL('./index.js', import.meta.url), 'utf8');

assert.match(source, /commandBus\.dispatch\([\s\S]*\{ until: 'accepted' \}\)/);
assert.match(source, /refs\.start[\s\S]*?new_session:\s*true/);
assert.match(source, /opensNewSession[\s\S]*?`browser_tab_\$\{now\}`/);
assert.match(source, /result\?\.opensNewSession[\s\S]*?selectedSessionId\s*=\s*result\.sessionId/);
assert.match(source, /requestedSessionId\s*=\s*result\.sessionId/);
assert.doesNotMatch(
  source.match(/async function startBrowserRuntimeSync[\s\S]*?\n\}/)?.[0] || '',
  /catch\s*\([^)]*\)\s*\{[\s\S]*console\.warn/,
  'browser sync startup errors must remain visible to the caller',
);

console.log('browser start command contract OK');
