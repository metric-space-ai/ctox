import { createCtx, loadSchema } from './_ctx.mjs';

export const NOW = 1_760_000_000_000;
const schemaUrl = new URL('../../collections.schema.json', import.meta.url);

export const topic = (o = {}) => ({
  id: 'topic_1', title: 'KI', prompt: 'Modelle', cadence: 'daily', language: 'de', categories: ['model'], max_posts: 5,
  enabled: true, payload: {}, created_at_ms: NOW - 10, updated_at_ms: NOW - 10, ...o,
});
export const source = (o = {}) => ({
  id: 'source_1', topic_id: 'topic_1', kind: 'reddit', label: 'Reddit', items: ['r/LocalLLaMA'], enabled: true,
  payload: {}, created_at_ms: NOW - 10, updated_at_ms: NOW - 10, ...o,
});

export async function setup({ seed = {}, caps = {}, commands = null, now = NOW } = {}) {
  return createCtx({ seed, caps, commands, now, schema: await loadSchema(schemaUrl) });
}
