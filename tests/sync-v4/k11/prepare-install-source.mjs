// Supplement a minimal, already verified portable fixture with tracked installer
// assets from the SAME immutable revision. Does not rebuild or mutate the input.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {verifiedBuild} from '../../sync-v4-e2e/native.mjs';
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical lane required');
const [receipt,archive,expectedSha,revision,directory]=process.argv.slice(2),build=verifiedBuild(receipt);
const output=resolve(directory),allowed=resolve(process.env.BUILD_LANE_BIN,'../artifacts')+'/';
if(!output.startsWith(allowed)||revision!==build.revision)throw Error('Owned output and matching immutable source required');
const actual=createHash('sha256').update(fs.readFileSync(archive)).digest('hex');
if(actual!==expectedSha)throw Error('Tracked supplemental archive checksum mismatch');
const paths=execFileSync('tar',['-tf',archive],{encoding:'utf8'}).trim().split('\n');
if(paths.some(p=>p.split('/').includes('..')||p.startsWith('/')||!(['install.sh','src/','src/core/','src/core/harness/','src/core/harness/Cargo.toml','src/core/rxdb/','src/core/rxdb/Cargo.toml'].includes(p)||'src/skills/system/'.startsWith(p)||p.startsWith('src/skills/system/'))))throw Error('Installer supplemental archive path escape');
fs.mkdirSync(output,{recursive:false,mode:0o700});fs.cpSync(build.source,join(output,'source'),{recursive:true});
execFileSync('tar',['-xf',archive,'-C',join(output,'source')]);
const updated={...build,source:join(output,'source'),custody:{...build.custody,supplement:{revision,archiveSha256:actual,paths,originalReceipt:resolve(receipt)}}};
fs.writeFileSync(join(output,'build.json'),JSON.stringify(updated,null,2));
console.log(join(output,'build.json'));
