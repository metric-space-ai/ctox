// Observe the actual E38 boot call; never provide/replace authority or peer IO.
export async function installBrowserEntry(context, origin) {
  await context.route(origin+'/kernel/boot.mjs',async route=>{
    const response=await route.fetch();
    if(response.status()!==200)return route.fulfill({response});
    const source=await response.text();
    // Original module bytes execute unchanged; append a result observer. The
    // real v4/boot.js still chooses launchContextUrl and resolveShell itself.
    const observer=`\n;{const actual=bootV4;bootV4=async function(options){const o=options??{};globalThis.__p6BootOptions={launchContextUrl:o.launchContextUrl,resolveShell:typeof o.resolveShell,customAdapter:['createPeer','fetchImpl','loaderFactory','subtle'].some(key=>o[key]!==undefined)};try{const result=await actual(options);globalThis.__p6Boot=result;return result;}catch(error){globalThis.__p6BootError={code:error.code??'unavailable',reason:error.reason??error.details?.reason??'boot-failed'};throw error;}};}\n`;
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
      if(url.origin!==location.origin || url.pathname!=='/api/business-os/launch-context' || options.resolveShell!=='function' || options.customAdapter || location.pathname!=='/')
        throw Error('not_run(e38-entry): launch-context → bootV4 → loader path absent');
      if(kernel.instanceId!==instanceId || kernel.userId!==userId || t.identity.userId!==userId || t.session.instance_id!==instanceId)
        throw Error('not_run(e38-identity): kernel/transport identity mismatch');
      if(kernel.storageKind!=='indexeddb')throw Error('Real kernel IndexedDB required');
      const deviceId=launch.hello.offer.device_proof.device_id;
      if(!deviceId || kernel.identity?.device_id!==deviceId)
        throw Error('not_run(native-device-enrollment): admitted device is not bound to kernel identity');
      const shell=kernel.shellApi();
      globalThis.__e2eOS={kernel,shell,transport:t,transportKind:'native-webrtc',nativePeerId:launch.signaling.nativePeerId,
        // Authentication is supplied by E38. Native regression operations are
        // deliberately absent until a real owner-backed provider exists.
        authenticatedControl:{async authenticate(args){
          if(args.userId!==kernel.userId || args.deviceId!==deviceId)throw Error('A separately authenticated enrolled context is required');
          return {userId:kernel.userId,deviceId,instanceId:kernel.instanceId,role:launch.actor.role,is_admin:launch.actor.is_admin};
        }},
      };
      return {instanceId,userId,deviceId,role:launch.actor.role,transportKind:'native-webrtc',helloAdmitted:true,loader:true,
        shellVersion:loader.shellVersion,storageKind:kernel.storageKind,entry:location.pathname,launchContext:true};
    },{instanceId:fixture.instanceId,userId:identity.id,role:identity.role});
    return {available:true,proof};
  };
}
