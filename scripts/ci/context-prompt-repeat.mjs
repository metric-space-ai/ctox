import { spawn } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
const [baseline, binary, output] = process.argv.slice(2);
if (!baseline || !binary || !output) throw Error('baseline source root, immutable native binary, output directory required');
mkdirSync(output, { recursive: true });
const after = process.cwd(), rows = [];
const deadline = Number(process.env.CONTEXT_PROOF_DEADLINE_MS || 0);
if (!Number.isFinite(deadline) || deadline <= Date.now()) throw Error('Explicit bounded proof deadline required');
const save = () => writeFileSync(path.join(output, 'summary.json'), JSON.stringify({
  baseline, after, binary, requiredRunsPerSource: 10, rows,
  complete: rows.filter(r => r.source === 'before' && r.timings.length).length === 10
    && rows.filter(r => r.source === 'after' && r.timings.length).length === 10,
}, null, 2) + '\n');
async function run(source, index) {
  const root = source === 'before' ? baseline : after;
  const lifecycle = path.join(output, `${source}-${index}-processes.json`);
  const startedAt = new Date().toISOString();
  const child = spawn(process.execPath, [path.join(root, 'src/core/rxdb/tools/browser_rust_smoke.js')], {
    cwd: root, detached: true,
    env: { ...process.env, CTOX_BIN: binary, SMOKE_MODE: 'business-os-threads-rightclick-ui',
      SMOKE_PAGE_PATH: '/index.html', BUSINESS_PORT: '19179', SIGNALING_PORT: '19178',
      SMOKE_PROCESS_LIFECYCLE_PATH: lifecycle },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const buffers = { stdout: '', stderr: '' };
  let errors = [], timings = [], warnings = null, timedOut = false;
  function data(stream, chunk) {
    let buffer = buffers[stream] + chunk.toString();
    let pos;
    while ((pos = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, pos); buffer = buffer.slice(pos + 1);
      const key = 'context_prompt_first_paint=';
      const start = line.indexOf(key);
      if (start >= 0) { try { timings.push(JSON.parse(line.slice(start + key.length))); } catch {} }
      if (/^browser_warning_count=/.test(line)) warnings = Number(line.split('=')[1]);
      if (/Context prompt .*exceeded 150ms/.test(line)) errors.push('prompt-budget');
      if (/STREAM_LIMIT_EXCEEDED/.test(line)) errors.push('stream-limit');
      if (/collection is not registered/.test(line)) errors.push('unregistered-collection');
    }
    // Never persist arbitrary wire/bootstrap output or credentials.
    buffers[stream] = buffer.length > 1024 * 1024 ? '' : buffer;
  }
  child.stdout.on('data', chunk => data('stdout', chunk));
  child.stderr.on('data', chunk => data('stderr', chunk));
  const timeout = setTimeout(() => { timedOut = true; child.kill('SIGTERM'); }, Math.min(65000, deadline - Date.now()));
  let force;
  child.on('exit', () => clearTimeout(force));
  force = setTimeout(() => { if (child.exitCode === null && child.signalCode === null) process.kill(-child.pid, 'SIGKILL'); }, Math.min(70000, deadline - Date.now() + 5000));
  const result = await new Promise(resolve => child.on('close', (code, signal) => resolve({ code, signal })));
  clearTimeout(timeout); clearTimeout(force);
  rows.push({ source, index, startedAt, endedAt: new Date().toISOString(), pid: child.pid,
    ...result, timedOut, timings, errorCategories: [...new Set(errors)], warnings, lifecycle });
  save(); console.log(JSON.stringify(rows.at(-1)));
}
for (let index = 1; index <= 10; index++) {
  for (const source of ['before', 'after']) {
    if (Date.now() + 70000 >= deadline) { save(); process.exit(2); }
    await run(source, index);
    if (!rows.at(-1).timings.length) { save(); process.exit(3); }
  }
}
save();
process.exitCode = rows.some(r => r.source === 'after' && (r.code !== 0 || !r.timings.length)) ? 1 : 0;
