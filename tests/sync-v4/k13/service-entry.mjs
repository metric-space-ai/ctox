// Test-only adapter for #810's Native lifecycle. No product/harness file edits.
import fs from 'node:fs';
import {resolve} from 'node:path';
import {hashFile} from './build-receipt.mjs';

export function installServiceEntry(Native) {
  Native.prototype.prepare=async function() {
    const {root,source,revision,owner,port}=this.config;
    if(fs.existsSync(root))throw Error('Never reuse a P6 tenant root');
    fs.mkdirSync(resolve(root,'runtime/home'),{recursive:true,mode:0o700});
    fs.writeFileSync(resolve(root,'.sync-v4-e2e.json'),JSON.stringify({owner,synthetic:true,source:revision}));
    for(const path of ['Cargo.toml','src','contracts'])fs.symlinkSync(resolve(source,path),resolve(root,path));
    fs.mkdirSync(resolve(root,'bin'));
    fs.symlinkSync(this.config.binary,resolve(root,'bin/ctox'));
    this.env.CTOX_BUSINESS_OS_WEB_ADDR='127.0.0.1:'+port;
    // V3 preparation writes SQLite directly. The real service owns schema
    // creation; native app/identity fixtures must use its authenticated control.
    return {profile:this.config.profile,fixtureReady:false,synthetic:true,
      reason:'Native scenario/control fixtures required; no legacy SQL seeding'};
  };
  Native.prototype.seed=function() {
    throw Error('Native scenario fixture/control is required; P6 never opens owner SQLite through the V3 seed helper');
  };
  Native.prototype.serve=function() {
    const args=['service','--foreground','--root',this.config.root];
    const actual=hashFile(this.config.binary);
    if(actual!==this.config.binarySha256)throw Error('P6 service binary changed after verification');
    const child=this.start('daemon',this.config.binary,args);
    fs.writeFileSync(resolve(this.config.output,'p6-daemon-entry.json'),JSON.stringify({
      revision:this.config.revision,binarySha256:actual,root:this.config.root,
      args,pid:child.pid,at:new Date().toISOString()
    },null,2),{mode:0o600});
  };
}
