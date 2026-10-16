// Quelltext-Waechter des Sitzungsstarts, umgestellt auf den v4-Vertrag (App-API 4.0, PORTING R5/R13).
// Die Regeln dahinter bleiben: ein Start vergibt eine neue Sitzung und Pacht, waehlt sie sofort aus,
// wartet nie auf die Annahme, und Start-Wuensche anderer Apps kommen ueber die Route an.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const source = await readFile(new URL('./index.js', import.meta.url), 'utf8');

assert.match(source, /env\.api\.command\(commandType, payload, payload\?\.record_id \? \{ record_id: payload\.record_id \} : undefined\)/, 'Befehle gehen ueber api.command (record_id als Option)');
assert.doesNotMatch(source, /commandBus|requestNative|until:\s*'(?:accepted|terminal)'/, 'kein v3-Befehlsbus, kein Warten');
assert.match(source, /startNewBrowserSession[\s\S]*?new_session:\s*true[\s\S]*?refs\.start[\s\S]*?startNewBrowserSession/);
assert.match(source, /opensNewSession[\s\S]*?`browser_tab_\$\{now\}`/);
assert.match(source, /result\?\.opensNewSession[\s\S]*?selectedSessionId\s*=\s*result\.sessionId/);
assert.match(source, /requestedSessionId\s*=\s*result\.sessionId/);
assert.match(source, /requestedSessionPending[\s\S]*?if \(!requestedSessionPending\)/);
assert.match(source, /startNewBrowserSession[\s\S]*?requestedSessionId\s*=\s*sessionId[\s\S]*?session_id:\s*sessionId[\s\S]*?refs\.start/);
assert.match(source, /applyLatestNavigationResult\(state, commands\)/);
assert.match(source, /candidate\.result\?\.url/);
assert.match(source, /renderSessionList\([^\n]*renderedTabs/);
assert.match(
  source,
  /api\.shell\.onRoute\(\(event\) => selectRequestedBrowserSession\(browserRouteArgs\(event\?\.params\)\)\)/,
  'an already-open Browser must consume later launch arguments (v4: Route statt DOM-Ereignis)',
);
// Startform der Shell/outbound: navigate('browser', {title, mode, args}) – wie v3 detail.args.
assert.match(
  source,
  /function browserRouteArgs[\s\S]*?value\.args && typeof value\.args === 'object'[\s\S]*?\? value\.args : value/,
  'launch arguments nested under params.args (outbound auth assist) must be unwrapped',
);
assert.match(
  source,
  /function selectRequestedBrowserSession[\s\S]*?openRequestedBrowserSession\(detail\)/,
  'a protected-source launch must select and start the requested session',
);
assert.match(
  source,
  /startSperrgrund\(state, now\)[\s\S]*?onSettled: \(\) => \{ state\.startPendingSince = 0; \}/,
  'die Startsperre loest sich erst mit dem Ende des Startbefehls',
);

console.log('browser start command contract OK');
