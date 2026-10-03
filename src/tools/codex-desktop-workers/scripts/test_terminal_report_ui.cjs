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
const capture='\nglobalThis.testAPI={compare,sorted,refresh,drawChart,prKey,parentScore,getVisible:()=>visible,getBoards:()=>boards,getMode:()=>mode,getStyles:()=>pairStyles,getPairs:()=>D.parent_worker_pairs};';
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
const legend=document.getElementById('legend'),colors=legend.querySelectorAll('[data-color]');
assert.equal(colors.length,api.getStyles().size);
if(colors.length){
 colors[0].value='#123456';colors[0].oninput();assert.ok(document.getElementById('scatter').innerHTML.includes('#123456'));
 const visibility=legend.querySelectorAll('[data-pair]');visibility[0].checked=false;visibility[0].onchange();
 assert.ok((document.getElementById('scatter').innerHTML.match(/class="plotpoint"/g)||[]).length<realPairs.length*2);
}
const query=document.getElementById('query');query.value='NO_SUCH_PR_987654321';api.refresh();assert.equal(api.getVisible().length,0);assert.match(document.getElementById('page').textContent,/0–0 von 0/);query.value='';api.refresh();
console.log('PASS: sortable 4/5/7 columns, numeric and null ordering, full-list sorting before pagination, score bars, filtered chart with real edges and first/end/arrows/color controls.');

