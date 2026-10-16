import { readFileSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

// App-API 4.0: 'ctox:shared/x.js' resolves onto the shell's shared/ (as the Service Worker rewrites it);
// no esbuild bundle needed any more.
const SHARED = fileURLToPath(new URL('../../../shared/', import.meta.url));
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('ctox:shared/')) {
      return nextResolve(pathToFileURL(SHARED + specifier.slice('ctox:shared/'.length)).href, context);
    }
    return nextResolve(specifier, context);
  },
});
const { __ticketTestHooks, renderTicketList } = await import('../index.js');
const indexJs = readFileSync(new URL('../index.js', import.meta.url), 'utf8');

// v4: command status no longer renders inside the app; failures go to the shell's status zone.
assert(!/data-ticket-command-status/.test(readFileSync(new URL('../index.html', import.meta.url), 'utf8')), 'no in-app command status line');
assert(/api\.status\.report\(\{ level: 'error', code, text: `\$\{commandType\}: \$\{commandFailureMessage\(value, handle\.id\)\}`/.test(indexJs), 'failed command is reported to the status zone');

assert(
  __ticketTestHooks.commandFailureMessage(
    { status: 'failed', result: { error: 'title is required' } },
    'cmd_missing_title',
  ) === 'title is required',
  'failed command surfaces native result error',
);

assert(
  __ticketTestHooks.commandFailureMessage({ status: 'failed' }, 'cmd_unknown') === 'Aktion cmd_unknown ist fehlgeschlagen.',
  'failed command has deterministic fallback',
);

assert(renderTicketList([], {}) === '', 'empty ticket list renders nothing (v4)');

const recordContext = __ticketTestHooks.ticketRecordContextForSmoke({
  id: 'local:one',
  ticket_key: 'TCK-1',
  title: 'Broken sync',
});
assert(recordContext['data-record-id'] === 'TCK-1', 'ticket context exposes record id');
assert(recordContext['data-record-type'] === 'ticket', 'ticket context exposes record type');
assert(recordContext['data-label'] === 'Broken sync', 'ticket context exposes label');

console.log('business-os tickets module smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
