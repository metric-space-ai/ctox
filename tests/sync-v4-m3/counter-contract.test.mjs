import assert from 'node:assert/strict';
import test from 'node:test';
import { reconcile } from './counters.mjs';

const attempt = {kind:'attempt',class:'control',id:'one'};
const accepted = {kind:'send_result',class:'control',id:'one',ok:true};
const delivered = {kind:'received',class:'control',id:'one',bytes:1};

test('SCTP acceptance lost during a drop remains an explicit M3 failure', () => {
 const row = reconcile([attempt,accepted], [], 600).control;
 assert.equal(row.reliableCountersEqual,false);
 assert.deepEqual(row.acceptedButNotReceived,['one']);
});
test('a duplicate delivery cannot masquerade as counter conservation', () => {
 const row = reconcile([attempt,accepted], [delivered,delivered],600).control;
 assert.equal(row.reliableCountersEqual,false);
 assert.equal(row.duplicateDeliveries,1);
});
test('explicitly failed attempts are distinct from accepted but missing messages', () => {
 const row = reconcile([attempt,{...accepted,ok:false}],[],600).control;
 assert.equal(row.reliableCountersEqual,true);
 assert.equal(row.explicitSendFailures,1);
 assert.equal(row.sendResultMissing,0);
 assert.deepEqual(row.acceptedButNotReceived,[]);
});
