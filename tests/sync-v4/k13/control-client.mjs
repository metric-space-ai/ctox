// Playwright host -> private native Unix socket. No browser HTTP data bridge.
import net from 'node:net';
export function nativeControl(config) {
  return (identity,operation,args={})=>new Promise((resolve,reject)=>{
    const socket=net.createConnection(config.root+'/p6-control.sock');
    let data='',settled=false;const timer=setTimeout(()=>socket.destroy(Error('native control deadline')),12000);
    socket.setEncoding('utf8');
    socket.on('connect',()=>socket.write(JSON.stringify({cookie:identity.cookie,operation,args})+'\n'));
    socket.on('data',chunk=>{
      data+=chunk;
      if(data.length>16*1024*1024)return socket.destroy(Error('native control response budget'));
      if(!data.includes('\n'))return;
      try{const response=JSON.parse(data);settled=true;if(!response.ok)reject(Error(response.error));else resolve(response.value);}
      catch{reject(Error('invalid native control response'));}
      socket.end();
    });
    socket.once('error',error=>reject(Error(error.code==='ENOENT'?'native control socket unavailable':error.message)));
    socket.once('close',()=>{clearTimeout(timer);if(!settled)reject(Error('native control closed without response'));});
  });
}
