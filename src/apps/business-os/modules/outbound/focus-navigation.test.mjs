import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('./index.js', import.meta.url), 'utf8');
const focus = source.match(/function focusRequestedOutboundRecord\(\) \{[\s\S]*?\n\}/)?.[0];
assert.ok(focus);
const receipts = [];
const microtasks = [];
const status = { dataset: {} };
const center = { dataset: {}, getClientRects: () => [1] };
let rendered = [];
// v4: App-root statt ctx.host, Rückmeldungen in die Zustandszone (reportStatus/clearStatus), Montage-Token.
const zone = [];
const state = {
  root: {
    querySelector: (selector) => selector === '.outbound-center' ? center : status,
    querySelectorAll: () => rendered,
  },
  mountToken: {},
  campaigns: [{ id: 'campaign' }],
  companies: [{ id: 'company', campaign_id: 'campaign' }],
  pipeline: [{ id: 'pipeline', campaign_id: 'campaign' }],
  engagements: [{ id: 'engagement', campaign_id: 'campaign' }],
  runs: [], activeOutreach: {},
};
const context = vm.createContext({ state, queueMicrotask: (fn) => microtasks.push(fn),
  reportOutboundFocus: (status, id) => receipts.push({ status, id }),
  reportStatus: (level, code, text) => zone.push({ level, code, text }),
  clearStatus: () => {},
});
vm.runInContext(focus, context);
function navigate(id, { hidden = false, staleCampaign = false } = {}) {
  state.requestedRecordId = id;
  context.focusRequestedOutboundRecord();
  // Render boundary follows the same mode choice as renderCenter.
  const renderedId = state.outreachView ? state.activeOutreach.selectedEngagementId
    : state.activeView === 'pipeline' ? state.selectedPipelineId : state.selectedCompanyId;
  center.dataset.renderedCampaignId = staleCampaign ? 'other' : state.selectedCampaignId;
  rendered = [{ dataset: { contextRecordId: renderedId }, getClientRects: () => hidden ? [] : [1] }];
  microtasks.splice(0).forEach((fn) => fn());
  return renderedId;
}
for (const id of ['company', 'pipeline', 'campaign']) {
  navigate('engagement');
  assert.equal(state.outreachView, true);
  assert.equal(receipts.at(-1).id, 'engagement');
  const visible = navigate(id);
  assert.equal(state.outreachView, false, `${id} leaves outreach mode`);
  if (id !== 'campaign') assert.equal(visible, id);
  assert.equal(receipts.at(-1).id, id);
  assert.equal(state.requestedRecordId, '');
}
const before = receipts.length;
navigate('company', { hidden: true });
assert.equal(receipts.length, before, 'hidden record cannot confirm focus');
assert.equal(state.requestedRecordId, 'company');
navigate('pipeline', { staleCampaign: true });
assert.equal(receipts.length, before, 'old campaign render cannot confirm focus');
assert.equal(state.requestedRecordId, 'pipeline');
// Unbekannter Datensatz: Hinweis nur in der Zustandszone, genau ein 'unavailable'-Receipt.
const n = receipts.length;
navigate('gibt-es-nicht');
navigate('gibt-es-nicht');
assert.equal(receipts.length, n + 1);
assert.equal(receipts.at(-1).status, 'unavailable');
assert.equal(zone.at(-1).code, 'outbound.route.record');
assert.equal(status.textContent, undefined, 'kein Statustext im Inhalt');
console.log('Outbound repeated focus/mode/render receipt regression passed');
