import assert from 'node:assert/strict';
import http from 'node:http';
import { test } from 'node:test';
import { pathToFileURL } from 'node:url';

const bundle = process.env.CTOX_PI_TEST_DIST
  ? pathToFileURL(process.env.CTOX_PI_TEST_DIST).href
  : new URL('../dist/ctox-pi-sidecar.mjs', import.meta.url).href;
const { handleTurnRequest, defaultStreamFn } = await import(bundle);
const before = 'export const v = 1;\n';
const after = 'export const v = 2;\n';
const canary = 'PRIVATE_TOOL_OR_PROVIDER_TEXT';

function responseEvents(turn, step) {
  const id = `response-${turn}`;
  const item = step.tool
    ? { type: 'function_call', id: `fc-${turn}`, call_id: `call-${turn}`, name: step.tool, arguments: JSON.stringify(step.args), status: 'completed' }
    : { type: 'message', id: `msg-${turn}`, role: 'assistant', status: 'completed', content: [{ type: 'output_text', text: step.text ?? 'Done', annotations: [] }] };
  const response = { id, object: 'response', status: step.incomplete ? 'incomplete' : 'completed', model: 'MiniMax-M3', output: [item], usage: { input_tokens: 10, output_tokens: 10, total_tokens: 20 }, ...(step.incomplete ? { incomplete_details: { reason: 'max_output_tokens' } } : {}) };
  const events = [
    { type: 'response.created', response: { ...response, status: 'in_progress', output: [] } },
    { type: 'response.output_item.added', output_index: 0, item: { ...item, status: 'in_progress', ...(step.tool ? { arguments: '' } : { content: [] }) } },
    ...(step.tool ? [{ type: 'response.function_call_arguments.delta', item_id: item.id, output_index: 0, delta: item.arguments }]
      : [{ type: 'response.output_text.delta', item_id: item.id, output_index: 0, content_index: 0, delta: item.content[0].text }]),
    { type: 'response.output_item.done', output_index: 0, item },
    { type: step.incomplete ? 'response.incomplete' : 'response.completed', response },
  ];
  return events.map(event => `event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`).join('');
}

async function runFixture(steps, maxAssistantTurns = 8) {
  const requests = [];
  const failures = [];
  const server = http.createServer(async (req, res) => {
    try {
      assert.equal(req.url, '/v1/responses');
      assert.equal(req.headers.authorization, 'Bearer ctox-loopback');
      let text = '';
      for await (const chunk of req) {
        text += chunk;
        assert.ok(text.length < 1_000_000, 'bounded fixture request');
      }
      const body = JSON.parse(text);
      assert.equal(body.model, 'MiniMax-M3');
      assert.equal(body.stream, true);
      assert.ok(body.max_output_tokens === undefined || body.max_output_tokens > 0);
      const turn = requests.length;
      requests.push(body);
      assert.ok(turn < steps.length, 'unexpected additional model request');
      if (turn >= 1) assert.ok(body.input.some(item => item.type === 'function_call_output' && item.call_id === 'call-0'), 'read result reached real Responses adapter');
      if (turn >= 2) assert.ok(body.input.some(item => item.type === 'function_call_output' && item.call_id === 'call-1'), 'edit result reached real Responses adapter');
      res.writeHead(200, { 'content-type': 'text/event-stream' });
      res.end(responseEvents(turn, steps[turn]));
    } catch (error) {
      failures.push(error);
      res.writeHead(500);
      res.end('fixture assertion failed');
    }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    const result = await handleTurnRequest({
      id: 'responses-edit-fixture', prompt: 'Read then edit index.js; finish with a short answer.',
      files: { 'index.js': before }, tools: ['read', 'edit'], maxAssistantTurns,
      model: { id: 'MiniMax-M3', name: 'CTOX fixture', api: 'openai-responses', provider: 'ctox-gateway', baseUrl: `http://127.0.0.1:${server.address().port}/v1`, reasoning: false, input: ['text'], cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, contextWindow: 0, maxTokens: 0 },
    }, defaultStreamFn());
    assert.equal(failures.length, 0, failures.map(error => error.message).join('\n'));
    assert.equal(requests.length, steps.length);
    return result;
  } finally {
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  }
}

const read = { tool: 'read', args: { path: 'index.js' } };
const edit = { tool: 'edit', args: { path: 'index.js', edits: [{ oldText: 'v = 1', newText: 'v = 2' }] } };

test('real bundled Responses READ → EDIT → STOP publishes the exact memory edit', { timeout: 15000 }, async () => {
  const result = await runFixture([read, edit, { text: 'Done' }]);
  assert.equal(result.ok, true, result.error);
  assert.equal(result.messages.filter(message => message.role === 'assistant').length, 3);
  assert.equal(result.messages.filter(message => message.role === 'toolResult').length, 2);
  assert.equal(result.messages.filter(message => message.role === 'toolResult' && message.isError).length, 0);
  assert.equal(result.snapshot.find(file => file.path.endsWith('/index.js')).content, after);
});

for (const [name, steps, limit, reason, terminalCalls, toolErrors] of [
  ['provider incomplete after edit', [read, edit, { incomplete: true, text: canary }], 8, 'length', 0, 0],
  ['assistant bound immediately after edit', [read, edit], 2, 'toolUse', 1, 0],
  ['failed exact edit before provider incomplete', [read, { tool: 'edit', args: { path: 'index.js', edits: [{ oldText: canary, newText: canary }] } }, { incomplete: true, text: canary }], 8, 'length', 0, 1],
]) {
  test(`real Responses ${name} rejects its snapshot with count-only evidence`, { timeout: 15000 }, async () => {
    const result = await runFixture(steps, limit);
    assert.equal(result.ok, false);
    assert.equal(result.error, 'pi coding turn failed: incomplete_turn');
    assert.deepEqual(result.diagnostics, {
      terminal_stop_reason: reason, assistant_turns: steps.length, tool_calls: 2,
      terminal_tool_calls: terminalCalls, tool_results: 2, tool_errors: toolErrors,
      max_assistant_turns: limit,
    });
    assert.equal(result.snapshot, undefined);
    assert.equal(result.messages, undefined);
    assert.equal(result.events, undefined);
    assert.equal(JSON.stringify(result).includes(canary), false);
  });
}
