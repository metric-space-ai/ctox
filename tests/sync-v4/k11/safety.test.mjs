import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {requireSandbox} from './upgrade.mjs';
test('installer cannot act in the host namespaces even with a claimed masked runtime',()=>{
 const host=Object.fromEntries(['net','mnt','pid'].map(n=>[n,fs.readlinkSync('/proc/self/ns/'+n)]));
 assert.throws(()=>requireSandbox({host,runtimeMasked:true}),/Private net namespace required/);
});
