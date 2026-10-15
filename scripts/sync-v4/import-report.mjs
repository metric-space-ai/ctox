#!/usr/bin/env node
// Input custody + payload-free accounting. Not a database writer.
import { readFile, readdir, mkdir, writeFile } from 'node:fs/promises';
import { resolve, join, dirname } from 'node:path';
import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';

export async function inventory(v4Root, platform) {
  const files = [];
  const load = async (path) => {
    const bytes = await readFile(path);
    files.push({path, bytes:bytes.length, sha256:createHash('sha256').update(bytes).digest('hex')});
    return JSON.parse(bytes);
  };
  const manifests = []; const schemas = {}; const migrations = {}; const removed = []; const fieldMerge = [];
  const strip = (value,path) => {
    if (!value || typeof value !== 'object') return;
    for (const key of ['syncProfile','conflictStrategy']) if (Object.hasOwn(value,key)) {
      removed.push(`${path}.${key}`);
      if (value[key] === 'field-merge') fieldMerge.push(path.split('.collections.')[1]?.split('.')[0] ?? path);
      delete value[key];
    }
    for (const [k,v] of Object.entries(value)) strip(v,`${path}.${k}`);
  };
  for (const name of (await readdir(join(v4Root,'manifests'))).filter(n=>n.endsWith('.manifest.json')).sort()) {
    const manifest = await load(join(v4Root,'manifests',name));
    if (manifest.app_id === 'ctox-platform') continue;
    const app = manifest.app_id;
    if (!/^[a-z0-9-]+$/.test(app)) throw Error('invalid app id');
    let schema;
    try { schema = await load(join(v4Root,'schemas',`${app}.collections.schema.json`)); }
    catch (error) {
      // The shell is itself an app, explicitly without collections (§10.2a).
      if (error.code !== 'ENOENT' || Object.keys(manifest.collections ?? {}).length) throw error;
      schema = {_base_release:'main@4794af03d',collections:{}};
    }
    if (schema._base_release !== 'main@4794af03d') throw Error(`wrong schema base: ${app}`);
    for (const [collection,migration] of Object.entries(schema.migrations ?? {})) {
      if (migration.from_ver !== 'main@4794af03d') throw Error(`wrong migration base: ${collection}`);
      if (migration._file) {
        const migrationPath = resolve(v4Root,'..',migration._file);
        if (!migrationPath.startsWith(resolve(v4Root,'schemas/migrations')+'/')) throw Error('migration path escapes release');
        const bytes = await readFile(migrationPath);
        migrations[collection] = { ...migration, sha256:createHash('sha256').update(bytes).digest('hex') };
        files.push({path:migrationPath,bytes:bytes.length,sha256:migrations[collection].sha256});
      } else migrations[collection] = migration;
    }
    strip(manifest,app); strip(schema,app); manifests.push(manifest); schemas[app] = schema;
  }
  const platformManifest = await load(platform); strip(platformManifest,'ctox-platform'); manifests.push(platformManifest);
  const names = new Set(manifests.flatMap(m=>Object.keys(m.collections ?? {})));
  for (const required of ['app_drafts','app_catalog','mail_server_state']) if (!names.has(required)) throw Error(`missing platform collection: ${required}`);
  return {schema:'ctox.v4.import_inventory.v1',from_ver:'main@4794af03d',files,
    apps:manifests.length,collections:[...names].filter(n=>n!=='business_module_catalog').sort(),
    migrations,removed_sync_settings:[...new Set(removed)].sort(),field_merge_core_rule:[...new Set(fieldMerge)].sort(),
    manifests,schemas};
}

export function accounting(report) {
  const collections = report.collections ?? {};
  return Object.fromEntries(Object.entries(collections).map(([collection,c]) => [collection,{
    ...c,complete:c.source_rows === c.target_documents && (c.rejected ?? 0) === 0,
  }]));
}
async function main() {
  const args = process.argv.slice(2); const options = {};
  for (let i=0;i<args.length;i+=2) {
    if (!['--v4-root','--platform','--output','--counts'].includes(args[i]) || !args[i+1]) throw Error('usage: --v4-root DIR --platform FILE --output FILE [--counts FILE]');
    options[args[i]] = args[i+1];
  }
  const result = await inventory(resolve(options['--v4-root']),resolve(options['--platform']));
  // Output is metadata only: no schema defaults, document/config bodies or rows.
  delete result.manifests; delete result.schemas;
  if (options['--counts']) {
    const counts = JSON.parse(await readFile(options['--counts'],'utf8'));
    const report = counts.report ?? counts;
    result.accounting = accounting(report);
    result.physical_source_rows = report.physical_source_rows ?? {};
  }
  await mkdir(dirname(resolve(options['--output'])),{recursive:true});
  await writeFile(options['--output'],JSON.stringify(result,null,2)+'\n');
  console.log(JSON.stringify({output:resolve(options['--output']),apps:result.apps,collections:result.collections.length}));
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) main().catch(e=>{console.error(e.message);process.exitCode=1;});
