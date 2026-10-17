#!/usr/bin/env node
// Build a real daemon at an immutable revision; never check out over the runner.
import { spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { createHash } from 'node:crypto';
const args = Object.fromEntries(process.argv.slice(2).reduce((a, x, i, all) => x.startsWith('--') ? [...a, [x.slice(2), all[i + 1]]] : a, []));
if (process.platform !== 'linux' || !process.env.CARGO_TARGET_DIR || !process.env.TMPDIR || !process.env.BUILD_LANE_BIN) throw Error('Linux build lane required');
function run(cmd, argv, cwd = process.cwd(), env = process.env) {
  const p = spawnSync(cmd, argv, { cwd, env, stdio: 'inherit', timeout: 65 * 60_000 });
  if (p.error || p.status !== 0) throw Error(`${cmd} failed: ${p.error?.message ?? p.status}`);
}
const revision = spawnSync('git', ['rev-parse', '--verify', '--end-of-options', `${args.revision || '4794af03d'}^{commit}`], { encoding: 'utf8' }).stdout.trim();
if (!/^[a-f0-9]{40}$/.test(revision)) throw Error('Revision must resolve to a commit');
const output = resolve(args.output || join(process.env.BUILD_LANE_BIN, '..', 'artifacts', `v4-e2e-build-${Date.now()}`));
const artifactRoot = resolve(process.env.BUILD_LANE_BIN, '..', 'artifacts');
if (!output.startsWith(artifactRoot + '/')) throw Error('Producer must stay in the private lane artifact root');
mkdirSync(output, { recursive: true, mode: 0o700 });
if (existsSync(join(output, 'build.json'))) {
  const previous = JSON.parse(readFileSync(join(output, 'build.json')));
  if (previous.revision !== revision) throw Error('Do not overwrite another revision producer');
  if (previous.terminal && previous.pass && existsSync(previous.binary)
      && createHash('sha256').update(readFileSync(previous.binary)).digest('hex') === previous.binarySha256) {
    console.log(`Reused verified producer ${revision} ${previous.binarySha256}`);
    process.exit(0);
  }
}
// A previous producer for this exact immutable revision is stronger evidence
// than recompiling it under a different absolute CMake source directory.
const candidates = (await import('node:fs')).readdirSync(artifactRoot)
  .filter(name => /^v4-e2e-/.test(name)).sort();
for (const name of candidates) {
  const receiptPath=join(artifactRoot,name,'build.json');
  if(receiptPath===join(output,'build.json')||!existsSync(receiptPath))continue;
  let prior;try{prior=JSON.parse(readFileSync(receiptPath));}catch{continue;}
  if(prior.revision!==revision||!prior.pass||!prior.terminal||!prior.binary||!prior.source||!existsSync(prior.binary)||!existsSync(prior.source))continue;
  if(createHash('sha256').update(readFileSync(prior.binary)).digest('hex')!==prior.binarySha256)continue;
  const sourceHead=spawnSync('git',['rev-parse','HEAD'],{cwd:prior.source,encoding:'utf8'});
  const clean=spawnSync('git',['diff','--exit-code','HEAD'],{cwd:prior.source});
  if(sourceHead.status!==0||sourceHead.stdout.trim()!==revision||clean.status!==0)continue;
  writeFileSync(join(output,'build.json'),JSON.stringify({...prior,reusedFrom:receiptPath,reusedAt:new Date().toISOString()},null,2));
  console.log(`Reused verified producer ${revision} ${prior.binarySha256} from ${receiptPath}`);
  process.exit(0);
}
const source = join(output, `source-${revision.slice(0, 12)}`);
if (!existsSync(source)) run('git', ['worktree', 'add', '--detach', source, revision]);
const sourceHead = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: source, encoding: 'utf8' });
const sourceDiff = spawnSync('git', ['diff', '--exit-code', 'HEAD'], { cwd: source });
if (sourceHead.status !== 0 || sourceHead.stdout.trim() !== revision || sourceDiff.status !== 0) throw Error('Producer source is not the immutable requested revision');
// Native model build scripts retain absolute CMake source paths. Sharing their
// OUT_DIR across detached revisions makes an otherwise valid second build fail.
// Keep prior artifacts/caches intact and give every immutable revision its own
// cache BELOW the canonical admitted target (still on the lane's bulk SSD).
const sourceKey=createHash('sha256').update(source).digest('hex').slice(0,12);
const revisionTarget = join(process.env.CARGO_TARGET_DIR, 'e2e-revisions', revision.slice(0,12)+'-'+sourceKey);
const env = { ...process.env, CARGO_TARGET_DIR: revisionTarget, CARGO_BUILD_JOBS: '2', CMAKE_BUILD_PARALLEL_LEVEL: '2', RUST_TEST_THREADS: '2',
  CARGO_PROFILE_DEV_DEBUG: '0', CARGO_INCREMENTAL: '0' };
const receipt = { revision, source, target: revisionTarget, startedAt: new Date().toISOString(), terminal: false, pass: false };
const save = () => writeFileSync(join(output, 'build.json'), JSON.stringify(receipt, null, 2));
save();
try {
  run('bash', ['-lc', 'ctox-prep.sh && cargo build --locked --bin ctox --no-default-features -j 2'], source, env);
  const binary = join(output, `ctox-${revision.slice(0, 12)}`);
  copyFileSync(join(env.CARGO_TARGET_DIR, 'debug/ctox'), binary);
  receipt.binary = binary;
  receipt.binarySha256 = createHash('sha256').update(readFileSync(binary)).digest('hex');
  receipt.pass = true;
} catch (e) { receipt.error = e.message; process.exitCode = 1; }
finally { receipt.terminal = true; receipt.finishedAt = new Date().toISOString(); save(); }
