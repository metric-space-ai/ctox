import fs from 'node:fs';
import {join} from 'node:path';
import {install} from './upgrade.mjs';
const c=JSON.parse(fs.readFileSync(process.argv[2]));
try{const r=await install(c);console.log(JSON.stringify({installerExecuted:r.installerExecuted,migrationExecuted:r.migrationExecuted,activeBinarySha256:r.activeBinarySha256}));}
catch(e){fs.writeFileSync(join(c.output,'failure.json'),JSON.stringify({error:e.message,stack:e.stack,at:new Date().toISOString(),installerExecuted:false,migrationExecuted:false},null,2));throw e;}
