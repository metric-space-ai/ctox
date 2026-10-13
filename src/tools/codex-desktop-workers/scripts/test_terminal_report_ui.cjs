'use strict';
const assert=require('node:assert/strict'),vm=require('node:vm'),cp=require('node:child_process');
const report=process.argv[2]||'/Users/michaelwelsch/.codex/proxy-workers/MODEL-EXPERIENCE.html';
const html=cp.execFileSync('greppy',['rg','--no-heading','--no-line-number','^',report],{encoding:'utf8',maxBuffer:128*1024*1024});
const data=html.match(/<script id="report-data" type="application\/json">([\s\S]*?)<\/script>/)[1];
const scripts=[...html.matchAll(/<script>([\s\S]*?)<\/script>/g)];
const elements=new Map(),headers={parent:[],worker:[],pr:[]},modes=[];
class Element{
 constructor(id=''){this.id=id;this.value='';this.innerHTML='';this.textContent='';this.dataset={};this.attrs={};this.classList={toggle(){}}}
 get innerHTML(){return this._html||""}
 set innerHTML(v){this._html=v;this.selectorCache={}}
 setAttribute(k,v){this.attrs[k]=v}
 addEventListener(){}
 querySelector(s){if(!this.children)this.children={};return this.children[s]||(this.children[s]=new Element())}
 querySelectorAll(s){
 if(this.id!=='legend')return [];
 if(this.selectorCache[s])return this.selectorCache[s];
 const attr=s==='[data-color]'?'color':s==='[data-pair]'?'pair':null;if(!attr)return [];
 return this.selectorCache[s]=[...this.innerHTML.matchAll(new RegExp('data-'+attr+'="(\\d+)"','g'))].map(m=>{const e=new Element();e.dataset[attr]=m[1];return e});
 }}
for(const name of ['parent','worker','pr']){
 const table=html.match(new RegExp('<table id="'+name+'table">([\\s\\S]*?)</thead>'))[1];
 headers[name]=[...table.matchAll(/<th data-sort="([^"]+)">([^<]+)<\/th>/g)].map(m=>{const e=new Element();e.dataset.sort=m[1];e.textContent=m[2];return e});
}
for(const mode of ['first','end','arrows']){const e=new Element();e.dataset.mode=mode;modes.push(e)}
const document={
 getElementById(id){if(!elements.has(id))elements.set(id,new Element(id));return elements.get(id)},
 querySelectorAll(s){const m=s.match(/^#(parent|worker|pr)table/);if(m)return headers[m[1]];if(s==='[data-mode]')return modes;return []}
};
document.getElementById('report-data').textContent=data;
document.getElementById('size').value='25';
const context=vm.createContext({document,console,Set,Map,JSON,Number,String,Math,Object,Blob,URL,setTimeout});
const capture='\nglobalThis.testAPI={compare,sorted,refresh,drawChart,drawDistributions,pairKey,prKey,mergeTarget,parentScore,reworkCount,reworkLowerBound,reworkDisplay,current,actorCell,scoreCell,getAll:()=>all,getAssessments:()=>D.assessments,getPRs:()=>D.prs,getVisible:()=>visible,setVisible:rows=>{visible=rows;draw();},getBoards:()=>boards,getMode:()=>mode,getStyles:()=>pairStyles,getPairs:()=>D.parent_worker_pairs};';
vm.runInContext(scripts[0][1]+capture,context,{timeout:10000});
const api=context.testAPI;
assert.equal(headers.parent.length,4);assert.equal(headers.worker.length,5);assert.equal(headers.pr.length,7);
for(const name of ['parent','worker','pr'])for(const th of headers[name])assert.match(th.innerHTML,/<button type="button">/);
function click(name,key){const th=headers[name].find(h=>h.dataset.sort===key);th.querySelector('button').onclick();return th}
function numericOrder(values,dir){
 let last=null,missing=false;
 for(const v of values){if(v==null){missing=true;continue}assert.equal(missing,false,'Unknowns must stay last');if(last!=null)assert.ok(dir===1?v>=last:v<=last);last=v}
}
for(const name of ['parent','worker']){
 const key=name==='parent'?'score':'end';click(name,key);numericOrder(api.getBoards()[name].map(x=>x[key]).sort((a,b)=>api.compare(a,b,1)),1);
 const rendered=[...document.getElementById(name+'board').innerHTML.matchAll(/<tr><td>(.*?)<\/td>/g)].map(m=>m[1]);
 const expected=Array.from(api.sorted(api.getBoards()[name],{key,dir:1}),x=>x.label);
 assert.deepEqual(rendered,expected);
 assert.equal(headers[name].find(h=>h.dataset.sort===key).attrs['aria-sort'],'ascending');
 click(name,key);assert.equal(headers[name].find(h=>h.dataset.sort===key).attrs['aria-sort'],'descending');
}
const expectedBoards=JSON.parse(data).leaderboards.filter(g=>g.rubric==='unified-actor-v1'&&(g.observed_prs??g.prs)>0);
for(const role of ['parent','worker'])for(const actual of api.getBoards()[role]){
 const expected=expectedBoards.find(g=>g.role===role&&g.model+' (@'+(g.harness||'—')+')'===actual.label);
 assert.ok(expected,actual.label);
 assert.equal(actual.prs,expected.observed_prs??expected.prs,'Individual observations share the exported cohort');
 if(role==='worker')for(const [key,stage] of [['first','first'],['end','corrected']]){
  const field=expected['observed_'+stage]||expected[stage];
  assert.equal(actual[key],field.mean,'A valid unpaired endpoint must remain visible');
 }
 const rw=expected.observed_rework;
 assert.equal(actual.rework,rw?(rw.n===rw.denominator?rw.mean:null):expected.rework.mean,'Incomplete correction histories cannot produce a complete average');
}
// One closing Parent label/score; earlier source actors remain in JSON and iteration evidence.
const parentFixture={url:'fixture-parent-pr'};
const earlierParent={pr_url:parentFixture.url,role:'parent',actor_id:'source-parent',rubric:'unified-actor-v1',model:'gpt-6-sol',harness:'Codex Desktop',first:null,corrected:{model:'gpt-6-sol',weighted_total:7.2},parent_completion:null,rework_iterations:2,iteration_scope:'pr'};
const closingParent={pr_url:parentFixture.url,role:'parent',actor_id:'closing-parent',rubric:'unified-actor-v1',model:'gpt-6-sol',harness:'Codex Desktop',first:null,corrected:null,parent_completion:{model:'gpt-6.1-sol',weighted_total:7.7},rework_iterations:null};
api.getAll().push(earlierParent,closingParent);
assert.equal(api.current(parentFixture,'parent').length,1);
assert.equal(api.current(parentFixture,'parent')[0].actor_id,'closing-parent');
assert.match(api.actorCell(api.current(parentFixture,'parent'),'P'),/gpt-6\.1-sol \(@codex\)/);
assert.ok(!api.actorCell(api.current(parentFixture,'parent'),'P').includes('gpt-6-sol'));
assert.equal((api.scoreCell(api.current(parentFixture,'parent'),'parent_completion').match(/class="scorebar/g)||[]).length,1);
assert.equal(api.reworkCount(parentFixture),2,'Hidden earlier source evidence must still support the complete PR iteration count');
assert.equal(api.current(parentFixture,'parent',false).length,2);
api.getAll().pop();api.getAll().pop();
const closureOnly={url:'fixture-closure-only',closing_review:{actor_id:'actual-closer',model:'gpt-6.1-sol',harness:'codex'}};
assert.match(api.actorCell(api.current(closureOnly,'parent'),'P'),/gpt-6\.1-sol \(@codex\)/);
assert.equal(api.parentScore(api.current(closureOnly,'parent')[0]),null,'Closing model evidence must not invent a grade');
assert.equal(api.scoreCell(api.current(closureOnly,'parent'),'parent_completion'),'—');
assert.equal(api.current(closureOnly,'parent',false).length,0,'Metadata is not an assessment or iteration history');
assert.equal(api.reworkCount(closureOnly),null,'A proven closer does not prove zero corrections');
assert.equal(api.current({url:'fixture-no-provenance'},'parent').length,0);
for(const pr of JSON.parse(data).prs){
 const parents=api.current(pr,'parent');
 if(parents.some(a=>api.parentScore(a)!=null))assert.ok(parents.every(a=>api.parentScore(a)!=null),'No historical source-only parent rows beside a completion score');
}
// GitHub may retarget a merged PR; display the evidenced historical merge target.
assert.equal(api.mergeTarget({baseRefName:'main',merge_target:{branch:'codex/devops-unification'}}),'codex/devops-unification');
assert.equal(api.mergeTarget({baseRefName:'main'}),'main');
for(const pr of JSON.parse(data).prs.filter(p=>p.merge_target)){
 assert.equal(pr.merge_target.head,pr.headRefOid);
 assert.equal(pr.merge_target.merged_at,pr.mergedAt);
}
const savedList=Array.from(api.getVisible());
api.setVisible([{url:'historical-target-fixture',repository:'fixture/repo',number:1,title:'integration merge',state:'MERGED',baseRefName:'main',merge_target:{branch:'codex/devops-unification'}}]);
assert.ok(document.getElementById('prlist').innerHTML.includes('gemergt → codex/devops-unification'));
assert.ok(!document.getElementById('prlist').innerHTML.includes('gemergt → main'));
api.setVisible(savedList);
// A worker's known count does not prove the whole PR correction history.
const fixture={url:'fixture-pr'},actor={pr_url:'fixture-pr',role:'worker',actor_id:'fixture-worker',rubric:'unified-actor-v1',model:'gpt-6.1-sol',rework_iterations:2,iteration_scope:'actor',iteration_evidence:[{kind:'correction',head:'one'},{kind:'correction',head:'two'}]};
api.getAll().push(actor);
assert.equal(api.reworkCount(fixture),null,'Actor-only counts must not masquerade as complete PR totals');
assert.equal(api.reworkDisplay(fixture),'—','An incomplete actor count must not become an exact PR count');
actor.whole_pr_rework_iterations_lower_bound=2;
const boundedParent={...actor,actor_id:'fixture-parent',role:'parent',rework_iterations:null,whole_pr_rework_iterations_lower_bound:5};
api.getAll().push(boundedParent);
assert.equal(api.reworkCount(fixture),null);
assert.equal(api.reworkDisplay(fixture),'≥5','Overlapping whole-PR lower bounds use the maximum, never the sum');
assert.equal(api.prKey(fixture,'rework'),5,'Sort on the displayed proved minimum');
api.getAll().pop();
actor.whole_pr_rework_iterations_lower_bound=0;
assert.equal(api.reworkDisplay(fixture),'—','No proved correction is not proof of an exact zero');
delete actor.whole_pr_rework_iterations_lower_bound;
actor.iteration_scope='pr';assert.equal(api.reworkCount(fixture),2);
assert.equal(api.reworkDisplay(fixture),'2','An exact complete count remains an absolute number');
api.getAll().pop();
click('pr','parent');numericOrder(api.getVisible().map(p=>api.prKey(p,'parent')),-1);
const first=api.getVisible()[0];assert.ok(document.getElementById('prlist').innerHTML.includes(first.url));
click('pr','parent');numericOrder(api.getVisible().map(p=>api.prKey(p,'parent')),1);
assert.equal(api.getVisible().length,JSON.parse(data).prs.length);
const body=document.getElementById('prlist').innerHTML;
for(const row of body.matchAll(/<tr\b[^>]*>([\s\S]*?)<\/tr>/g)){
 const widths=[...row[1].matchAll(/<td\b([^>]*)>/g)].map(m=>Number(m[1].match(/colspan="(\d+)"/)?.[1]||1));
 assert.equal(widths.reduce((s,n)=>s+n,0),7);
}
assert.ok((body.match(/<tr\b/g)||[]).length<=25);
assert.equal(api.compare(null,9,1),1);assert.equal(api.compare(null,9,-1),1);
assert.ok(api.compare('model 2','model 10',1)<0);
const actualData=JSON.parse(data);
const roleScores=(role,stage)=>actualData.assessments.filter(a=>a.role===role&&a[stage]?.weighted_total!=null);
for(const [id,role,stages] of [['parent-distribution','parent',['parent_completion']],['worker-distribution','worker',['first','corrected']]]){
 const expected=stages.reduce((n,s)=>n+roleScores(role,s).length,0),svg=document.getElementById(id);
 assert.equal(Number(svg.dataset.total),expected,'Every retained score contributes exactly once to the distribution');
 assert.equal([...svg.innerHTML.matchAll(/data-count="(\d+)"/g)].reduce((n,m)=>n+Number(m[1]),0),expected);
}
assert.equal(document.getElementById('coverage').textContent,actualData.coverage.terminal_prs+' PRs · '+actualData.coverage.scored_prs+' mit Scores · '+actualData.coverage.pending_prs+' Prüfungen offen · '+actualData.coverage.stopped_prs+' STOP');
function markers(){
 return [...document.getElementById('scatter').innerHTML.matchAll(/<circle class="plotpoint" data-pr="([^"]+)" data-stage="([^"]+)" data-score="([^"]*)" data-parent="([^"]*)"/g)].map(m=>({pr:m[1],stage:m[2],score:m[3],parent:m[4]}));
}
function eligible(p,mode){return mode==='arrows'?p.first_end_scope_comparable!==false&&p.worker_first!=null&&p.worker_end!=null:p['worker_'+mode]!=null}
for(const name of ['first','end','arrows']){
 modes.find(m=>m.dataset.mode===name).onclick();
 const expected=api.getPairs().filter(p=>eligible(p,name)),points=markers();
 assert.equal(points.length,expected.length*(name==='arrows'?2:1),'Scatter contains only genuine source-bound pairs');
 assert.equal(document.getElementById('chart-count').textContent,expected.length+' belegte Paare');
 assert.equal(document.getElementById('scatter').attrs.viewBox,undefined,'Density must not grow chart extent');
 for(const point of points){
  const p=expected.find(p=>p.pr_url===point.pr&&p.parent_score===Number(point.parent)&&p['worker_'+point.stage]===Number(point.score));
  assert.ok(p,'Every displayed endpoint must match a retained joined assessment');
 }
 assert.equal((document.getElementById('scatter').innerHTML.match(/class="arrow"/g)||[]).length,name==='arrows'?expected.filter(p=>p.worker_first!==p.worker_end).length:0);
}
const expandedPair={...api.getPairs()[0],pr_url:'changed-scope-fixture',worker_first:4,worker_end:8,first_end_scope_comparable:false};
api.getPairs().push(expandedPair);
modes.find(m=>m.dataset.mode==='arrows').onclick();
assert.equal(markers().filter(p=>p.pr===expandedPair.pr_url).length,0,'Scope changes are not improvement arrows');
modes.find(m=>m.dataset.mode==='first').onclick();
assert.equal(markers().filter(p=>p.pr===expandedPair.pr_url).length,1,'Independent first score remains observable');
api.getPairs().pop();
const firstOnly={pr_url:'partial-fixture',record_id:'partial',actor_id:'partial',role:'worker',schema:'unified-actor-v1',first:{weighted_total:4,model:'gpt-6.1-sol'},corrected:null};
api.getAssessments().push(firstOnly);api.drawDistributions();
assert.equal(Number(document.getElementById('worker-distribution').dataset.total),roleScores('worker','first').length+roleScores('worker','corrected').length+1,'An unpaired Worker score belongs in the distribution');
api.getAssessments().pop();api.drawDistributions();
api.setVisible([{url:'pending-fixture',repository:'fixture/repo',number:2,title:'review pending',state:'CLOSED',coverage_reason:'Independent review pending'}]);
assert.match(document.getElementById('prlist').innerHTML,/colspan="5" class="review-pending"/);
assert.ok(!document.getElementById('prlist').innerHTML.includes('actor'),'No repeated empty actor/score matrix for pending reviews');
api.setVisible(savedList);
const selector=document.getElementById('pair'),color=document.getElementById('pair-color');
modes.find(m=>m.dataset.mode==='end').onclick();
const options=[...selector.innerHTML.matchAll(/<option value="[^"]*">([^<]+)<\/option>/g)].map(m=>m[1]);
assert.equal(new Set(options).size,options.length,'Visible model/harness combinations cannot have duplicate classifications');
assert.equal(options[0],'Alle Modellkombinationen');
assert.equal(color.disabled,true);
if(options.length>1){
 selector.value=api.pairKey(api.getPairs()[0]);selector.onchange();
 assert.equal(color.disabled,false);
 const expected=api.getPairs().filter(p=>api.pairKey(p)===selector.value&&eligible(p,'end'));
 assert.deepEqual([...new Set(markers().map(p=>p.pr))].sort(),[...new Set(expected.map(p=>p.pr_url))].sort(),'Selecting a combination hides all other pairs');
 color.value='#123456';color.oninput();assert.ok(document.getElementById('scatter').innerHTML.includes('#123456'));
 selector.value='';selector.onchange();assert.equal(color.disabled,true);
}
for(const point of document.getElementById('scatter').innerHTML.matchAll(/<circle class="plotpoint"[^>]* cx="([^"]+)" cy="([^"]+)" r="([^"]+)"/g)){
 const [x,y,r]=point.slice(1).map(Number);assert.ok(x-r>=0&&x+r<=930&&y-r>=0&&y+r<=300,'Scores stay inside a fixed compact chart');
}
assert.ok(!document.getElementById('parentboard').innerHTML.includes('Codex Desktop'));
assert.ok(!document.getElementById('workerboard').innerHTML.includes('Codex Desktop'));
assert.ok(!html.includes('<div class="filters">'));
console.log('PASS: retained individual Worker scores; separate comparison cohorts; all role-specific distribution scores; genuine bounded pair plot; filtering, arrows, sorting, pagination and pending rows.');
