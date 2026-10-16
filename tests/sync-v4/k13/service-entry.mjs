// Test-only adapter for #810's Native lifecycle. No product/harness file edits.
import fs from 'node:fs';
import {resolve} from 'node:path';
import {spawnSync,execFileSync} from 'node:child_process';
import {hashFile} from './build-receipt.mjs';

export function installServiceEntry(Native) {
  const start=Native.prototype.start;
  Native.prototype.start=function(...args) {
    const child=start.apply(this,args);
    const record=this.receipts.find(row=>row.pid===child.pid);
    record.owner=this.config.owner;record.pidNamespace=fs.readlinkSync('/proc/self/ns/pid');
    record.pidScope='private namespace';this.save();return child;
  };
  Native.prototype.prepare=async function() {
    const {root,source,revision,owner}=this.config;
    const mounts=[];
    for(const [index,path] of [root,root+'.voter2',root+'.voter3'].entries()) {
      if(fs.existsSync(path))throw Error('Never reuse a P6 tenant root');
      fs.mkdirSync(path,{recursive:true,mode:0o700});
      // The socket-visible root stays short; large SQLite/managed Node data
      // uses this runner's SSD backing, mounted only inside the private netns.
      const backing=resolve(this.config.output,'tenant-runtime-'+index);
      fs.mkdirSync(backing,{mode:0o700});
      execFileSync('mount',['--bind',backing,path]);
      fs.mkdirSync(resolve(path,'runtime/home'),{recursive:true,mode:0o700});
      for(const item of ['Cargo.toml','src','contracts'])fs.symlinkSync(resolve(source,item),resolve(path,item));
      mounts.push({root:path,rootBacking:backing,mountNamespace:fs.readlinkSync('/proc/self/ns/mnt')});
    }
    fs.writeFileSync(resolve(this.config.output,'p6-root-mounts.json'),JSON.stringify(mounts,null,2));
    fs.writeFileSync(resolve(root,'.sync-v4-e2e.json'),JSON.stringify({owner,synthetic:true,source:revision}));
    fs.mkdirSync(resolve(root,'bin'));
    fs.symlinkSync(this.config.binary,resolve(root,'bin/ctox'));
    delete this.env.CTOX_BUSINESS_OS_ENABLE_SMOKE_CONTROLS;
    delete this.env.CTOX_BUSINESS_OS_SIGNALING_URLS;
    if(!this.config.fixtureBinary || hashFile(this.config.fixtureBinary)!==this.config.fixtureBinarySha256)throw Error('Checksum-bound native fixture executable required');
    const fixtureEnv={...process.env,CTOX_P6_FIXTURE_CONFIG:this.config.configPath};
    delete fixtureEnv.CTOX_STATE_ROOT;delete fixtureEnv.CTOX_ROOT;
    const result=spawnSync(this.config.fixtureBinary,['p6_prepare_enrolled_profiles','--ignored','--nocapture'],{
      cwd:source,encoding:'utf8',timeout:90000,maxBuffer:1024*1024,
      env:fixtureEnv,
    });
    fs.writeFileSync(resolve(this.config.output,'fixture.log'),result.stdout+result.stderr,{mode:0o600});
    if(result.status!==0 || !/1 passed/.test(result.stdout))throw Error('not_run(native-enrollment): real foreground fixture failed');
    this.fixture=JSON.parse(fs.readFileSync(this.config.fixturePrivate));
    if(this.fixture.root!==root || this.fixture.revision!==revision || this.fixture.nativeVoters!==3)
      throw Error('Native fixture source/root mismatch');
    return {fixtureReady:true,nativeVoters:3,browserAdmission:'must be proven by E38 hello'};
  };
  Native.prototype.seed=function() {
    throw Error('Native scenario fixture/control is required; P6 never opens owner SQLite through the V3 seed helper');
  };
  Native.prototype.serve=function() {
    for(const root of this.fixture.roots.slice(1)) {
      this.start('voter-'+root.split('.').at(-1),this.config.binary,['service','--foreground','--root',root],
        {...this.env,CTOX_ROOT:root,CTOX_STATE_ROOT:resolve(root,'runtime')});
    }
    const args=['service','--foreground','--root',this.config.root];
    const actual=hashFile(this.config.binary);
    if(actual!==this.config.binarySha256)throw Error('P6 service binary changed after verification');
    const child=this.start('daemon',this.config.binary,args);this.daemon=child;
    const processRecord=this.receipts.find(row=>row.pid===child.pid);
    processRecord.owner=this.config.owner;processRecord.pidNamespace=fs.readlinkSync('/proc/self/ns/pid');
    processRecord.pidScope='private namespace';this.save();
    fs.writeFileSync(resolve(this.config.output,'p6-daemon-entry.json'),JSON.stringify({
      revision:this.config.revision,binarySha256:actual,root:this.config.root,
      args,pid:child.pid,pidNamespace:fs.readlinkSync('/proc/self/ns/pid'),at:new Date().toISOString()
    },null,2),{mode:0o600});
  };
}
