// No candidate code is imported here. Semantic evaluation is an explicit,
// fail-closed OS sandbox, separate from static text/contract validation.
import { existsSync, realpathSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

export function runModuleSemanticCheck(moduleDir, request) {
  const scripts = realpathSync(dirname(fileURLToPath(import.meta.url)));
  const moduleRoot = realpathSync(moduleDir);
  const node = realpathSync(process.execPath);
  const worker = join(scripts, 'module_semantic_worker.mjs');
  // Avoid ambient OpenSSL configuration; newer Node versions detect ESM without this flag.
  const nodeArgs = ['--openssl-config=/dev/null'];
  if (process.allowedNodeEnvironmentFlags.has('--experimental-default-type')) {
    nodeArgs.push('--experimental-default-type=module');
  }
  nodeArgs.push(worker);
  const runtimeRoots = [dirname(node), '/usr/lib', '/usr/share', '/System',
    '/opt/homebrew/Cellar', '/opt/homebrew/opt'].filter(existsSync);
  let executable;
  let args;
  if (process.platform === 'darwin') {
    executable = '/usr/bin/sandbox-exec';
    const literal = (value) => JSON.stringify(value);
    const reads = [moduleRoot, scripts, ...runtimeRoots]
      .map((path) => `(subpath ${literal(path)})`).join(' ');
    // The existing harness restricted-read platform policy distinguishes dylib
    // mapping and dyld's Sandbox syscall from ordinary file reads.
    const executableReads = runtimeRoots.map((path) => `(subpath ${literal(path)})`).join(' ');
    const profile = `(version 1)

(deny default)
(allow process-exec (literal ${literal(node)}))
(allow process-info* (target self))
(allow signal (target self))
(allow sysctl-read)
(allow file-map-executable ${executableReads})
(allow system-mac-syscall (require-all (mac-policy-name "Sandbox") (mac-syscall-number 67)))

(allow file-read-metadata)
; dyld inspects the root directory before initializing Node; this grants no descendants.
(allow file-read-data (literal "/"))
(allow file-read* ${reads} (literal "/dev/null") (literal "/dev/urandom"))
(allow file-write-data (literal "/dev/null"))`;
    args = ['-p', profile, node, ...nodeArgs];
  } else if (process.platform === 'linux') {
    executable = '/usr/bin/bwrap';
    args = ['--die-with-parent', '--new-session', '--unshare-all', '--cap-drop', 'ALL',
      '--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp'];
    for (const path of [...new Set(['/usr', '/lib', '/lib64', '/bin', dirname(node), scripts, moduleRoot])]) {
      if (existsSync(path)) args.push('--ro-bind', path, path);
    }
    args.push('--chdir', moduleRoot, '--', node, ...nodeArgs);
  } else {
    throw new Error('OS-isolated module semantic validation is unavailable on this platform');
  }
  if (!existsSync(executable)) throw new Error(`required semantic sandbox is unavailable: ${executable}`);
  const run = spawnSync(executable, args, {
    cwd: moduleRoot,
    env: { PATH: dirname(node), HOME: '/nonexistent', LANG: 'C', TZ: 'UTC' },
    input: JSON.stringify(request), encoding: 'utf8', timeout: 5000,
    killSignal: 'SIGKILL', maxBuffer: 1024 * 1024,
  });
  if (run.error || run.status !== 0) {
    throw new Error(`isolated semantic check failed: ${run.error?.message || run.stderr || run.signal || run.status}`);
  }
  return JSON.parse(run.stdout);
}
