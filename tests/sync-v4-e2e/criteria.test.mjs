import test from 'node:test';
import assert from 'node:assert/strict';
import { completeReload, completeFirstWindow, checkpointDelivered, catalogAccepted } from './criteria.mjs';
test('fast but incomplete data and an absent positive control cannot become acceptance', () => {
  const good = { visibleMs: 100, rows: 2, preQueryLocalCount: 2, missing: [], duplicates: 0, emptyRender: false, receivesEvents: true };
  assert.equal(completeReload(good), true);
  assert.equal(completeReload({ ...good, missing: ['native-row'] }), false);
  assert.equal(completeReload({ ...good, preQueryLocalCount: 1 }), false);
  assert.equal(completeReload({ ...good, visibleMs: NaN }), false);
  assert.equal(completeFirstWindow({ ...good, missing: ['native-row'] }), false);
  assert.equal(completeFirstWindow({ ...good, receivesEvents: false }), false);
  const row = { id: 'e2e-row', lwt: 42 };
  assert.equal(checkpointDelivered({found:true}, {found:false}, row, row), false);
  assert.equal(checkpointDelivered({found:true}, {found:true}, row, row), false);
  assert.equal(checkpointDelivered({found:true}, {found:true,nativeConfirmed:true}, row, row), true);
});

test('catalog acceptance requires actual complete measured scenarios, never zero or a partial slice',()=>{
 const report={terminal:true,cleanup:'owned groups absent',scenarios:[{id:'K01',status:'measured',pass:true,acceptanceComplete:true}]};
 assert.equal(catalogAccepted(report,['K01']),true);
 assert.equal(catalogAccepted({...report,scenarios:[]},[]),false);
 assert.equal(catalogAccepted({...report,scenarios:[{...report.scenarios[0],acceptanceComplete:false}]},['K01']),false);
 assert.equal(catalogAccepted({...report,scenarios:[{...report.scenarios[0],status:'not_run'}]},['K01']),false);
});
