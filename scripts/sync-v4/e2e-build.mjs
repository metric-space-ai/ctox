#!/usr/bin/env node
// Build a real daemon at an immutable revision; never check out over the runner.
import { spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { createHash } from 'node:crypto';
const args = Object.fromEntries(process.argv.slice(2).reduce((a, x, i, all) => x.startsWith('--') ? [...a, [x.slice(2), all[i + 1]]] : a, []));
if (process.platform !== 'linux' || !process.env.CARGO_TARGET_DIR || !process.env.TMPDIR) throw Error('Linux build lane required');
function run(cmd, argv, cwd = process.cwd(), env = process.env) {
  const p = spawnSync(cmd, argv, { cwd, env, stdio: 'inherit', timeout: 65 * 60_000 });
  if (p.error || p.status !== 0) throw Error(`${cmd} failed: ${p.error?.message ?? p.status}`);
}
const revision = spawnSync('git', ['rev-parse', '--verify', `${args.revision || '4794af03d'}^{commit}`], { encoding: 'utf8' }).stdout.trim();
if (!/^[a-f0-9]{40}$/.test(revision)) throw Error('Revision must resolve to a commit');
const output = resolve(args.output || join(process.env.TMPDIR, 'e2e-build'));
mkdirSync(output, { recursive: true, mode: 0o700 });
const source = join(output, `source-${revision.slice(0, 12)}`);
if (!existsSync(source)) run('git', ['worktree', 'add', '--detach', source, revision]);
const env = { ...process.env, CARGO_BUILD_JOBS: '2', CMAKE_BUILD_PARALLEL_LEVEL: '2', RUST_TEST_THREADS: '2',
  CARGO_PROFILE_DEV_DEBUG: '0', CARGO_INCREMENTAL: '0' };
const receipt = { revision, source, startedAt: new Date().toISOString(), terminal: false, pass: false };
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
