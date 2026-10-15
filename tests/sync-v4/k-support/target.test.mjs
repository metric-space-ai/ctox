import test from 'node:test';
import assert from 'node:assert/strict';
import {parseTarget,resolveRevision,verdict,comparison} from './target.mjs';
test('unexecuted targets and incomplete scenarios never become a green V4 comparison',()=>{
 assert.equal(parseTarget('v4=v4/cutover').requested,'v4/cutover');
 assert.throws(()=>parseTarget('v4=--all'));
 assert.equal(resolveRevision('refs/heads/e2e-deliberately-absent-169a'),null);
 const missing={status:'not_run',pass:true,acceptanceComplete:true};
 assert.equal(verdict(missing),'not_run');
 assert.equal(verdict({status:'measured',pass:true,acceptanceComplete:false}),'red');
 assert.equal(verdict({status:'measured',pass:true,acceptanceComplete:true}),'green');
 const baseline={scenarios:{K01:{value:{losses:10},targetValue:{losses:0},verdict:'red'}}};
 const before=JSON.stringify(baseline);const out=comparison(baseline,[]);
 assert.equal(out[0].verdict,'not_run');assert.equal(JSON.stringify(baseline),before);
});
