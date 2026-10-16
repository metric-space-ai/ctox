// K-1g: files nach CAP-3b (#844) und model.run (#830) – App-API und Handler-Kontext im Fake-Core.
import { testLocks } from './web-locks.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createStubKernel } from '../kernel.mjs';

let n = 0;
async function mk(t, o = {}) {
  const k = createStubKernel({ locks: testLocks, instanceId: `k1g${process.pid}-${++n}`, userId: 'u1', latencyMs: 2, ...o });
  t.after(() => k.close());
  await k.ready;
  return k;
}
const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));
async function until(pred, ms = 4000) { const t0 = Date.now(); while (!(await pred())) { if (Date.now() - t0 > ms) throw new Error('until'); await tick(5); } }
const MOUNT = 'export function mount(){ return { unmount(){} }; }';
const sha = (b) => createHash('sha256').update(b).digest('hex');
const noBase64 = (v) => !JSON.stringify(v, (k, x) => (x instanceof Uint8Array ? `<${x.length} bytes>` : x)).includes('base64');
const cmdEnd = async (h) => { await until(() => ['done', 'failed'].includes(h.status.get().status)); return h.status.get(); };

test('CAP-3b App-API: files.put(bytes|{text}, {name, mime}) → {id, ref}; get/list/delete nur eigene App, Ergebnisse immer Objekte', async (t) => {
  const k = await mk(t);
  await k.sim.publishAppVersion('crm', '1', { files: { 'index.js': MOUNT } });
  await k.sim.publishAppVersion('hr', '1', { files: { 'index.js': MOUNT } });
  const crm = k.appApi('crm', '1'), hr = k.appApi('hr', '1');
  const a = await crm.files.put({ text: 'hallo' }, { name: 'a.txt', mime: 'text/plain' });
  assert.deepEqual(Object.keys(a).sort(), ['id', 'ref']);
  assert.equal(a.ref, sha('hallo'));
  const bin = await crm.files.put(Uint8Array.from([0xff, 0xfe, 0x00]), { name: 'b.bin' });
  const ga = await crm.files.get({ ref: a.ref });
  assert.deepEqual(ga, { found: true, id: a.id, ref: a.ref, name: 'a.txt', mime: 'text/plain', size: 5, text: 'hallo' });
  const gb = await crm.files.get({ id: bin.id });
  assert.ok(gb.bytes instanceof Uint8Array && gb.text === undefined, 'kein UTF-8 → bytes, nie Base64');
  assert.deepEqual([...gb.bytes], [0xff, 0xfe, 0x00]);
  assert.ok(noBase64(ga) && noBase64(gb));
  assert.deepEqual((await crm.files.list()).files.map((f) => f.name).sort(), ['a.txt', 'b.bin']);
  assert.deepEqual(await crm.files.get({ ref: sha('gibt es nicht') }), { found: false, ref: sha('gibt es nicht') });
  // fremde App: lesen und löschen verweigert, Liste leer
  await assert.rejects(hr.files.get({ ref: a.ref }), (e) => e.code === 'forbidden');
  await assert.rejects(hr.files.delete({ id: a.id }), (e) => e.code === 'forbidden');
  assert.deepEqual(await hr.files.list(), { files: [] });
  assert.deepEqual(await crm.files.delete({ id: a.id }), { deleted: true, id: a.id });
  assert.deepEqual((await crm.files.get({ id: a.id })).found, false);
});

test('#830 Handler: ctx.model(req) und ctx.cap.files.* – deterministisch, Objekte, fremde Datei verweigert, Deklaration nötig', async (t) => {
  const k = await mk(t);
  const h = (body) => `export default async function handle(c, ctx) { ${body} }`;
  await k.sim.publishAppVersion('crm', '1', {
    files: {
      'index.js': MOUNT,
      'handlers/run.js': h("const f = await ctx.cap.files.put({ name: 'n.txt', text: c.payload.text }); const got = await ctx.cap.files.get({ ref: f.ref }); const listed = await ctx.cap.files.list({}); const answer = await ctx.model({ input: c.payload.text }); return { f, got, listed, answer };"),
    },
    handlers: { 'crm.run': 'handlers/run.js' }, capabilitiesRequired: ['files', 'model'],
  });
  await k.sim.publishAppVersion('hr', '1', {
    files: { 'index.js': MOUNT, 'handlers/peek.js': h('return ctx.cap.files.get({ ref: c.payload.ref });'), 'handlers/think.js': h('return ctx.model({ input: "x" });') },
    handlers: { 'hr.peek': 'handlers/peek.js', 'hr.think': 'handlers/think.js' }, capabilitiesRequired: ['files'],
  });
  const r1 = await cmdEnd(k.appApi('crm', '1').command('crm.run', { text: 'Angebot prüfen' }));
  assert.equal(r1.status, 'done', JSON.stringify(r1));
  const { f, got, listed, answer } = r1.result;
  assert.deepEqual(got, { found: true, id: f.id, ref: sha('Angebot prüfen'), name: 'n.txt', mime: 'text/plain; charset=utf-8', size: Buffer.byteLength('Angebot prüfen'), text: 'Angebot prüfen' });
  assert.deepEqual(listed.files.map((x) => x.id), [f.id]);
  assert.match(answer.output_text, /^Simulierte Modellantwort \(fake-core\) für crm: Angebot prüfen$/);
  assert.equal(answer.output[0].content[0].text, answer.output_text);
  const r2 = await cmdEnd(k.appApi('crm', '1').command('crm.run', { text: 'Angebot prüfen' }));
  assert.equal(r2.result.answer.id, answer.id, 'deterministisch');
  const peek = await cmdEnd(k.appApi('hr', '1').command('hr.peek', { ref: f.ref }));
  assert.match(peek.error, /^forbidden/, 'Datei einer fremden App');
  const think = await cmdEnd(k.appApi('hr', '1').command('hr.think', {}));
  assert.match(think.error, /^forbidden: cap\.model\.run nicht in capabilities_required/);
});

test('#830 App-API: api.model.run(req) – Responses-förmig, nur mit deklariertem model', async (t) => {
  const k = await mk(t);
  await k.sim.publishAppVersion('crm', '1', { files: { 'index.js': MOUNT }, capabilitiesRequired: ['model'] });
  await k.sim.publishAppVersion('hr', '1', { files: { 'index.js': MOUNT } });
  const r = await k.appApi('crm', '1').model.run({ model: 'preset-a', input: [{ role: 'user', content: 'Hallo Welt' }] });
  assert.deepEqual([r.object, r.status, r.model, r.output_text], ['response', 'completed', 'preset-a', 'Simulierte Modellantwort (preset-a) für crm: Hallo Welt']);
  await assert.rejects(k.appApi('crm', '1').model.run('nur text'), (e) => e.code === 'invalid');
  await assert.rejects(k.appApi('hr', '1').model.run({ input: 'x' }), (e) => e.code === 'forbidden');
});
