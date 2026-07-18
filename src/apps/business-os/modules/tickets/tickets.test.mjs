import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { __ticketTestHooks } from './index.js';

const context = __ticketTestHooks.ticketRecordContextForSmoke({
  id: 'ticket-42',
  ticket_key: 'SUP-42',
  title: 'Login unavailable',
});
assert.equal(context['data-context-module'], 'tickets');
assert.equal(context['data-context-submodule'], 'inbox');
assert.equal(context['data-context-record-type'], 'ticket');
assert.equal(context['data-context-record-id'], 'SUP-42');
assert.equal(context['data-context-label'], 'Login unavailable');

assert.equal(__ticketTestHooks.commandFailureMessage({ status: 'failed', error: 'denied' }), 'denied');
assert.equal(__ticketTestHooks.isCollectionDiagnosticsReady({ connectionStatus: 'connected' }), true);

const css = await readFile(new URL('./index.css', import.meta.url), 'utf8');
const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
const presentationSource = `${css}\n${html}`;
const forbiddenSurfacePattern = new RegExp(['ctox-pane--gla' + 'ss', 'Prem' + 'ium', 'gla' + 'ss'].join('|'), 'i');

assert.doesNotMatch(presentationSource, forbiddenSurfacePattern);
assert.doesNotMatch(presentationSource, /border-(?:left|right)\s*:\s*(?:[2-9]|[0-9]{2,})px/);
assert.doesNotMatch(presentationSource, /border-radius:\s*(?:8|10|12|14|16|18|20|24)px/);
assert.doesNotMatch(presentationSource, /box-shadow:\s*(?:0|inset|rgba|color-mix)/);
assert.match(html, /class="ctox-workspace tickets-module"/);
assert.match(html, /class="ctox-pane tickets-pane tickets-left"/);
assert.match(css, /--ctox-left-width: 340px/);
assert.match(css, /--ctox-right-width: 360px/);
assert.match(css, /@container business-app-window \(max-width: 1160px\)/);
assert.match(css, /@container business-app-window \(max-width: 640px\)/);
assert.match(html, /data-resizer-var="--ctox-left-width"/);
assert.match(html, /data-resizer-var="--ctox-right-width"/);

console.log('tickets module context and failure contract smoke OK');
