// Small Rust unit runner for the actual projection helpers and schema types.
// This does not replace the full CTOX/RxDB integration suites.
import assert from 'node:assert/strict';
import { readFile, writeFile, mkdir, mkdtemp } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';

const nativeRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const scratchRoot = process.env.TMPDIR;
assert(scratchRoot?.startsWith('/Volumes/tmp/'), 'Set TMPDIR under mounted /Volumes/tmp.');
const source = await readFile(resolve(nativeRoot, 'rxdb_peer.rs'), 'utf8');
const functionNames = ['prepare_projection_tombstone_document', 'projection_tombstone_required_default'];
const definitions = functionNames.map((name) => {
  const start = source.indexOf(`fn ${name}(`);
  assert(start >= 0, `Missing ${name}`);
  assert.equal(source.indexOf(`fn ${name}(`, start + 1), -1, `Ambiguous ${name}`);
  const end = source.indexOf('\n}\n', start);
  assert(end > start, `Missing function boundary for ${name}`);
  return source.slice(start, end + 3);
}).join('\n');
const scratch = await mkdtemp(resolve(scratchRoot, 'office-tombstone-unit-'));
await mkdir(resolve(scratch, 'src'));
const schemaPath = resolve(nativeRoot, '../rxdb/src/types/schema.rs');
const testsPath = resolve(nativeRoot, 'rxdb_peer_projection_tombstone_tests.rs');
await writeFile(resolve(scratch, 'Cargo.toml'), `[package]
name = "ctox-office-tombstone-unit"
version = "0.0.0"
edition = "2021"
[workspace]
[dependencies]
serde = { version = "=1.0.228", features = ["derive"] }
serde_json = { version = "=1.0.149", features = ["preserve_order"] }
`);
await writeFile(resolve(scratch, 'src/lib.rs'), `#![allow(dead_code)]
#[path = ${JSON.stringify(schemaPath)}]
mod schema;
use schema::RxJsonSchema;
use serde_json::Value;
${definitions}
#[cfg(test)]
#[path = ${JSON.stringify(testsPath)}]
mod projection_tombstone_tests;
`);
console.log(JSON.stringify({ scratch, source_sha256: createHash('sha256').update(definitions).digest('hex'), scope: 'actual-helper-and-schema-unit-tests' }));
const result = spawnSync('cargo', ['test', '--offline', '--manifest-path', resolve(scratch, 'Cargo.toml'), '--', '--nocapture'], {
  stdio: 'inherit',
  env: { ...process.env, CARGO_TARGET_DIR: resolve(scratchRoot, 'office-tombstone-unit-target') },
});
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
