import fs from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';
import { join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
export const owner = '01a087a0-169a-7e23-ba3d-71352256cbfb';
export const pause = ms => new Promise(r => setTimeout(r, ms));
export async function until(fn, ms, description) {
  const deadline = Date.now() + ms; let last;
  do { try { const value = await fn(); if (value) return value; } catch (e) { last = e; }
    await pause(100);
  } while (Date.now() < deadline);
  throw Error(`${description}: ${last?.message || 'deadline exceeded'}`);
}
export class Native {
  constructor(config) { this.config = config; this.children = new Set(); this.receipts = [];
    this.env = { ...process.env, HOME: join(config.root, "runtime/home"), CTOX_ROOT: config.root, CTOX_STATE_ROOT: join(config.root, 'runtime'),
      CTOX_BUSINESS_OS_SIGNALING_URLS: `ws://127.0.0.1:${config.signalingPort}`,
      CTOX_BUSINESS_OS_ENABLE_SMOKE_CONTROLS: '1', CTOX_WEBRTC_UDP_BIND_ADDR: '192.0.2.1:0',
      CTOX_BUSINESS_OS_DISABLE_BACKGROUND_FILE_INDEX: '1' };
  }
  save() { fs.writeFileSync(join(this.config.output, 'processes.json'), JSON.stringify(this.receipts, null, 2)); }
  cli(args) {
    const p = spawnSync(this.config.binary, args, { cwd: this.config.root, env: this.env, encoding: 'utf8', timeout: 60000, maxBuffer: 4*1024*1024 });
    if (p.status !== 0 || p.error) throw Error(`Isolated CLI ${args.slice(0,3).join(' ')} failed: ${p.status}: ${p.stderr?.slice(-1500)}`);
    return p.stdout;
  }
  async prepare(schema, here) {
    if (fs.existsSync(this.config.root)) throw Error('Never reuse a tenant root');
    fs.mkdirSync(join(this.config.root, 'runtime'), { recursive: true, mode: 0o700 });
    fs.writeFileSync(join(this.config.root, '.sync-v4-e2e.json'), JSON.stringify({ owner, synthetic: true, source: this.config.revision }));
    fs.mkdirSync(join(this.config.root, 'runtime/home'), { recursive: true, mode: 0o700 });
    fs.symlinkSync(join(this.config.source, 'Cargo.toml'), join(this.config.root, 'Cargo.toml'));
    fs.symlinkSync(join(this.config.source, 'src'), join(this.config.root, 'src'));
    fs.symlinkSync(join(this.config.source, 'contracts'), join(this.config.root, 'contracts'));
    fs.mkdirSync(join(this.config.root, 'bin'));
    fs.symlinkSync(this.config.binary, join(this.config.root, 'bin/ctox'));
    this.cli(['business-os','rxdb','init','--root',this.config.root]);
    const module = join(this.config.root, 'runtime/business-os/local-modules/e2e-probe'); fs.mkdirSync(module, { recursive: true });
    fs.writeFileSync(join(module, 'module.json'), JSON.stringify({ id: 'e2e-probe', title: 'Synthetic E2E regression app', version: '1.0.0', entry: 'local-modules/e2e-probe/index.js', collections: ['e2e_leads'] }));
    fs.copyFileSync(join(here, 'fixture-app.mjs'), join(module, 'index.js'));
    fs.writeFileSync(join(module, 'schema.js'), 'export const collections = {};\n');
    fs.writeFileSync(join(module, 'collections.schema.json'), JSON.stringify({ schema_format: 'ctox-business-os-module-collections-v1', collections: { e2e_leads: { schema, syncProfile: 'eager' } } }));
    const fixture = this.seed('seed', here);
    // Register the background volume too, without making it eager or executing
    // any real command semantics. Unknown tables must not distort readiness.
    const volumeNames = fixture.populations.map(p => p.collection).filter(n => n !== 'e2e_leads');
    const volume = join(this.config.root, 'runtime/business-os/local-modules/e2e-volume');
    fs.mkdirSync(volume, { recursive: true });
    fs.writeFileSync(join(volume, 'module.json'), JSON.stringify({ id: 'e2e-volume', title: 'Synthetic background volume',
      version: '1.0.0', entry: 'local-modules/e2e-volume/index.js', collections: volumeNames }));
    fs.writeFileSync(join(volume, 'index.js'), 'export function mount() { return () => {}; }\n');
    fs.writeFileSync(join(volume, 'schema.js'), 'export const collections = {};\n');
    fs.writeFileSync(join(volume, 'collections.schema.json'), JSON.stringify({ schema_format: 'ctox-business-os-module-collections-v1',
      collections: Object.fromEntries(volumeNames.map(name => [name, { schema, syncProfile: 'demand-only' }])) }));
    return fixture;
  }
  seed(operation, here) {
    const p = spawnSync('python3', [join(here,'seed.py'),'--root',this.config.root,'--profile',this.config.profile,'--operation',operation], { encoding:'utf8', timeout:180000, maxBuffer:16*1024*1024 });
    if (p.status !== 0 || p.error) throw Error(`Synthetic ${operation} failed: ${p.stderr?.slice(-1500)}`);
    return JSON.parse(p.stdout);
  }
  start(kind, executable, args, env = this.env) {
    const log = fs.openSync(join(this.config.output, `${kind}.log`), 'a', 0o600);
    const child = spawn(executable, args, { cwd: this.config.root, env, detached: true, stdio: ['ignore', log, log] });
    fs.closeSync(log); this.children.add(child);
    const row = { owner, pid: child.pid, pgid: child.pid, kind, stop: 'runner finally or absolute run deadline', startedAt: new Date().toISOString() };
    this.receipts.push(row); this.save(); child.once('error', e => { row.error = e.message; });
    child.once('exit', (code, signal) => { row.code = code; row.signal = signal; this.save(); }); return child;
  }
  async stop(child, signal = 'SIGTERM') {
    if (!child?.pid) return;
    try { process.kill(-child.pid, signal); } catch (e) { if (e.code !== 'ESRCH') throw e; }
    await until(() => child.exitCode !== null || child.signalCode !== null, 5000, 'owned process exit').catch(() => {});
    try { process.kill(-child.pid, 'SIGKILL'); } catch (e) { if (e.code !== 'ESRCH') throw e; }
    const row = this.receipts.find(r => r.pid === child.pid);
    await until(() => { try { process.kill(-child.pid,0); return false; } catch (e) { return e.code === 'ESRCH'; } },5000,'owned group retirement');
    row.groupAbsent = true; this.children.delete(child); this.save();
  }
  serve() { this.daemon = this.start('daemon', this.config.binary, ['business-os','serve','--addr',`127.0.0.1:${this.config.port}`]); }
  async restart() { await this.stop(this.daemon,'SIGKILL'); this.serve(); }
  async close() { for (const child of [...this.children].reverse()) await this.stop(child); }
}
export function verifiedBuild(path) {
  const build = JSON.parse(fs.readFileSync(path));
  if (!build.pass || !build.terminal || !/^[a-f0-9]{40}$/.test(build.revision)) throw Error('Terminal real-daemon build required');
  const hash = createHash('sha256').update(fs.readFileSync(build.binary)).digest('hex');
  if (hash !== build.binarySha256) throw Error('Daemon binary checksum differs from producer');
  return { ...build, binary: resolve(build.binary), source: resolve(build.source) };
}
