// SKL-D: gemeinsamer Kontext-Chat-Baustein. node --import ./v4/tools/node/ctox-shared.mjs --test shared/context-chat.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  APP_MODIFY_COMMAND,
  CONTEXT_CHAT_COMMAND,
  buildContextChatCommand,
  contextChatModeMarkup,
  contextChatModes,
  normalizeContextChatMode,
  sendContextChat,
} from './context-chat.js';

test('Menüeinträge: data immer, ask optional, app nur mit Darstellungsrecht; Beschriftung überschreibbar', () => {
  assert.deepEqual(contextChatModes().map((e) => e.mode), ['data', 'ask']);
  assert.deepEqual(contextChatModes({ canModifyApp: true, includeAsk: false }).map((e) => e.mode), ['data', 'app']);
  assert.equal(contextChatModes({ labels: { data: 'Mit Dokumenten arbeiten' } })[0].label, 'Mit Dokumenten arbeiten');
  const html = contextChatModeMarkup({ canModifyApp: true, labelClass: 'x', labels: { ask: 'Frage <b>' } });
  assert.equal((html.match(/name="contextMode"/g) || []).length, 3);
  assert.match(html, /value="data" checked/);
  assert.doesNotMatch(html, /value="ask" checked/);
  assert.match(html, /Frage &lt;b&gt;/);
  assert.match(html, /<label class="x">/);
});

test('Modus: app ohne Recht → data, ask ohne Angebot → data, Unbekanntes → data', () => {
  assert.equal(normalizeContextChatMode('app'), 'data');
  assert.equal(normalizeContextChatMode('app', { canModifyApp: true }), 'app');
  assert.equal(normalizeContextChatMode('ask', { includeAsk: false }), 'data');
  assert.equal(normalizeContextChatMode('ask'), 'ask');
  assert.equal(normalizeContextChatMode('drop'), 'data');
});

test('Kontext → Standardbefehl: Typ je Modus, Payload {source_app, record_ref, selection, text, mode, context}', () => {
  const data = buildContextChatCommand({ source_app: 'spreadsheets', record_ref: 'sheet-1', selection: ' A1:B2 ', text: ' Summe? ', mode: 'ask', context: { column: 'grid', fn: () => 1 } });
  assert.equal(data.type, CONTEXT_CHAT_COMMAND);
  assert.equal(data.record_id, 'sheet-1');
  assert.deepEqual(data.payload, { source_app: 'spreadsheets', record_ref: 'sheet-1', selection: 'A1:B2', text: 'Summe?', mode: 'ask', context: { column: 'grid' } });
  const app = buildContextChatCommand({ source_app: 'spreadsheets', text: 'x', mode: 'app', canModifyApp: true, title: 'T', thread_key: 'k' });
  assert.equal(app.type, APP_MODIFY_COMMAND);
  assert.equal(app.record_id, 'spreadsheets');
  assert.equal(app.payload.title, 'T');
  assert.equal(app.payload.thread_key, 'k');
  assert.equal(buildContextChatCommand({ source_app: 'a', text: 'x', mode: 'app' }).type, CONTEXT_CHAT_COMMAND, 'ohne Recht kein App-Ändern-Befehl');
  for (const bad of [{ text: 'x' }, { source_app: 'a', text: '  ' }, { source_app: 'a', text: 'x'.repeat(8001) }]) {
    assert.throws(() => buildContextChatCommand(bad), { code: 'invalid' });
  }
});

test('Absenden: api.command(type, payload, {record_id}) und Sprung in die Chat-App; eigener Sender ersetzt api.command', () => {
  const calls = [];
  const api = {
    command: (type, payload, opts) => { calls.push(['command', type, payload.text, opts]); return { id: 'c1' }; },
    shell: { navigate: (app, params) => { calls.push(['navigate', app, params]); return true; } },
  };
  const handle = sendContextChat(api, { source_app: 'mail', record_ref: 'm-1', text: 'Bitte antworten', mode: 'data' });
  assert.equal(handle.id, 'c1');
  assert.deepEqual(calls, [
    ['command', 'ctox.chat.task', 'Bitte antworten', { record_id: 'm-1' }],
    ['navigate', 'ctox', { command_id: 'c1', chat_id: 'c1', source_app: 'mail', mode: 'data' }],
  ]);
  calls.length = 0;
  const sent = [];
  sendContextChat(api, { source_app: 'mail', text: 'x' }, { send: (type, payload, opts) => { sent.push([type, opts]); return { id: 'c2' }; }, navigate: false });
  assert.deepEqual(sent, [['ctox.chat.task', { record_id: 'mail' }]]);
  assert.deepEqual(calls, [], 'kein api.command und kein Sprung');
  assert.throws(() => sendContextChat(api, { source_app: 'mail', text: '' }), { code: 'invalid' });
  assert.deepEqual(calls, []);
});
