// Extend the frozen base manifest with ONLY the owned synthetic fixture app.
// No signed app receipt is invented; this proves schema/data import only.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {schema} from '../../sync-v4-e2e/browser.mjs';
const [directory,archive,custodyPath]=process.argv.slice(2),root=resolve(directory);
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN||!root.startsWith(resolve(process.env.BUILD_LANE_BIN,'../artifacts')+'/'))throw Error('Owned lane input copy required');
const custody=JSON.parse(fs.readFileSync(custodyPath));
if(createHash('sha256').update(fs.readFileSync(archive)).digest('hex')!==custody.archiveSha256)throw Error('Base input archive differs');
const listing=spawnSync('tar',['-tf',archive],{encoding:'utf8'});if(listing.status!==0)throw Error('Input archive unavailable');
if(listing.stdout.trim().split('\n').some(path=>path.startsWith('/')||path.split('/').includes('..')||!custody.files.some(f=>f.path===path||(path.endsWith('/')&&f.path.startsWith(path)))))throw Error('Unlisted base input');
fs.mkdirSync(root,{mode:0o700});
const extracted=spawnSync('tar',['-xf',archive,'-C',root]);if(extracted.status!==0)throw Error('Base extraction failed');
for(const file of custody.files)if(createHash('sha256').update(fs.readFileSync(join(root,file.path))).digest('hex')!==file.sha256)throw Error('Base input file differs');
fs.copyFileSync(custodyPath,join(root,'source-custody.json'));
const names=['e2e_leads','e2e_history'],app='e2e-probe';
const manifest={app_id:app,app_ver:'main@4794af03d+v4',title:'Owned synthetic regression fixture',collections:Object.fromEntries(names.map(name=>[name,{class:'full',client_limit:100000,byte_budget:33554432,schema}]))};
fs.writeFileSync(join(root,'v4/manifests',app+'.manifest.json'),JSON.stringify(manifest));
fs.writeFileSync(join(root,'v4/schemas',app+'.collections.schema.json'),JSON.stringify({_base_release:'main@4794af03d',collections:Object.fromEntries(names.map(name=>[name,schema]))}));
console.log(JSON.stringify({fixtureApp:app,collections:names,signedAppPublished:false}));
