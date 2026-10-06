import assert from 'node:assert/strict';
import { test } from 'node:test';
import { pathToFileURL } from 'node:url';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';

const bundle = process.env.CTOX_PI_TEST_DIST
  ? pathToFileURL(process.env.CTOX_PI_TEST_DIST).href
  : new URL('../dist/ctox-pi-sidecar.mjs', import.meta.url).href;
const { handleTurnRequest, createVercelPiCodingTextMessage, createVercelPiCodingToolCallMessage } = await import(bundle);

for (const [detail, expected] of [
  ['524 Gateway Time-out: https://private.invalid/?token=SECRET Authorization: Bearer SECRET', 'provider_error (provider HTTP 524)'],
  ['HTTP status: 503: upstream body SECRET', 'provider_error (provider HTTP 503)'],
  ['418 {"message":"SECRET"}', 'provider_error (provider HTTP 418)'],
  ['Private URL https://private.invalid/524?token=SECRET', 'provider_error'],
  ['Bearer SECRET: HTTP 503 upstream failure', 'provider_error'],
  ['5031 SECRET failure', 'provider_error'],
  ['401 Unauthorized Bearer SECRET', 'authentication_error'],
  ['429 rate limit SECRET', 'rate_limited'],
  ['request timed out SECRET', 'timeout'],
]) {
  test('failed stream retains only safe classification: ' + expected, async () => {
    const response = await handleTurnRequest({
      id: 'safe-status', prompt: 'Produce a bounded source helper', files: {'index.js': 'export const original = 1;'},
    }, () => {
      const stream = createAssistantMessageEventStream();
      const message = {...createVercelPiCodingTextMessage('SECRET partial source'), stopReason: 'error', errorMessage: detail};
      stream.push({type: 'error', reason: 'error', error: message});
      return stream;
    });
    assert.deepEqual(response, {id: 'safe-status', ok: false, error: 'pi coding turn failed: ' + expected});
    assert.equal(JSON.stringify(response).includes('SECRET'), false);
  });
}

test('provider failure after a tool edit cannot release the partial source', async () => {
  const response = await handleTurnRequest({
    id: 'partial-status', prompt: 'Make coordinated edits', files: {'index.js': 'export const original = 1;'},
  }, (_model, context) => {
    const stream = createAssistantMessageEventStream();
    if (!context.messages.some(message => message.role === 'toolResult')) {
      stream.push({type:'done',reason:'toolUse',message:createVercelPiCodingToolCallMessage('write',{path:'index.js',content:'SECRET partial source'},'partial-edit')});
    } else {
      const message = {...createVercelPiCodingTextMessage('SECRET partial response'), stopReason:'error', errorMessage:'500 SECRET Authorization: Bearer SECRET'};
      stream.push({type:'error',reason:'error',error:message});
    }
    return stream;
  });
  assert.deepEqual(response,{id:'partial-status',ok:false,error:'pi coding turn failed: provider_error (provider HTTP 500)'});
  assert.equal(JSON.stringify(response).includes('SECRET'), false);
});
