// Accept only the actual, retired installer transition in our synthetic prefix.
// This is a harness custody guard, never an alternate deployment entry point.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {createHash} from 'node:crypto';
export function installedProfile(file,build,lane,owner){
 file=resolve(file);
 if(!file.startsWith(join(lane,'artifacts')+'/'))throw Error('Private installation evidence required');
 const proof=JSON.parse(fs.readFileSync(file));
 if(proof.id!=='K11'||proof.terminal!==true||proof.installerExecuted!==true||proof.dataPreserved!==true||proof.controller?.groupAbsent!==true||proof.afterRevision!==build.revision||proof.installer?.activeBinarySha256!==build.binarySha256||!proof.beforeSnapshot?.quiesced||!proof.afterSnapshot?.quiesced)throw Error('Incomplete or mismatched installed profile');
 const root=resolve(proof.root),current=fs.realpathSync(join(root,'managed/current'));
 if(!root.startsWith(join(lane,'artifacts')+'/')||current!==proof.currentRelease||!current.startsWith(root+'/managed/releases/'))throw Error('Installed profile path mismatch');
 const state=join(root,'runtime');
 if(fs.lstatSync(state).isSymbolicLink()||!fs.lstatSync(state).isDirectory()||fs.realpathSync(join(current,'runtime'))!==state)throw Error('Installed runtime must remain inside the owned profile');
 const marker=JSON.parse(fs.readFileSync(join(root,'.sync-v4-e2e.json')));
 if(marker.owner!==owner||marker.synthetic!==true||createHash('sha256').update(fs.readFileSync(join(current,'bin/ctox'))).digest('hex')!==build.binarySha256)throw Error('Installed profile owner/binary mismatch');
 return {root:current,preparedProfile:{evidence:file,root,fixture:proof.afterFixture}};
}
