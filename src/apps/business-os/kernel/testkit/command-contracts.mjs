import {observeCommand} from '../command-observer.mjs';
// Shared by the active browser harness and the isolated browser proof. A real
// host supplies its already authenticated kernel; absence never selects V3.
export async function exerciseKernelCommands(kernel,{appId='probe',appVer='1',type='business_os.smoke',samples=1,timeoutMs=8000}={}) {
  if(!kernel?.appApi)throw Error('Authenticated V4 kernel required for command acceptance');
  const api=kernel.appApi(appId,appVer),results=[],ids=new Set();
  for(let i=0;i<samples;i++){
    const handle=api.command(type,{probe:i}),observer=observeCommand(handle,{timeoutMs});
    try{
      await observer.local;const terminal=await observer.done;
      if(terminal.status!=='done')throw Error('Canonical terminal success required');
      if(ids.has(handle.id))throw Error('Command identity reused');ids.add(handle.id);
      const sample=observer.snapshot();
      for(const stage of ['local_persistence','total'])if(!Number.isFinite(sample.stagesMs[stage])||sample.stagesMs[stage]<0)throw Error('Missing measured stage '+stage);
      results.push({id:handle.id,status:terminal.status,...sample});
    }finally{observer.close();}
  }
  return {samples:results,commandIds:[...ids],storage:kernel.storageKind};
}
