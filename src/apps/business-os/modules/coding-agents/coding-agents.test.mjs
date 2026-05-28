import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});

const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __codingAgentsTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

test('workspace path validation blocks empty and relative paths', () => {
  assert.equal(hooks.validateWorkspacePath('').valid, false);
  assert.equal(hooks.validateWorkspacePath('relative/project').valid, false);
  assert.equal(hooks.validateWorkspacePath('/Users/michaelwelsch/Documents/ctox').valid, true);
  assert.equal(hooks.validateWorkspacePath('~/Documents/ctox').valid, true);
});

test('workspace grants parser preserves spaces in absolute paths', () => {
  const grants = hooks.parseGrantsStdout(`
    Grants
      • /Users/michaelwelsch/Documents/Client Project
      • command(*)
  `);

  assert.deepEqual(grants, ['/Users/michaelwelsch/Documents/Client Project', 'command(*)']);
});

test('new session validation requires a meaningful first instruction', () => {
  assert.equal(hooks.validateNewSessionPrompt('').valid, false);
  assert.equal(hooks.validateNewSessionPrompt('fix').valid, false);
  assert.equal(hooks.validateNewSessionPrompt('Fix failing billing parser test').valid, true);
});

test('session table output parses selectable sessions', () => {
  const sessions = hooks.parseSessionsStdout(`
SHORT ID | ID | UPDATED AT | PROMPT
abc123 | sess_full_1 | 2026-05-27 12:00 | Fix reports empty state
def456 | sess_full_2 | 2026-05-27 12:05 | Review agent logs
`, 'codex');

  assert.deepEqual(sessions.map((session) => session.id), ['sess_full_1', 'sess_full_2']);
  assert.equal(sessions[0].app, 'codex');
});

test('session detail parser keeps user, assistant, and tool records', () => {
  const records = hooks.parseSessionGetStdout(`
[12:01] User: Run the app smoke test
[12:02] Assistant: The smoke test is green
OK Tool Run: node --test
`);

  assert.deepEqual(records.map((record) => record.type), ['user', 'assistant', 'tool']);
  assert.equal(records[2].status, 'success');
});

test('workspace backend errors stay diagnostic instead of becoming empty state', () => {
  assert.match(hooks.workspaceLoadErrorFromResult(null), /Command-Bus/);
  assert.equal(
    hooks.workspaceLoadErrorFromResult({ ok: false, stderr: 'backend offline' }),
    'backend offline'
  );
});

test('agent command arguments are scoped once to the requested app', () => {
  assert.deepEqual(hooks.buildAgyCommandArgs(['status'], 'codex'), ['--app', 'codex', 'status']);
  assert.deepEqual(hooks.buildAgyCommandArgs(['session', 'list'], 'antigravity'), ['--app', 'antigravity', 'session', 'list']);
});

test('escaped session text is safe to inject into render fragments', () => {
  assert.equal(hooks.escapeHtml('<script>alert(1)</script>'), '&lt;script&gt;alert(1)&lt;/script&gt;');
});
