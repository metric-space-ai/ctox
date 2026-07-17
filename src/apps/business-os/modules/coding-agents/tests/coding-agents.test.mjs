import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import { __codingAgentsTestHooks as hooks } from '../index.js';

test('presentation layer stays compact and shell-native', () => {
  const css = readFileSync(new URL('../index.css', import.meta.url), 'utf8');
  const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
  const js = readFileSync(new URL('../index.js', import.meta.url), 'utf8');
  const source = `${css}\n${html}\n${js}`;
  const surfacePattern = new RegExp(['ctox-pane--gla' + 'ss', 'gla' + 'ss', 'Prem' + 'ium'].join('|'), 'i');
  const sidePattern = new RegExp('border-' + '(?:left|right)\\s*:\\s*(?:[2-9]|[0-9]{2,})px');
  const radiusPattern = new RegExp('border-' + 'radius:\\s*(?:8|10|12|14|16|18|20|24)px');
  const shadowPattern = new RegExp('box-' + 'shadow:\\s*(?:0|inset|rgba|color-mix|var\\(--panel-shadow\\)|var\\(--shadow-sm\\)|var\\(--shadow-md\\))');
  const gradientPattern = new RegExp(['linear-grad' + 'ient', 'radial-grad' + 'ient'].join('|'));
  const hardNeutralPattern = new RegExp(['#00' + '0', '#ff' + 'f'].join('|'), 'i');

  assert.doesNotMatch(source, surfacePattern);
  assert.doesNotMatch(source, sidePattern);
  assert.doesNotMatch(source, radiusPattern);
  assert.doesNotMatch(source, shadowPattern);
  assert.doesNotMatch(source, gradientPattern);
  assert.doesNotMatch(source, hardNeutralPattern);
  // Standard shell frame: kit workspace + declarative resizer, no DIY grid.
  assert.match(html, /class="ctox-workspace ctox-workspace--two-pane coding-agents-module/);
  assert.match(html, /data-resize-frame/);
  assert.match(html, /class="ctox-column-resizer"[^>]*data-resizer-var="--ctox-left-width"/);
  assert.doesNotMatch(js, /CtoxResizer/);
  assert.doesNotMatch(css, /--coding-agents-left-width/);
  assert.match(css, /\.lifecycle-status-row/);
  assert.match(css, /\.browser-log-box/);
});

test('workspace path validation blocks empty and relative paths', () => {
  assert.equal(hooks.validateWorkspacePath('').valid, false);
  assert.equal(hooks.validateWorkspacePath('relative/project').valid, false);
  assert.equal(hooks.validateWorkspacePath('/Users/you/Documents/ctox').valid, true);
  assert.equal(hooks.validateWorkspacePath('~/Documents/ctox').valid, true);
});

test('workspace grants parser preserves spaces in absolute paths', () => {
  const grants = hooks.parseGrantsStdout(`
    Grants
      * /Users/you/Documents/Client Project
      * command(*)
  `);

  assert.deepEqual(grants, ['/Users/you/Documents/Client Project', 'command(*)']);
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

test('typed agent commands carry provider and structured payloads', () => {
  assert.deepEqual(hooks.buildAgyCommandArgs(['status'], 'codex'), ['--app', 'codex', 'status']);

  assert.deepEqual(
    hooks.buildCodingAgentCommand(['status'], 'codex'),
    {
      commandType: 'ctox.coding_agent.status',
      payload: { provider: 'codex' }
    }
  );
  assert.deepEqual(
    hooks.buildCodingAgentCommand(['install', '--apply'], 'codex'),
    {
      commandType: 'ctox.coding_agent.install',
      payload: { provider: 'codex', apply: true }
    }
  );
  assert.deepEqual(
    hooks.buildCodingAgentCommand(['config', 'grant', '/Users/you/Documents/ctox'], 'claude'),
    {
      commandType: 'ctox.coding_agent.workspace.grant',
      payload: { provider: 'claude', path: '/Users/you/Documents/ctox' }
    }
  );
  assert.deepEqual(
    hooks.buildCodingAgentCommand(
      ['session', 'create', '-p', '/Users/you/Documents/ctox', 'Fix failing billing parser test'],
      'antigravity'
    ),
    {
      commandType: 'ctox.coding_agent.session.create',
      payload: {
        provider: 'antigravity',
        workspace_root: '/Users/you/Documents/ctox',
        prompt: 'Fix failing billing parser test'
      }
    }
  );

  assert.equal(
    hooks.codingAgentCommandWaitTimeoutMs('ctox.coding_agent.session.create'),
    10 * 60 * 1000
  );
  assert.equal(
    hooks.codingAgentCommandWaitTimeoutMs('ctox.coding_agent.session.prompt'),
    10 * 60 * 1000
  );
  assert.equal(hooks.codingAgentCommandWaitTimeoutMs('ctox.coding_agent.status'), undefined);
});

test('diagnostics status refresh does not create idle polling command traffic', () => {
  let scheduledIntervals = 0;
  let refreshCalls = 0;
  const handle = hooks.startDiagnosticsAutoRefresh({
    setIntervalFn: () => {
      scheduledIntervals += 1;
      return { id: 'unexpected-interval' };
    },
    refresh: () => {
      refreshCalls += 1;
    }
  });

  assert.equal(handle, null);
  assert.equal(scheduledIntervals, 0);
  assert.equal(refreshCalls, 0);

  const source = readFileSync(new URL('../index.js', import.meta.url), 'utf8');
  assert.equal(source.includes('setInterval('), false);
  const initialLoadBody = source.match(/function startInitialLoadWithTimeout\(\) \{([\s\S]*?)\n\}/)?.[1] || '';
  assert.match(initialLoadBody, /refreshProjectedData\(\)/);
  assert.doesNotMatch(initialLoadBody, /refreshAllData\(\)/);
});

test('structured command outcomes avoid stdout parsing where available', () => {
  const grants = hooks.grantsFromOutcome({ data: { grants: ['/Users/project with spaces'] } });
  assert.deepEqual(grants, ['/Users/project with spaces']);

  const sessions = hooks.sessionsFromOutcome({
    data: {
      sessions: [
        {
          session_id: 'ca_codex_abcdef123456',
          provider: 'codex',
          updated_at_ms: 1781297405320,
          last_prompt: 'Continue task',
          workspace_root: '/Users/project',
          status: 'running'
        }
      ]
    }
  }, 'codex');
  assert.equal(sessions[0].shortId, 'abcdef12');
  assert.equal(sessions[0].prompt, 'Continue task');

  const events = hooks.sessionEventsFromOutcome({
    data: {
      events: [
        { role: 'user', text: 'Run test', status: 'accepted', created_at_ms: 1 },
        { role: 'assistant', text: 'Done', status: 'completed', created_at_ms: 2 }
      ]
    }
  });
  assert.deepEqual(events.map((event) => event.type), ['user', 'assistant']);
  assert.equal(events[1].text, 'Done');

  const diag = hooks.diagnosticsFromOutcome({
    data: {
      installed: true,
      controllable: true,
      mode: 'claude-code-cli',
      binary: '/opt/homebrew/bin/claude',
      label: 'Anthropic Claude Code',
      version: '2.1.163',
      auth: { ready: true }
    }
  });
  assert.equal(diag.online, true);
  assert.equal(diag.port, 'claude-code-cli');
});

test('escaped session text is safe to inject into render fragments', () => {
  assert.equal(hooks.escapeHtml('<script>alert(1)</script>'), '&lt;script&gt;alert(1)&lt;/script&gt;');
});
