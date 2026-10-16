// Same terminal-receipt/checksum contract as #810, with bounded-memory hashing
// for real debug executables larger than Node's 2 GiB whole-file read limit.
import fs from 'node:fs';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
export function hashFile(path) {
  const hash=createHash('sha256'),buffer=Buffer.allocUnsafe(1024*1024),fd=fs.openSync(path,'r');
  try {
    let size;
    while((size=fs.readSync(fd,buffer,0,buffer.length,null))!==0)hash.update(buffer.subarray(0,size));
    return hash.digest('hex');
  } finally {fs.closeSync(fd);}
}
export function verifiedBuild(path) {
  const build=JSON.parse(fs.readFileSync(path));
  if(!build.pass || !build.terminal || !/^[a-f0-9]{40}$/.test(build.revision))throw Error('Terminal real-daemon build required');
  if(hashFile(build.binary)!==build.binarySha256)throw Error('Daemon binary checksum differs from producer');
  return {...build,binary:resolve(build.binary),source:resolve(build.source)};
}
