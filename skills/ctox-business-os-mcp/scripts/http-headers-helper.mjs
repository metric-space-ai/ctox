// Origin: CTOX — License: AGPL-3.0-only
// stdout is a private credential pipe to Codex, never a diagnostic stream.
import { spawn } from 'node:child_process';
import { isAbsolute } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const script = fileURLToPath(import.meta.url);
export const LIMITS = Object.freeze({ timeoutMs: 20000, stdoutBytes: 32768, stderrBytes: 8192 });
function validPath(path) { return typeof path === 'string' && isAbsolute(path) && path.endsWith('.mjs'); }
function validToken(token) {
  return typeof token === 'string' && token.length <= 16384 && /^[A-Za-z0-9._~+/-]+=*$/.test(token);
}
function validateHeaders(text) {
  let value;
  try { value = JSON.parse(text); } catch { throw new Error('invalid_loader_output'); }
  if (!value || Array.isArray(value) || typeof value !== 'object'
    || Object.keys(value).length !== 1 || typeof value.Authorization !== 'string'
    || !value.Authorization.startsWith('Bearer ') || !validToken(value.Authorization.slice(7))) {
    throw new Error('invalid_loader_output');
  }
  return value;
}

// Imports run in a child so unexpected loader output/errors never reach Codex.
async function loadChild(loaderPath) {
  if (!validPath(loaderPath)) throw new Error('invalid_configuration');
  const module = await import(pathToFileURL(loaderPath).href);
  if (typeof module.loadBearerToken !== 'function') throw new Error('invalid_loader');
  const token = await module.loadBearerToken();
  if (!validToken(token)) throw new Error('invalid_token');
  await new Promise((resolve, reject) => process.stdout.write(
    JSON.stringify({ Authorization: `Bearer ${token}` }) + '\n',
    (error) => error ? reject(error) : resolve(),
  ));
}

export function readHeaders({ loaderPath, runtime = process.execPath, spawnImpl = spawn,
  signal, limits = LIMITS }) {
  if (!validPath(loaderPath)) return Promise.reject(new Error('invalid_configuration'));
  if (signal?.aborted) return Promise.reject(new Error('helper_aborted'));
  return new Promise((resolve, reject) => {
    let child, timer, stdout = [], stdoutBytes = 0, stderrBytes = 0, settled = false;
    const grouped = process.platform !== 'win32';
    const kill = () => {
      // Child and its owned subprocesses share a dedicated process group on Unix.
      // No arbitrary process IDs or user shells are targets.
      try {
        if (grouped && child?.pid) process.kill(-child.pid, 'SIGKILL');
        else child?.kill('SIGKILL');
      } catch { /* Already exited. */ }
    };
    const finish = (code) => {
      if (settled) return;
      settled = true; clearTimeout(timer); signal?.removeEventListener('abort', abort);
      kill();
      try {
        if (code) throw new Error(code);
        if (stderrBytes) throw new Error('unexpected_loader_stderr');
        const text = new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(stdout));
        resolve(validateHeaders(text));
      } catch (error) {
        // Parser/network/import exceptions may contain a secret; never propagate their text.
        reject(new Error(code || (stderrBytes ? 'unexpected_loader_stderr' : 'invalid_loader_output')));
      } finally { stdout = []; }
    };
    const abort = () => finish('helper_aborted');
    try {
      child = spawnImpl(runtime, [script, '--load-child', loaderPath], {
        stdio: ['ignore', 'pipe', 'pipe'], detached: grouped, windowsHide: true,
      });
      timer = setTimeout(() => finish('helper_timeout'), limits.timeoutMs);
      signal?.addEventListener('abort', abort, { once: true });
      child.on('error', () => finish('helper_spawn_failed'));
      child.stdout.on('error', () => finish('helper_pipe_failed'));
      child.stderr.on('error', () => finish('helper_pipe_failed'));
      child.stdout.on('data', (chunk) => {
        if (settled) return;
        stdoutBytes += chunk.length;
        if (stdoutBytes > limits.stdoutBytes) finish('loader_stdout_limit');
        else stdout.push(Buffer.from(chunk));
      });
      child.stderr.on('data', (chunk) => {
        if (settled) return;
        stderrBytes += chunk.length;
        if (stderrBytes > limits.stderrBytes) finish('loader_stderr_limit');
        // Do not retain or echo stderr, even when it is within bounds.
      });
      child.on('close', (code) => finish(code === 0 ? undefined : 'loader_failed'));
      if (signal?.aborted) abort();
    } catch { finish('helper_spawn_failed'); }
  });
}

async function main() {
  const args = process.argv.slice(2);
  if (args.length !== 2 || !validPath(args[1])) throw new Error('invalid_configuration');
  if (args[0] === '--load-child') {
    await loadChild(args[1]);
    // Flush completed above; do not let a loader's abandoned timers keep this child alive.
    process.exit(0);
  }
  if (args[0] !== '--secret-loader') throw new Error('invalid_configuration');
  const controller = new AbortController();
  const abort = () => controller.abort();
  process.once('SIGINT', abort); process.once('SIGTERM', abort);
  try {
    const headers = await readHeaders({ loaderPath: args[1], signal: controller.signal });
    await new Promise((resolve, reject) => process.stdout.write(JSON.stringify(headers) + '\n',
      (error) => error ? reject(error) : resolve()));
  } finally { process.off('SIGINT', abort); process.off('SIGTERM', abort); }
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  // No exception stack, args, loader stdout/stderr or header values in diagnostics.
  process.stdout.on('error', () => process.exit(1));
  main().catch(() => {
    if (process.argv[2] !== '--load-child') process.stderr.write('ctox-mcp: header helper failed\n');
    process.exitCode = 1;
  });
}
