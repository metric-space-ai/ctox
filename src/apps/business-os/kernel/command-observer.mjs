import {CtoxError,CTOX_ERROR_CODES} from './values.mjs';
const terminal=new Set(['done','failed','cancelled','rejected']);
/** Observe the original durable command. A deadline never cancels or resubmits
 * it. Only local persistence, native write acceptance and observed status are
 * timed; missing native stages are never manufactured from a healthy link. */
export function observeCommand(handle,{timeoutMs=8000,now=()=>performance.now(),onTiming=()=>{},report=()=>{}}={}) {
  if(!handle?.id || !handle.local || !handle.result || !handle.status?.subscribe || !Number.isFinite(timeoutMs) || timeoutMs<=0)throw new CtoxError('invalid','Command handle and finite positive deadline required');
  const marks={dispatch_started:now()};let off,timer,settled=false,resolve,reject;
  const snapshot=()=>{
    const phases={};
    for(const [name,a,b]of [['local_persistence','dispatch_started','local_inserted'],['native_acceptance','local_inserted','write_accepted'],['total','dispatch_started','terminal_observed']])if(marks[a]!==undefined&&marks[b]!==undefined)phases[name]=Math.max(0,marks[b]-marks[a]);
    return {commandId:handle.id,marks:{...marks},stagesMs:phases,...(marks.terminal_observed===undefined?{pendingStage:marks.local_inserted===undefined?'local_persistence':marks.write_accepted===undefined?'native_acceptance':'terminal_observation',pendingMs:Math.max(0,now()-(marks.write_accepted??marks.local_inserted??marks.dispatch_started))}:{})};
  };
  const emit=()=>{try{onTiming(snapshot());}catch{}};
  const finish=(error,value)=>{if(settled)return;settled=true;clearTimeout(timer);off?.();emit();error?reject(error):resolve(value);};
  const done=new Promise((r,j)=>{resolve=r;reject=j;});done.catch(()=>{});
  timer=setTimeout(()=>{
    const error=new CtoxError('unavailable','Command progress deadline exceeded',{detail:{command_id:handle.id}});
    try{report({level:'error',code:'command.no_progress',text:error.message,ref:handle.id});}catch{}
    finish(error);
  },timeoutMs);
  const local=Promise.resolve(handle.local).then(()=>{marks.local_inserted=now();emit();},error=>{finish(error);throw error;});local.catch(()=>{});
  const accepted=local.then(()=>handle.result).then(result=>{
    if(result.status==='rejected'){const error=new CtoxError(CTOX_ERROR_CODES.includes(result.code??result.reason)?(result.code??result.reason):'unavailable','Native command write rejected',{reason:result.reason,detail:{command_id:handle.id}});finish(error);throw error;}
    marks.write_accepted=now();emit();return result;
  });accepted.catch(()=>{});
  const listener=state=>{
    if(!terminal.has(state?.status))return;
    marks.terminal_observed=now();
    if(state.status==='done')finish(null,state);
    else finish(new CtoxError('unavailable','Command did not complete successfully',{reason:state.status,detail:{command_id:handle.id,error:state.error}}));
  };
  off=handle.status.subscribe(listener);if(settled)off();else listener(handle.status.get());
  return Object.freeze({id:handle.id,local,accepted,done,snapshot,close(){finish(new CtoxError('unavailable','Command observer closed',{detail:{command_id:handle.id}}));}});
}
