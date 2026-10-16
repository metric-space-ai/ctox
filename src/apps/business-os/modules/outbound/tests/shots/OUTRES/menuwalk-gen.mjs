// Erzeugt menuwalk.mjs: rundgang2.mjs (Server, Stub, echter v4-Rahmen, Launcher-Klick, Maximieren) unverändert
// übernommen, nur Pfade absolut und main() durch einen gezielten Menü-Rundgang (echte Maus + Tastatur) ersetzt.
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';

const BOS = join(homedir(), 'Documents/ctox-shell-v4/src/apps/business-os');
const RG = join(BOS, 'v4/tools/rundgang');
let src = readFileSync(join(RG, 'rundgang2.mjs'), 'utf8');
src = src.replace(`'../geometry/geometry-v4.mjs'`, JSON.stringify(join(BOS, 'v4/tools/geometry/geometry-v4.mjs')));
src = src.replace(/const HERE = dirname\(fileURLToPath\(import\.meta\.url\)\);/, `const HERE = ${JSON.stringify(RG)};`);
src = src.replace(/^#!.*\n/, '');
src = src.replace(/main\(\)\.catch\(\(e\) => \{ console\.error\(e\); process\.exit\(1\); \}\);\s*$/, '');
src = src.replace("sh.setTitle = (x) => {", "const nv = sh.navigate; sh.navigate = (a, b) => { (rg.nav = rg.nav || []).push(String(a) + ' ' + JSON.stringify(b)); return nv.call(sh, a, b); };\n  sh.setTitle = (x) => {");
if (!src.includes('rg.nav')) throw new Error('navigate-Hook nicht gesetzt');
const custom = readFileSync(new URL('./menuwalk-body.mjs', import.meta.url), 'utf8');
writeFileSync(new URL('./menuwalk.mjs', import.meta.url), src + '\n' + custom);
console.log('menuwalk.mjs geschrieben');
