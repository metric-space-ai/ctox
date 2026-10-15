import {spawnSync} from 'node:child_process';
export function parseTarget(value='v3') {
 if(value==='v3')return {kind:'v3',revision:'4794af03db8cbf28babee0369af9c416f755019d'};
 if(!value.startsWith('v4=')||!value.slice(3)||value.slice(3).startsWith('-')||/[\s~^:{}\\]/.test(value.slice(3)))throw Error('--target v3 | v4=<branch/commit>');
 return {kind:'v4',requested:value.slice(3)};
}
export function resolveRevision(ref,cwd=process.cwd()) {
 for(const name of [ref,'refs/remotes/origin/'+ref]){
  const p=spawnSync('git',['rev-parse','--verify','--end-of-options',name+'^{commit}'],{cwd,encoding:'utf8'});
  if(p.status===0&&/^[a-f0-9]{40}$/.test(p.stdout.trim()))return p.stdout.trim();
 }
 return null;
}
export function verdict(row) {
 if(!row||!['measured','failed'].includes(row.status))return 'not_run';
 return row.status==='measured'&&row.pass===true&&row.acceptanceComplete===true?'green':'red';
}
export function comparison(baseline,rows) {
 return Object.entries(baseline.scenarios).map(([id,b])=>{
  const row=rows.find(r=>r.id===id);
  const criteria=row?.bindingOnly?Object.values(row.measured?.criteria??{}):[];
  const state=row?.bindingOnly?(criteria.some(c=>c.status==='FAIL'&&c.subs>0)?'red':'not_run'):verdict(row);
  return {id,v3Value:b.value,v3Verdict:b.verdict,v3Evidence:b.evidence,v4Value:row?.measured??null,targetValue:b.targetValue,
   verdict:state,reason:row?.reason??row?.error??'No V4 scenario executed',evidence:row?.evidence??null};
 });
}
export function markdown(rows) {
 const esc=x=>JSON.stringify(x??'—').replaceAll('|','\\|');
 return '| K | V3 value | V4 value | target value | red/green/not_run | evidence / reason |\n|---|---|---|---|---|---|\n'+rows.map(r=>`| ${r.id} | ${esc(r.v3Value)} | ${esc(r.v4Value)} | ${esc(r.targetValue)} | ${r.verdict} | ${esc(r.evidence??r.reason)} |`).join('\n')+'\n';
}
