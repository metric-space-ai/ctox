import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {basename} from 'node:path';
import {fileURLToPath} from 'node:url';

// Include re-exports and side-effect imports, as well as literal dynamic imports.
// Do not evaluate modules: the guard also works on a slot without testkit files.
function moduleSpecifiers(source) {
  const staticImports=[...source.matchAll(/^\s*(?:import\s+(?:[^;]*?\s+from\s+)?|export\s+[^;]*?\s+from\s+)['"]([^'"]+)['"]/gm)].map(m=>m[1]);
  const dynamicImports=[...source.matchAll(/\bimport\s*\(\s*['"]([^'"]+)['"]\s*\)/g)].map(m=>m[1]);
  return [...staticImports,...dynamicImports];
}
test('E52 production entry graphs never require the testkit or fake core',async()=>{
  const root=new URL('../',import.meta.url),seen=new Set(),forbidden=[];
  async function visit(url,chain=[]) {
    const path=fileURLToPath(url),route=[...chain,basename(path)];
    if(/^(?:fake-core.*|testkit)\.mjs$/.test(basename(path))){forbidden.push(route.join(' -> '));return;}
    if(seen.has(path))return;seen.add(path);
    for(const spec of moduleSpecifiers(await readFile(url,'utf8'))) {
      if(!spec.startsWith('.'))continue;
      await visit(new URL(spec,url),route);
    }
  }
  for(const entry of ['index.mjs','boot.mjs'])await visit(new URL(entry,root));
  assert.deepEqual(forbidden,[],'production requires unserved test modules: '+forbidden.join('; '));
  assert.ok(seen.has(fileURLToPath(new URL('kernel.mjs',root))),'guard must follow the index re-export');
  assert.ok(seen.has(fileURLToPath(new URL('transport-webrtc.mjs',root))),'guard must follow real transport dependencies');
});
