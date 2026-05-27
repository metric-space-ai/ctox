import { __ticketTestHooks } from './index.js';

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
  __ticketTestHooks.commandFailureMessage({ status: 'failed' }, 'cmd_unknown') === 'Command cmd_unknown failed',
  'failed command has deterministic fallback',
);

console.log('business-os tickets module smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
