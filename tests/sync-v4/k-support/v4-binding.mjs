// Browser-side preflight against the actual isolated daemon. Reuse only the
// existing signaling/RTC implementation: no V3 collection, query, lease, pull
// or write path is imported into the V4 scenario adapter.
export async function probeNativeV4({timeoutMs=30000}) {
 const result={startedAt:new Date().toISOString(),phase:'native-bootstrap',nativeConnected:false,kernelImported:false,protocolV4Negotiated:false,scenarioBinding:false,events:[],businessRequestsSent:0};
 const response=await fetch('/api/business-os/sync/config');
 result.bootstrapHttpStatus=response.status;
 if(!response.ok)throw Error('Native control-plane bootstrap HTTP '+response.status);
 const config=await response.json();
 result.bootstrap={instanceId:config.instance_id,nativePeerId:config.native_peer_id,nativePeerAvailable:config.native_rxdb_peer_available,syncMode:config.sync_mode};
 const raw=config.signaling_urls?.[0];
 if(!raw)throw Error('Native bootstrap has no signaling endpoint');
 const url=new URL(raw,location.href);
 result.signaling={protocol:url.protocol,port:url.port,path:url.pathname};
 if(!['127.0.0.1','localhost','[::1]'].includes(url.hostname))throw Error('Isolated fixture must not contact external signaling');
 // Same role-bound signaling contract as shared/sync.js, without importing its
 // V3 collection coordinator. Credential values never enter the returned proof.
 const now=Math.floor(Date.now()/1000);
 for(const [key,value] of Object.entries({token:config.signaling_browser_token,token_iat:now,token_exp:now+86400,auth_version:config.signaling_auth_version,browser_token_hash:config.signaling_browser_token_hash,native_token_hash:config.signaling_native_token_hash}))url.searchParams.set(key,String(value));
 const capabilities=['ctox-control-plane-v1','ctox-role-bound-signaling-v1','ctox-rxdb-browser-v1'];
 const {createCtoxWebRtcNativePeer}=await import('/rxdb/src/webrtc-native.mjs');
 const {WebRTCKernelTransport}=await import('/kernel/transport-webrtc.mjs');
 result.kernelImported=typeof WebRTCKernelTransport==='function';
 const peer=createCtoxWebRtcNativePeer({signalingUrl:url.href,room:config.sync_room,token:config.signaling_browser_token,instanceId:config.instance_id,role:'browser',clientId:'browser-v4-e2e-'+crypto.randomUUID(),capabilities,expectedNativePeerId:config.native_peer_id});
 const send=peer.send.bind(peer);peer.send=(id,frame,...rest)=>{if(frame?.method)result.businessRequestsSent++;return send(id,frame,...rest);};
 result.phase='native-rtc-connect';
 let timer;
 try{
  await new Promise((resolve,reject)=>{
   timer=setTimeout(()=>reject(Error('Native RTC channel not open before '+timeoutMs+'ms')),timeoutMs);
   peer.on('signaling-open',()=>result.events.push({at:Date.now(),type:'signaling-open'}));
   peer.on('error',({detail:error})=>result.events.push({at:Date.now(),type:'error',code:error?.code??error?.name}));
   peer.on('message',({detail:{payload}})=>result.events.push({at:Date.now(),type:'native-frame',frameType:payload?.type??null,protocol:payload?.protocol??null}));
   peer.on('peer-open',({detail:{peerId}})=>{result.nativeConnected=true;result.connectedPeerId=peerId;result.phase='native-v4-authorities';resolve();});
   peer.connect();
  });
  const pairs=[];
  for(const connection of peer.connections.values()){
   const pc=connection.peer;if(pc.connectionState!=='connected')continue;
   const stats=await pc.getStats();for(const stat of stats.values())if(stat.type==='transport'&&stat.selectedCandidatePairId){
    const pair=stats.get(stat.selectedCandidatePairId),local=stats.get(pair.localCandidateId),remote=stats.get(pair.remoteCandidateId);
    pairs.push({localType:local?.candidateType,remoteType:remote?.candidateType,rttMs:pair.currentRoundTripTime*1000});
   }
  }
  result.relay={pairs,forced:pairs.length>0&&pairs.every(p=>p.localType==='relay')};
  if(!result.relay.forced)throw Error('Selected candidate pair did not prove forced TURN');

  // A transport without host-owned hello, package verification and canonical
  // scopes cannot become a V4 scenario. Do not manufacture those authorities or
  // replace them with fixture signatures/MemoryCoreTransport.
  result.reason='Actual daemon RTC connection established; native host-owned V4 hello/device enrollment, package verifier and canonical scope resolver are not supplied to this browser entry point. No V4 business frame sent.';
  result.requiredAuthorities=['hello/device enrollment','verifyPackage','resolveScope'];
 }catch(error){result.error=error.message;result.reason='Native preflight failed before any V4 scenario: '+error.message;}
 finally{clearTimeout(timer);result.sockets=(globalThis.__e2eSockets??[]).map(s=>({...s.observation}));peer.close();result.finishedAt=new Date().toISOString();}
 return result;
}
