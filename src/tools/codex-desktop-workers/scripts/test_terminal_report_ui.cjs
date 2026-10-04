'use strict';
const assert=require('node:assert/strict'),vm=require('node:vm'),cp=require('node:child_process');
const report=process.argv[2]||'/Users/michaelwelsch/.codex/proxy-workers/MODEL-EXPERIENCE.html';
const html=cp.execFileSync('greppy',['rg','--no-heading','--no-line-number','^',report],{encoding:'utf8',maxBuffer:32*1024*1024});
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
const capture='\nglobalThis.testAPI={compare,sorted,refresh,drawChart,prKey,mergeTarget,parentScore,reworkCount,current,actorCell,scoreCell,getAll:()=>all,getVisible:()=>visible,setVisible:rows=>{visible=rows;draw();},getBoards:()=>boards,getMode:()=>mode,getStyles:()=>pairStyles,getPairs:()=>D.parent_worker_pairs};';
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
const expectedBoards=JSON.parse(data).leaderboards.filter(g=>g.rubric==='unified-actor-v1'&&g.prs>0);
for(const role of ['parent','worker'])for(const actual of api.getBoards()[role]){
 const expected=expectedBoards.find(g=>g.role===role&&g.model+' (@'+(g.harness||'—')+')'===actual.label);
 assert.ok(expected,actual.label);
 assert.equal(actual.rework,expected.rework.mean,'Leaderboard must show average corrections per PR');
 if(actual.rework!=null){assert.equal(actual.rework,expected.rework.iterations/actual.prs);assert.ok(document.getElementById(role+'board').innerHTML.includes('>'+actual.rework.toFixed(1)+'</td>'));}
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
const fixture={url:'fixture-pr'},actor={pr_url:'fixture-pr',role:'worker',actor_id:'fixture-worker',rubric:'unified-actor-v1',model:'m',rework_iterations:2,iteration_scope:'actor',iteration_evidence:[{kind:'correction',head:'one'},{kind:'correction',head:'two'}]};
api.getAll().push(actor);
assert.equal(api.reworkCount(fixture),null,'Actor-only counts must not masquerade as complete PR totals');
actor.iteration_scope='pr';assert.equal(api.reworkCount(fixture),2);
api.getAll().pop();
click('pr','parent');numericOrder(api.getVisible().map(p=>api.prKey(p,'parent')),-1);
const first=api.getVisible()[0];assert.ok(document.getElementById('prlist').innerHTML.includes(first.url));
click('pr','parent');numericOrder(api.getVisible().map(p=>api.prKey(p,'parent')),1);
assert.equal(api.getVisible().length,JSON.parse(data).prs.length);
const body=document.getElementById('prlist').innerHTML;
for(const row of body.matchAll(/<tr>([\s\S]*?)<\/tr>/g))assert.equal((row[1].match(/<td\b/g)||[]).length,7);
assert.ok((body.match(/<tr>/g)||[]).length<=25);
assert.equal(api.compare(null,9,1),1);assert.equal(api.compare(null,9,-1),1);
assert.ok(api.compare('model 2','model 10',1)<0);
assert.ok(html.includes('Parent Score'));assert.ok(!html.includes('<th>Parent Erst'));
for(const mode of modes){mode.onclick();assert.equal(api.getMode(),mode.dataset.mode);assert.equal(mode.attrs['aria-pressed'],'true')}
const svg=document.getElementById('scatter').innerHTML;
assert.ok(svg.includes('Parent Score')&&svg.includes('Worker Score'));
for(const p of api.getPairs()){
 assert.ok(p.parent_id&&p.worker_id&&p.parent_record_id&&p.worker_record_id);
 assert.ok(p.parent_score>=0&&p.parent_score<=10);
}
assert.ok(api.getPairs().length>0,'Actual report must exercise paired scores');
const realPairs=api.getPairs().filter(p=>p.worker_first!=null&&p.worker_end!=null);
assert.equal((svg.match(/class="plotpoint"/g)||[]).length,realPairs.length*2);
assert.equal((svg.match(/class="arrow"/g)||[]).length,realPairs.filter(p=>p.worker_first!==p.worker_end).length);
const selector=document.getElementById('pair'),color=document.getElementById('pair-color');
const options=[...selector.innerHTML.matchAll(/<option value="[^"]*">([^<]+)<\/option>/g)].map(m=>m[1]);
assert.equal(new Set(options).size,options.length,'Identical visible model/harness pairs must share one classification');
assert.equal(options.length,api.getStyles().size+1);
assert.equal(options[0],'Alle Kombinationen');
assert.equal(color.disabled,true);
assert.ok(!html.includes('id="legend"'));
assert.ok(options.slice(1).every(label=>(label.match(/\(@[^)]+\)/g)||[]).length===2));
const nativeClaude=api.getAll().filter(a=>a.harness==='claude-desktop');
if(nativeClaude.length){
 assert.ok([...api.getBoards().parent,...api.getBoards().worker].some(row=>row.label.includes('(@claude)')));
 for(const id of ['parentboard','workerboard','prlist'])assert.ok(!document.getElementById(id).innerHTML.includes('claude-desktop'));
}
if(options.length>1){
 selector.value=realPairs[0].combination;selector.onchange();
 assert.equal(color.disabled,false);assert.ok(document.getElementById('scatter').innerHTML.includes('opacity="0.16"'));
 color.value='#123456';color.oninput();assert.ok(document.getElementById('scatter').innerHTML.includes('#123456'));
 selector.value='';selector.onchange();assert.equal(color.disabled,true);assert.ok(!document.getElementById('scatter').innerHTML.includes('opacity="0.16"'));
}
assert.ok(!document.getElementById('parentboard').innerHTML.includes('Codex Desktop'));
assert.ok(!document.getElementById('workerboard').innerHTML.includes('Codex Desktop'));
assert.ok(!document.getElementById('prlist').innerHTML.includes('Codex Desktop'));
assert.ok(!html.includes('<div class="filters">'));
console.log('PASS: sortable 4/5/7 columns, numeric and null ordering, full-list sorting before pagination, score bars, chart with real edges and first/end/arrows/color controls.');

