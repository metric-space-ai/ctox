// Observe the actual E38 boot call; never provide/replace authority or peer IO.
export async function installBrowserEntry(context, origin) {
  await context.route(origin+'/kernel/boot.mjs',async route=>{
    const response=await route.fetch();
    if(response.status()!==200)return route.fulfill({response});
    const source=await response.text();
    // Original module bytes execute unchanged; append a result observer. The
    // real v4/boot.js still chooses launchContextUrl and resolveShell itself.
    const observer=`\n;{const actual=bootV4;bootV4=async function(options){globalThis.__p6BootOptions={launchContextUrl:options.launchContextUrl,resolveShell:typeof options.resolveShell};try{const result=await actual(options);globalThis.__p6Boot=result;return result;}catch(error){globalThis.__p6BootError={code:error.code??'unavailable',reason:error.reason??error.details?.reason??'boot-failed'};throw error;}};}\n`;
    await route.fulfill({response,body:source+observer});
  });
  return async(page,identity,fixture)=>{
    try{await page.waitForFunction(()=>globalThis.__p6Boot || globalThis.__p6BootError,{},{timeout:20000});}
    catch{throw Error('not_run(e38-entry): GET / did not call kernel bootV4');}
    const proof=await page.evaluate(({instanceId,userId,role})=>{
      const boot=globalThis.__p6Boot,options=globalThis.__p6BootOptions;
      if(!boot)throw Error('not_run(e38-boot): '+(globalThis.__p6BootError?.reason??'boot did not complete'));
      const {loader,kernel,transport:t,launchContext:launch}=boot;
      if(launch.instanceId!==instanceId || launch.userId!==userId || launch.actor?.id!==userId || launch.actor?.role!==role)
        throw Error('not_run(e38-identity): launch-context identity does not match enrolled user');
      if(!loader || loader.kernel!==kernel || !t?.session || t.closed || !Object.keys(t.channels??{}).length)
        throw Error('not_run(native-browser-hello): real native session was not admitted');
      const url=new URL(options.launchContextUrl??'/api/business-os/launch-context',location.href);
      if(url.origin!==location.origin || url.pathname!=='/api/business-os/launch-context' || options.resolveShell!=='function' || location.pathname!=='/')
        throw Error('not_run(e38-entry): launch-context → bootV4 → loader path absent');
      if(kernel.instanceId!==instanceId || kernel.userId!==userId || t.identity.userId!==userId || t.session.instance_id!==instanceId)
        throw Error('not_run(e38-identity): kernel/transport identity mismatch');
      if(kernel.storageKind!=='indexeddb')throw Error('Real kernel IndexedDB required');
      const shell=kernel.shellApi();
      globalThis.__e2eOS={kernel,shell,transport:t,transportKind:'native-webrtc',nativePeerId:launch.signaling.nativePeerId,
        regression:{contract:'ctox.sync-v4.regression-control.v1',
          async prepare(){throw Error('not_run(native-scenario-control): E38 defines authentication, not native regression fixture/commit/send-trace control');},
          async control(operation,args){
            if(operation!=='authenticate')throw Error('not_run(native-scenario-control): unsupported authenticated native operation');
            if(args.userId!==kernel.userId)throw Error('Identity switches require a separately authenticated browser context');
            return {userId:kernel.userId,instanceId:kernel.instanceId,role:launch.actor.role,is_admin:launch.actor.is_admin};
          }},
      };
      return {instanceId,userId,role:launch.actor.role,transportKind:'native-webrtc',helloAdmitted:true,loader:true,
        shellVersion:loader.shellVersion,storageKind:kernel.storageKind,entry:location.pathname,launchContext:true};
    },{instanceId:fixture.instanceId,userId:identity.id,role:identity.role});
    return {available:true,proof};
  };
}
