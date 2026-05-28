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
const { __ticketTestHooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

const statusEl = {
  hidden: true,
  textContent: '',
  dataset: {},
};

const ctx = {
  host: {
    querySelector(selector) {
      if (selector === '[data-ticket-command-status]') return statusEl;
      throw new Error(`unexpected selector ${selector}`);
    },
  },
};

__ticketTestHooks.setCommandStatusForSmoke(ctx, 'Ticket command rejected by native peer', true);
assert(statusEl.hidden === false, 'command status is visible');
assert(statusEl.textContent === 'Ticket command rejected by native peer', 'command status text is rendered');
assert(statusEl.dataset.state === 'error', 'command status is marked as error');

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

console.log('business-os tickets module smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
